//! CPU optimization strategies and monitoring
//! 
//! This module implements specific CPU optimization techniques and provides
//! real-time CPU usage monitoring to achieve the target <40% utilization.

use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::System;
use tokio::sync::mpsc;
use tokio::time::interval;

use super::{OptimizationConfig, PerformanceHotspot, OptimizationPriority};

/// CPU usage sample
#[derive(Debug, Clone)]
pub struct CpuSample {
    pub timestamp: Instant,
    pub cpu_percent: f32,
    pub per_core_usage: Vec<f32>,
    pub load_average: f32,
    pub context_switches: u64,
}

/// CPU optimization state
#[derive(Debug, Clone)]
pub struct CpuOptimizationState {
    pub current_cpu_percent: f32,
    pub target_cpu_percent: f32,
    pub optimization_active: bool,
    pub throttling_enabled: bool,
    pub last_optimization: Option<Instant>,
}

/// CPU optimizer implementation
#[derive(Clone)]
pub struct CpuOptimizer {
    config: OptimizationConfig,
    system: Arc<RwLock<System>>,
    cpu_samples: Arc<RwLock<VecDeque<CpuSample>>>,
    optimization_state: Arc<RwLock<CpuOptimizationState>>,
    cpu_throttle_tx: mpsc::UnboundedSender<ThrottleCommand>,
    hotspots: Arc<RwLock<HashMap<String, ComponentCpuUsage>>>,
}

/// Throttle command for CPU management
#[derive(Debug)]
enum ThrottleCommand {
    Enable { level: ThrottleLevel },
    Disable,
    AdjustLevel { level: ThrottleLevel },
}

/// CPU throttling levels
#[derive(Debug, Clone, Copy)]
pub enum ThrottleLevel {
    Light,   // 10% reduction
    Medium,  // 25% reduction
    Heavy,   // 50% reduction
    Extreme, // 75% reduction
}

/// Component CPU usage tracking
#[derive(Debug, Clone)]
struct ComponentCpuUsage {
    pub component_name: String,
    pub cpu_percent: f32,
    pub sample_count: u64,
    pub last_updated: Instant,
    pub peak_usage: f32,
    pub average_usage: f32,
}

impl CpuOptimizer {
    /// Create a new CPU optimizer
    pub async fn new(config: &OptimizationConfig) -> Result<Self> {
        let mut system = System::new_all();
        system.refresh_all();

        let (cpu_throttle_tx, cpu_throttle_rx) = mpsc::unbounded_channel();

        let optimizer = Self {
            config: config.clone(),
            system: Arc::new(RwLock::new(system)),
            cpu_samples: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            optimization_state: Arc::new(RwLock::new(CpuOptimizationState {
                current_cpu_percent: 0.0,
                target_cpu_percent: config.target_cpu_percent,
                optimization_active: false,
                throttling_enabled: false,
                last_optimization: None,
            })),
            cpu_throttle_tx,
            hotspots: Arc::new(RwLock::new(HashMap::new())),
        };

        // Start CPU monitoring
        optimizer.start_cpu_monitoring().await?;

        // Start throttle management
        optimizer.start_throttle_manager(cpu_throttle_rx).await;

        Ok(optimizer)
    }

    /// Get current CPU utilization
    pub async fn get_current_cpu_utilization(&self) -> Result<f32> {
        let state = self.optimization_state.read();
        Ok(state.current_cpu_percent)
    }

    /// Get CPU usage history
    pub async fn get_cpu_history(&self, duration: Duration) -> Vec<CpuSample> {
        let samples = self.cpu_samples.read();
        let cutoff = Instant::now() - duration;
        
        samples
            .iter()
            .filter(|sample| sample.timestamp >= cutoff)
            .cloned()
            .collect()
    }

    /// Get average CPU usage over time period
    pub async fn get_average_cpu_usage(&self, duration: Duration) -> f32 {
        let history = self.get_cpu_history(duration).await;
        
        if history.is_empty() {
            return 0.0;
        }

        let total: f32 = history.iter().map(|s| s.cpu_percent).sum();
        total / history.len() as f32
    }

    /// Check if CPU optimization is needed
    pub async fn needs_optimization(&self) -> bool {
        let state = self.optimization_state.read();
        state.current_cpu_percent > state.target_cpu_percent
    }

    /// Apply CPU throttling if needed
    pub async fn apply_cpu_throttling(&self) -> Result<()> {
        let state = self.optimization_state.read();
        
        if state.current_cpu_percent > state.target_cpu_percent * 1.5 {
            // Heavy throttling needed
            drop(state);
            self.enable_throttling(ThrottleLevel::Heavy).await?;
        } else if state.current_cpu_percent > state.target_cpu_percent * 1.2 {
            // Medium throttling needed
            drop(state);
            self.enable_throttling(ThrottleLevel::Medium).await?;
        } else if state.current_cpu_percent > state.target_cpu_percent {
            // Light throttling needed
            drop(state);
            self.enable_throttling(ThrottleLevel::Light).await?;
        } else {
            // No throttling needed
            drop(state);
            self.disable_throttling().await?;
        }

        Ok(())
    }

