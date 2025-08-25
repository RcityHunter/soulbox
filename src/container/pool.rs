use std::collections::{HashMap, VecDeque, BTreeMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore, mpsc};
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

    /// Mark container as in use
    pub fn mark_in_use(&mut self) {
        self.in_use = true;
        self.last_used = Instant::now();
    }

    /// Mark container as released
    pub fn mark_released(&mut self) {
        self.in_use = false;
        self.last_used = Instant::now();
    }

    /// Check if container is idle for too long
    pub fn is_idle(&self, max_idle_time: Duration) -> bool {
        !self.in_use && self.last_used.elapsed() > max_idle_time
    }
}

/// Resource cleanup wrapper for containers
pub struct ContainerHandle {
    container_id: String,
    manager: Arc<ContainerManager>,
    cleaned_up: bool,
}

impl ContainerHandle {
    pub fn new(container_id: String, manager: Arc<ContainerManager>) -> Self {
        Self {
            container_id,
            manager,
            cleaned_up: false,
        }
    }

    /// Manually cleanup container resources
    pub async fn cleanup(&mut self) -> Result<()> {
        if !self.cleaned_up {
            self.manager.remove_container(&self.container_id).await?;
            self.cleaned_up = true;
            info!("Container {} cleaned up successfully", self.container_id);
        }
        Ok(())
    }

    /// Get container ID
    pub fn id(&self) -> &str {
        &self.container_id
    }
}

impl Drop for ContainerHandle {
    fn drop(&mut self) {
        if !self.cleaned_up {
            let container_id = self.container_id.clone();
            let manager = self.manager.clone();
            
            // 在后台线程中清理资源，避免阻塞Drop
            tokio::spawn(async move {
                if let Err(e) = manager.remove_container(&container_id).await {
                    error!("Failed to cleanup container {} during drop: {}", container_id, e);
                } else {
                    info!("Container {} cleaned up in background during drop", container_id);
                }
            });
        }
    }
}

/// Container cache levels for multi-tier caching
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheLevel {
    /// Hot cache - immediately available, fully warmed up
    Hot,
    /// Warm cache - started but not fully initialized
    Warm,
    /// Cold cache - created but not started
    Cold,
}

/// Result of a comprehensive health check
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// Overall health status
    pub healthy: bool,
    /// Whether the runtime is responsive
    pub runtime_responsive: bool,
    /// Error message if unhealthy
    pub error_message: Option<String>,
}

/// Pool statistics for monitoring and optimization
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
    /// Current pool sizes
    pub hot_pool_size: usize,
    pub warm_pool_size: usize,
    pub cold_pool_size: usize,
    pub total_pool_size: usize,
    /// Pool efficiency metrics
    pub hot_pool_efficiency: f64,
    pub avg_container_age_seconds: u64,
    /// Runtime-specific statistics
    pub runtime_stats: HashMap<RuntimeType, RuntimeStats>,
    /// Overall hit rate
    pub hit_rate: f64,
}

/// Runtime-specific statistics
#[derive(Debug, Clone, Default)]
pub struct RuntimeStats {
    /// Total requests for this runtime
    pub total_requests: u64,
    /// Pool hits (containers were available)
    pub pool_hits: u64,
    /// Pool misses (new container had to be created)
    pub pool_misses: u64,
    /// Hit rate percentage
    pub hit_rate: f64,
    /// Available containers in all pools
    pub containers_available: usize,
    /// Containers currently in use
    pub containers_in_use: usize,
    /// Average wait time for container allocation
    pub avg_wait_time: Duration,
}

/// Runtime usage pattern for optimization
#[derive(Debug, Clone)]
pub struct RuntimeUsagePattern {
    /// Average requests per hour
    pub requests_per_hour: f64,
    /// Average execution time in milliseconds
    pub avg_execution_time_ms: f64,
    /// Peak concurrent containers needed
    pub peak_concurrency: usize,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f64,
}

/// Prediction model for container demand
#[derive(Debug, Clone)]
struct DemandPredictor {
    /// Historical usage data by time window
    usage_history: BTreeMap<i64, RuntimeUsage>,
    /// Prediction confidence threshold
    confidence_threshold: f64,
}

#[derive(Debug, Clone, Default)]
struct RuntimeUsage {
    python_requests: u64,
    nodejs_requests: u64,
    rust_requests: u64,
    other_requests: u64,
}

