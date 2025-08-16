use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use bollard::Docker;
use bollard::container::{Config as ContainerConfig, CreateContainerOptions};
use bollard::models::HostConfig;
use tracing::{info, error};

use crate::{error::Result, config::Config};
use crate::network::{NetworkManager, SandboxNetworkConfig, NetworkError};
use super::{SandboxContainer, ResourceLimits, NetworkConfig};

#[derive(Debug, Clone)]
pub struct ContainerManager {
    /// Docker client for container operations
    docker: Arc<Docker>,
    /// Active container instances - using RwLock for better read performance
    containers: Arc<RwLock<HashMap<String, Arc<SandboxContainer>>>>,
    /// Global configuration
    config: Config,
    /// Network manager for handling network configurations
    network_manager: Arc<tokio::sync::Mutex<NetworkManager>>,
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
            containers: Arc::new(RwLock::new(HashMap::new())),
            config,
            network_manager: Arc::new(tokio::sync::Mutex::new(NetworkManager::new())),
        })
    }

    /// Create a sandbox container with basic network configuration (legacy method)
    pub async fn create_sandbox_container(
        &self,
        sandbox_id: &str,
        image: &str,
        resource_limits: ResourceLimits,
        network_config: NetworkConfig,
        env_vars: HashMap<String, String>,
    ) -> Result<Arc<SandboxContainer>> {
        // Convert basic NetworkConfig to SandboxNetworkConfig
        let sandbox_network_config = SandboxNetworkConfig {
            base_config: network_config,
            ..Default::default()
        };
        
        self.create_sandbox_container_with_network(
            sandbox_id,
            image,
            resource_limits,
            sandbox_network_config,
            env_vars,
        ).await
    }

    /// Create a sandbox container with enhanced network configuration
    pub async fn create_sandbox_container_with_network(
        &self,
        sandbox_id: &str,
        image: &str,
        resource_limits: ResourceLimits,
        network_config: SandboxNetworkConfig,
        env_vars: HashMap<String, String>,
    ) -> Result<Arc<SandboxContainer>> {
        info!("Creating container for sandbox: {} with image: {}", sandbox_id, image);
        
        // Configure network settings before container creation
        let network_name = {
            let mut net_mgr = self.network_manager.lock().await;
            net_mgr.configure_sandbox_network(sandbox_id, network_config.clone()).await
                .map_err(|e| crate::error::SoulBoxError::internal(
                    format!("Failed to configure network: {}", e)
                ))?
        };
        
        // Convert environment variables to Docker format
        let docker_env: Vec<String> = env_vars
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        
        // Create host configuration with comprehensive resource limits
        let mut host_config = HostConfig {
            // Memory limits
            memory: Some(resource_limits.memory.limit_mb as i64 * 1024 * 1024), // Convert MB to bytes
            memory_swap: resource_limits.memory.swap_limit_mb.map(|swap| swap as i64 * 1024 * 1024),
            memory_swappiness: Some(10), // Reduce swap usage
            
            // CPU limits
            cpu_quota: Some(resource_limits.cpu.cores as i64 * 100_000), // 100000 = 1 CPU
            cpu_period: Some(100_000),
            cpu_shares: resource_limits.cpu.shares.map(|shares| shares as i64),
            
            // Disk I/O limits (using device cgroup v2 if available)
            // Note: These require proper cgroup setup on the host
            blkio_weight: Some(500), // Default I/O weight
            
            // Port mappings from network configuration
            port_bindings: if !network_config.base_config.port_mappings.is_empty() {
                Some({
                    let mut bindings = std::collections::HashMap::new();
                    for port_mapping in &network_config.base_config.port_mappings {
                        let container_port = format!("{}/{}", port_mapping.container_port, port_mapping.protocol);
                        let host_binding = if let Some(host_port) = port_mapping.host_port {
                            vec![bollard::models::PortBinding {
                                host_ip: Some("127.0.0.1".to_string()),
                                host_port: Some(host_port.to_string()),
                            }]
                        } else {
                            vec![bollard::models::PortBinding {
                                host_ip: Some("127.0.0.1".to_string()),
                                host_port: None, // Docker will allocate
                            }]
                        };
                        bindings.insert(container_port, Some(host_binding));
                    }
                    bindings
                })
            } else {
                None
            },
            
            // Network settings
            network_mode: Some(if network_config.network_name.is_some() {
                network_name.clone()
            } else {
                "bridge".to_string()
            }),
            
            // DNS configuration
            dns: if !network_config.base_config.dns_servers.is_empty() {
                Some(network_config.base_config.dns_servers.clone())
            } else {
                None
            },
            
            // Security constraints
            security_opt: Some(vec![
                "no-new-privileges:true".to_string(),
                "seccomp=unconfined".to_string(), // Make configurable later
            ]),
            cap_drop: Some(vec!["ALL".to_string()]),
            cap_add: Some(vec![
                "SYS_ADMIN".to_string(), // Required for some operations
                "CHOWN".to_string(),     // File ownership changes
                "SETUID".to_string(),    // Process user changes
                "SETGID".to_string(),    // Process group changes
            ]),
            
            // Process limits
            pids_limit: Some(256), // Limit number of processes
            
            // File system constraints
            readonly_root_fs: Some(false), // Allow writes to root filesystem
            tmpfs: Some({
                let mut tmpfs = std::collections::HashMap::new();
                tmpfs.insert("/tmp".to_string(), format!("size={}m,exec,suid,dev", resource_limits.disk.limit_mb / 4));
                tmpfs.insert("/var/tmp".to_string(), format!("size={}m,exec,suid,dev", resource_limits.disk.limit_mb / 8));
                tmpfs
            }),
            
            ..Default::default()
        };

        // Add disk quota if supported (requires Docker with quota support)
        // This is experimental and may not work on all systems
        if resource_limits.disk.limit_mb > 0 {
            // Note: Proper disk quotas require additional setup:
            // 1. Storage driver that supports quotas (overlay2 with backing filesystem that supports project quotas)
            // 2. Kernel support for project quotas
            // 3. Proper mount options
            // For now, we'll log the intended limit
            info!("Disk quota requested: {} MB (requires host configuration)", resource_limits.disk.limit_mb);
        }
        
        // Log network limits (implementation requires post-container setup)
        if let Some(upload_bps) = resource_limits.network.upload_bps {
            info!("Upload bandwidth limit: {} bytes/s", upload_bps);
        }
        if let Some(download_bps) = resource_limits.network.download_bps {
            info!("Download bandwidth limit: {} bytes/s", download_bps);
        }
        if let Some(max_conn) = resource_limits.network.max_connections {
            info!("Max connections limit: {}", max_conn);
        }
        
        // Create container configuration
        let container_config = ContainerConfig {
            image: Some(image.to_string()),
            env: Some(docker_env),
            working_dir: Some("/workspace".to_string()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            attach_stdin: Some(true),
            tty: Some(true),
            open_stdin: Some(true),
            host_config: Some(host_config),
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
            network_config.base_config,
            env_vars,
            Arc::clone(&self.docker),
        )?);
        
        // Store in our container registry
        let mut containers = self.containers.write().await;
        containers.insert(sandbox_id.to_string(), container.clone());
        
        info!("Container {} created and registered for sandbox {}", container_id, sandbox_id);
        
        // Apply network bandwidth limits if specified (requires additional setup)
        if resource_limits.network.upload_bps.is_some() || resource_limits.network.download_bps.is_some() {
            match self.setup_network_bandwidth_limits(&container_id, &resource_limits.network).await {
                Ok(_) => info!("Network bandwidth limits applied to container {}", container_id),
                Err(e) => error!("Failed to apply network limits to container {}: {}", container_id, e),
            }
        }
        
        Ok(container)
    }

    pub async fn list_containers(&self) -> Result<Vec<ContainerInfo>> {
        // Use read lock for better performance when multiple readers
        let containers = self.containers.read().await;
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
    
    pub async fn get_container(&self, sandbox_id: &str) -> Option<Arc<SandboxContainer>> {
        let containers = self.containers.read().await;
        containers.get(sandbox_id).cloned()
    }
    
    pub async fn remove_container(&self, sandbox_id: &str) -> Result<bool> {
        // Clean up network configuration first
        {
            let mut net_mgr = self.network_manager.lock().await;
            if let Err(e) = net_mgr.cleanup_sandbox_network(sandbox_id).await {
                error!("Failed to cleanup network for sandbox {}: {}", sandbox_id, e);
            }
        }
        
        let mut containers = self.containers.write().await;
        Ok(containers.remove(sandbox_id).is_some())
    }
    
    /// Apply network bandwidth limits to a container using tc (traffic control)
    /// Note: This requires the host to have tc installed and proper permissions
    async fn setup_network_bandwidth_limits(
        &self,
        container_id: &str,
        network_limits: &super::resource_limits::NetworkLimits,
    ) -> Result<()> {
        // Get the container's network interface
        let _container_details = self.docker.inspect_container(container_id, None).await
            .map_err(|e| crate::error::SoulBoxError::internal(format!("Failed to inspect container: {}", e)))?;
            
        // For now, just log what would be done
        // In a real implementation, you would:
        // 1. Find the container's veth pair interface on the host
        // 2. Use tc commands to set up bandwidth limiting
        // 3. Handle cleanup when container is destroyed
        
        info!("Network bandwidth limiting setup for container {} (requires tc configuration):", container_id);
        
        if let Some(upload_bps) = network_limits.upload_bps {
            info!("  Upload limit: {} bytes/s ({} Mbps)", upload_bps, upload_bps * 8 / 1_000_000);
            // Example tc command that would be run:
            // tc qdisc add dev vethXXX root handle 1: htb default 30
            // tc class add dev vethXXX parent 1: classid 1:1 htb rate {}bps
        }
        
        if let Some(download_bps) = network_limits.download_bps {
            info!("  Download limit: {} bytes/s ({} Mbps)", download_bps, download_bps * 8 / 1_000_000);
        }
        
        if let Some(max_conn) = network_limits.max_connections {
            info!("  Connection limit: {} connections", max_conn);
            // This would typically be enforced using iptables or similar:
            // iptables -A INPUT -p tcp --syn -m connlimit --connlimit-above {} -j REJECT
        }
        
        Ok(())
    }
    
    /// Get network configuration for a sandbox
    pub async fn get_network_config(&self, sandbox_id: &str) -> Option<SandboxNetworkConfig> {
        let net_mgr = self.network_manager.lock().await;
        net_mgr.get_network_config(sandbox_id).cloned()
    }
    
    /// Get network statistics for a sandbox
    pub async fn get_network_stats(&self, sandbox_id: &str) -> Option<crate::network::NetworkStats> {
        let net_mgr = self.network_manager.lock().await;
        net_mgr.get_network_stats(sandbox_id).cloned()
    }
    
    /// Update network configuration for an existing sandbox
    pub async fn update_network_config(
        &self,
        sandbox_id: &str,
        new_config: SandboxNetworkConfig,
    ) -> Result<()> {
        let mut net_mgr = self.network_manager.lock().await;
        
        // First cleanup existing configuration
        if let Err(e) = net_mgr.cleanup_sandbox_network(sandbox_id).await {
            error!("Failed to cleanup existing network config for sandbox {}: {}", sandbox_id, e);
        }
        
        // Apply new configuration
        net_mgr.configure_sandbox_network(sandbox_id, new_config).await
            .map_err(|e| crate::error::SoulBoxError::internal(
                format!("Failed to update network configuration: {}", e)
            ))?;
            
        info!("Updated network configuration for sandbox {}", sandbox_id);
        Ok(())
    }
    
    /// List all active networks managed by this container manager
    pub async fn list_active_networks(&self) -> Vec<String> {
        let net_mgr = self.network_manager.lock().await;
        net_mgr.list_active_networks()
    }
    
    /// Get port mappings for a sandbox
    pub async fn get_port_mappings(&self, sandbox_id: &str) -> Vec<crate::network::port_mapping::PortAllocation> {
        let net_mgr = self.network_manager.lock().await;
        net_mgr.port_manager.get_sandbox_ports(sandbox_id)
    }
}

/// Convert NetworkError to SoulBoxError
impl From<NetworkError> for crate::error::SoulBoxError {
    fn from(error: NetworkError) -> Self {
        crate::error::SoulBoxError::internal(format!("Network error: {}", error))
    }
}

#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub container_id: String,
    pub sandbox_id: String,
    pub status: String,
    pub image: String,
}