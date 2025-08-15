use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use crate::error::Result;
use crate::container::{ResourceLimits, NetworkConfig};


/// Common interface for sandbox runtimes
#[async_trait]
pub trait SandboxRuntime: Send + Sync {
    /// Create a new sandbox instance
    async fn create_sandbox(
        &self,
        sandbox_id: &str,
        image: &str,
        resource_limits: ResourceLimits,
        network_config: NetworkConfig,
        env_vars: HashMap<String, String>,
    ) -> Result<Arc<dyn SandboxInstance>>;

    /// Get an existing sandbox
    async fn get_sandbox(&self, sandbox_id: &str) -> Option<Arc<dyn SandboxInstance>>;

    /// Remove a sandbox
    async fn remove_sandbox(&self, sandbox_id: &str) -> Result<()>;

    /// List all sandboxes
    async fn list_sandboxes(&self) -> Result<Vec<String>>;

}

/// Common interface for sandbox instances
#[async_trait]
pub trait SandboxInstance: Send + Sync {
    /// Start the sandbox
    async fn start(&self) -> Result<()>;

    /// Stop the sandbox
    async fn stop(&self) -> Result<()>;

    /// Execute a command in the sandbox
    async fn execute_command(&self, command: Vec<String>) -> Result<(String, String, i32)>;

    /// Get sandbox status
    async fn get_status(&self) -> Result<String>;

    /// Get sandbox ID
    fn get_id(&self) -> &str;
}

/// Docker-based sandbox runtime implementation
pub struct DockerRuntime {
    container_manager: Arc<crate::container::ContainerManager>,
}

impl DockerRuntime {
    pub fn new(container_manager: Arc<crate::container::ContainerManager>) -> Self {
        Self { container_manager }
    }
}

#[async_trait]
impl SandboxRuntime for DockerRuntime {
    async fn create_sandbox(
        &self,
        sandbox_id: &str,
        image: &str,
        resource_limits: ResourceLimits,
        network_config: NetworkConfig,
        env_vars: HashMap<String, String>,
    ) -> Result<Arc<dyn SandboxInstance>> {
        let container = self.container_manager
            .create_sandbox_container(sandbox_id, image, resource_limits, network_config, env_vars)
            .await?;
        
        Ok(Arc::new(DockerSandboxInstance { container }))
    }

    async fn get_sandbox(&self, _sandbox_id: &str) -> Option<Arc<dyn SandboxInstance>> {
        // TODO: Implement container lookup
        None
    }

    async fn remove_sandbox(&self, _sandbox_id: &str) -> Result<()> {
        // TODO: Implement container removal
        Ok(())
    }

    async fn list_sandboxes(&self) -> Result<Vec<String>> {
        // TODO: Implement container listing
        Ok(vec![])
    }

}

/// Docker sandbox instance wrapper
struct DockerSandboxInstance {
    container: Arc<crate::container::SandboxContainer>,
}

#[async_trait]
impl SandboxInstance for DockerSandboxInstance {
    async fn start(&self) -> Result<()> {
        self.container.start().await
    }

    async fn stop(&self) -> Result<()> {
        self.container.stop().await
    }

    async fn execute_command(&self, command: Vec<String>) -> Result<(String, String, i32)> {
        let result = self.container.execute_command(command).await?;
        Ok((result.stdout, result.stderr, result.exit_code))
    }

    async fn get_status(&self) -> Result<String> {
        self.container.get_status().await
    }

    fn get_id(&self) -> &str {
        self.container.get_id()
    }
}

/// Firecracker-based sandbox runtime implementation
pub struct FirecrackerRuntime {
    vm_manager: Arc<crate::firecracker::FirecrackerManager>,
}

impl FirecrackerRuntime {
    pub fn new(vm_manager: Arc<crate::firecracker::FirecrackerManager>) -> Self {
        Self { vm_manager }
    }
}

#[async_trait]
impl SandboxRuntime for FirecrackerRuntime {
    async fn create_sandbox(
        &self,
        sandbox_id: &str,
        template: &str,
        resource_limits: ResourceLimits,
        network_config: NetworkConfig,
        env_vars: HashMap<String, String>,
    ) -> Result<Arc<dyn SandboxInstance>> {
        let vm = self.vm_manager
            .create_sandbox_vm(sandbox_id, template, resource_limits, network_config, env_vars)
            .await?;
        
        Ok(Arc::new(FirecrackerSandboxInstance { vm }))
    }

    async fn get_sandbox(&self, sandbox_id: &str) -> Option<Arc<dyn SandboxInstance>> {
        self.vm_manager.get_vm(sandbox_id).await
            .map(|vm| Arc::new(FirecrackerSandboxInstance { vm }) as Arc<dyn SandboxInstance>)
    }

    async fn remove_sandbox(&self, sandbox_id: &str) -> Result<()> {
        self.vm_manager.remove_vm(sandbox_id).await
    }

    async fn list_sandboxes(&self) -> Result<Vec<String>> {
        let vms = self.vm_manager.list_vms().await;
        Ok(vms.into_iter().map(|(id, _)| id).collect())
    }

}

/// Firecracker sandbox instance wrapper
struct FirecrackerSandboxInstance {
    vm: Arc<crate::firecracker::vm::FirecrackerVM>,
}

#[async_trait]
impl SandboxInstance for FirecrackerSandboxInstance {
    async fn start(&self) -> Result<()> {
        // VM is started during creation
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.vm.stop().await
    }

    async fn execute_command(&self, command: Vec<String>) -> Result<(String, String, i32)> {
        // Convert command to code execution
        // For Firecracker VMs, we use vsock-based code execution
        if command.len() >= 3 && command[0] == "sh" && command[1] == "-c" {
            // Extract code from shell command
            let code = command[2].clone();
            let language = "bash";
            
            self.vm.execute_code(
                language.to_string(),
                code,
                30, // 30 second timeout
                std::collections::HashMap::new(),
            ).await
        } else {
            // Direct command execution not supported in Firecracker
            Err(crate::error::SoulBoxError::unsupported(
                "Direct command execution not supported in Firecracker VMs. Use code execution instead."
            ))
        }
    }

    async fn get_status(&self) -> Result<String> {
        let state = self.vm.get_state().await;
        Ok(format!("{:?}", state))
    }

    fn get_id(&self) -> &str {
        &self.vm.id
    }
}