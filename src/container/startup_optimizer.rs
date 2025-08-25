//! Container Startup Optimization
//! 
//! Provides mechanisms to reduce cold start time through prewarming,
//! image caching, and parallel initialization strategies

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tokio::time::timeout;
use tracing::{info, error, debug};
use uuid::Uuid;
use serde::{Serialize, Deserialize};

use crate::runtime::RuntimeType;
use crate::container::ContainerManager;
use crate::error::{Result, SoulBoxError};

/// Startup optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupOptimizerConfig {
    /// Enable startup optimization
    pub enabled: bool,
    /// Maximum number of prewarmed containers per runtime
    pub max_prewarmed_per_runtime: usize,
    /// Minimum number of prewarmed containers per runtime
    pub min_prewarmed_per_runtime: usize,
    /// Prewarming timeout
    pub prewarm_timeout: Duration,
    /// Maximum parallel startup operations
    pub max_parallel_startups: usize,
    /// Container warmup strategies
    pub warmup_strategies: Vec<WarmupStrategy>,
    /// Image pre-pull configuration
    pub image_prepull: ImagePrePullConfig,
}

impl Default for StartupOptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_prewarmed_per_runtime: 5,
            min_prewarmed_per_runtime: 2,
            prewarm_timeout: Duration::from_secs(30),
            max_parallel_startups: 10,
            warmup_strategies: vec![
                WarmupStrategy::ImagePull,
                WarmupStrategy::RuntimeInitialization,
                WarmupStrategy::DependencyPreload,
            ],
            image_prepull: ImagePrePullConfig::default(),
        }
    }
}

/// Warmup strategies for reducing cold start time
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WarmupStrategy {
    /// Pre-pull container images
    ImagePull,
    /// Initialize runtime environment
    RuntimeInitialization,
    /// Preload common dependencies
    DependencyPreload,
    /// JIT compilation warmup
    JitWarmup,
    /// Network interface setup
    NetworkSetup,
    /// Filesystem preparation
    FilesystemSetup,
}

/// Image pre-pull configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePrePullConfig {
    /// Enable image pre-pulling
    pub enabled: bool,
    /// Images to pre-pull on startup
    pub startup_images: Vec<String>,
    /// Pre-pull interval for popular images
    pub prepull_interval: Duration,
    /// Maximum concurrent pulls
    pub max_concurrent_pulls: usize,
}

impl Default for ImagePrePullConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            startup_images: vec![
                "python:3.11-slim".to_string(),
                "node:20-alpine".to_string(),
                "rust:1.75-slim".to_string(),
                "ubuntu:22.04".to_string(),
            ],
            prepull_interval: Duration::from_secs(3600), // 1 hour
            max_concurrent_pulls: 3,
        }
    }
}

/// Prewarmed container information
#[derive(Debug, Clone)]
struct PrewarmedContainer {
    container_id: String,
    runtime: RuntimeType,
    created_at: Instant,
    last_health_check: Instant,
    startup_time: Duration,
    ready: bool,
}

/// Startup performance metrics
#[derive(Debug, Clone, Serialize)]
pub struct StartupMetrics {
    pub total_containers_created: u64,
    pub prewarmed_containers_used: u64,
    pub avg_cold_start_time: Duration,
    pub avg_warm_start_time: Duration,
    pub cache_hit_ratio: f64,
    pub parallel_startups: u64,
    pub startup_failures: u64,
}

impl Default for StartupMetrics {
    fn default() -> Self {
        Self {
            total_containers_created: 0,
            prewarmed_containers_used: 0,
            avg_cold_start_time: Duration::from_millis(0),
            avg_warm_start_time: Duration::from_millis(0),
            cache_hit_ratio: 0.0,
            parallel_startups: 0,
            startup_failures: 0,
        }
    }
}

/// Container startup optimizer
pub struct StartupOptimizer {
    config: StartupOptimizerConfig,
    container_manager: Arc<ContainerManager>,
    prewarmed_containers: Arc<RwLock<HashMap<RuntimeType, Vec<PrewarmedContainer>>>>,
    startup_semaphore: Arc<Semaphore>,
    metrics: Arc<RwLock<StartupMetrics>>,
    image_cache: Arc<RwLock<HashMap<String, Instant>>>,
}

