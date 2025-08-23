use crate::container::{ContainerManager, CodeExecutor, CodeExecutionResult, ResourceLimits, NetworkConfig};
use crate::error::{Result, SoulBoxError};
use crate::template::{Template, DockerfileParser, DockerImageBuilder};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};
use std::time::Duration;
use uuid::Uuid;

/// High-level sandbox manager that coordinates containers and code execution
pub struct SandboxManager {
    container_manager: Arc<ContainerManager>,
    active_sandboxes: Arc<RwLock<HashMap<String, SandboxInfo>>>,
    default_timeout: Duration,
}

#[derive(Clone)]
struct SandboxInfo {
    sandbox_id: String,
    container_id: String,
    executor: Arc<CodeExecutor>,
    created_at: std::time::Instant,
    language: String,
}

impl SandboxManager {
    pub fn new(container_manager: Arc<ContainerManager>) -> Self {
        Self {
            container_manager,
            active_sandboxes: Arc::new(RwLock::new(HashMap::new())),
            default_timeout: Duration::from_secs(30),
        }
    }

    /// Create a new sandbox for code execution
    pub async fn create_sandbox(
        &self,
        language: &str,
        resource_limits: Option<ResourceLimits>,
        network_config: Option<NetworkConfig>,
    ) -> Result<String> {
        let sandbox_id = Uuid::new_v4().to_string();
        info!("Creating new sandbox {} for language: {}", sandbox_id, language);

        // Select appropriate Docker image based on language
        let image = match language.to_lowercase().as_str() {
            "python" | "py" => "python:3.11-slim",
            "javascript" | "js" | "node" | "nodejs" => "node:20-slim",
            "rust" => "rust:latest",
            "go" => "golang:latest",
            "java" => "openjdk:17-slim",
            _ => "ubuntu:22.04", // Default fallback
        };

        // Use default resource limits if not provided
        let limits = resource_limits.unwrap_or_else(|| ResourceLimits::default());
        let network = network_config.unwrap_or_else(|| NetworkConfig::default());

        // Create container with necessary tools
        let mut env_vars = HashMap::new();
        env_vars.insert("LANG".to_string(), "C.UTF-8".to_string());
        env_vars.insert("PYTHONUNBUFFERED".to_string(), "1".to_string());

        // Create the container
        let container = self.container_manager
            .create_sandbox_container(&sandbox_id, image, limits, network, env_vars)
            .await?;

        // Start the container
        container.start().await?;

        // Create code executor
        let executor = Arc::new(CodeExecutor::new(container.clone()));

        // Store sandbox info
        let info = SandboxInfo {
            sandbox_id: sandbox_id.clone(),
            container_id: container.get_id().to_string(),
            executor: executor.clone(),
            created_at: std::time::Instant::now(),
            language: language.to_string(),
        };

        self.active_sandboxes.write().await.insert(sandbox_id.clone(), info);

        info!("Sandbox {} created successfully", sandbox_id);
        Ok(sandbox_id)
    }

    /// Create a new sandbox from a template
    pub async fn create_sandbox_from_template(
        &self,
        template: &Template,
        resource_limits: Option<ResourceLimits>,
        network_config: Option<NetworkConfig>,
    ) -> Result<String> {
        let sandbox_id = Uuid::new_v4().to_string();
        info!("Creating sandbox {} from template: {}", sandbox_id, template.metadata.name);

        // Use the base image from template
        let image = template.base_image.clone();

        // Use template's resource limits or provided ones
        let limits = resource_limits.unwrap_or_else(|| {
            // Import the resource limit types
            use crate::container::resource_limits::{CpuLimits, MemoryLimits, DiskLimits, NetworkLimits};
            
            ResourceLimits {
                cpu: CpuLimits {
                    cores: template.resource_limits.cpu_cores.unwrap_or(1.0),
                    shares: Some(512),
                    cpu_percent: Some(80.0),
                },
                memory: MemoryLimits {
                    limit_mb: template.resource_limits.memory_mb.unwrap_or(512),
                    swap_limit_mb: template.resource_limits.memory_mb.map(|m| m * 2),
                    swap_mb: template.resource_limits.memory_mb.map(|m| m * 2),
                },
                disk: DiskLimits {
                    limit_mb: template.resource_limits.disk_mb.unwrap_or(1024),
                    iops_limit: None,
                },
                network: NetworkLimits {
                    upload_bps: None,
                    download_bps: None,
                    max_connections: None,
                },
            }
        });

        let network = network_config.unwrap_or_else(|| NetworkConfig::default());

        // Set up environment variables from template
        let mut env_vars = template.environment_vars.clone();
        env_vars.insert("SOULBOX_TEMPLATE".to_string(), template.metadata.slug.clone());
        env_vars.insert("SOULBOX_SANDBOX_ID".to_string(), sandbox_id.clone());

        // Create the container
        let container = self.container_manager
            .create_sandbox_container(&sandbox_id, &image, limits, network, env_vars)
            .await?;

        // Start the container
        container.start().await?;

        // Run setup commands if specified
        if !template.setup_commands.is_empty() {
            info!("Running setup commands for template: {}", template.metadata.name);
            for cmd in &template.setup_commands {
                let exec_result = container.execute_command(vec!["sh".to_string(), "-c".to_string(), cmd.clone()]).await?;
                if exec_result.exit_code != 0 {
                    warn!("Setup command failed: {}", cmd);
                    error!("Setup error: {}", exec_result.stderr);
                }
            }
        }

        // Create code executor
        let executor = Arc::new(CodeExecutor::new(container.clone()));

        // Store sandbox info
        let info = SandboxInfo {
            sandbox_id: sandbox_id.clone(),
            container_id: container.get_id().to_string(),
            executor: executor.clone(),
            created_at: std::time::Instant::now(),
            language: template.metadata.runtime_type.to_string(),
        };

        self.active_sandboxes.write().await.insert(sandbox_id.clone(), info);

        info!("Sandbox {} created successfully from template: {}", sandbox_id, template.metadata.name);
        Ok(sandbox_id)
    }

