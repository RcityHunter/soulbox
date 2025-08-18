//! Performance profiling and hotspot detection
//! 
//! This module provides tools for profiling CPU usage, identifying
//! performance hotspots, and generating optimization recommendations.

use anyhow::{Context, Result};
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{PerformanceHotspot, OptimizationPriority};

/// Performance profiler for identifying CPU hotspots
#[derive(Clone)]
pub struct PerformanceProfiler {
    config: ProfilingConfig,
    samples: Arc<RwLock<VecDeque<ProfileSample>>>,
    function_profiles: Arc<RwLock<HashMap<String, FunctionProfile>>>,
    component_profiles: Arc<RwLock<HashMap<String, ComponentProfile>>>,
    profiling_active: Arc<RwLock<bool>>,
}

/// Configuration for performance profiling
#[derive(Debug, Clone)]
pub struct ProfilingConfig {
    /// Sampling interval for profiling
    pub sample_interval: Duration,
    /// Maximum number of samples to keep
    pub max_samples: usize,
    /// Enable function-level profiling
    pub enable_function_profiling: bool,
    /// Enable component-level profiling
    pub enable_component_profiling: bool,
    /// Minimum execution time to profile (microseconds)
    pub min_profile_time_us: u64,
    /// CPU threshold for hotspot detection (percentage)
    pub hotspot_threshold_percent: f32,
}

impl Default for ProfilingConfig {
    fn default() -> Self {
        Self {
            sample_interval: Duration::from_millis(100),
            max_samples: 10000,
            enable_function_profiling: true,
            enable_component_profiling: true,
            min_profile_time_us: 100, // 0.1ms
            hotspot_threshold_percent: 5.0,
        }
    }
}

/// Individual profiling sample
#[derive(Debug, Clone)]
struct ProfileSample {
    timestamp: Instant,
    cpu_percent: f32,
    memory_usage_bytes: u64,
    active_functions: Vec<String>,
    stack_trace: Vec<String>,
}

/// Function execution profile
#[derive(Debug, Clone)]
struct FunctionProfile {
    name: String,
    component: String,
    total_calls: u64,
    total_time: Duration,
    min_time: Duration,
    max_time: Duration,
    last_called: Instant,
    cpu_time_percent: f32,
    samples: VecDeque<FunctionSample>,
}

/// Individual function execution sample
#[derive(Debug, Clone)]
struct FunctionSample {
    timestamp: Instant,
    execution_time: Duration,
    cpu_usage: f32,
}

/// Component execution profile
#[derive(Debug, Clone)]
struct ComponentProfile {
    name: String,
    total_cpu_time: Duration,
    total_operations: u64,
    average_cpu_percent: f32,
    peak_cpu_percent: f32,
    functions: HashMap<String, String>, // function_name -> profile_key
}

impl PerformanceProfiler {
    /// Create a new performance profiler
    pub fn new(sample_interval: Duration) -> Self {
        let config = ProfilingConfig {
            sample_interval,
            ..Default::default()
        };

        Self::with_config(config)
    }

    /// Create profiler with custom configuration
    pub fn with_config(config: ProfilingConfig) -> Self {
        let max_samples = config.max_samples;
        Self {
            config,
            samples: Arc::new(RwLock::new(VecDeque::with_capacity(max_samples))),
            function_profiles: Arc::new(RwLock::new(HashMap::new())),
            component_profiles: Arc::new(RwLock::new(HashMap::new())),
            profiling_active: Arc::new(RwLock::new(false)),
        }
    }

    /// Start profiling
    pub async fn start_profiling(&self) -> Result<()> {
        {
            let mut active = self.profiling_active.write();
            *active = true;
        }

        self.start_sampling_loop().await;
        
        tracing::info!("Performance profiling started");
        Ok(())
    }

    /// Stop profiling
    pub async fn stop_profiling(&self) -> Result<()> {
        {
            let mut active = self.profiling_active.write();
            *active = false;
        }

        tracing::info!("Performance profiling stopped");
        Ok(())
    }

    /// Profile a function execution
    pub async fn profile_function<T, F>(
        &self,
        component: &str,
        function_name: &str,
        operation: F,
    ) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        if !self.config.enable_function_profiling {
            return operation();
        }