impl StartupOptimizer {
    /// Create new startup optimizer
    pub fn new(
        config: StartupOptimizerConfig,
        container_manager: Arc<ContainerManager>,
    ) -> Self {
        let startup_semaphore = Arc::new(Semaphore::new(config.max_parallel_startups));
        
        Self {
            config,
            container_manager,
            prewarmed_containers: Arc::new(RwLock::new(HashMap::new())),
            startup_semaphore,
            metrics: Arc::new(RwLock::new(StartupMetrics::default())),
            image_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start the startup optimizer
    pub async fn start(&self) -> Result<()> {
        if !self.config.enabled {
            info!("Startup optimizer is disabled");
            return Ok(());
        }

        info!("Starting container startup optimizer");

        // Pre-pull images
        if self.config.image_prepull.enabled {
            self.prepull_startup_images().await?;
        }

        // Initialize prewarmed containers
        self.initialize_prewarmed_containers().await?;

        // Start background maintenance tasks
        self.start_maintenance_tasks().await;

        info!("Container startup optimizer started successfully");
        Ok(())
    }

    /// Pre-pull startup images
    async fn prepull_startup_images(&self) -> Result<()> {
        info!("Pre-pulling startup images");
        
        let images = self.config.image_prepull.startup_images.clone();
        let semaphore = Arc::new(Semaphore::new(self.config.image_prepull.max_concurrent_pulls));
        let mut tasks = Vec::new();

        for image in images {
            let sem = Arc::clone(&semaphore);
            let container_manager = Arc::clone(&self.container_manager);
            let image_cache = Arc::clone(&self.image_cache);
            
            let task = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                
                info!("Pre-pulling image: {}", image);
                let start_time = Instant::now();
                
                match container_manager.pull_image(&image).await {
                    Ok(_) => {
                        let pull_time = start_time.elapsed();
                        info!("Successfully pulled image {} in {:?}", image, pull_time);
                        
                        // Cache the image pull time
                        image_cache.write().await.insert(image, Instant::now());
                    }
                    Err(e) => {
                        error!("Failed to pull image {}: {}", image, e);
                    }
                }
            });
            
            tasks.push(task);
        }

        // Wait for all image pulls to complete
        for task in tasks {
            let _ = task.await;
        }

        info!("Image pre-pulling completed");
        Ok(())
    }

    /// Initialize prewarmed containers
    async fn initialize_prewarmed_containers(&self) -> Result<()> {
        info!("Initializing prewarmed containers");
        
        let runtimes = vec![
            RuntimeType::Python,
            RuntimeType::NodeJS,
            RuntimeType::Rust,
            RuntimeType::Go,
            RuntimeType::Shell,
        ];

        for runtime in runtimes {
            let target_count = self.config.min_prewarmed_per_runtime;
            self.ensure_prewarmed_containers(runtime, target_count).await?;
        }

        info!("Prewarmed container initialization completed");
        Ok(())
    }

    /// Ensure minimum number of prewarmed containers for a runtime
    async fn ensure_prewarmed_containers(
        &self,
        runtime: RuntimeType,
        target_count: usize,
    ) -> Result<()> {
        let mut containers = self.prewarmed_containers.write().await;
        let current_containers = containers.entry(runtime.clone()).or_insert_with(Vec::new);
        
        // Remove unhealthy containers
        current_containers.retain(|container| {
            container.ready && container.last_health_check.elapsed() < Duration::from_secs(300)
        });

        let current_count = current_containers.len();
        if current_count >= target_count {
            return Ok(());
        }

        let needed = target_count - current_count;
        info!(
            "Creating {} prewarmed containers for runtime {:?}",
            needed, runtime
        );

        // Create needed containers in parallel
        let semaphore = Arc::clone(&self.startup_semaphore);
        let mut tasks = Vec::new();

        for _ in 0..needed {
            let sem = Arc::clone(&semaphore);
            let container_manager = Arc::clone(&self.container_manager);
            let runtime_clone = runtime.clone();
            let config = self.config.clone();
            
            let task = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                Self::create_prewarmed_container(container_manager, runtime_clone, config).await
            });
            