impl DemandPredictor {
    fn new() -> Self {
        Self {
            usage_history: BTreeMap::new(),
            confidence_threshold: 0.7,
        }
    }
    
    /// Predict demand for the next time window
    fn predict_demand(&self, runtime: &RuntimeType) -> usize {
        // Simple moving average prediction
        let recent_windows = 5;
        let recent_usage: Vec<u64> = self.usage_history
            .iter()
            .rev()
            .take(recent_windows)
            .map(|(_, usage)| match runtime {
                RuntimeType::Python => usage.python_requests,
                RuntimeType::NodeJS => usage.nodejs_requests,
                RuntimeType::Rust => usage.rust_requests,
                _ => usage.other_requests,
            })
            .collect();
        
        if recent_usage.is_empty() {
            return 2; // Default prediction
        }
        
        let avg = recent_usage.iter().sum::<u64>() as f64 / recent_usage.len() as f64;
        (avg * 1.2) as usize // Add 20% buffer
    }
    
    /// Record usage for learning
    fn record_usage(&mut self, runtime: &RuntimeType) {
        let timestamp = chrono::Utc::now().timestamp();
        let window = timestamp / 300; // 5-minute windows
        
        let usage = self.usage_history.entry(window).or_default();
        match runtime {
            RuntimeType::Python => usage.python_requests += 1,
            RuntimeType::NodeJS => usage.nodejs_requests += 1,
            RuntimeType::Rust => usage.rust_requests += 1,
            _ => usage.other_requests += 1,
        }
        
        // Keep only last 24 hours of data
        let cutoff = window - (24 * 60 / 5);
        self.usage_history.retain(|&k, _| k > cutoff);
    }
}

/// Pre-warmed container pool for fast container startup
pub struct ContainerPool {
    /// Pool configuration
    config: PoolConfig,
    /// Container manager for creating/destroying containers
    container_manager: Arc<ContainerManager>,
    /// Multi-tier cache pools by runtime and cache level
    hot_pools: Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
    warm_pools: Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
    cold_pools: Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
    /// Semaphores for limiting concurrent container operations per runtime
    semaphores: HashMap<RuntimeType, Arc<Semaphore>>,
    /// Background task handle
    maintenance_task: Option<tokio::task::JoinHandle<()>>,
    /// Pool statistics
    stats: Arc<RwLock<PoolStats>>,
    /// Demand predictor for intelligent pre-warming
    predictor: Arc<RwLock<DemandPredictor>>,
    /// Channel for async container warming
    warming_tx: mpsc::Sender<WarmingRequest>,
    warming_rx: Arc<RwLock<mpsc::Receiver<WarmingRequest>>>,
}

impl Clone for ContainerPool {
    fn clone(&self) -> Self {
        // Create new channels for the cloned pool
        let (warming_tx, warming_rx) = mpsc::channel(100);
        
        Self {
            config: self.config.clone(),
            container_manager: self.container_manager.clone(),
            hot_pools: self.hot_pools.clone(),
            warm_pools: self.warm_pools.clone(),
            cold_pools: self.cold_pools.clone(),
            semaphores: self.semaphores.clone(),
            maintenance_task: None, // Don't clone the maintenance task
            stats: self.stats.clone(),
            predictor: self.predictor.clone(),
            warming_tx,
            warming_rx: Arc::new(RwLock::new(warming_rx)),
        }
    }
}

#[derive(Debug)]
struct WarmingRequest {
    runtime: RuntimeType,
    target_level: CacheLevel,
}