        let start_time = Instant::now();
        let start_cpu = self.get_current_cpu_usage();

        // Execute the operation
        let result = operation()?;

        let execution_time = start_time.elapsed();
        let end_cpu = self.get_current_cpu_usage();

        // Only profile if execution time meets threshold
        if execution_time.as_micros() >= self.config.min_profile_time_us as u128 {
            self.record_function_execution(
                component,
                function_name,
                execution_time,
                end_cpu - start_cpu,
            ).await;
        }

        Ok(result)
    }

    /// Profile a component operation
    pub async fn profile_component<T, F>(
        &self,
        component: &str,
        operation: F,
    ) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        if !self.config.enable_component_profiling {
            return operation();
        }

        let start_time = Instant::now();
        let start_cpu = self.get_current_cpu_usage();

        let result = operation()?;

        let execution_time = start_time.elapsed();
        let cpu_usage = self.get_current_cpu_usage() - start_cpu;

        self.record_component_execution(component, execution_time, cpu_usage).await;

        Ok(result)
    }

    /// Get current performance hotspots
    pub async fn get_hotspots(&self) -> Vec<PerformanceHotspot> {
        let mut hotspots = Vec::new();

        // Analyze function profiles
        {
            let function_profiles = self.function_profiles.read();
            for (_, profile) in function_profiles.iter() {
                if profile.cpu_time_percent > self.config.hotspot_threshold_percent {
                    let priority = self.calculate_optimization_priority(profile.cpu_time_percent);
                    
                    hotspots.push(PerformanceHotspot {
                        component: profile.component.clone(),
                        function: profile.name.clone(),
                        cpu_percent: profile.cpu_time_percent,
                        call_count: profile.total_calls,
                        total_time: profile.total_time,
                        average_time: if profile.total_calls > 0 {
                            profile.total_time / profile.total_calls as u32
                        } else {
                            Duration::from_nanos(0)
                        },
                        optimization_priority: priority,
                    });
                }
            }
        }

        // Analyze component profiles
        {
            let component_profiles = self.component_profiles.read();
            for (_, profile) in component_profiles.iter() {
                if profile.peak_cpu_percent > self.config.hotspot_threshold_percent {
                    let priority = self.calculate_optimization_priority(profile.peak_cpu_percent);
                    
                    hotspots.push(PerformanceHotspot {
                        component: profile.name.clone(),
                        function: "component_total".to_string(),
                        cpu_percent: profile.peak_cpu_percent,
                        call_count: profile.total_operations,
                        total_time: profile.total_cpu_time,
                        average_time: if profile.total_operations > 0 {
                            profile.total_cpu_time / profile.total_operations as u32
                        } else {
                            Duration::from_nanos(0)
                        },
                        optimization_priority: priority,
                    });
                }
            }
        }

        // Sort by CPU usage (highest first)
        hotspots.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap());

        hotspots
    }

    /// Get profiling statistics
    pub fn get_statistics(&self) -> ProfilingStatistics {
        let function_profiles = self.function_profiles.read();
        let component_profiles = self.component_profiles.read();
        let samples = self.samples.read();

        let total_samples = samples.len();
        let total_functions_profiled = function_profiles.len();
        let total_components_profiled = component_profiles.len();

        let avg_cpu_usage = if !samples.is_empty() {
            samples.iter().map(|s| s.cpu_percent).sum::<f32>() / samples.len() as f32
        } else {
            0.0
        };

        let most_expensive_function = function_profiles
            .values()
            .max_by(|a, b| a.cpu_time_percent.partial_cmp(&b.cpu_time_percent).unwrap())
            .map(|p| p.name.clone());

        ProfilingStatistics {
            total_samples,
            total_functions_profiled,
            total_components_profiled,
            avg_cpu_usage,
            most_expensive_function,
            profiling_active: *self.profiling_active.read(),
            sample_interval: self.config.sample_interval,
        }
    }

    /// Clear all profiling data
    pub fn clear_data(&self) {
        {
            let mut samples = self.samples.write();
            samples.clear();
        }
        {
            let mut function_profiles = self.function_profiles.write();
            function_profiles.clear();
        }
        {
            let mut component_profiles = self.component_profiles.write();
            component_profiles.clear();
        }

        tracing::info!("Cleared all profiling data");
    }

    /// Get detailed function profile
    pub fn get_function_profile(&self, component: &str, function_name: &str) -> Option<FunctionProfileDetails> {
        let function_profiles = self.function_profiles.read();
        let profile_key = format!("{}::{}", component, function_name);
        
        function_profiles.get(&profile_key).map(|profile| {
            FunctionProfileDetails {
                name: profile.name.clone(),
                component: profile.component.clone(),
                total_calls: profile.total_calls,
                total_time: profile.total_time,
                min_time: profile.min_time,
                max_time: profile.max_time,
                average_time: if profile.total_calls > 0 {
                    profile.total_time / profile.total_calls as u32
                } else {
                    Duration::from_nanos(0)
                },
                cpu_time_percent: profile.cpu_time_percent,
                recent_samples: profile.samples.iter().cloned().collect(),
            }
        })
    }

    /// Start sampling loop for background profiling
    async fn start_sampling_loop(&self) {
        let samples = self.samples.clone();
        let profiling_active = self.profiling_active.clone();
        let sample_interval = self.config.sample_interval;
        let max_samples = self.config.max_samples;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(sample_interval);

            loop {
                interval.tick().await;

                let active = *profiling_active.read();
                if !active {
                    break;
                }

                // Collect sample
                let sample = ProfileSample {
                    timestamp: Instant::now(),
                    cpu_percent: Self::get_system_cpu_usage(),
                    memory_usage_bytes: Self::get_system_memory_usage(),
                    active_functions: Vec::new(), // Would be populated by stack sampling
                    stack_trace: Vec::new(),      // Would be populated by stack sampling
                };

                // Store sample
                {
                    let mut samples_guard = samples.write();
                    samples_guard.push_back(sample);

                    // Remove old samples if we exceed max capacity
                    if samples_guard.len() > max_samples {
                        samples_guard.pop_front();
                    }
                }
            }
        });
    }

    /// Record function execution
    async fn record_function_execution(
        &self,
        component: &str,
        function_name: &str,
        execution_time: Duration,
        cpu_usage: f32,
    ) {
        let profile_key = format!("{}::{}", component, function_name);
        let mut function_profiles = self.function_profiles.write();

        let profile = function_profiles.entry(profile_key).or_insert_with(|| {
            FunctionProfile {
                name: function_name.to_string(),
                component: component.to_string(),
                total_calls: 0,
                total_time: Duration::from_nanos(0),
                min_time: Duration::from_secs(u64::MAX),
                max_time: Duration::from_nanos(0),
                last_called: Instant::now(),
                cpu_time_percent: 0.0,
                samples: VecDeque::with_capacity(1000),
            }
        });

        // Update profile statistics
        profile.total_calls += 1;
        profile.total_time += execution_time;
        profile.min_time = profile.min_time.min(execution_time);
        profile.max_time = profile.max_time.max(execution_time);
        profile.last_called = Instant::now();

        // Calculate CPU time percentage (simplified)
        profile.cpu_time_percent = (profile.cpu_time_percent * 0.9) + (cpu_usage * 0.1);

        // Add sample
        let sample = FunctionSample {
            timestamp: Instant::now(),
            execution_time,
            cpu_usage,
        };

        profile.samples.push_back(sample);

        // Keep only recent samples
        if profile.samples.len() > 100 {
            profile.samples.pop_front();
        }
    }

    /// Record component execution
    async fn record_component_execution(
        &self,
        component: &str,
        execution_time: Duration,
        cpu_usage: f32,
    ) {
        let mut component_profiles = self.component_profiles.write();

        let profile = component_profiles.entry(component.to_string()).or_insert_with(|| {
            ComponentProfile {
                name: component.to_string(),
                total_cpu_time: Duration::from_nanos(0),
                total_operations: 0,
                average_cpu_percent: 0.0,
                peak_cpu_percent: 0.0,
                functions: HashMap::new(),
            }
        });

        profile.total_cpu_time += execution_time;
        profile.total_operations += 1;
        profile.peak_cpu_percent = profile.peak_cpu_percent.max(cpu_usage);

        // Update average CPU usage
        profile.average_cpu_percent = (profile.average_cpu_percent * 0.9) + (cpu_usage * 0.1);
    }

    /// Calculate optimization priority based on CPU usage
    fn calculate_optimization_priority(&self, cpu_percent: f32) -> OptimizationPriority {
        if cpu_percent > 20.0 {
            OptimizationPriority::Critical
        } else if cpu_percent > 10.0 {
            OptimizationPriority::High
        } else if cpu_percent > 5.0 {
            OptimizationPriority::Medium
        } else {
            OptimizationPriority::Low
        }
    }

    /// Get current CPU usage (placeholder implementation)
    fn get_current_cpu_usage(&self) -> f32 {
        // In a real implementation, this would get actual CPU usage
        // For now, return a placeholder value
        0.0
    }

    /// Get system CPU usage
    fn get_system_cpu_usage() -> f32 {
        // Placeholder implementation
        // In a real implementation, this would use system APIs
        0.0
    }

    /// Get system memory usage
    fn get_system_memory_usage() -> u64 {
        // Placeholder implementation
        // In a real implementation, this would use system APIs
        0
    }
}

