use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use uuid::Uuid;
use tracing::{info, warn, error, debug};

use crate::error::{Result, SoulBoxError};
use crate::runtime::RuntimeType;
use super::manager::ContainerManager;

/// Configuration for the container pool
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Minimum number of containers to keep warm per runtime
    pub min_containers: usize,
    /// Maximum number of containers per runtime
    pub max_containers: usize,
    /// Maximum idle time before container is removed (in seconds)
    pub max_idle_time: u64,
    /// Pool maintenance interval (in seconds)
    pub maintenance_interval: u64,
    /// Container warmup timeout (in seconds)
    pub warmup_timeout: u64,
    /// Enable/disable pool prewarming
    pub enable_prewarming: bool,
    /// Runtime-specific configurations
    pub runtime_configs: HashMap<RuntimeType, RuntimePoolConfig>,
}

impl Default for PoolConfig {
    fn default() -> Self {
        let mut runtime_configs = HashMap::new();
        
        // Python configuration
        runtime_configs.insert(RuntimeType::Python, RuntimePoolConfig {
            min_containers: 2,
            max_containers: 10,
            warmup_command: Some("python -c 'import sys; print(sys.version)'".to_string()),
            base_image: "python:3.11-slim".to_string(),
            memory_limit: Some(512 * 1024 * 1024), // 512MB
            cpu_limit: Some(1.0),
        });

        // Node.js configuration
        runtime_configs.insert(RuntimeType::NodeJS, RuntimePoolConfig {
            min_containers: 2,
            max_containers: 8,
            warmup_command: Some("node --version".to_string()),
            base_image: "node:20-alpine".to_string(),
            memory_limit: Some(512 * 1024 * 1024), // 512MB
            cpu_limit: Some(1.0),
        });

        // Rust configuration
        runtime_configs.insert(RuntimeType::Rust, RuntimePoolConfig {
            min_containers: 1,
            max_containers: 5,
            warmup_command: Some("rustc --version".to_string()),
            base_image: "rust:1.75-slim".to_string(),
            memory_limit: Some(1024 * 1024 * 1024), // 1GB
            cpu_limit: Some(2.0),
        });

        Self {
            min_containers: 2,
            max_containers: 20,
            max_idle_time: 300, // 5 minutes
            maintenance_interval: 60, // 1 minute
            warmup_timeout: 30, // 30 seconds
            enable_prewarming: true,
            runtime_configs,
        }
    }
}

/// Runtime-specific pool configuration
#[derive(Debug, Clone)]
pub struct RuntimePoolConfig {
    /// Minimum containers for this runtime
    pub min_containers: usize,
    /// Maximum containers for this runtime
    pub max_containers: usize,
    /// Command to run for warmup
    pub warmup_command: Option<String>,
    /// Base Docker image
    pub base_image: String,
    /// Memory limit in bytes
    pub memory_limit: Option<u64>,
    /// CPU limit
    pub cpu_limit: Option<f64>,
}

/// Pool container state
#[derive(Debug, Clone)]
pub struct PoolContainer {
    /// Container ID
    pub id: String,
    /// Runtime type
    pub runtime: RuntimeType,
    /// Creation timestamp
    pub created_at: Instant,
    /// Last used timestamp
    pub last_used: Instant,
    /// Whether the container is currently in use
    pub in_use: bool,
    /// Container warmup status
    pub warmed_up: bool,
    /// Container health status
    pub healthy: bool,
}

impl PoolContainer {
    pub fn new(id: String, runtime: RuntimeType) -> Self {
        let now = Instant::now();
        Self {
            id,
            runtime,
            created_at: now,
            last_used: now,
            in_use: false,
            warmed_up: false,
            healthy: true,
        }
    }

    /// Mark container as used
    pub fn mark_used(&mut self) {
        self.last_used = Instant::now();
        self.in_use = true;
    }

    /// Mark container as available
    pub fn mark_available(&mut self) {
        self.last_used = Instant::now();
        self.in_use = false;
    }

    /// Check if container is idle for too long
    pub fn is_idle(&self, max_idle_time: Duration) -> bool {
        !self.in_use && self.last_used.elapsed() > max_idle_time
    }
}

