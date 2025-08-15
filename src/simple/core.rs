// Core SoulBox implementation - No bullshit, just working code
use std::collections::HashMap;
use bollard::{Docker, container::{Config, CreateContainerOptions, StartContainerOptions}};
use uuid::Uuid;
use anyhow::{Result, anyhow};
use tracing::{info, error};

/// Simple sandbox representation - no over-engineering
#[derive(Debug, Clone)]
pub struct Sandbox {
    pub id: String,
    pub container_id: String,
    pub image: String,
    pub status: SandboxStatus,
}

/// Clear status enum - no special cases
#[derive(Debug, Clone, PartialEq)]
pub enum SandboxStatus {
    Running,
    Stopped,
    Failed(String),
}

/// The core manager - one HashMap, one responsibility
pub struct SandboxManager {
    docker: Docker,
    sandboxes: HashMap<String, Sandbox>,
    // That's it. No Arc<Mutex<Option<Arc<dyn ComplexTrait>>>>
}

impl SandboxManager {
    /// Create a new manager - simple constructor
    pub fn new() -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| anyhow!("Failed to connect to Docker: {}", e))?;
        
        Ok(Self {
            docker,
            sandboxes: HashMap::new(),
        })
    }

    /// Create a sandbox - one function, clear purpose
    pub async fn create_sandbox(&mut self, image: &str) -> Result<String> {
        let sandbox_id = Uuid::new_v4().to_string();
        
        // Simple container config - no overcomplication
        let config = Config {
            image: Some(image.to_string()),
            working_dir: Some("/workspace".to_string()),
            cmd: Some(vec!["/bin/sh".to_string(), "-c".to_string(), "sleep 3600".to_string()]),
            attach_stdin: Some(false),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(false),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: format!("soulbox-{}", sandbox_id),
            ..Default::default()
        };

        // Create and start container
        let container = self.docker
            .create_container(Some(options), config)
            .await
            .map_err(|e| anyhow!("Failed to create container: {}", e))?;

        self.docker
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| anyhow!("Failed to start container: {}", e))?;

        // Store sandbox info
        let container_id = container.id.clone();
        let sandbox = Sandbox {
            id: sandbox_id.clone(),
            container_id: container.id,
            image: image.to_string(),
            status: SandboxStatus::Running,
        };

        self.sandboxes.insert(sandbox_id.clone(), sandbox);
        
        info!("Created sandbox {} with container {}", sandbox_id, container_id);
        Ok(sandbox_id)
    }

    /// Execute code - the core function, no special cases per language
    pub async fn execute(&self, sandbox_id: &str, command: Vec<String>) -> Result<(String, String, i32)> {
        let sandbox = self.sandboxes
            .get(sandbox_id)
            .ok_or_else(|| anyhow!("Sandbox {} not found", sandbox_id))?;

        if sandbox.status != SandboxStatus::Running {
            return Err(anyhow!("Sandbox {} is not running", sandbox_id));
        }

        // Execute command in container
        use bollard::exec::{CreateExecOptions, StartExecResults};
        use futures::StreamExt;

        let exec = self.docker
            .create_exec(
                &sandbox.container_id,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(command),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| anyhow!("Failed to create exec: {}", e))?;

        let mut stdout_output = String::new();
        let mut stderr_output = String::new();
        
        if let StartExecResults::Attached { mut output, .. } = self.docker.start_exec(&exec.id, None).await? {
            while let Some(Ok(msg)) = output.next().await {
                match msg {
                    bollard::container::LogOutput::StdOut { message } => {
                        stdout_output.push_str(&String::from_utf8_lossy(&message));
                    }
                    bollard::container::LogOutput::StdErr { message } => {
                        stderr_output.push_str(&String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }
        }

        // Get exit code
        let exec_inspect = self.docker.inspect_exec(&exec.id).await?;
        let exit_code = exec_inspect.exit_code.unwrap_or(1).try_into().unwrap_or(1);

        Ok((stdout_output, stderr_output, exit_code))
    }

    /// Remove a sandbox - cleanup is important
    pub async fn remove_sandbox(&mut self, sandbox_id: &str) -> Result<()> {
        if let Some(sandbox) = self.sandboxes.remove(sandbox_id) {
            // Stop and remove container
            let _ = self.docker.stop_container(&sandbox.container_id, None).await;
            let _ = self.docker.remove_container(&sandbox.container_id, None).await;
            
            info!("Removed sandbox {}", sandbox_id);
        }
        Ok(())
    }

    /// List sandboxes - simple getter
    pub fn list_sandboxes(&self) -> Vec<&Sandbox> {
        self.sandboxes.values().collect()
    }

    /// Get sandbox by ID
    pub fn get_sandbox(&self, sandbox_id: &str) -> Option<&Sandbox> {
        self.sandboxes.get(sandbox_id)
    }
}