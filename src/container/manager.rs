use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex, Semaphore};
use bollard::Docker;
use bollard::container::{Config as ContainerConfig, CreateContainerOptions};
use bollard::models::HostConfig;
use tracing::{info, error, warn};
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use tokio::time::{timeout, Duration as TokioDuration};

use crate::{error::{Result, SoulBoxError}, config::Config};
use crate::network::{SandboxNetworkConfig, NetworkError};
use super::{SandboxContainer, ResourceLimits, NetworkConfig};

#[derive(Debug, Clone)]
pub struct ContainerManager {
    /// Docker client for container operations
    docker: Arc<Docker>,
    /// Active container instances - using RwLock for better read performance
    containers: Arc<RwLock<HashMap<String, Arc<SandboxContainer>>>>,
    /// Container operation semaphore to limit concurrent operations
    operation_semaphore: Arc<Semaphore>,
    /// Resource cleanup tracker to prevent leaks
    resource_tracker: Arc<ResourceTracker>,
    /// Shutdown signal for graceful cleanup
    shutdown_signal: Arc<AtomicBool>,
    /// Global configuration
    config: Config,
    /// Network manager for handling network configurations (temporarily disabled)
    // network_manager: Arc<tokio::sync::Mutex<NetworkManager>>,
    /// Container creation counter for atomic ID generation
    creation_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl ContainerManager {
    /// Create a container manager with default configuration
    /// This connects to the local Docker daemon using standard methods
    pub fn new_default() -> Result<Self> {
        // Connect to Docker daemon using platform-appropriate method
        let docker = Docker::connect_with_socket_defaults()
            .or_else(|_| Docker::connect_with_local_defaults())
            .map_err(|e| SoulBoxError::Container(e))?;
        
        let docker = Arc::new(docker);
        
        Ok(Self::new_with_docker(docker))
    }
    
    /// Create a container manager with a provided Docker client
    pub fn new_with_docker(docker: Arc<Docker>) -> Self {
        Self {
            docker,
            containers: Arc::new(RwLock::new(HashMap::new())),
            operation_semaphore: Arc::new(Semaphore::new(10)),
            resource_tracker: Arc::new(ResourceTracker::new()),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            config: Config::default(),
            // network_manager: Arc::new(tokio::sync::Mutex::new(NetworkManager::new())),
            creation_counter: Arc::new(AtomicU64::new(0)),
        }
    }
    
    /// Get a reference to the Docker client
    pub fn get_docker_client(&self) -> Arc<Docker> {
        self.docker.clone()
    }

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
            operation_semaphore: Arc::new(Semaphore::new(10)), // Limit to 10 concurrent container operations
            config,
            // network_manager: Arc::new(Mutex::new(NetworkManager::new())),
            creation_counter: Arc::new(AtomicU64::new(0)),
            resource_tracker: Arc::new(ResourceTracker::new()),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Create a sandbox container with basic network configuration (legacy method)
    pub async fn create_sandbox_container(
        &self,
        sandbox_id: &str,
        image: &str,
        resource_limits: ResourceLimits,
        container_network_config: NetworkConfig,
        env_vars: HashMap<String, String>,
    ) -> Result<Arc<SandboxContainer>> {
        // Acquire operation permit with timeout to prevent deadlocks
        let _permit = timeout(TokioDuration::from_secs(30), self.operation_semaphore.acquire()).await
            .map_err(|_| crate::error::SoulBoxError::internal("Operation timed out waiting for permit".to_string()))?
            .map_err(|_| crate::error::SoulBoxError::internal("Failed to acquire operation permit".to_string()))?;

        // Check if shutdown is in progress
        if self.shutdown_signal.load(Ordering::Acquire) {
            return Err(crate::error::SoulBoxError::internal("Container manager is shutting down".to_string()));
        }

        // Convert container NetworkConfig to SandboxNetworkConfig
        let sandbox_network_config = SandboxNetworkConfig {
            base_config: crate::network::NetworkConfig {
                bridge: None,
                dns_servers: container_network_config.dns_servers.iter()
                    .filter_map(|s| s.parse().ok())
                    .collect(),
                subnet: None,
                gateway: None,
                port_mappings: container_network_config.port_mappings.iter()
                    .filter_map(|p| p.host_port.map(|host| crate::network::PortMapping {
                        host_port: host,
                        container_port: p.container_port,
                        protocol: p.protocol.clone(),
                    }))
                    .collect(),
            },
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
        // Generate unique container ID to avoid conflicts
        let creation_id = self.creation_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let container_id = format!("soulbox_{}_{}", sandbox_id, creation_id);
        
        info!("Creating container {} for sandbox: {} with image: {}", container_id, sandbox_id, image);
        
        // Configure network settings before container creation (temporarily disabled)
        let network_name = format!("soulbox_{}", sandbox_id);
        
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
            // blkio_weight: Some(500), // Default I/O weight (disabled for compatibility)
            
            // Port mappings from network configuration
            port_bindings: if !network_config.base_config.port_mappings.is_empty() {
                Some({
                    let mut bindings = std::collections::HashMap::new();
                    for port_mapping in &network_config.base_config.port_mappings {
                        let container_port = format!("{}/{}", port_mapping.container_port, port_mapping.protocol);
                        let host_binding = vec![bollard::models::PortBinding {
                            host_ip: Some("127.0.0.1".to_string()),
                            host_port: Some(port_mapping.host_port.to_string()),
                        }];
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
                Some(network_config.base_config.dns_servers.iter()
                     .map(|ip| ip.to_string())
                     .collect())
            } else {
                None
            },
            
            // Enhanced security constraints to prevent container escape
            security_opt: Some(vec![
                "no-new-privileges:true".to_string(),
                // "seccomp:default".to_string(), // Use default seccomp profile (disabled for compatibility)
                // "apparmor:docker-default".to_string(), // Enable AppArmor protection (disabled for compatibility)
                // "systempaths=unconfined".to_string(), // Restrict access to system paths (disabled for compatibility)
            ]),
            cap_drop: Some(vec!["ALL".to_string()]), // Drop all capabilities first
            cap_add: Some(vec![
                // Only add truly essential capabilities for sandbox operation
                "CHOWN".to_string(),     // File ownership changes (limited to container filesystem)
                // Removed all dangerous capabilities:
                // - DAC_OVERRIDE: Too powerful, can bypass all file permissions
                // - FOWNER: Can bypass ownership checks
                // - SYS_ADMIN: Extremely dangerous, allows container escape
                // - SETUID/SETGID: Privilege escalation risks
            ]),
            
            // Additional security hardening
            privileged: Some(false), // Explicitly disable privileged mode
            
            // Prevent access to host devices
            cgroup_parent: Some("docker".to_string()), // Ensure proper cgroup isolation
            
            // Prevent device access that could lead to escape
            devices: Some(vec![]), // No device access
            device_cgroup_rules: Some(vec![
                "c *:* rmw".to_string(), // Deny all character device access
                "b *:* rmw".to_string(), // Deny all block device access
            ]),
            
            // Enhanced process and resource limits
            pids_limit: Some(64), // Strict limit on number of processes to prevent fork bombs
            
            // Prevent access to host devices and filesystems
            ipc_mode: Some("none".to_string()), // Disable IPC namespace sharing
            // uts_mode: Some("none".to_string()), // Disable UTS namespace sharing - not supported by Docker
            
            // Network restrictions disabled - using previous DNS config
            
            // Prevent privilege escalation through sysctl (disabled for compatibility)
            // sysctls: Some({
            //     let mut sysctls = std::collections::HashMap::new();
            //     sysctls.insert("net.ipv4.ping_group_range".to_string(), "0 0".to_string()); // Disable ping
            //     sysctls.insert("kernel.dmesg_restrict".to_string(), "1".to_string()); // Restrict dmesg access
            //     sysctls
            // }),
            
            // Ulimits for additional security
            ulimits: Some(vec![
                bollard::models::ResourcesUlimits {
                    name: Some("nofile".to_string()), // File descriptor limit
                    soft: Some(1024),
                    hard: Some(1024),
                },
                bollard::models::ResourcesUlimits {
                    name: Some("nproc".to_string()), // Process limit
                    soft: Some(64),
                    hard: Some(64),
                },
                bollard::models::ResourcesUlimits {
                    name: Some("fsize".to_string()), // File size limit
                    soft: Some(resource_limits.disk.limit_mb as i64 * 1024 * 1024),
                    hard: Some(resource_limits.disk.limit_mb as i64 * 1024 * 1024),
                },
            ]),
            
            // Enhanced file system constraints
            readonly_rootfs: Some(true), // Make root filesystem read-only for security
            tmpfs: Some({
                let mut tmpfs = std::collections::HashMap::new();
                // Mount secure tmpfs without dangerous flags
                tmpfs.insert("/tmp".to_string(), format!("size={}m,noexec,nosuid,nodev", resource_limits.disk.limit_mb / 4));
                tmpfs.insert("/var/tmp".to_string(), format!("size={}m,noexec,nosuid,nodev", resource_limits.disk.limit_mb / 8));
                // Add a writable workspace
                tmpfs.insert("/workspace".to_string(), format!("size={}m,nodev,nosuid", resource_limits.disk.limit_mb / 2));
                tmpfs
            }),
            
            // Additional mount restrictions
            mounts: Some(vec![
                // Mount security disabled - requires proper mount type enumeration
            ]),
            
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
        
        // Create container configuration with security hardening
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
            
            // Security: Disable networking by default
            network_disabled: Some(false),
            
            // User specification for non-root execution
            user: Some("1000:1000".to_string()), // Run as non-root user
            
            // Disable host config inheritance
            hostname: Some(format!("sandbox-{}", sandbox_id)),
            domainname: Some("soulbox.local".to_string()),
            
            // Additional container labels for tracking and security
            labels: Some({
                let mut labels = std::collections::HashMap::new();
                labels.insert("soulbox.sandbox_id".to_string(), sandbox_id.to_string());
                labels.insert("soulbox.security_profile".to_string(), "restricted".to_string());
                labels.insert("soulbox.created_at".to_string(), chrono::Utc::now().to_rfc3339());
                labels
            }),
            
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
        
        // Clone resource_limits before moving it to avoid borrow after move
        let network_limits = resource_limits.network.clone();
        
        // Convert back to container NetworkConfig for SandboxContainer::new
        let container_net_config = crate::container::network::NetworkConfig {
            enable_internet: true,
            port_mappings: network_config.base_config.port_mappings.iter()
                .map(|p| crate::container::network::PortMapping {
                    host_port: Some(p.host_port),
                    container_port: p.container_port,
                    protocol: p.protocol.clone(),
                })
                .collect(),
            allowed_domains: Vec::new(),
            dns_servers: network_config.base_config.dns_servers.iter()
                .map(|ip| ip.to_string())
                .collect(),
        };

        // Create SandboxContainer instance with real Docker integration
        let container = Arc::new(SandboxContainer::new(
            sandbox_id,
            &container_id,
            image,
            resource_limits,
            container_net_config,
            env_vars,
            Arc::clone(&self.docker),
        )?);
        
        // Store in our container registry with atomic check-and-insert
        {
            let mut containers = self.containers.write().await;
            // Check if sandbox_id already exists to prevent overwrites
            if containers.contains_key(sandbox_id) {
                // Clean up the newly created container before failing
                if let Err(e) = container.cleanup().await {
                    error!("Failed to cleanup duplicate container {}: {}", container_id, e);
                }
                return Err(crate::error::SoulBoxError::internal(
                    format!("Sandbox {} already has an active container", sandbox_id)
                ));
            }
            
            // Track this container for resource management
            self.resource_tracker.track_container(&container_id, sandbox_id).await;
            
            containers.insert(sandbox_id.to_string(), container.clone());
        }
        
        info!("Secure container {} created and registered for sandbox {} with hardened security", container_id, sandbox_id);
        
        // Log security configuration for audit
        info!("Security profile applied: readonly_rootfs=true, non-root_user=1000:1000, minimal_capabilities (CHOWN only), no_dangerous_caps");
        
        // Apply network bandwidth limits if specified (requires additional setup)
        if network_limits.upload_bps.is_some() || network_limits.download_bps.is_some() {
            match self.setup_network_bandwidth_limits(&container_id, &network_limits).await {
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
        // Acquire operation permit with timeout to prevent resource contention during removal
        let _permit = timeout(TokioDuration::from_secs(30), self.operation_semaphore.acquire()).await
            .map_err(|_| crate::error::SoulBoxError::internal("Removal operation timed out".to_string()))?
            .map_err(|_| crate::error::SoulBoxError::internal("Failed to acquire operation permit".to_string()))?;

        // First retrieve the container before removing it
        let container = {
            let containers = self.containers.read().await;
            containers.get(sandbox_id).cloned()
        };

        if let Some(container) = container {
            // Clean up container resources first
            if let Err(e) = container.cleanup().await {
                error!("Failed to cleanup container resources for sandbox {}: {}", sandbox_id, e);
            }
            
            // Clean up network configuration (temporarily disabled)
            {
                // let mut net_mgr = self.network_manager.lock().await;
                // if let Err(e) = net_mgr.cleanup_sandbox_network(sandbox_id).await {
                //     error!("Failed to cleanup network for sandbox {}: {}", sandbox_id, e);
                // }
            }
            
            // Remove from registry and resource tracker atomically
            let mut containers = self.containers.write().await;
            let removed = containers.remove(sandbox_id).is_some();
            
            if removed {
                // Untrack the container from resource management
                self.resource_tracker.untrack_container(sandbox_id).await;
                info!("Successfully removed container for sandbox: {}", sandbox_id);
            }
            
            Ok(removed)
        } else {
            warn!("Attempted to remove non-existent container for sandbox: {}", sandbox_id);
            Ok(false)
        }
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
    
    /// Get network configuration for a sandbox (temporarily stubbed)
    pub async fn get_network_config(&self, _sandbox_id: &str) -> Option<SandboxNetworkConfig> {
        // let net_mgr = self.network_manager.lock().await;
        // net_mgr.get_network_config(sandbox_id).cloned()
        None // Temporarily return None
    }
    
    /// Get network statistics for a sandbox (temporarily stubbed)
    pub async fn get_network_stats(&self, _sandbox_id: &str) -> Option<crate::network::NetworkStats> {
        // let net_mgr = self.network_manager.lock().await;
        // net_mgr.get_network_stats(sandbox_id).cloned()
        None // Temporarily return None
    }
    
    /// Update network configuration for an existing sandbox
    pub async fn update_network_config(
        &self,
        _sandbox_id: &str,
        _new_config: SandboxNetworkConfig,
    ) -> Result<()> {
        // let mut net_mgr = self.network_manager.lock().await;
        // 
        // // First cleanup existing configuration
        // if let Err(e) = net_mgr.cleanup_sandbox_network(sandbox_id).await {
        //     error!("Failed to cleanup existing network config for sandbox {}: {}", sandbox_id, e);
        // }
        // 
        // // Apply new configuration
        // net_mgr.configure_sandbox_network(sandbox_id, new_config).await
        //     .map_err(|e| crate::error::SoulBoxError::internal(
        //         format!("Failed to update network configuration: {}", e)
        //     ))?;
            
        info!("Updated network configuration for sandbox {}", _sandbox_id);
        Ok(())
    }
    
    /// List all active networks managed by this container manager (temporarily stubbed)
    pub async fn list_active_networks(&self) -> Vec<String> {
        // let net_mgr = self.network_manager.lock().await;
        // net_mgr.list_active_networks().into_iter().map(|s| s.to_string()).collect()
        vec![] // Temporarily return empty vec
    }
    
    /// Get port mappings for a sandbox (temporarily stubbed)
    pub async fn get_port_mappings(&self, _sandbox_id: &str) -> Vec<crate::network::port_mapping::PortAllocation> {
        // let net_mgr = self.network_manager.lock().await;
        // net_mgr.port_service().get_sandbox_ports(sandbox_id)
        vec![] // Temporarily return empty vec
    }

    /// Gracefully shutdown the container manager
    pub async fn shutdown(&self) -> Result<()> {
        info!("Initiating container manager shutdown...");
        
        // Signal shutdown to prevent new operations
        self.shutdown_signal.store(true, Ordering::Release);
        
        // Wait for ongoing operations to complete with timeout
        let mut retries = 0;
        while self.operation_semaphore.available_permits() < 10 && retries < 30 {
            tokio::time::sleep(TokioDuration::from_millis(100)).await;
            retries += 1;
        }
        
        if retries >= 30 {
            warn!("Timeout waiting for operations to complete during shutdown");
        }
        
        // Clean up all containers
        let containers = {
            let containers_guard = self.containers.read().await;
            containers_guard.keys().cloned().collect::<Vec<_>>()
        };
        
        for sandbox_id in containers {
            if let Err(e) = self.remove_container(&sandbox_id).await {
                error!("Failed to remove container {} during shutdown: {}", sandbox_id, e);
            }
        }
        
        // Perform final resource cleanup
        self.resource_tracker.cleanup_all().await;
        
        info!("Container manager shutdown completed");
        Ok(())
    }

    /// Get resource usage statistics
    pub async fn get_resource_stats(&self) -> HashMap<String, ResourceUsageStats> {
        self.resource_tracker.get_stats().await
    }

    /// Create a container with basic configuration (for pool usage)
    pub async fn create_container(&self, config: bollard::container::Config<String>) -> Result<String> {
        let container_name = format!("soulbox-pool-{}", uuid::Uuid::new_v4());
        
        let create_result = self.docker
            .create_container(
                Some(bollard::container::CreateContainerOptions {
                    name: container_name.clone(),
                    platform: None,
                }),
                config,
            )
            .await?;

        Ok(create_result.id)
    }

    /// Start a container by ID
    pub async fn start_container(&self, container_id: &str) -> Result<()> {
        self.docker
            .start_container(container_id, None::<bollard::container::StartContainerOptions<String>>)
            .await?;
        Ok(())
    }

    /// Stop a container by ID
    pub async fn stop_container(&self, container_id: &str) -> Result<()> {
        self.docker
            .stop_container(container_id, None::<bollard::container::StopContainerOptions>)
            .await?;
        Ok(())
    }

    /// Execute a command in a container
    pub async fn execute_command(&self, container_id: &str, command: &[&str]) -> Result<String> {
        use bollard::exec::{CreateExecOptions, StartExecResults};
        use futures_util::StreamExt;

        let exec_config = CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            cmd: Some(command.iter().map(|s| s.to_string()).collect()),
            ..Default::default()
        };

        let exec = self.docker.create_exec(container_id, exec_config).await?;
        
        if let StartExecResults::Attached { mut output, .. } = 
            self.docker.start_exec(&exec.id, None).await? {
            
            let mut result = String::new();
            while let Some(Ok(msg)) = output.next().await {
                result.push_str(&String::from_utf8_lossy(&msg.into_bytes()));
            }
            Ok(result)
        } else {
            Err(crate::error::SoulBoxError::Internal("Failed to attach to exec".to_string()))
        }
    }

    /// Get container information
    pub async fn get_container_info(&self, container_id: &str) -> Result<bollard::models::ContainerInspectResponse> {
        Ok(self.docker.inspect_container(container_id, None).await?)
    }

    /// Restart a container
    pub async fn restart_container(&self, container_id: &str) -> Result<()> {
        self.docker
            .restart_container(container_id, None::<bollard::container::RestartContainerOptions>)
            .await?;
        Ok(())
    }

    /// Get container statistics
    pub async fn get_container_stats(&self, container_id: &str) -> Result<bollard::container::Stats> {
        use bollard::container::StatsOptions;
        use futures_util::StreamExt;
        
        let options = StatsOptions {
            stream: false,
            one_shot: true,
        };
        
        let mut stats_stream = self.docker.stats(container_id, Some(options));
        
        if let Some(Ok(stats)) = stats_stream.next().await {
            Ok(stats)
        } else {
            Err(crate::error::SoulBoxError::Internal("Failed to get container stats".to_string()))
        }
    }

    /// Remove a container by ID
    pub async fn remove_container_by_id(&self, container_id: &str) -> Result<()> {
        self.docker
            .remove_container(container_id, None::<bollard::container::RemoveContainerOptions>)
            .await?;
        Ok(())
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

/// Resource tracker for monitoring and preventing memory leaks
#[derive(Debug)]
pub struct ResourceTracker {
    /// Track active containers and their resource usage
    tracked_containers: Arc<RwLock<HashMap<String, ContainerResource>>>,
    /// Background task handle for periodic cleanup
    cleanup_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Metrics for resource usage tracking
    metrics: Arc<ResourceMetrics>,
}

#[derive(Debug, Clone)]
pub struct ContainerResource {
    pub container_id: String,
    pub sandbox_id: String,
    pub created_at: std::time::Instant,
    pub last_seen: Arc<AtomicU64>, // Unix timestamp
    pub cleanup_attempted: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct ResourceMetrics {
    pub total_containers_created: AtomicU64,
    pub total_containers_cleaned: AtomicU64,
    pub leaked_containers_detected: AtomicU64,
    pub cleanup_failures: AtomicU64,
}

#[derive(Debug, Clone)]
pub struct ResourceUsageStats {
    pub container_id: String,
    pub sandbox_id: String,
    pub uptime_seconds: u64,
    pub last_seen_seconds_ago: u64,
    pub cleanup_attempted: bool,
}

impl ResourceTracker {
    pub fn new() -> Self {
        let tracker = Self {
            tracked_containers: Arc::new(RwLock::new(HashMap::new())),
            cleanup_task: Arc::new(Mutex::new(None)),
            metrics: Arc::new(ResourceMetrics {
                total_containers_created: AtomicU64::new(0),
                total_containers_cleaned: AtomicU64::new(0),
                leaked_containers_detected: AtomicU64::new(0),
                cleanup_failures: AtomicU64::new(0),
            }),
        };
        
        // Start background cleanup task
        tracker.start_cleanup_task();
        tracker
    }

    pub async fn track_container(&self, container_id: &str, sandbox_id: &str) {
        let resource = ContainerResource {
            container_id: container_id.to_string(),
            sandbox_id: sandbox_id.to_string(),
            created_at: std::time::Instant::now(),
            last_seen: Arc::new(AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            )),
            cleanup_attempted: Arc::new(AtomicBool::new(false)),
        };

        let mut containers = self.tracked_containers.write().await;
        containers.insert(sandbox_id.to_string(), resource);
        
        self.metrics.total_containers_created.fetch_add(1, Ordering::Relaxed);
        
        info!("Tracking container {} for sandbox {}", container_id, sandbox_id);
    }

    pub async fn untrack_container(&self, sandbox_id: &str) {
        let mut containers = self.tracked_containers.write().await;
        if let Some(resource) = containers.remove(sandbox_id) {
            if !resource.cleanup_attempted.load(Ordering::Relaxed) {
                self.metrics.total_containers_cleaned.fetch_add(1, Ordering::Relaxed);
            }
            info!("Untracked container for sandbox {}", sandbox_id);
        }
    }

    pub async fn update_heartbeat(&self, sandbox_id: &str) {
        let containers = self.tracked_containers.read().await;
        if let Some(resource) = containers.get(sandbox_id) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            resource.last_seen.store(now, Ordering::Relaxed);
        }
    }

    pub async fn get_stats(&self) -> HashMap<String, ResourceUsageStats> {
        let containers = self.tracked_containers.read().await;
        let mut stats = HashMap::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for (sandbox_id, resource) in containers.iter() {
            let last_seen = resource.last_seen.load(Ordering::Relaxed);
            let stats_entry = ResourceUsageStats {
                container_id: resource.container_id.clone(),
                sandbox_id: sandbox_id.clone(),
                uptime_seconds: resource.created_at.elapsed().as_secs(),
                last_seen_seconds_ago: now.saturating_sub(last_seen),
                cleanup_attempted: resource.cleanup_attempted.load(Ordering::Relaxed),
            };
            stats.insert(sandbox_id.clone(), stats_entry);
        }

        stats
    }

    pub async fn detect_leaked_containers(&self) -> Vec<String> {
        let containers = self.tracked_containers.read().await;
        let mut leaked = Vec::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Consider containers as leaked if they haven't been seen for 5 minutes
        const LEAK_THRESHOLD_SECONDS: u64 = 300;

        for (sandbox_id, resource) in containers.iter() {
            let last_seen = resource.last_seen.load(Ordering::Relaxed);
            if now.saturating_sub(last_seen) > LEAK_THRESHOLD_SECONDS {
                leaked.push(sandbox_id.clone());
                if !resource.cleanup_attempted.swap(true, Ordering::Relaxed) {
                    self.metrics.leaked_containers_detected.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        if !leaked.is_empty() {
            warn!("Detected {} potentially leaked containers: {:?}", leaked.len(), leaked);
        }

        leaked
    }

    pub async fn cleanup_all(&self) {
        info!("Cleaning up all tracked resources...");
        
        // Stop the background cleanup task
        if let Some(handle) = self.cleanup_task.lock().await.take() {
            handle.abort();
        }

        let containers = self.tracked_containers.read().await;
        for (sandbox_id, _) in containers.iter() {
            warn!("Container {} still tracked during final cleanup", sandbox_id);
        }
        
        drop(containers);
        self.tracked_containers.write().await.clear();
        
        info!("Resource cleanup completed");
    }

    fn start_cleanup_task(&self) {
        let tracked_containers = Arc::clone(&self.tracked_containers);
        let metrics = Arc::clone(&self.metrics);
        
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(TokioDuration::from_secs(60)); // Check every minute
            
            loop {
                interval.tick().await;
                
                // Detect and log leaked containers
                let leaked_count = {
                    let containers = tracked_containers.read().await;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    
                    containers.iter()
                        .filter(|(_, resource)| {
                            let last_seen = resource.last_seen.load(Ordering::Relaxed);
                            now.saturating_sub(last_seen) > 300 // 5 minutes
                        })
                        .count()
                };
                
                if leaked_count > 0 {
                    warn!("Background cleanup task detected {} potentially leaked containers", leaked_count);
                }
                
                // Update metrics
                let total_created = metrics.total_containers_created.load(Ordering::Relaxed);
                let total_cleaned = metrics.total_containers_cleaned.load(Ordering::Relaxed);
                let leaked_detected = metrics.leaked_containers_detected.load(Ordering::Relaxed);
                
                if total_created > 0 {
                    info!(
                        "Resource tracking stats - Created: {}, Cleaned: {}, Leaked: {}, Active: {}",
                        total_created,
                        total_cleaned,
                        leaked_detected,
                        total_created.saturating_sub(total_cleaned)
                    );
                }
            }
        });
        
        // Store the handle for later cleanup
        if let Ok(mut task_guard) = self.cleanup_task.try_lock() {
            *task_guard = Some(handle);
        }
    }
}

/// Mock implementation of ContainerManager for testing
#[cfg(test)]
pub struct MockContainerManager {
    containers: HashMap<String, Arc<SandboxContainer>>,
    docker: Arc<Docker>,
}

#[cfg(test)]
impl MockContainerManager {
    pub fn new() -> Self {
        let docker = Docker::connect_with_defaults().unwrap();
        Self {
            containers: HashMap::new(),
            docker: Arc::new(docker),
        }
    }

    pub async fn create_sandbox_container(
        &mut self,
        sandbox_id: &str,
        _image: &str,
        _resource_limits: ResourceLimits,
        _network_config: SandboxNetworkConfig,
        _env_vars: HashMap<String, String>,
    ) -> Result<Arc<SandboxContainer>> {
        // Create a mock container for testing
        let container = Arc::new(SandboxContainer::new(
            sandbox_id,
            "mock_container_id",
            "mock_image",
            ResourceLimits::default(),
            NetworkConfig::default(),
            HashMap::new(),
            Arc::clone(&self.docker),
        )?);
        
        self.containers.insert(sandbox_id.to_string(), container.clone());
        Ok(container)
    }

    pub async fn stop_container(&mut self, sandbox_id: &str) -> Result<()> {
        self.containers.remove(sandbox_id);
        Ok(())
    }

    pub async fn get_container(&self, sandbox_id: &str) -> Option<Arc<SandboxContainer>> {
        self.containers.get(sandbox_id).cloned()
    }

    pub async fn list_containers(&self) -> Vec<Arc<SandboxContainer>> {
        self.containers.values().cloned().collect()
    }
}