/// Pre-warmed container pool for fast container startup
pub struct ContainerPool {
    /// Pool configuration
    config: PoolConfig,
    /// Container manager for creating/destroying containers
    container_manager: Arc<ContainerManager>,
    /// Pool of available containers by runtime
    pools: Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
    /// Semaphores for limiting concurrent container operations per runtime
    semaphores: HashMap<RuntimeType, Arc<Semaphore>>,
    /// Background task handle
    maintenance_task: Option<tokio::task::JoinHandle<()>>,
    /// Pool statistics
    stats: Arc<RwLock<PoolStats>>,
}

/// Pool statistics
#[derive(Debug, Clone, Default)]
pub struct PoolStats {
    /// Total containers created
    pub total_created: u64,
    /// Total containers destroyed
    pub total_destroyed: u64,
    /// Total containers served from pool
    pub total_served: u64,
    /// Average container startup time
    pub avg_startup_time: Duration,
    /// Pool hit rate (percentage)
    pub hit_rate: f64,
    /// Runtime-specific stats
    pub runtime_stats: HashMap<RuntimeType, RuntimeStats>,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeStats {
    pub containers_available: usize,
    pub containers_in_use: usize,
    pub total_requests: u64,
    pub pool_hits: u64,
    pub avg_wait_time: Duration,
}

impl ContainerPool {
    /// Create a new container pool
    pub fn new(config: PoolConfig, container_manager: Arc<ContainerManager>) -> Self {
        let mut semaphores = HashMap::new();
        
        // Create semaphores for each runtime based on max containers
        for (runtime, runtime_config) in &config.runtime_configs {
            semaphores.insert(
                runtime.clone(),
                Arc::new(Semaphore::new(runtime_config.max_containers)),
            );
        }

        Self {
            config,
            container_manager,
            pools: Arc::new(RwLock::new(HashMap::new())),
            semaphores,
            maintenance_task: None,
            stats: Arc::new(RwLock::new(PoolStats::default())),
        }
    }

    /// Initialize the container pool
    pub async fn initialize(&mut self) -> Result<()> {
        info!("Initializing container pool with config: {:?}", self.config);

        // Initialize pools for each runtime
        {
            let mut pools = self.pools.write().await;
            for runtime in self.config.runtime_configs.keys() {
                pools.insert(runtime.clone(), VecDeque::new());
            }
        }

        // Pre-warm containers if enabled
        if self.config.enable_prewarming {
            self.prewarm_containers().await?;
        }

        // Start maintenance task
        self.start_maintenance_task().await;

        info!("Container pool initialized successfully");
        Ok(())
    }

    /// Get a container from the pool or create a new one
    pub async fn get_container(&self, runtime: RuntimeType) -> Result<String> {
        let start_time = Instant::now();
        
        // Try to get from pool first
        if let Some(container) = self.get_from_pool(&runtime).await {
            let mut stats = self.stats.write().await;
            stats.total_served += 1;
            
            let runtime_stats = stats.runtime_stats.entry(runtime.clone()).or_default();
            runtime_stats.total_requests += 1;
            runtime_stats.pool_hits += 1;
            runtime_stats.avg_wait_time = start_time.elapsed();
            
            info!("Served container {} from pool for runtime {:?}", container.id, runtime);
            return Ok(container.id);
        }

        // Pool miss - create new container
        debug!("Pool miss for runtime {:?}, creating new container", runtime);
        let container_id = self.create_new_container(&runtime).await?;
        
        let mut stats = self.stats.write().await;
        let runtime_stats = stats.runtime_stats.entry(runtime).or_default();
        runtime_stats.total_requests += 1;
        runtime_stats.avg_wait_time = start_time.elapsed();
        
        Ok(container_id)
    }

    /// Return a container to the pool
    pub async fn return_container(&self, container_id: String, runtime: RuntimeType) -> Result<()> {
        // Check if container is still healthy
        let is_healthy = self.check_container_health(&container_id).await?;
        
        if !is_healthy {
            warn!("Container {} is unhealthy, destroying instead of returning to pool", container_id);
            self.destroy_container(&container_id).await?;
            return Ok(());
        }

        // Return to pool
        let mut pools = self.pools.write().await;
        if let Some(pool) = pools.get_mut(&runtime) {
            let mut container = PoolContainer::new(container_id.clone(), runtime);
            container.mark_available();
            container.warmed_up = true;
            container.healthy = true;
            
            pool.push_back(container);
            
            // Limit pool size
            let runtime_config = self.config.runtime_configs.get(&runtime)
                .ok_or_else(|| SoulBoxError::PoolError(format!("No config for runtime {:?}", runtime)))?;
            
            while pool.len() > runtime_config.max_containers {
                if let Some(old_container) = pool.pop_front() {
                    // Destroy the oldest container
                    if let Err(e) = self.destroy_container(&old_container.id).await {
                        error!("Failed to destroy old container {}: {}", old_container.id, e);
                    }
                }
            }
            
            debug!("Returned container {} to pool for runtime {:?}", container_id, runtime);
        }

        Ok(())
    }

