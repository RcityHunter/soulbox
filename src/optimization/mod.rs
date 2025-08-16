//! CPU optimization and performance enhancement module
//! 
//! This module provides tools and strategies to optimize CPU usage across
//! the SoulBox system, targeting the goal of reducing CPU utilization to <40%.

pub mod cpu_optimizer;
pub mod lazy_eval;
pub mod serialization;
pub mod allocation_pool;
pub mod profiling;

use anyhow::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// CPU optimization configuration
#[derive(Debug, Clone)]
pub struct OptimizationConfig {
    /// Target CPU utilization percentage
    pub target_cpu_percent: f32,
    /// Profiling sample interval
    pub profiling_interval: Duration,
    /// Enable lazy evaluation optimizations
    pub enable_lazy_eval: bool,
    /// Enable allocation pooling
    pub enable_allocation_pooling: bool,
    /// Enable serialization optimizations
    pub enable_serialization_opt: bool,
    /// Maximum number of concurrent optimizations
    pub max_concurrent_optimizations: usize,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            target_cpu_percent: 40.0,
            profiling_interval: Duration::from_secs(5),
            enable_lazy_eval: true,
            enable_allocation_pooling: true,
            enable_serialization_opt: true,
            max_concurrent_optimizations: 4,
        }
    }
}

/// Performance hotspot information
#[derive(Debug, Clone)]
pub struct PerformanceHotspot {
    pub component: String,
    pub function: String,
    pub cpu_percent: f32,
    pub call_count: u64,
    pub total_time: Duration,
    pub average_time: Duration,
    pub optimization_priority: OptimizationPriority,
}

/// Optimization priority levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OptimizationPriority {
    Critical,
    High,
    Medium,
    Low,
}

/// Optimization strategy result
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    pub strategy: String,
    pub before_cpu_percent: f32,
    pub after_cpu_percent: f32,
    pub improvement_percent: f32,
    pub success: bool,
    pub error: Option<String>,
}

/// Main optimization manager
pub struct OptimizationManager {
    config: OptimizationConfig,
    cpu_optimizer: cpu_optimizer::CpuOptimizer,
    lazy_evaluator: lazy_eval::LazyEvaluator,
    serialization_optimizer: serialization::SerializationOptimizer,
    allocation_pool: allocation_pool::AllocationPool,
    profiler: profiling::PerformanceProfiler,
    hotspots: Arc<RwLock<Vec<PerformanceHotspot>>>,
    optimization_results: Arc<RwLock<Vec<OptimizationResult>>>,
    optimization_tx: mpsc::UnboundedSender<OptimizationTask>,
}

/// Optimization task
#[derive(Debug)]
struct OptimizationTask {
    hotspot: PerformanceHotspot,
    strategy: OptimizationStrategy,
}

/// Available optimization strategies
#[derive(Debug, Clone)]
pub enum OptimizationStrategy {
    ReduceAllocations,
    OptimizeSerialization,
    EnableLazyEvaluation,
    PoolResources,
    CacheResults,
    ParallelizeOperation,
    ReduceContextSwitching,
}

impl OptimizationManager {
    /// Create a new optimization manager
    pub async fn new(config: OptimizationConfig) -> Result<Self> {
        let cpu_optimizer = cpu_optimizer::CpuOptimizer::new(&config).await?;
        let lazy_evaluator = lazy_eval::LazyEvaluator::new();
        let serialization_optimizer = serialization::SerializationOptimizer::new();
        let allocation_pool = allocation_pool::AllocationPool::new(1000)?; // 1000 initial objects
        let profiler = profiling::PerformanceProfiler::new(config.profiling_interval);

        let (optimization_tx, optimization_rx) = mpsc::unbounded_channel();

        let manager = Self {
            config: config.clone(),
            cpu_optimizer,
            lazy_evaluator,
            serialization_optimizer,
            allocation_pool,
            profiler,
            hotspots: Arc::new(RwLock::new(Vec::new())),
            optimization_results: Arc::new(RwLock::new(Vec::new())),
            optimization_tx,
        };

        // Start optimization worker
        manager.start_optimization_worker(optimization_rx).await;

        Ok(manager)
    }