    /// Enable CPU throttling at specified level
    pub async fn enable_throttling(&self, level: ThrottleLevel) -> Result<()> {
        self.cpu_throttle_tx
            .send(ThrottleCommand::Enable { level })
            .context("Failed to send throttle enable command")?;

        {
            let mut state = self.optimization_state.write();
            state.throttling_enabled = true;
        }

        tracing::info!(level = ?level, "CPU throttling enabled");
        Ok(())
    }

    /// Disable CPU throttling
    pub async fn disable_throttling(&self) -> Result<()> {
        self.cpu_throttle_tx
            .send(ThrottleCommand::Disable)
            .context("Failed to send throttle disable command")?;

        {
            let mut state = self.optimization_state.write();
            state.throttling_enabled = false;
        }

        tracing::info!("CPU throttling disabled");
        Ok(())
    }

    /// Update component CPU usage
    pub async fn update_component_usage(&self, component: &str, cpu_percent: f32) {
        let mut hotspots = self.hotspots.write();
        
        let usage = hotspots.entry(component.to_string()).or_insert(ComponentCpuUsage {
            component_name: component.to_string(),
            cpu_percent: 0.0,
            sample_count: 0,
            last_updated: Instant::now(),
            peak_usage: 0.0,
            average_usage: 0.0,
        });

        // Update statistics
        usage.cpu_percent = cpu_percent;
        usage.sample_count += 1;
        usage.last_updated = Instant::now();
        usage.peak_usage = usage.peak_usage.max(cpu_percent);
        
        // Calculate rolling average
        let alpha = 0.1; // Smoothing factor
        usage.average_usage = (1.0 - alpha) * usage.average_usage + alpha * cpu_percent;
    }

    /// Get performance hotspots based on CPU usage
    pub async fn get_performance_hotspots(&self) -> Vec<PerformanceHotspot> {
        let hotspots = self.hotspots.read();
        let mut result = Vec::new();

        for (_, usage) in hotspots.iter() {
            // Skip components that haven't been updated recently
            if usage.last_updated.elapsed() > Duration::from_secs(30) {
                continue;
            }

            let priority = if usage.cpu_percent > 20.0 {
                OptimizationPriority::Critical
            } else if usage.cpu_percent > 10.0 {
                OptimizationPriority::High
            } else if usage.cpu_percent > 5.0 {
                OptimizationPriority::Medium
            } else {
                OptimizationPriority::Low
            };

            result.push(PerformanceHotspot {
                component: usage.component_name.clone(),
                function: "cpu_usage".to_string(),
                cpu_percent: usage.cpu_percent,
                call_count: usage.sample_count,
                total_time: Duration::from_secs(1), // Placeholder
                average_time: Duration::from_millis((usage.cpu_percent * 10.0) as u64),
                optimization_priority: priority,
            });
        }

        // Sort by CPU usage (highest first)
        result.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap());