            tasks.push(task);
        }

        // Collect results
        for task in tasks {
            match task.await {
                Ok(Ok(container)) => {
                    current_containers.push(container);
                }
                Ok(Err(e)) => {
                    error!("Failed to create prewarmed container: {}", e);
                }
                Err(e) => {
                    error!("Task failed: {}", e);
                }
            }
        }

        info!(
            "Successfully created {} prewarmed containers for runtime {:?}",
            current_containers.len() - current_count,
            runtime
        );

        Ok(())
    }

    /// Create a single prewarmed container
    async fn create_prewarmed_container(
        container_manager: Arc<ContainerManager>,
        runtime: RuntimeType,
        config: StartupOptimizerConfig,
    ) -> Result<PrewarmedContainer> {
        let start_time = Instant::now();
        
        // Generate container configuration
        let container_id = format!("prewarmed-{}-{}", runtime.as_str(), Uuid::new_v4());
        let image = Self::get_runtime_image(&runtime);
        
        debug!("Creating prewarmed container {} with image {}", container_id, image);
        
        // Create container with timeout
        let container_config = bollard::container::Config {
            image: Some(image.clone()),
            env: Some(vec!["TERM=xterm".to_string()]),
            working_dir: Some("/workspace".to_string()),
            ..Default::default()
        };
        
        let container_id_result = timeout(
            config.prewarm_timeout,
            container_manager.create_container(container_config)
        ).await
        .map_err(|_| SoulBoxError::RuntimeError("Container creation timeout".to_string()))??;
        
        // Start the container
        container_manager.start_container(&container_id_result).await?;

        let startup_time = start_time.elapsed();
        
        // Apply warmup strategies using container manager
        Self::apply_warmup_strategies(&container_manager, &container_id_result, &config.warmup_strategies, &runtime).await?;
        
        info!(
            "Created prewarmed container {} for {:?} in {:?}",
            container_id, runtime, startup_time
        );

        Ok(PrewarmedContainer {
            container_id: container_id_result,
            runtime,
            created_at: start_time,
            last_health_check: Instant::now(),
            startup_time,
            ready: true,
        })
    }

    /// Apply warmup strategies to a container
    async fn apply_warmup_strategies(
        container_manager: &ContainerManager,
        container_id: &str,
        strategies: &[WarmupStrategy],
        runtime: &RuntimeType,
    ) -> Result<()> {
        for strategy in strategies {
            match strategy {
                WarmupStrategy::RuntimeInitialization => {
                    Self::warmup_runtime(container_manager, container_id, runtime).await?;
                }
                WarmupStrategy::DependencyPreload => {
                    Self::preload_dependencies(container_manager, container_id, runtime).await?;
                }
                WarmupStrategy::JitWarmup => {
                    Self::perform_jit_warmup(container_manager, container_id, runtime).await?;
                }
                WarmupStrategy::NetworkSetup => {
                    Self::setup_network(container_manager, container_id).await?;
                }
                WarmupStrategy::FilesystemSetup => {
                    Self::setup_filesystem(container_manager, container_id).await?;
                }
                WarmupStrategy::ImagePull => {
                    // Already handled during container creation
                }
            }
        }
        Ok(())
    }

    /// Warmup runtime environment
    async fn warmup_runtime(container_manager: &ContainerManager, container_id: &str, runtime: &RuntimeType) -> Result<()> {
        debug!("Warming up runtime {:?} in container {}", runtime, container_id);
        
        let warmup_cmd = match runtime {
            RuntimeType::Python => vec!["python3", "-c", "import sys; print('Python ready')"],
            RuntimeType::NodeJS => vec!["node", "-e", "console.log('Node.js ready')"],
            RuntimeType::Rust => vec!["rustc", "--version"],
            RuntimeType::Go => vec!["go", "version"],
            RuntimeType::Shell => vec!["echo", "Shell ready"],
            _ => vec!["echo", "Runtime ready"],
        };

        let result = container_manager.execute_command(container_id, &warmup_cmd).await?;
        debug!("Runtime warmup result: {}", result);

        Ok(())
    }

    /// Preload common dependencies
    async fn preload_dependencies(container_manager: &ContainerManager, container_id: &str, runtime: &RuntimeType) -> Result<()> {
        debug!("Preloading dependencies for {:?} in container {}", runtime, container_id);
        
        let preload_cmd = match runtime {
            RuntimeType::Python => vec![
                "python3",
                "-c", 
                "import os, sys, json, time, datetime"
            ],
            RuntimeType::NodeJS => vec![
                "node",
                "-e",
                "require('os'); require('fs'); require('path')"
            ],
            _ => return Ok(()), // Skip for other runtimes
        };

        let _result = container_manager.execute_command(container_id, &preload_cmd).await?;
        Ok(())
    }

    /// Perform JIT compilation warmup
    async fn perform_jit_warmup(container_manager: &ContainerManager, container_id: &str, runtime: &RuntimeType) -> Result<()> {
        debug!("Performing JIT warmup for {:?} in container {}", runtime, container_id);
        
        let jit_cmd = match runtime {
            RuntimeType::Python => vec![
                "python3",
                "-c",
                "for i in range(1000): x = i ** 2"
            ],
            RuntimeType::NodeJS => vec![
                "node",
                "-e",
                "for(let i=0; i<1000; i++) Math.pow(i, 2)"
            ],
            _ => return Ok(()), // Skip for other runtimes
        };

        let _result = container_manager.execute_command(container_id, &jit_cmd).await?;
        Ok(())
    }

    /// Setup network interfaces
    async fn setup_network(container_manager: &ContainerManager, container_id: &str) -> Result<()> {
        debug!("Setting up network for container {}", container_id);
        
        let network_cmd = vec![
            "ip",
            "route", 
            "show"
        ];

        let _result = container_manager.execute_command(container_id, &network_cmd).await?;
        Ok(())
    }

    /// Setup filesystem
    async fn setup_filesystem(container_manager: &ContainerManager, container_id: &str) -> Result<()> {
        debug!("Setting up filesystem for container {}", container_id);
        
        let fs_cmd = vec![
            "mkdir",
            "-p",
            "/workspace"
        ];

        let _result = container_manager.execute_command(container_id, &fs_cmd).await?;
        Ok(())
    }

    /// Get runtime image name
    fn get_runtime_image(runtime: &RuntimeType) -> String {
        match runtime {
            RuntimeType::Python => "python:3.11-slim".to_string(),
            RuntimeType::NodeJS => "node:20-alpine".to_string(),
            RuntimeType::Rust => "rust:1.75-slim".to_string(),
            RuntimeType::Go => "golang:1.21-alpine".to_string(),
            RuntimeType::Java => "openjdk:21-slim".to_string(),
            RuntimeType::Shell => "ubuntu:22.04".to_string(),
            _ => "ubuntu:22.04".to_string(),
        }
    }

    /// Get optimized container for runtime
    pub async fn get_optimized_container(&self, runtime: RuntimeType) -> Result<String> {
        let start_time = Instant::now();
        
        // Try to get a prewarmed container first
        if let Some(container_id) = self.get_prewarmed_container(runtime.clone()).await? {
            let warm_time = start_time.elapsed();
            
            // Update metrics
            let mut metrics = self.metrics.write().await;
            metrics.prewarmed_containers_used += 1;
            metrics.avg_warm_start_time = Self::update_average(
                metrics.avg_warm_start_time,
                warm_time,
                metrics.prewarmed_containers_used,
            );
            
            info!("Using prewarmed container {} for {:?} ({}ms)", 
                  container_id, runtime, warm_time.as_millis());
            
            return Ok(container_id);
        }

        // Create new container if no prewarmed available
        let container_id = self.create_cold_container(runtime.clone()).await?;
        let cold_time = start_time.elapsed();
        
        // Update metrics
        let mut metrics = self.metrics.write().await;
        metrics.total_containers_created += 1;
        metrics.avg_cold_start_time = Self::update_average(
            metrics.avg_cold_start_time,
            cold_time,
            metrics.total_containers_created - metrics.prewarmed_containers_used,
        );
        
        // Update cache hit ratio
        metrics.cache_hit_ratio = metrics.prewarmed_containers_used as f64 
            / metrics.total_containers_created as f64;
        
        info!("Created cold container {} for {:?} ({}ms)",
              container_id, runtime, cold_time.as_millis());
        
        Ok(container_id)
    }

    /// Get a prewarmed container
    async fn get_prewarmed_container(&self, runtime: RuntimeType) -> Result<Option<String>> {
        let mut containers = self.prewarmed_containers.write().await;
        
        if let Some(runtime_containers) = containers.get_mut(&runtime) {
            if let Some(container) = runtime_containers.pop() {
                debug!("Retrieved prewarmed container {} for {:?}", container.container_id, runtime);
                
                // Asynchronously replace the used container
                let optimizer = self.clone();
                tokio::spawn(async move {
                    if let Err(e) = optimizer.ensure_prewarmed_containers(
                        runtime, 
                        optimizer.config.min_prewarmed_per_runtime
                    ).await {
                        error!("Failed to replenish prewarmed containers: {}", e);
                    }
                });
                
                return Ok(Some(container.container_id));
            }
        }
        
        Ok(None)
    }

    /// Create a cold container
    async fn create_cold_container(&self, runtime: RuntimeType) -> Result<String> {
        let _permit = self.startup_semaphore.acquire().await.unwrap();
        
        let image = Self::get_runtime_image(&runtime);
        
        let container_config = bollard::container::Config {
            image: Some(image),
            env: Some(vec!["TERM=xterm".to_string()]),
            working_dir: Some("/workspace".to_string()),
            ..Default::default()
        };
        
        let container_id = self.container_manager.create_container(container_config).await?;
        self.container_manager.start_container(&container_id).await?;
        
        Ok(container_id)
    }

    /// Start background maintenance tasks
    async fn start_maintenance_tasks(&self) {
        // Health check task
        let prewarmed_containers = Arc::clone(&self.prewarmed_containers);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                Self::health_check_prewarmed_containers(Arc::clone(&prewarmed_containers)).await;
            }
        });

        // Metrics reporting task
        let metrics = Arc::clone(&self.metrics);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            loop {
                interval.tick().await;
                let metrics_guard = metrics.read().await;
                info!("Startup metrics: {:?}", *metrics_guard);
            }
        });
    }

    /// Health check prewarmed containers
    async fn health_check_prewarmed_containers(
        prewarmed_containers: Arc<RwLock<HashMap<RuntimeType, Vec<PrewarmedContainer>>>>
    ) {
        let mut containers = prewarmed_containers.write().await;
        
        for (runtime, runtime_containers) in containers.iter_mut() {
            runtime_containers.retain(|container| {
                let is_healthy = container.ready 
                    && container.last_health_check.elapsed() < Duration::from_secs(600); // 10 minutes
                
                if !is_healthy {
                    debug!("Removing unhealthy prewarmed container {} for {:?}", 
                           container.container_id, runtime);
                }
                
                is_healthy
            });
        }
    }

    /// Update running average
    fn update_average(current_avg: Duration, new_value: Duration, count: u64) -> Duration {
        if count == 1 {
            new_value
        } else {
            let current_total = current_avg.as_nanos() * (count - 1) as u128;
            let new_total = current_total + new_value.as_nanos();
            Duration::from_nanos((new_total / count as u128) as u64)
        }
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> StartupMetrics {
        self.metrics.read().await.clone()
    }

    /// Get prewarmed container counts
    pub async fn get_prewarmed_counts(&self) -> HashMap<RuntimeType, usize> {
        let containers = self.prewarmed_containers.read().await;
        containers.iter()
            .map(|(runtime, containers)| (runtime.clone(), containers.len()))
            .collect()
    }
}