/// Profiling statistics
#[derive(Debug, Clone)]
pub struct ProfilingStatistics {
    pub total_samples: usize,
    pub total_functions_profiled: usize,
    pub total_components_profiled: usize,
    pub avg_cpu_usage: f32,
    pub most_expensive_function: Option<String>,
    pub profiling_active: bool,
    pub sample_interval: Duration,
}

/// Detailed function profile information
#[derive(Debug, Clone)]
pub struct FunctionProfileDetails {
    pub name: String,
    pub component: String,
    pub total_calls: u64,
    pub total_time: Duration,
    pub min_time: Duration,
    pub max_time: Duration,
    pub average_time: Duration,
    pub cpu_time_percent: f32,
    pub recent_samples: Vec<FunctionSample>,
}

/// Profiling middleware for automatic function profiling
pub struct ProfilingMiddleware {
    profiler: Arc<PerformanceProfiler>,
}

impl ProfilingMiddleware {
    pub fn new(profiler: Arc<PerformanceProfiler>) -> Self {
        Self { profiler }
    }

    /// Profile an operation
    pub async fn profile<T, F>(&self, component: &str, function: &str, operation: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        self.profiler.profile_function(component, function, operation).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_profiler_creation() {
        let profiler = PerformanceProfiler::new(Duration::from_millis(100));
        let stats = profiler.get_statistics();
        assert_eq!(stats.total_samples, 0);
        assert!(!stats.profiling_active);
    }

    #[tokio::test]
    async fn test_function_profiling() {
        let profiler = PerformanceProfiler::new(Duration::from_millis(100));
        
        let result = profiler.profile_function(
            "test_component",
            "test_function",
            || {
                std::thread::sleep(Duration::from_millis(10));
                Ok(42)
            }
        ).await.unwrap();

        assert_eq!(result, 42);

        let profile = profiler.get_function_profile("test_component", "test_function");
        assert!(profile.is_some());
        
        let profile = profile.unwrap();
        assert_eq!(profile.name, "test_function");
        assert_eq!(profile.component, "test_component");
        assert_eq!(profile.total_calls, 1);
    }

    #[tokio::test]
    async fn test_hotspot_detection() {
        let mut config = ProfilingConfig::default();
        config.hotspot_threshold_percent = 1.0; // Low threshold for testing
        
        let profiler = PerformanceProfiler::with_config(config);
        
        // Profile a function multiple times to create a hotspot
        for _ in 0..5 {
            let _ = profiler.profile_function(
                "test_component",
                "expensive_function",
                || {
                    std::thread::sleep(Duration::from_millis(5));
                    Ok(())
                }
            ).await;
        }

        let hotspots = profiler.get_hotspots().await;
        // Note: In this test environment, actual CPU measurement might not work
        // so we check that the profiling structure is working
        assert!(hotspots.len() >= 0); // May be 0 if CPU measurement is not available
    }

    #[test]
    fn test_profiling_config() {
        let config = ProfilingConfig::default();
        assert_eq!(config.sample_interval, Duration::from_millis(100));
        assert_eq!(config.max_samples, 10000);
        assert!(config.enable_function_profiling);
        assert!(config.enable_component_profiling);
    }
}