    /// Execute code in a sandbox
    pub async fn execute_code(
        &self,
        sandbox_id: &str,
        code: &str,
        timeout: Option<Duration>,
    ) -> Result<CodeExecutionResult> {
        info!("Executing code in sandbox: {}", sandbox_id);

        let sandboxes = self.active_sandboxes.read().await;
        let sandbox_info = sandboxes.get(sandbox_id)
            .ok_or_else(|| SoulBoxError::not_found(format!("Sandbox not found: {}", sandbox_id)))?;

        let executor = sandbox_info.executor.clone();
        let language = sandbox_info.language.clone();
        drop(sandboxes); // Release read lock

        let execution_timeout = timeout.unwrap_or(self.default_timeout);
        
        // Execute code based on language
        let result = executor.execute(code, Some(&language), execution_timeout).await?;

        info!("Code execution completed. Exit code: {}, Time: {:?}", 
              result.exit_code, result.execution_time);

        Ok(result)
    }

    /// Execute code in a new temporary sandbox
    pub async fn execute_in_new_sandbox(
        &self,
        code: &str,
        language: &str,
        resource_limits: Option<ResourceLimits>,
        timeout: Option<Duration>,
    ) -> Result<CodeExecutionResult> {
        info!("Creating temporary sandbox for {} code execution", language);

        // Create sandbox
        let sandbox_id = self.create_sandbox(language, resource_limits, None).await?;

        // Execute code
        let result = self.execute_code(&sandbox_id, code, timeout).await;

        // Clean up sandbox
        if let Err(e) = self.destroy_sandbox(&sandbox_id).await {
            warn!("Failed to destroy temporary sandbox {}: {}", sandbox_id, e);
        }

        result
    }

    /// Destroy a sandbox and clean up resources
    pub async fn destroy_sandbox(&self, sandbox_id: &str) -> Result<()> {
        info!("Destroying sandbox: {}", sandbox_id);

        // Remove from active sandboxes
        let sandbox_info = self.active_sandboxes.write().await.remove(sandbox_id);

        if let Some(info) = sandbox_info {
            // Stop and remove container
            self.container_manager.remove_container(&info.container_id).await?;
            info!("Sandbox {} destroyed successfully", sandbox_id);
        } else {
            warn!("Sandbox {} not found in active sandboxes", sandbox_id);
        }

        Ok(())
    }

    /// List all active sandboxes
    pub async fn list_sandboxes(&self) -> Vec<SandboxSummary> {
        let sandboxes = self.active_sandboxes.read().await;
        
        sandboxes.values().map(|info| {
            SandboxSummary {
                sandbox_id: info.sandbox_id.clone(),
                language: info.language.clone(),
                created_at: info.created_at,
                uptime: info.created_at.elapsed(),
            }
        }).collect()
    }

    /// Check if sandbox manager is available
    pub fn is_available(&self) -> bool {
        true // Simple availability check
    }

    /// Clean up old sandboxes
    pub async fn cleanup_old_sandboxes(&self, max_age: Duration) -> Result<usize> {
        let mut count = 0;
        let now = std::time::Instant::now();
        
        let sandboxes = self.active_sandboxes.read().await.clone();
        
        for (sandbox_id, info) in sandboxes {
            if now.duration_since(info.created_at) > max_age {
                if let Err(e) = self.destroy_sandbox(&sandbox_id).await {
                    error!("Failed to cleanup old sandbox {}: {}", sandbox_id, e);
                } else {
                    count += 1;
                }
            }
        }
        
        info!("Cleaned up {} old sandboxes", count);
        Ok(count)
    }
}

/// Summary information about a sandbox
#[derive(Debug, Clone)]
pub struct SandboxSummary {
    pub sandbox_id: String,
    pub language: String,
    pub created_at: std::time::Instant,
    pub uptime: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_summary() {
        let summary = SandboxSummary {
            sandbox_id: "test-sandbox".to_string(),
            language: "python".to_string(),
            created_at: std::time::Instant::now(),
            uptime: Duration::from_secs(60),
        };

        assert_eq!(summary.sandbox_id, "test-sandbox");
        assert_eq!(summary.language, "python");
        assert!(summary.uptime.as_secs() == 60);
    }
}