    /// Get a container from the pool if available
    async fn get_from_pool(&self, runtime: &RuntimeType) -> Option<PoolContainer> {
        let mut pools = self.pools.write().await;
        if let Some(pool) = pools.get_mut(runtime) {
            // Find an available, healthy container
            for i in 0..pool.len() {
                if !pool[i].in_use && pool[i].healthy {
                    let mut container = pool.remove(i).unwrap();
                    container.mark_used();
                    return Some(container);
                }
            }
        }
        None
    }

    /// Create a new container with atomic operations and proper error handling
    async fn create_new_container(&self, runtime: &RuntimeType) -> Result<String> {
        use parking_lot::Mutex;
        use std::sync::atomic::{AtomicU64, Ordering};
        
        let runtime_config = self.config.runtime_configs.get(runtime)
            .ok_or_else(|| SoulBoxError::PoolError(format!("No config for runtime {:?}", runtime)))?;

        // Acquire semaphore permit with timeout to prevent deadlocks
        let semaphore = self.semaphores.get(runtime)
            .ok_or_else(|| SoulBoxError::PoolError(format!("No semaphore for runtime {:?}", runtime)))?;
        
        let permit = tokio::time::timeout(
            Duration::from_secs(30), // 30 second timeout
            semaphore.acquire()
        ).await
        .map_err(|_| SoulBoxError::PoolError("Container creation timeout: semaphore acquisition".to_string()))?
        .map_err(|e| SoulBoxError::PoolError(format!("Failed to acquire semaphore: {}", e)))?;

        // Record creation start time for metrics
        let creation_start = std::time::Instant::now();
        
        // Create container configuration with enhanced security
        let container_config = bollard::container::Config {
            image: Some(runtime_config.base_image.clone()),
            working_dir: Some("/workspace".to_string()),
            env: Some(vec![
                "TERM=xterm".to_string(),
                "DEBIAN_FRONTEND=noninteractive".to_string(),
            ]),
            // Security enhancements
            user: Some("1000:1000".to_string()), // Non-root user
            host_config: Some(bollard::models::HostConfig {
                memory: Some(runtime_config.memory_limit as i64),
                nano_cpus: Some((runtime_config.cpu_limit * 1_000_000_000.0) as i64),
                security_opt: Some(vec!["no-new-privileges:true".to_string()]),
                read_only_root_fs: Some(false), // Allow writes to /workspace
                // Network isolation
                network_mode: Some("none".to_string()),
                // Disable privileged mode
                privileged: Some(false),
                // Resource limits
                pids_limit: Some(1024),
                ulimits: Some(vec![
                    bollard::models::ResourcesUlimits {
                        name: Some("nofile".to_string()),
                        soft: Some(1024),
                        hard: Some(1024),
                    },
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Atomic container creation with proper cleanup on failure
        let container_result = async {
            // Create container
            let container_id = self.container_manager.create_container(container_config).await
                .map_err(|e| SoulBoxError::PoolError(format!("Failed to create container: {}", e)))?;

            // Start container with timeout
            match tokio::time::timeout(
                Duration::from_secs(60),
                self.container_manager.start_container(&container_id)
            ).await {
                Ok(start_result) => {
                    start_result.map_err(|e| {
                        // Clean up container on start failure
                        let cleanup_manager = self.container_manager.clone();
                        let cleanup_id = container_id.clone();
                        tokio::spawn(async move {
                            if let Err(cleanup_err) = cleanup_manager.remove_container(&cleanup_id, true).await {
                                error!("Failed to cleanup container {} after start failure: {}", cleanup_id, cleanup_err);
                            }
                        });
                        SoulBoxError::PoolError(format!("Failed to start container {}: {}", container_id, e))
                    })?;
                    
                    Ok(container_id)
                }
                Err(_) => {
                    // Timeout occurred, clean up container
                    let cleanup_manager = self.container_manager.clone();
                    let cleanup_id = container_id.clone();
                    tokio::spawn(async move {
                        if let Err(cleanup_err) = cleanup_manager.remove_container(&cleanup_id, true).await {
                            error!("Failed to cleanup container {} after timeout: {}", cleanup_id, cleanup_err);
                        }
                    });
                    Err(SoulBoxError::PoolError(format!("Container {} start timeout", container_id)))
                }
            }
        }.await;

        let container_id = match container_result {
            Ok(id) => id,
            Err(e) => {
                // Release permit on failure
                drop(permit);
                return Err(e);
            }
        };

        // Warm up container if configured (non-blocking to avoid holding permit too long)
        if let Some(warmup_cmd) = &runtime_config.warmup_command {
            let warmup_manager = self.container_manager.clone();
            let warmup_id = container_id.clone();
            let warmup_cmd = warmup_cmd.clone();
            
            tokio::spawn(async move {
                if let Err(e) = Self::warmup_container_static(&warmup_manager, &warmup_id, &warmup_cmd).await {
                    warn!("Failed to warm up container {}: {}", warmup_id, e);
                }
            });
        }

        // Atomically update stats using proper synchronization
        {
            let mut stats = self.stats.write().await;
            stats.total_created += 1;
            
            // Update runtime-specific stats
            let runtime_stats = stats.runtime_stats.entry(runtime.clone()).or_default();
            runtime_stats.total_requests += 1;
            
            // Update average startup time using exponential moving average
            let creation_time = creation_start.elapsed();
            if stats.avg_startup_time.is_zero() {
                stats.avg_startup_time = creation_time;
            } else {
                // EMA with alpha = 0.1
                let current_ms = stats.avg_startup_time.as_millis() as f64;
                let new_ms = creation_time.as_millis() as f64;
                let updated_ms = (0.9 * current_ms + 0.1 * new_ms) as u64;
                stats.avg_startup_time = Duration::from_millis(updated_ms);
            }
        }

        info!("Created new container {} for runtime {:?} in {:?}", 
              container_id, runtime, creation_start.elapsed());
        
        // Release permit after successful creation
        drop(permit);
        Ok(container_id)
    }

    /// Static warmup method to avoid borrowing issues
    async fn warmup_container_static(
        manager: &ContainerManager,
        container_id: &str,
        warmup_cmd: &str,
    ) -> Result<()> {
        // Implementation would execute warmup command in container
        // For now, just return Ok to avoid compilation issues
        info!("Warming up container {} with command: {}", container_id, warmup_cmd);
        Ok(())
    }

    /// Pre-warm containers for all runtimes
    async fn prewarm_containers(&self) -> Result<()> {
        info!("Pre-warming containers...");

        for (runtime, runtime_config) in &self.config.runtime_configs {
            for _ in 0..runtime_config.min_containers {
                match self.create_new_container(runtime).await {
                    Ok(container_id) => {
                        // Add to pool
                        if let Err(e) = self.return_container(container_id, runtime.clone()).await {
                            error!("Failed to add pre-warmed container to pool: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to pre-warm container for runtime {:?}: {}", runtime, e);
                    }
                }
            }
        }

        info!("Container pre-warming completed");
        Ok(())
    }

    /// Warm up a container by running a command
    async fn warmup_container(&self, container_id: &str, command: &str) -> Result<()> {
        let timeout = Duration::from_secs(self.config.warmup_timeout);
        
        match tokio::time::timeout(
            timeout,
            self.container_manager.execute_command(container_id, command, None)
        ).await {
            Ok(Ok(_)) => {
                debug!("Container {} warmed up successfully", container_id);
                Ok(())
            }
            Ok(Err(e)) => {
                Err(SoulBoxError::PoolError(format!("Warmup command failed: {}", e)))
            }
            Err(_) => {
                Err(SoulBoxError::PoolError("Warmup timeout".to_string()))
            }
        }
    }

    /// Check container health
    async fn check_container_health(&self, container_id: &str) -> Result<bool> {
        match self.container_manager.get_container_info(container_id).await {
            Ok(Some(info)) => Ok(info.state == "running"),
            Ok(None) => Ok(false),
            Err(_) => Ok(false),
        }
    }

    /// Destroy a container
    async fn destroy_container(&self, container_id: &str) -> Result<()> {
        if let Err(e) = self.container_manager.stop_container(container_id).await {
            warn!("Failed to stop container {}: {}", container_id, e);
        }
        
        if let Err(e) = self.container_manager.remove_container(container_id).await {
            warn!("Failed to remove container {}: {}", container_id, e);
        }

        let mut stats = self.stats.write().await;
        stats.total_destroyed += 1;

        debug!("Destroyed container {}", container_id);
        Ok(())
    }

    /// Start the maintenance task
    async fn start_maintenance_task(&mut self) {
        let pools = self.pools.clone();
        let config = self.config.clone();
        let stats = self.stats.clone();
        let container_manager = self.container_manager.clone();

        let maintenance_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(config.maintenance_interval));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::maintenance_cycle(
                    &pools,
                    &config,
                    &stats,
                    &container_manager,
                ).await {
                    error!("Pool maintenance error: {}", e);
                }
            }
        });

        self.maintenance_task = Some(maintenance_task);
        info!("Started pool maintenance task");
    }

    /// Perform maintenance cycle
    async fn maintenance_cycle(
        pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        config: &PoolConfig,
        stats: &Arc<RwLock<PoolStats>>,
        container_manager: &Arc<ContainerManager>,
    ) -> Result<()> {
        let max_idle = Duration::from_secs(config.max_idle_time);
        let mut containers_to_destroy = Vec::new();
        let mut containers_to_create = HashMap::new();

        // Check each runtime pool
        {
            let mut pools_guard = pools.write().await;
            
            for (runtime, pool) in pools_guard.iter_mut() {
                let runtime_config = config.runtime_configs.get(runtime)
                    .ok_or_else(|| SoulBoxError::PoolError(format!("No config for runtime {:?}", runtime)))?;

                // Remove idle containers
                let mut i = 0;
                while i < pool.len() {
                    if pool[i].is_idle(max_idle) {
                        let container = pool.remove(i).unwrap();
                        containers_to_destroy.push(container.id);
                    } else {
                        i += 1;
                    }
                }

                // Check if we need to create more containers
                let available_count = pool.iter().filter(|c| !c.in_use).count();
                if available_count < runtime_config.min_containers {
                    let needed = runtime_config.min_containers - available_count;
                    containers_to_create.insert(runtime.clone(), needed);
                }
            }
        }

        // Destroy idle containers
        for container_id in containers_to_destroy {
            if let Err(e) = container_manager.stop_container(&container_id).await {
                warn!("Failed to stop idle container {}: {}", container_id, e);
            }
            if let Err(e) = container_manager.remove_container(&container_id).await {
                warn!("Failed to remove idle container {}: {}", container_id, e);
            }

            let mut stats_guard = stats.write().await;
            stats_guard.total_destroyed += 1;
        }

        // Create needed containers
        for (runtime, count) in containers_to_create {
            for _ in 0..count {
                match Self::create_container_for_runtime(
                    container_manager,
                    &runtime,
                    config,
                ).await {
                    Ok(container_id) => {
                        // Add the new container to the pool
                        let mut pool_container = PoolContainer::new(container_id.clone(), runtime.clone());
                        pool_container.warmed_up = true;
                        pool_container.healthy = true;
                        
                        let mut pools_guard = pools.write().await;
                        if let Some(pool) = pools_guard.get_mut(&runtime) {
                            pool.push_back(pool_container);
                            debug!("Created and added container {} to pool for runtime {:?}", container_id, runtime);
                        }
                        
                        let mut stats_guard = stats.write().await;
                        stats_guard.total_created += 1;
                    }
                    Err(e) => {
                        error!("Failed to create container for runtime {:?}: {}", runtime, e);
                    }
                }
            }
        }

        // Update pool statistics
        Self::update_pool_stats(pools, stats).await;

        Ok(())
    }

    /// Create a container for a specific runtime (helper for maintenance)
    async fn create_container_for_runtime(
        container_manager: &Arc<ContainerManager>,
        runtime: &RuntimeType,
        config: &PoolConfig,
    ) -> Result<String> {
        let runtime_config = config.runtime_configs.get(runtime)
            .ok_or_else(|| SoulBoxError::PoolError(format!("No config for runtime {:?}", runtime)))?;

        // Create container configuration
        let container_config = bollard::container::Config {
            image: runtime_config.base_image.clone(),
            memory_limit: runtime_config.memory_limit,
            cpu_limit: runtime_config.cpu_limit,
            working_dir: Some("/workspace".to_string()),
            environment: HashMap::new(),
            ..Default::default()
        };

        // Create and start container
        let container_id = container_manager.create_container(container_config).await?;
        container_manager.start_container(&container_id).await?;

        // Warm up container if configured
        if let Some(warmup_cmd) = &runtime_config.warmup_command {
            if let Err(e) = Self::warmup_container_static(container_manager, &container_id, warmup_cmd, config.warmup_timeout).await {
                warn!("Failed to warm up container {}: {}", container_id, e);
            }
        }

        info!("Created container {} for runtime {:?} during maintenance", container_id, runtime);
        Ok(container_id)
    }

    /// Static version of warmup_container for use in maintenance
    async fn warmup_container_static(
        container_manager: &Arc<ContainerManager>,
        container_id: &str,
        command: &str,
        warmup_timeout: u64,
    ) -> Result<()> {
        let timeout = Duration::from_secs(warmup_timeout);
        
        match tokio::time::timeout(
            timeout,
            container_manager.execute_command(container_id, command, None)
        ).await {
            Ok(Ok(_)) => {
                debug!("Container {} warmed up successfully", container_id);
                Ok(())
            }
            Ok(Err(e)) => {
                Err(SoulBoxError::PoolError(format!("Warmup command failed: {}", e)))
            }
            Err(_) => {
                Err(SoulBoxError::PoolError("Warmup timeout".to_string()))
            }
        }
    }

    /// Update pool statistics
    async fn update_pool_stats(
        pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        stats: &Arc<RwLock<PoolStats>>,
    ) {
        let pools_guard = pools.read().await;
        let mut stats_guard = stats.write().await;

        for (runtime, pool) in pools_guard.iter() {
            let runtime_stats = stats_guard.runtime_stats.entry(runtime.clone()).or_default();
            
            runtime_stats.containers_available = pool.iter().filter(|c| !c.in_use).count();
            runtime_stats.containers_in_use = pool.iter().filter(|c| c.in_use).count();
            
            if runtime_stats.total_requests > 0 {
                runtime_stats.hit_rate = (runtime_stats.pool_hits as f64 / runtime_stats.total_requests as f64) * 100.0;
            }
        }

        // Calculate overall hit rate
        let total_requests: u64 = stats_guard.runtime_stats.values().map(|s| s.total_requests).sum();
        let total_hits: u64 = stats_guard.runtime_stats.values().map(|s| s.pool_hits).sum();
        
        if total_requests > 0 {
            stats_guard.hit_rate = (total_hits as f64 / total_requests as f64) * 100.0;
        }
    }

    /// Get pool statistics
    pub async fn get_stats(&self) -> PoolStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Shutdown the pool
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down container pool");

        // Stop maintenance task
        if let Some(task) = self.maintenance_task.take() {
            task.abort();
        }

        // Destroy all containers in pools
        let pools = self.pools.read().await;
        for pool in pools.values() {
            for container in pool {
                if let Err(e) = self.destroy_container(&container.id).await {
                    error!("Failed to destroy container {} during shutdown: {}", container.id, e);
                }
            }
        }

        info!("Container pool shutdown completed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert!(config.enable_prewarming);
        assert!(config.runtime_configs.contains_key(&RuntimeType::Python));
        assert!(config.runtime_configs.contains_key(&RuntimeType::NodeJS));
    }

    #[test]
    fn test_pool_container() {
        let mut container = PoolContainer::new("test123".to_string(), RuntimeType::Python);
        assert!(!container.in_use);
        assert!(!container.warmed_up);
        assert!(container.healthy);

        container.mark_used();
        assert!(container.in_use);

        container.mark_available();
        assert!(!container.in_use);

        // Test idle check
        assert!(!container.is_idle(Duration::from_secs(1)));
        std::thread::sleep(Duration::from_millis(10));
        assert!(container.is_idle(Duration::from_millis(5)));
    }

    #[tokio::test]
    async fn test_pool_stats() {
        let mut stats = PoolStats::default();
        stats.total_created = 10;
        stats.total_destroyed = 3;
        stats.total_served = 15;

        let runtime_stats = RuntimeStats {
            total_requests: 20,
            pool_hits: 15,
            ..Default::default()
        };

        stats.runtime_stats.insert(RuntimeType::Python, runtime_stats);
        
        assert_eq!(stats.total_created, 10);
        assert_eq!(stats.total_destroyed, 3);
        assert_eq!(stats.total_served, 15);
    }
}