    /// Start the optimization process
    pub async fn start_optimization(&self) -> Result<()> {
        tracing::info!("Starting CPU optimization process");

        // Start profiling
        self.profiler.start_profiling().await?;

        // Start continuous optimization loop
        let hotspots = self.hotspots.clone();
        let optimization_tx = self.optimization_tx.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.profiling_interval);
            loop {
                interval.tick().await;

                // Read current hotspots
                let current_hotspots = {
                    let hotspots_guard = hotspots.read();
                    hotspots_guard.clone()
                };

                // Queue optimization tasks for high-priority hotspots
                for hotspot in current_hotspots {
                    if hotspot.optimization_priority >= OptimizationPriority::High
                        && hotspot.cpu_percent > config.target_cpu_percent / 4.0 
                    {
                        let strategy = Self::select_optimization_strategy(&hotspot);
                        let task = OptimizationTask { hotspot, strategy };
                        
                        if let Err(e) = optimization_tx.send(task) {
                            tracing::error!("Failed to queue optimization task: {}", e);
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Update performance hotspots from profiling data
    pub async fn update_hotspots(&self, new_hotspots: Vec<PerformanceHotspot>) {
        let mut hotspots = self.hotspots.write();
        *hotspots = new_hotspots;

        tracing::debug!(
            hotspot_count = hotspots.len(),
            "Updated performance hotspots"
        );
    }

    /// Get current CPU utilization
    pub async fn get_current_cpu_utilization(&self) -> Result<f32> {
        self.cpu_optimizer.get_current_cpu_utilization().await
    }

    /// Get optimization results
    pub async fn get_optimization_results(&self) -> Vec<OptimizationResult> {
        let results = self.optimization_results.read();
        results.clone()
    }

    /// Get current performance hotspots
    pub async fn get_hotspots(&self) -> Vec<PerformanceHotspot> {
        let hotspots = self.hotspots.read();
        hotspots.clone()
    }

    /// Apply specific optimization strategy
    pub async fn apply_optimization(
        &self,
        hotspot: &PerformanceHotspot,
        strategy: OptimizationStrategy,
    ) -> Result<OptimizationResult> {
        let before_cpu = self.get_current_cpu_utilization().await?;

        let result = match strategy {
            OptimizationStrategy::ReduceAllocations => {
                self.apply_allocation_optimization(hotspot).await
            },
            OptimizationStrategy::OptimizeSerialization => {
                self.apply_serialization_optimization(hotspot).await
            },
            OptimizationStrategy::EnableLazyEvaluation => {
                self.apply_lazy_evaluation_optimization(hotspot).await
            },
            OptimizationStrategy::PoolResources => {
                self.apply_resource_pooling_optimization(hotspot).await
            },
            OptimizationStrategy::CacheResults => {
                self.apply_result_caching_optimization(hotspot).await
            },
            OptimizationStrategy::ParallelizeOperation => {
                self.apply_parallelization_optimization(hotspot).await
            },
            OptimizationStrategy::ReduceContextSwitching => {
                self.apply_context_switching_optimization(hotspot).await
            },
        };

        // Wait a moment for the optimization to take effect
        tokio::time::sleep(Duration::from_millis(100)).await;

        let after_cpu = self.get_current_cpu_utilization().await?;
        let improvement = if before_cpu > 0.0 {
            ((before_cpu - after_cpu) / before_cpu) * 100.0
        } else {
            0.0
        };

        let optimization_result = OptimizationResult {
            strategy: format!("{:?}", strategy),
            before_cpu_percent: before_cpu,
            after_cpu_percent: after_cpu,
            improvement_percent: improvement,
            success: result.is_ok(),
            error: result.err().map(|e| e.to_string()),
        };

        // Store result
        {
            let mut results = self.optimization_results.write();
            results.push(optimization_result.clone());
            
            // Keep only the last 100 results
            if results.len() > 100 {
                results.drain(0..results.len() - 100);
            }
        }

        tracing::info!(
            strategy = ?strategy,
            component = hotspot.component,
            improvement_percent = improvement,
            success = optimization_result.success,
            "Applied optimization strategy"
        );

        Ok(optimization_result)
    }

    /// Select the best optimization strategy for a given hotspot
    fn select_optimization_strategy(hotspot: &PerformanceHotspot) -> OptimizationStrategy {
        // Choose strategy based on component and function patterns
        match hotspot.component.as_str() {
            "serialization" | "json" | "protobuf" => OptimizationStrategy::OptimizeSerialization,
            "container" | "docker" => OptimizationStrategy::PoolResources,
            "filesystem" | "fs" => OptimizationStrategy::CacheResults,
            "network" | "grpc" | "websocket" => OptimizationStrategy::ReduceContextSwitching,
            "computation" | "execution" => OptimizationStrategy::ParallelizeOperation,
            _ => {
                // Choose based on function patterns
                if hotspot.function.contains("alloc") || hotspot.function.contains("new") {
                    OptimizationStrategy::ReduceAllocations
                } else if hotspot.call_count > 1000 {
                    OptimizationStrategy::EnableLazyEvaluation
                } else {
                    OptimizationStrategy::CacheResults
                }
            }
        }
    }

    /// Start the optimization worker task
    async fn start_optimization_worker(&self, mut rx: mpsc::UnboundedReceiver<OptimizationTask>) {
        let self_clone = self.clone_for_worker();
        
        tokio::spawn(async move {
            let mut active_optimizations = 0;
            
            while let Some(task) = rx.recv().await {
                if active_optimizations >= self_clone.config.max_concurrent_optimizations {
                    tracing::debug!("Skipping optimization task due to concurrency limit");
                    continue;
                }

                active_optimizations += 1;
                let self_inner = self_clone.clone();
                
                tokio::spawn(async move {
                    match self_inner.apply_optimization(&task.hotspot, task.strategy).await {
                        Ok(result) => {
                            tracing::info!(
                                component = task.hotspot.component,
                                strategy = result.strategy,
                                improvement = result.improvement_percent,
                                "Optimization completed"
                            );
                        },
                        Err(e) => {
                            tracing::error!(
                                component = task.hotspot.component,
                                error = %e,
                                "Optimization failed"
                            );
                        }
                    }
                });
                
                active_optimizations -= 1;
            }
        });
    }

    /// Clone self for worker tasks (simplified version with only necessary fields)
    fn clone_for_worker(&self) -> Self {
        Self {
            config: self.config.clone(),
            cpu_optimizer: self.cpu_optimizer.clone(),
            lazy_evaluator: self.lazy_evaluator.clone(),
            serialization_optimizer: self.serialization_optimizer.clone(),
            allocation_pool: self.allocation_pool.clone(),
            profiler: self.profiler.clone(),
            hotspots: self.hotspots.clone(),
            optimization_results: self.optimization_results.clone(),
            optimization_tx: self.optimization_tx.clone(),
        }
    }

    // Optimization strategy implementations
    async fn apply_allocation_optimization(&self, hotspot: &PerformanceHotspot) -> Result<()> {
        tracing::debug!("Applying allocation optimization for {}", hotspot.component);
        self.allocation_pool.optimize_for_component(&hotspot.component).await
    }

    async fn apply_serialization_optimization(&self, hotspot: &PerformanceHotspot) -> Result<()> {
        tracing::debug!("Applying serialization optimization for {}", hotspot.component);
        self.serialization_optimizer.optimize_component(&hotspot.component).await
    }

    async fn apply_lazy_evaluation_optimization(&self, hotspot: &PerformanceHotspot) -> Result<()> {
        tracing::debug!("Applying lazy evaluation optimization for {}", hotspot.component);
        self.lazy_evaluator.enable_for_component(&hotspot.component).await
    }

    async fn apply_resource_pooling_optimization(&self, hotspot: &PerformanceHotspot) -> Result<()> {
        tracing::debug!("Applying resource pooling optimization for {}", hotspot.component);
        // Implementation would create resource pools for the specific component
        Ok(())
    }

    async fn apply_result_caching_optimization(&self, hotspot: &PerformanceHotspot) -> Result<()> {
        tracing::debug!("Applying result caching optimization for {}", hotspot.component);
        // Implementation would enable result caching for the specific component
        Ok(())
    }

    async fn apply_parallelization_optimization(&self, hotspot: &PerformanceHotspot) -> Result<()> {
        tracing::debug!("Applying parallelization optimization for {}", hotspot.component);
        // Implementation would parallelize operations in the specific component
        Ok(())
    }

    async fn apply_context_switching_optimization(&self, hotspot: &PerformanceHotspot) -> Result<()> {
        tracing::debug!("Applying context switching optimization for {}", hotspot.component);
        // Implementation would reduce context switching for the specific component
        Ok(())
    }
}

/// Helper trait for components to report their CPU usage
#[async_trait::async_trait]
pub trait CpuProfileable {
    async fn get_cpu_hotspots(&self) -> Result<Vec<PerformanceHotspot>>;
    async fn apply_optimization(&mut self, strategy: OptimizationStrategy) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_optimization_config_default() {
        let config = OptimizationConfig::default();
        assert_eq!(config.target_cpu_percent, 40.0);
        assert!(config.enable_lazy_eval);
        assert!(config.enable_allocation_pooling);
        assert!(config.enable_serialization_opt);
    }

    #[test]
    fn test_optimization_priority_ordering() {
        assert!(OptimizationPriority::Critical > OptimizationPriority::High);
        assert!(OptimizationPriority::High > OptimizationPriority::Medium);
        assert!(OptimizationPriority::Medium > OptimizationPriority::Low);
    }

    #[test]
    fn test_select_optimization_strategy() {
        let hotspot = PerformanceHotspot {
            component: "serialization".to_string(),
            function: "serde_json::to_string".to_string(),
            cpu_percent: 15.0,
            call_count: 1000,
            total_time: Duration::from_millis(500),
            average_time: Duration::from_micros(500),
            optimization_priority: OptimizationPriority::High,
        };

        let strategy = OptimizationManager::select_optimization_strategy(&hotspot);
        assert!(matches!(strategy, OptimizationStrategy::OptimizeSerialization));
    }
}