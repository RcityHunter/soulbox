use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;
use tokio::sync::Mutex;
use tracing::{info, warn};
use uuid::Uuid;

use crate::error::Result;
use crate::config::Config;
use crate::container::{ResourceLimits, NetworkConfig as ContainerNetworkConfig};
use super::{vm::FirecrackerVM, config::{VMConfig, MachineConfig}};

/// Manages multiple Firecracker VMs
pub struct FirecrackerManager {
    /// Active VM instances
    vms: Arc<Mutex<HashMap<String, Arc<FirecrackerVM>>>>,
    /// Path to kernel image
    kernel_path: PathBuf,
    /// Path to rootfs images
    rootfs_base_path: PathBuf,
    /// Global configuration
    config: Config,
}

impl FirecrackerManager {
    pub async fn new(config: Config) -> Result<Self> {
        // Default paths - these should be configurable
        let kernel_path = PathBuf::from("/opt/firecracker/vmlinux");
        let rootfs_base_path = PathBuf::from("/opt/firecracker/rootfs");

        // Verify kernel exists
        if !kernel_path.exists() {
            warn!("Firecracker kernel not found at {:?}. Please download from:", kernel_path);
            warn!("https://github.com/firecracker-microvm/firecracker/releases");
        }

        Ok(Self {
            vms: Arc::new(Mutex::new(HashMap::new())),
            kernel_path,
            rootfs_base_path,
            config,
        })
    }

    /// Create a new Firecracker VM for a sandbox
    pub async fn create_sandbox_vm(
        &self,
        sandbox_id: &str,
        template: &str,
        resource_limits: ResourceLimits,
        _network_config: ContainerNetworkConfig,
        _env_vars: HashMap<String, String>,
    ) -> Result<Arc<FirecrackerVM>> {
        info!("Creating Firecracker VM for sandbox: {}", sandbox_id);

        // Generate VM ID
        let vm_id = format!("vm_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());

        // Prepare rootfs for this VM (copy from template)
        let rootfs_path = self.prepare_rootfs(&vm_id, template).await?;

        // Create VM configuration
        let mut vm_config = VMConfig::new(
            vm_id.clone(),
            self.kernel_path.clone(),
            rootfs_path,
        );

        // Apply resource limits
        vm_config.machine_config = MachineConfig {
            vcpu_count: resource_limits.cpu.cores.ceil() as u32,
            mem_size_mib: resource_limits.memory.limit_mb as u32,
            smt: false,
            track_dirty_pages: false,
        };

        // Create VM instance
        let vm = Arc::new(FirecrackerVM::new(vm_config));

        // Start the VM
        vm.start().await?;

        // Store VM reference
        let mut vms = self.vms.lock().await;
        vms.insert(sandbox_id.to_string(), vm.clone());

        info!("Firecracker VM {} created for sandbox {}", vm_id, sandbox_id);
        Ok(vm)
    }

    /// Prepare rootfs for a new VM by copying from template
    async fn prepare_rootfs(&self, vm_id: &str, template: &str) -> Result<PathBuf> {
        let template_rootfs = match template {
            "node" | "javascript" | "typescript" => "node-rootfs.ext4",
            "python" | "python3" => "python-rootfs.ext4",
            "rust" => "rust-rootfs.ext4",
            "go" => "go-rootfs.ext4",
            _ => "ubuntu-rootfs.ext4",
        };

        let source_path = self.rootfs_base_path.join(template_rootfs);
        let dest_path = PathBuf::from(format!("/tmp/firecracker/{}/rootfs.ext4", vm_id));

        // Create directory
        tokio::fs::create_dir_all(dest_path.parent().unwrap()).await?;

        // Copy rootfs (in production, use copy-on-write)
        tokio::fs::copy(&source_path, &dest_path).await
            .map_err(|e| crate::error::SoulBoxError::internal(
                format!("Failed to prepare rootfs: {}. Make sure rootfs images are available at {:?}", e, source_path)
            ))?;

        Ok(dest_path)
    }

    /// Get a VM by sandbox ID
    pub async fn get_vm(&self, sandbox_id: &str) -> Option<Arc<FirecrackerVM>> {
        let vms = self.vms.lock().await;
        vms.get(sandbox_id).cloned()
    }

    /// Remove a VM
    pub async fn remove_vm(&self, sandbox_id: &str) -> Result<()> {
        let mut vms = self.vms.lock().await;
        if let Some(vm) = vms.remove(sandbox_id) {
            vm.stop().await?;
            
            // Clean up VM resources
            let vm_dir = PathBuf::from(format!("/tmp/firecracker/{}", vm.id));
            let _ = tokio::fs::remove_dir_all(&vm_dir).await;
        }
        Ok(())
    }

    /// List all VMs
    pub async fn list_vms(&self) -> Vec<(String, String)> {
        let vms = self.vms.lock().await;
        vms.iter()
            .map(|(sandbox_id, vm)| (sandbox_id.clone(), vm.id.clone()))
            .collect()
    }

    /// Execute command in a VM
    pub async fn execute_in_vm(
        &self,
        sandbox_id: &str,
        command: Vec<String>,
    ) -> Result<(String, String, i32)> {
        let vms = self.vms.lock().await;
        let vm = vms.get(sandbox_id)
            .ok_or_else(|| crate::error::SoulBoxError::not_found(
                format!("VM not found for sandbox: {}", sandbox_id)
            ))?;

        vm.execute_command(command).await
    }
}

impl Clone for FirecrackerManager {
    fn clone(&self) -> Self {
        Self {
            vms: Arc::clone(&self.vms),
            kernel_path: self.kernel_path.clone(),
            rootfs_base_path: self.rootfs_base_path.clone(),
            config: self.config.clone(),
        }
    }
}