// Duplicate struct definitions removed - keeping the ones above

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
        
        // Create warming channel
        let (warming_tx, warming_rx) = mpsc::channel(100);

        Self {
            config,
            container_manager,
            hot_pools: Arc::new(RwLock::new(HashMap::new())),
            warm_pools: Arc::new(RwLock::new(HashMap::new())),
            cold_pools: Arc::new(RwLock::new(HashMap::new())),
            semaphores,
            maintenance_task: None,
            stats: Arc::new(RwLock::new(PoolStats::default())),
            predictor: Arc::new(RwLock::new(DemandPredictor::new())),
            warming_tx,
            warming_rx: Arc::new(RwLock::new(warming_rx)),
        }
    }

    /// Initialize the container pool
    pub async fn initialize(&mut self) -> Result<()> {
        info!("Initializing container pool with config: {:?}", self.config);

        // Initialize all cache levels for each runtime
        {
            let mut hot_pools = self.hot_pools.write().await;
            let mut warm_pools = self.warm_pools.write().await;
            let mut cold_pools = self.cold_pools.write().await;
            
            for runtime in self.config.runtime_configs.keys() {
                hot_pools.insert(runtime.clone(), VecDeque::new());
                warm_pools.insert(runtime.clone(), VecDeque::new());
                cold_pools.insert(runtime.clone(), VecDeque::new());
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
        
        // Try to get from hot pool first (fastest)
        if let Some(container) = self.get_from_hot_pool(&runtime).await {
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

        // Return to appropriate pool based on container state
        let mut hot_pools = self.hot_pools.write().await;
        if let Some(pool) = hot_pools.get_mut(&runtime) {
            let mut container = PoolContainer::new(container_id.clone(), runtime.clone());
            container.mark_released();
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

    /// Get a container from hot pool (fastest)
    async fn get_from_hot_pool(&self, runtime: &RuntimeType) -> Option<PoolContainer> {
        let mut hot_pools = self.hot_pools.write().await;
        if let Some(pool) = hot_pools.get_mut(runtime) {
            // Find an available, healthy container
            for i in 0..pool.len() {
                if !pool[i].in_use && pool[i].healthy && pool[i].warmed_up {
                    let mut container = pool.remove(i).unwrap();
                    container.mark_in_use();
                    return Some(container);
                }
            }
        }
        
        // If not found in hot pool, try warm pool
        if let Some(container) = self.get_from_warm_pool(runtime).await {
            return Some(container);
        }
        
        // Finally, try cold pool
        self.get_from_cold_pool(runtime).await
    }
    
    /// Get a container from warm pool (medium speed)
    async fn get_from_warm_pool(&self, runtime: &RuntimeType) -> Option<PoolContainer> {
        let mut warm_pools = self.warm_pools.write().await;
        if let Some(pool) = warm_pools.get_mut(runtime) {
            for i in 0..pool.len() {
                if !pool[i].in_use && pool[i].healthy {
                    let mut container = pool.remove(i)?;
                    container.mark_in_use();
                    
                    // Promote to hot pool after warming
                    // Simplified for MVP - would send warming request in production
                    
                    return Some(container);
                }
            }
        }
        None
    }
    
    /// Get a container from cold pool (slowest)
    async fn get_from_cold_pool(&self, runtime: &RuntimeType) -> Option<PoolContainer> {
        let mut cold_pools = self.cold_pools.write().await;
        if let Some(pool) = cold_pools.get_mut(runtime) {
            for i in 0..pool.len() {
                if !pool[i].in_use {
                    let mut container = pool.remove(i)?;
                    
                    // Start the container (it's created but not running)
                    if let Err(e) = self.container_manager.start_container(&container.id).await {
                        error!("Failed to start cold container {}: {}", container.id, e);
                        continue;
                    }
                    
                    container.mark_in_use();
                    
                    // Promote to warm pool after starting  
                    // Simplified for MVP - would send warming request in production
                    
                    return Some(container);
                }
            }
        }
        None
    }

    /// Create a new container with atomic operations and proper error handling
    async fn create_new_container(&self, runtime: &RuntimeType) -> Result<String> {
        
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
                memory: runtime_config.memory_limit.map(|m| m as i64),
                nano_cpus: runtime_config.cpu_limit.map(|cpu| (cpu * 1_000_000_000.0) as i64),
                security_opt: Some(vec!["no-new-privileges:true".to_string()]),
                readonly_rootfs: Some(false), // Allow writes to /workspace
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
                            if let Err(cleanup_err) = cleanup_manager.remove_container(&cleanup_id).await {
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
                        if let Err(cleanup_err) = cleanup_manager.remove_container(&cleanup_id).await {
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
        // Split command into parts for execute_command
        let command_parts: Vec<&str> = warmup_cmd.split_whitespace().collect();
        if command_parts.is_empty() {
            return Err(SoulBoxError::PoolError("Empty warmup command".to_string()));
        }
        
        info!("Warming up container {} with command: {}", container_id, warmup_cmd);
        
        match manager.execute_command(container_id, &command_parts).await {
            Ok(output) => {
                debug!("Container {} warmed up successfully with output: {}", container_id, output.trim());
                Ok(())
            }
            Err(e) => {
                warn!("Warmup command failed for container {}: {}", container_id, e);
                // Don't fail container creation if warmup fails - it's a nice-to-have
                Ok(())
            }
        }
    }

    /// Pre-warm containers for all runtimes with parallel execution
    async fn prewarm_containers(&self) -> Result<()> {
        info!("Pre-warming containers...");

        let mut prewarming_tasks = Vec::new();
        
        // Clone the runtime configs to avoid lifetime issues
        let runtime_configs = self.config.runtime_configs.clone();

        for (runtime, runtime_config) in runtime_configs {
            let runtime_for_task = runtime.clone();  // Clone for use inside the task
            let pool = self.clone();
            let min_containers = runtime_config.min_containers;
            
            let task = tokio::spawn(async move {
                let mut successful_containers = 0;
                
                for i in 0..min_containers {
                    match pool.create_new_container(&runtime_for_task).await {
                        Ok(container_id) => {
                            // Add to pool
                            if let Err(e) = pool.return_container(container_id.clone(), runtime_for_task.clone()).await {
                                error!("Failed to add pre-warmed container {} to pool: {}", container_id, e);
                            } else {
                                successful_containers += 1;
                                debug!("Pre-warmed container {}/{} for runtime {:?}", i + 1, min_containers, runtime_for_task);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to pre-warm container {}/{} for runtime {:?}: {}", i + 1, min_containers, runtime_for_task, e);
                        }
                    }
                }
                
                info!("Pre-warmed {}/{} containers for runtime {:?}", successful_containers, min_containers, runtime_for_task);
                successful_containers
            });
            
            prewarming_tasks.push((runtime, task));
        }

        // Wait for all prewarming tasks to complete
        let mut total_prewarmed = 0;
        for (runtime, task) in prewarming_tasks {
            match task.await {
                Ok(count) => total_prewarmed += count,
                Err(e) => error!("Pre-warming task failed for runtime {:?}: {}", runtime, e),
            }
        }

        info!("Container pre-warming completed: {} total containers pre-warmed", total_prewarmed);
        Ok(())
    }

    /// Warm up a container by running a command
    async fn warmup_container(&self, container_id: &str, command: &str) -> Result<()> {
        let timeout = Duration::from_secs(self.config.warmup_timeout);
        
        // Split command into parts for execute_command
        let command_parts: Vec<&str> = command.split_whitespace().collect();
        if command_parts.is_empty() {
            return Err(SoulBoxError::PoolError("Empty warmup command".to_string()));
        }
        
        match tokio::time::timeout(
            timeout,
            self.container_manager.execute_command(container_id, &command_parts)
        ).await {
            Ok(Ok(output)) => {
                debug!("Container {} warmed up successfully with output: {}", container_id, output.trim());
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

    /// Check container health with comprehensive checks
    async fn check_container_health(&self, container_id: &str) -> Result<bool> {
        match self.container_manager.get_container_info(container_id).await {
            Ok(info) => {
                // Check if container state indicates it's running
                let is_running = info.state
                    .as_ref()
                    .and_then(|state| state.status.as_ref())
                    .map(|status| matches!(status, bollard::models::ContainerStateStatusEnum::RUNNING))
                    .unwrap_or(false);
                
                if !is_running {
                    debug!("Container {} is not running", container_id);
                    return Ok(false);
                }
                
                // Additional health checks
                let has_exit_code = info.state
                    .as_ref()
                    .and_then(|state| state.exit_code)
                    .unwrap_or(0) == 0;
                
                let is_restarting = info.state
                    .as_ref()
                    .and_then(|state| state.restarting)
                    .unwrap_or(false);
                
                let health_status = !is_restarting && has_exit_code;
                
                debug!("Container {} health check: running={}, exit_code=ok={}, not_restarting={}, overall={}", 
                       container_id, is_running, has_exit_code, !is_restarting, health_status);
                
                Ok(health_status)
            },
            Err(e) => {
                debug!("Failed to get container {} info: {}", container_id, e);
                Ok(false)
            },
        }
    }
    
    /// Perform comprehensive health check with runtime-specific validation
    async fn comprehensive_health_check(&self, container_id: &str, runtime: &RuntimeType) -> Result<HealthCheckResult> {
        // Basic container state check
        let basic_health = self.check_container_health(container_id).await?;
        if !basic_health {
            return Ok(HealthCheckResult {
                healthy: false,
                runtime_responsive: false,
                error_message: Some("Container is not in running state".to_string()),
            });
        }
        
        // Runtime-specific health check
        let runtime_config = self.config.runtime_configs.get(runtime);
        if let Some(config) = runtime_config {
            if let Some(warmup_cmd) = &config.warmup_command {
                // Use warmup command as a simple runtime health check
                match self.warmup_container(container_id, warmup_cmd).await {
                    Ok(()) => Ok(HealthCheckResult {
                        healthy: true,
                        runtime_responsive: true,
                        error_message: None,
                    }),
                    Err(e) => Ok(HealthCheckResult {
                        healthy: false,
                        runtime_responsive: false,
                        error_message: Some(format!("Runtime health check failed: {}", e)),
                    }),
                }
            } else {
                // No runtime-specific check available, basic health is sufficient
                Ok(HealthCheckResult {
                    healthy: true,
                    runtime_responsive: true,
                    error_message: None,
                })
            }
        } else {
            // No configuration available, assume healthy if basic checks pass
            Ok(HealthCheckResult {
                healthy: true,
                runtime_responsive: true,
                error_message: None,
            })
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
        let hot_pools = self.hot_pools.clone();
        let warm_pools = self.warm_pools.clone();
        let cold_pools = self.cold_pools.clone();
        let config = self.config.clone();
        let stats = self.stats.clone();
        let container_manager = self.container_manager.clone();
        let predictor = self.predictor.clone();

        let maintenance_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(config.maintenance_interval));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::maintenance_cycle(
                    &hot_pools,
                    &warm_pools,
                    &cold_pools,
                    &config,
                    &stats,
                    &container_manager,
                    &predictor,
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
        hot_pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        warm_pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        cold_pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        config: &PoolConfig,
        stats: &Arc<RwLock<PoolStats>>,
        container_manager: &Arc<ContainerManager>,
        _predictor: &Arc<RwLock<DemandPredictor>>,
    ) -> Result<()> {
        let max_idle = Duration::from_secs(config.max_idle_time);
        let mut containers_to_destroy = Vec::new();
        let mut containers_to_create = HashMap::new();

        // Check each runtime pool level
        {
            let mut hot_pools_guard = hot_pools.write().await;
            let _warm_pools_guard = warm_pools.write().await;
            let _cold_pools_guard = cold_pools.write().await;
            
            // Process hot pools
            for (runtime, pool) in hot_pools_guard.iter_mut() {
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
        
        // Perform health checks on remaining containers
        Self::perform_health_checks(hot_pools, warm_pools, cold_pools, container_manager, config).await;

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
                        
                        let mut hot_pools_guard = hot_pools.write().await;
                        if let Some(pool) = hot_pools_guard.get_mut(&runtime) {
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
        Self::update_pool_stats(hot_pools, warm_pools, cold_pools, stats).await;

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
            image: Some(runtime_config.base_image.clone()),
            working_dir: Some("/workspace".to_string()),
            env: Some(vec![]), // Empty environment for now
            host_config: Some(bollard::models::HostConfig {
                memory: runtime_config.memory_limit.map(|m| m as i64),
                nano_cpus: runtime_config.cpu_limit.map(|cpu| (cpu * 1_000_000_000.0) as i64),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Create and start container
        let container_id = container_manager.create_container(container_config).await?;
        container_manager.start_container(&container_id).await?;

        // Warm up container if configured
        if let Some(warmup_cmd) = &runtime_config.warmup_command {
            if let Err(e) = Self::warmup_container_with_timeout(container_manager, &container_id, warmup_cmd, 30).await {
                warn!("Failed to warm up container {}: {}", container_id, e);
            }
        }

        info!("Created container {} for runtime {:?} during maintenance", container_id, runtime);
        Ok(container_id.to_string())
    }

    /// Static version of warmup_container for use in maintenance
    async fn warmup_container_with_timeout(
        container_manager: &Arc<ContainerManager>,
        container_id: &str,
        command: &str,
        warmup_timeout: u64,
    ) -> Result<()> {
        let timeout = Duration::from_secs(warmup_timeout);
        
        match tokio::time::timeout(
            timeout,
            container_manager.execute_command(container_id, &[command])
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
        hot_pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        warm_pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        cold_pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        stats: &Arc<RwLock<PoolStats>>,
    ) {
        let hot_pools_guard = hot_pools.read().await;
        let warm_pools_guard = warm_pools.read().await;
        let cold_pools_guard = cold_pools.read().await;
        let mut stats_guard = stats.write().await;

        // Combine stats from all pool levels
        for (runtime, hot_pool) in hot_pools_guard.iter() {
            let warm_pool = warm_pools_guard.get(runtime);
            let cold_pool = cold_pools_guard.get(runtime);
            let runtime_stats = stats_guard.runtime_stats.entry(runtime.clone()).or_default();
            
            runtime_stats.containers_available = hot_pool.iter().filter(|c| !c.in_use).count()
                + warm_pool.map_or(0, |p| p.iter().filter(|c| !c.in_use).count())
                + cold_pool.map_or(0, |p| p.iter().filter(|c| !c.in_use).count());
            runtime_stats.containers_in_use = hot_pool.iter().filter(|c| c.in_use).count()
                + warm_pool.map_or(0, |p| p.iter().filter(|c| c.in_use).count())
                + cold_pool.map_or(0, |p| p.iter().filter(|c| c.in_use).count());
            
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

        // Destroy all containers in all pool levels
        let hot_pools = self.hot_pools.read().await;
        let warm_pools = self.warm_pools.read().await;
        let cold_pools = self.cold_pools.read().await;
        
        for pool in hot_pools.values().chain(warm_pools.values()).chain(cold_pools.values()) {
            for container in pool {
                if let Err(e) = self.destroy_container(&container.id).await {
                    error!("Failed to destroy container {} during shutdown: {}", container.id, e);
                }
            }
        }

        info!("Container pool shutdown completed");
        Ok(())
    }
    
// Duplicate get_stats method removed - keeping the one above
    
    /// Optimize pool configuration based on usage patterns
    pub async fn optimize_pool_configuration(&self) -> Result<()> {
        let stats = self.stats.read().await;
        let current_efficiency = stats.hot_pool_efficiency;
        
        // If hot pool efficiency is too low (< 70%), adjust pool sizes
        if current_efficiency < 70.0 {
            info!("Pool efficiency is {}%, optimizing pool configuration", current_efficiency);
            
            // Analyze usage patterns by runtime
            let usage_stats = self.analyze_runtime_usage().await;
            
            // Adjust pool sizes based on actual usage
            for (runtime, usage) in usage_stats {
                let current_config = self.config.runtime_configs.get(&runtime);
                if let Some(config) = current_config {
                    let recommended_min = std::cmp::max(1, (usage.requests_per_hour / 10.0) as usize);
                    let recommended_max = std::cmp::max(recommended_min * 2, config.max_containers);
                    
                    if recommended_min != config.min_containers {
                        info!(
                            "Recommending pool size adjustment for {:?}: min {} -> {}, max {} -> {}",
                            runtime, config.min_containers, recommended_min, config.max_containers, recommended_max
                        );
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Analyze runtime usage patterns
    async fn analyze_runtime_usage(&self) -> HashMap<RuntimeType, RuntimeUsagePattern> {
        // This would typically analyze historical data
        // For now, return a simple analysis based on current pool states
        let mut usage_patterns = HashMap::new();
        
        let hot_pools = self.hot_pools.read().await;
        let _stats = self.stats.read().await;
        
        for (runtime, pool) in hot_pools.iter() {
            let usage_pattern = RuntimeUsagePattern {
                requests_per_hour: pool.len() as f64 * 10.0, // Simplified calculation
                avg_execution_time_ms: 500.0, // Default assumption
                peak_concurrency: pool.len(),
                success_rate: 0.95, // Default assumption
            };
            usage_patterns.insert(runtime.clone(), usage_pattern);
        }
        
        usage_patterns
    }
    
    /// Perform health checks on all containers in pools
    async fn perform_health_checks(
        hot_pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        warm_pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        cold_pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        container_manager: &Arc<ContainerManager>,
        config: &PoolConfig,
    ) {
        let mut unhealthy_containers = Vec::new();
        
        // Check hot pools
        {
            let mut hot_pools_guard = hot_pools.write().await;
            for (runtime, pool) in hot_pools_guard.iter_mut() {
                let mut healthy_containers = VecDeque::new();
                
                while let Some(mut container) = pool.pop_front() {
                    // Perform basic health check
                    match Self::check_container_health_static(container_manager, &container.id).await {
                        Ok(true) => {
                            container.healthy = true;
                            healthy_containers.push_back(container);
                        }
                        Ok(false) => {
                            warn!("Container {} failed health check, removing from pool", container.id);
                            container.healthy = false;
                            unhealthy_containers.push(container.id);
                        }
                        Err(e) => {
                            warn!("Error checking health of container {}: {}", container.id, e);
                            unhealthy_containers.push(container.id);
                        }
                    }
                }
                
                *pool = healthy_containers;
            }
        }
        
        // Check warm and cold pools similarly
        Self::check_pool_health(warm_pools, container_manager, &mut unhealthy_containers).await;
        Self::check_pool_health(cold_pools, container_manager, &mut unhealthy_containers).await;
        
        // Remove unhealthy containers and track runtime types for recovery
        let mut runtime_recovery_count: std::collections::HashMap<crate::runtime::RuntimeType, usize> = std::collections::HashMap::new();
        
        for container_id in unhealthy_containers {
            // Try to determine runtime type from container for recovery
            if let Ok(info) = container_manager.get_container_info(&container_id).await {
                if let Some(labels) = info.config.and_then(|c| c.labels) {
                    if let Some(runtime_str) = labels.get("soulbox.runtime") {
                        if let Ok(runtime_type) = runtime_str.parse::<crate::runtime::RuntimeType>() {
                            *runtime_recovery_count.entry(runtime_type).or_insert(0) += 1;
                        }
                    }
                }
            }
            
            if let Err(e) = container_manager.remove_container(&container_id).await {
                error!("Failed to remove unhealthy container {}: {}", container_id, e);
            } else {
                info!("Removed unhealthy container: {}", container_id);
            }
        }
        
        // Proactively create replacement containers for removed unhealthy ones
        for (runtime_type, count) in runtime_recovery_count {
            debug!("Scheduling recovery of {} containers for runtime {:?}", count, runtime_type);
            for _ in 0..count {
                let manager_clone = container_manager.clone();
                let config_clone = config.clone();
                let runtime_clone = runtime_type.clone();
                
                // Schedule recovery in background to avoid blocking health checks
                tokio::spawn(async move {
                    match Self::create_container_for_runtime(&manager_clone, &runtime_clone, &config_clone).await {
                        Ok(new_container_id) => {
                            info!("Successfully created recovery container {} for runtime {:?}", new_container_id, runtime_clone);
                            // Note: The new container will be picked up by the maintenance task
                        }
                        Err(e) => {
                            warn!("Failed to create recovery container for runtime {:?}: {}", runtime_clone, e);
                        }
                    }
                });
            }
        }
    }
    
    /// Helper method to check health of a specific pool
    async fn check_pool_health(
        pools: &Arc<RwLock<HashMap<RuntimeType, VecDeque<PoolContainer>>>>,
        container_manager: &Arc<ContainerManager>,
        unhealthy_containers: &mut Vec<String>,
    ) {
        let mut pools_guard = pools.write().await;
        for (_runtime, pool) in pools_guard.iter_mut() {
            let mut healthy_containers = VecDeque::new();
            
            while let Some(mut container) = pool.pop_front() {
                match Self::check_container_health_static(container_manager, &container.id).await {
                    Ok(true) => {
                        container.healthy = true;
                        healthy_containers.push_back(container);
                    }
                    Ok(false) | Err(_) => {
                        unhealthy_containers.push(container.id);
                    }
                }
            }
            
            *pool = healthy_containers;
        }
    }
    
    /// Static method to check container health (for use in static contexts)
    async fn check_container_health_static(
        container_manager: &Arc<ContainerManager>,
        container_id: &str,
    ) -> Result<bool> {
        match container_manager.get_container_info(container_id).await {
            Ok(info) => {
                let is_running = info.state
                    .as_ref()
                    .and_then(|state| state.status.as_ref())
                    .map(|status| matches!(status, bollard::models::ContainerStateStatusEnum::RUNNING))
                    .unwrap_or(false);
                    
                Ok(is_running)
            },
            Err(_) => Ok(false),
        }
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

        container.mark_in_use();
        assert!(container.in_use);

        container.mark_released();
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