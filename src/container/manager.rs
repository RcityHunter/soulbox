use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use bollard::Docker;
use bollard::models::{ContainerConfig, HostConfig};
use bollard::container::CreateContainerOptions;
use tracing::{info, warn, error};

use crate::{error::Result, config::Config};
use super::{SandboxContainer, ResourceLimits, NetworkConfig};

#[derive(Debug, Clone)]
pub struct ContainerManager {
    /// Docker client for container operations
    docker: Arc<Docker>,
    /// Active container instances
    containers: Arc<Mutex<HashMap<String, Arc<SandboxContainer>>>>,
    /// Global configuration
    config: Config,
}

impl ContainerManager {
    pub async fn new(config: Config) -> Result<Self> {
        // Initialize Docker client
        let docker = Docker::connect_with_socket_defaults()
            .map_err(|e| crate::error::SoulBoxError::internal(format!("Failed to connect to Docker: {}", e)))?;
        
        // Verify Docker is available
        match docker.version().await {
            Ok(version) => {
                info!("Connected to Docker Engine version: {}", version.version.unwrap_or_default());
                info!("Docker API version: {}", version.api_version.unwrap_or_default());
            }
            Err(e) => {
                error!("Failed to get Docker version: {}", e);
                return Err(crate::error::SoulBoxError::internal(
                    "Docker is not available. Please ensure Docker is running.".to_string()
                ));
            }
        }
        
        Ok(Self {
            docker: Arc::new(docker),
            containers: Arc::new(Mutex::new(HashMap::new())),
            config,
        })
    }

    pub async fn create_sandbox_container(
        &self,
        sandbox_id: &str,
        image: &str,
        resource_limits: ResourceLimits,
        network_config: NetworkConfig,
        env_vars: HashMap<String, String>,
    ) -> Result<Arc<SandboxContainer>> {
        info!("Creating container for sandbox: {} with image: {}", sandbox_id, image);
        
        // Convert environment variables to Docker format
        let docker_env: Vec<String> = env_vars
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        
        // Create host configuration with resource limits
        let host_config = HostConfig {
            memory: Some(resource_limits.memory.limit_mb as i64 * 1024 * 1024), // Convert MB to bytes
            cpu_quota: Some(resource_limits.cpu.cores as i64 * 100_000), // 100000 = 1 CPU
            cpu_period: Some(100_000),
            security_opt: Some(vec![
                "no-new-privileges:true".to_string(),
                "seccomp=unconfined".to_string(), // For now, we'll make this configurable later
            ]),
            cap_drop: Some(vec!["ALL".to_string()]),
            cap_add: Some(vec!["SYS_ADMIN".to_string()]), // Minimal required capabilities
            ..Default::default()
        };
        
        // Create container configuration
        let container_config = ContainerConfig {
            image: Some(image.to_string()),
            env: Some(docker_env),
            working_dir: Some("/workspace".to_string()),
            host_config: Some(host_config),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            attach_stdin: Some(true),
            tty: Some(true),
            open_stdin: Some(true),
            ..Default::default()
        };
        
        // Generate unique container name
        let container_name = format!("soulbox-{}", sandbox_id);
        
        // Create the container
        let create_result = self.docker
            .create_container(
                Some(CreateContainerOptions {
                    name: container_name.clone(),
                    platform: None,
                }),
                container_config,
            )
            .await
            .map_err(|e| crate::error::SoulBoxError::internal(
                format!("Failed to create container: {}", e)
            ))?;
            
        let container_id = create_result.id;
        info!("Created Docker container: {} for sandbox: {}", container_id, sandbox_id);
        
        // Create SandboxContainer instance with real Docker integration
        let container = Arc::new(SandboxContainer::new(
            sandbox_id,
            &container_id,
            image,
            resource_limits,
            network_config,
            env_vars,
            Arc::clone(&self.docker),
        )?);
        
        // Store in our container registry
        let mut containers = self.containers.lock().await;
        containers.insert(sandbox_id.to_string(), container.clone());
        
        Ok(container)
    }

    pub async fn list_containers(&self) -> Result<Vec<ContainerInfo>> {
        // TODO: Implement actual container listing
        let containers = self.containers.lock().await;
        let mut container_infos = Vec::new();
        
        for (id, container) in containers.iter() {
            container_infos.push(ContainerInfo {
                container_id: container.get_container_id().to_string(),
                sandbox_id: id.clone(),
                status: container.get_status().await.unwrap_or_else(|_| "unknown".to_string()),
                image: container.get_image().to_string(),
            });
        }

        Ok(container_infos)
    }
}

#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub container_id: String,
    pub sandbox_id: String,
    pub status: String,
    pub image: String,
}