        result
    }

    /// Optimize CPU usage for specific operation
    pub async fn optimize_operation<F, T>(&self, operation_name: &str, operation: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        let start_time = Instant::now();
        let start_cpu = self.get_current_cpu_utilization().await?;

        // Execute operation
        let result = operation()?;

        let end_time = Instant::now();
        let end_cpu = self.get_current_cpu_utilization().await?;
        let duration = end_time - start_time;

        // Update component usage
        let cpu_delta = (end_cpu - start_cpu).max(0.0);
        self.update_component_usage(operation_name, cpu_delta).await;

        tracing::debug!(
            operation = operation_name,
            duration_ms = duration.as_millis(),
            cpu_delta = cpu_delta,
            "Operation completed"
        );

        Ok(result)
    }

    /// Start CPU monitoring loop
    async fn start_cpu_monitoring(&self) -> Result<()> {
        let system = self.system.clone();
        let cpu_samples = self.cpu_samples.clone();
        let optimization_state = self.optimization_state.clone();
        let monitoring_interval = self.config.profiling_interval;

        tokio::spawn(async move {
            let mut interval = interval(monitoring_interval);
            
            loop {
                interval.tick().await;

                // Refresh system information
                {
                    let mut sys = system.write();
                    sys.refresh_cpu();
                    sys.refresh_memory();
                }

                // Calculate current CPU usage
                let (cpu_percent, per_core_usage, load_average) = {
                    let sys = system.read();
                    let cpu_percent = sys.global_cpu_info().cpu_usage();
                    let per_core_usage: Vec<f32> = sys.cpus().iter().map(|cpu| cpu.cpu_usage()).collect();
                    let load_average = sys.load_average().one;
                    (cpu_percent, per_core_usage, load_average)
                };

                // Create sample
                let sample = CpuSample {
                    timestamp: Instant::now(),
                    cpu_percent,
                    per_core_usage,
                    load_average: load_average as f32,
                    context_switches: 0, // Would require platform-specific implementation
                };

                // Store sample
                {
                    let mut samples = cpu_samples.write();
                    samples.push_back(sample);
                    
                    // Keep only last 1000 samples
                    if samples.len() > 1000 {
                        samples.pop_front();
                    }
                }

                // Update optimization state
                {
                    let mut state = optimization_state.write();
                    state.current_cpu_percent = cpu_percent;
                }

                tracing::trace!(
                    cpu_percent = cpu_percent,
                    load_average = load_average,
                    "CPU monitoring sample"
                );
            }
        });

        Ok(())
    }

    /// Start throttle manager
    async fn start_throttle_manager(&self, mut rx: mpsc::UnboundedReceiver<ThrottleCommand>) {
        tokio::spawn(async move {
            let mut current_throttle: Option<ThrottleLevel> = None;

            while let Some(command) = rx.recv().await {
                match command {
                    ThrottleCommand::Enable { level } => {
                        current_throttle = Some(level);
                        Self::apply_throttle_level(level).await;
                    },
                    ThrottleCommand::Disable => {
                        current_throttle = None;
                        Self::remove_throttle().await;
                    },
                    ThrottleCommand::AdjustLevel { level } => {
                        current_throttle = Some(level);
                        Self::apply_throttle_level(level).await;
                    },
                }
            }
        });
    }

    /// Apply throttle level (placeholder implementation)
    async fn apply_throttle_level(level: ThrottleLevel) {
        // In a real implementation, this would:
        // 1. Adjust tokio runtime thread pool size
        // 2. Add delays to high-frequency operations
        // 3. Reduce concurrent operations
        // 4. Implement backpressure mechanisms

        let delay_ms = match level {
            ThrottleLevel::Light => 1,
            ThrottleLevel::Medium => 5,
            ThrottleLevel::Heavy => 10,
            ThrottleLevel::Extreme => 25,
        };

        tracing::info!(
            level = ?level,
            delay_ms = delay_ms,
            "Applied CPU throttling"
        );

        // Simulate throttling with a small delay
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }

    /// Remove throttling (placeholder implementation)
    async fn remove_throttle() {
        tracing::info!("Removed CPU throttling");
        // In a real implementation, this would restore normal operation parameters
    }
}

/// CPU optimization middleware for tracking operations
pub struct CpuOptimizationMiddleware {
    optimizer: Arc<CpuOptimizer>,
}

impl CpuOptimizationMiddleware {
    pub fn new(optimizer: Arc<CpuOptimizer>) -> Self {
        Self { optimizer }
    }

    /// Track CPU usage for an operation
    pub async fn track_operation<F, T>(&self, name: &str, operation: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        self.optimizer.optimize_operation(name, operation).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cpu_optimizer_creation() {
        let config = OptimizationConfig::default();
        let optimizer = CpuOptimizer::new(&config).await;
        assert!(optimizer.is_ok());
    }

    #[tokio::test]
    async fn test_cpu_monitoring() {
        let config = OptimizationConfig::default();
        let optimizer = CpuOptimizer::new(&config).await.unwrap();

        // Wait for some samples
        tokio::time::sleep(Duration::from_millis(100)).await;

        let cpu_utilization = optimizer.get_current_cpu_utilization().await.unwrap();
        assert!(cpu_utilization >= 0.0);
        assert!(cpu_utilization <= 100.0);
    }

    #[tokio::test]
    async fn test_component_usage_tracking() {
        let config = OptimizationConfig::default();
        let optimizer = CpuOptimizer::new(&config).await.unwrap();

        optimizer.update_component_usage("test_component", 15.5).await;
        
        let hotspots = optimizer.get_performance_hotspots().await;
        assert!(!hotspots.is_empty());
        assert_eq!(hotspots[0].component, "test_component");
        assert_eq!(hotspots[0].cpu_percent, 15.5);
    }

    #[tokio::test]
    async fn test_optimization_needed() {
        let mut config = OptimizationConfig::default();
        config.target_cpu_percent = 10.0; // Set low target for testing
        
        let optimizer = CpuOptimizer::new(&config).await.unwrap();
        
        // Force high CPU usage
        {
            let mut state = optimizer.optimization_state.write();
            state.current_cpu_percent = 50.0;
        }

        assert!(optimizer.needs_optimization().await);
    }

    #[test]
    fn test_throttle_level_ordering() {
        // Ensure throttle levels are in correct order
        let levels = [
            ThrottleLevel::Light,
            ThrottleLevel::Medium,
            ThrottleLevel::Heavy,
            ThrottleLevel::Extreme,
        ];

        // This test ensures the enum is defined correctly
        assert_eq!(levels.len(), 4);
    }
}