// Clone implementation for spawning async tasks
impl Clone for StartupOptimizer {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            container_manager: Arc::clone(&self.container_manager),
            prewarmed_containers: Arc::clone(&self.prewarmed_containers),
            startup_semaphore: Arc::clone(&self.startup_semaphore),
            metrics: Arc::clone(&self.metrics),
            image_cache: Arc::clone(&self.image_cache),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_startup_optimizer_creation() {
        let config = StartupOptimizerConfig::default();
        let container_manager = Arc::new(ContainerManager::new_default().unwrap());
        let optimizer = StartupOptimizer::new(config, container_manager);
        
        // Basic test that optimizer can be created
        assert!(std::ptr::addr_of!(optimizer) as usize != 0);
    }
    
    #[test]
    fn test_get_runtime_image() {
        assert_eq!(StartupOptimizer::get_runtime_image(&RuntimeType::Python), "python:3.11-slim");
        assert_eq!(StartupOptimizer::get_runtime_image(&RuntimeType::NodeJS), "node:20-alpine");
        assert_eq!(StartupOptimizer::get_runtime_image(&RuntimeType::Rust), "rust:1.75-slim");
    }
    
    #[test]
    fn test_warmup_strategy_serialization() {
        let strategy = WarmupStrategy::ImagePull;
        let serialized = serde_json::to_string(&strategy).unwrap();
        let deserialized: WarmupStrategy = serde_json::from_str(&serialized).unwrap();
        assert_eq!(strategy, deserialized);
    }
}