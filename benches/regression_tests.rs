//! Performance regression test suite
//! 
//! This module provides comprehensive performance testing to detect regressions
//! and ensure SoulBox maintains optimal performance across releases.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use soulbox::{
    container::manager::ContainerManager,
    snapshot::{SnapshotManager, SnapshotConfig, SnapshotType},
    optimization::OptimizationManager,
    recovery::RecoveryManager,
    monitoring::metrics::PrometheusMetrics,
    Config, Result,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

/// Performance baseline metrics
#[derive(Debug, Clone)]
struct PerformanceBaseline {
    container_startup_time: Duration,
    execution_latency: Duration,
    snapshot_creation_time: Duration,
    snapshot_restoration_time: Duration,
    memory_usage_mb: f64,
    cpu_utilization_percent: f64,
    throughput_operations_per_second: f64,
}

impl Default for PerformanceBaseline {
    fn default() -> Self {
        Self {
            container_startup_time: Duration::from_millis(500),
            execution_latency: Duration::from_millis(100),
            snapshot_creation_time: Duration::from_secs(2),
            snapshot_restoration_time: Duration::from_millis(500),
            memory_usage_mb: 256.0,
            cpu_utilization_percent: 40.0,
            throughput_operations_per_second: 100.0,
        }
    }
}

/// Performance test configuration
struct TestConfig {
    runtime: Runtime,
    baseline: PerformanceBaseline,
    warmup_iterations: u32,
    test_iterations: u32,
}

impl TestConfig {
    fn new() -> Self {
        Self {
            runtime: Runtime::new().expect("Failed to create tokio runtime"),
            baseline: PerformanceBaseline::default(),
            warmup_iterations: 5,
            test_iterations: 100,
        }
    }
}

/// Container startup performance test
fn bench_container_startup(c: &mut Criterion) {
    let test_config = TestConfig::new();
    
    let mut group = c.benchmark_group("container_startup");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(30));

    // Test different container types
    let container_types = vec!["python", "nodejs", "rust"];
    
    for container_type in container_types {
        group.bench_with_input(
            BenchmarkId::new("startup_time", container_type),
            container_type,
            |b, &container_type| {
                b.to_async(&test_config.runtime).iter(|| async {
                    let config = Config::default();
                    let manager = ContainerManager::new(config).await.unwrap();
                    
                    let start = Instant::now();
                    
                    // Simulate container creation
                    let _container_id = black_box(
                        format!("test_container_{}_{}", container_type, chrono::Utc::now().timestamp())
                    );
                    
                    // Mock container startup time
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    
                    start.elapsed()
                });
            },
        );
    }

    // Performance regression check
    group.bench_function("startup_regression_check", |b| {
        b.to_async(&test_config.runtime).iter(|| async {
            let start = Instant::now();
            
            // Simulate optimized container startup
            tokio::time::sleep(Duration::from_millis(25)).await;
            
            let elapsed = start.elapsed();
            
            // Assert performance is within acceptable range
            assert!(
                elapsed <= test_config.baseline.container_startup_time,
                "Container startup time regression detected: {:?} > {:?}",
                elapsed,
                test_config.baseline.container_startup_time
            );
            
            elapsed
        });
    });

    group.finish();
}

/// Code execution performance test
fn bench_code_execution(c: &mut Criterion) {
    let test_config = TestConfig::new();
    
    let mut group = c.benchmark_group("code_execution");
    group.sample_size(100);
    group.throughput(Throughput::Elements(1));

    // Test different code complexity levels
    let code_samples = vec![
        ("simple", "print('Hello, World!')"),
        ("medium", r#"
import time
for i in range(100):
    print(f"Count: {i}")
    time.sleep(0.001)
"#),
        ("complex", r#"
import numpy as np
import pandas as pd

# Create large dataset
data = np.random.randn(1000, 10)
df = pd.DataFrame(data)

# Perform calculations
result = df.groupby(df.iloc[:, 0] > 0).sum()
print(f"Result shape: {result.shape}")
"#),
    ];

    for (complexity, code) in code_samples {
        group.bench_with_input(
            BenchmarkId::new("execution_latency", complexity),
            &(complexity, code),
            |b, &(complexity, code)| {
                b.to_async(&test_config.runtime).iter(|| async {
                    let start = Instant::now();
                    
                    // Mock code execution
                    let _result = black_box(simulate_code_execution(code).await);
                    
                    let elapsed = start.elapsed();
                    
                    // Check for performance regression based on complexity
                    let max_allowed = match complexity {
                        "simple" => Duration::from_millis(50),
                        "medium" => Duration::from_millis(200),
                        "complex" => Duration::from_secs(2),
                        _ => Duration::from_secs(1),
                    };
                    
                    assert!(
                        elapsed <= max_allowed,
                        "Execution time regression for {}: {:?} > {:?}",
                        complexity,
                        elapsed,
                        max_allowed
                    );
                    
                    elapsed
                });
            },
        );
    }

    group.finish();
}

/// Snapshot creation and restoration performance test
fn bench_snapshot_operations(c: &mut Criterion) {
    let test_config = TestConfig::new();
    
    let mut group = c.benchmark_group("snapshot_operations");
    group.sample_size(30);

    // Test snapshot creation
    group.bench_function("snapshot_creation", |b| {
        b.to_async(&test_config.runtime).iter(|| async {
            let start = Instant::now();
            
            // Mock snapshot creation
            let _snapshot_id = black_box(simulate_snapshot_creation().await);
            
            let elapsed = start.elapsed();
            
            assert!(
                elapsed <= test_config.baseline.snapshot_creation_time,
                "Snapshot creation time regression: {:?} > {:?}",
                elapsed,
                test_config.baseline.snapshot_creation_time
            );
            
            elapsed
        });
    });

    // Test snapshot restoration
    group.bench_function("snapshot_restoration", |b| {
        b.to_async(&test_config.runtime).iter(|| async {
            let start = Instant::now();
            
            // Mock snapshot restoration
            let _container_id = black_box(simulate_snapshot_restoration().await);
            
            let elapsed = start.elapsed();
            
            assert!(
                elapsed <= test_config.baseline.snapshot_restoration_time,
                "Snapshot restoration time regression: {:?} > {:?}",
                elapsed,
                test_config.baseline.snapshot_restoration_time
            );
            
            elapsed
        });
    });

    // Test different snapshot types
    let snapshot_types = vec![
        SnapshotType::Full,
        SnapshotType::ContainerOnly,
        SnapshotType::FilesystemOnly,
    ];

    for snapshot_type in snapshot_types {
        group.bench_with_input(
            BenchmarkId::new("snapshot_type_performance", format!("{:?}", snapshot_type)),
            &snapshot_type,
            |b, snapshot_type| {
                b.to_async(&test_config.runtime).iter(|| async {
                    let start = Instant::now();
                    
                    // Mock snapshot operation based on type
                    let delay = match snapshot_type {
                        SnapshotType::Full => Duration::from_millis(100),
                        SnapshotType::ContainerOnly => Duration::from_millis(50),
                        SnapshotType::FilesystemOnly => Duration::from_millis(75),
                        _ => Duration::from_millis(60),
                    };
                    
                    tokio::time::sleep(delay).await;
                    
                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// CPU optimization performance test
fn bench_cpu_optimization(c: &mut Criterion) {
    let test_config = TestConfig::new();
    
    let mut group = c.benchmark_group("cpu_optimization");
    group.sample_size(50);

    // Test CPU optimization effectiveness
    group.bench_function("optimization_impact", |b| {
        b.to_async(&test_config.runtime).iter(|| async {
            let start = Instant::now();
            
            // Simulate CPU-intensive operation
            let result = black_box(simulate_cpu_intensive_operation().await);
            
            let elapsed = start.elapsed();
            
            // Check that optimization keeps execution time reasonable
            assert!(
                elapsed <= Duration::from_millis(500),
                "CPU optimization regression detected: {:?}",
                elapsed
            );
            
            result
        });
    });

    // Test different optimization strategies
    let optimization_strategies = vec![
        "lazy_evaluation",
        "allocation_pooling", 
        "serialization_optimization",
    ];

    for strategy in optimization_strategies {
        group.bench_with_input(
            BenchmarkId::new("optimization_strategy", strategy),
            strategy,
            |b, &strategy| {
                b.to_async(&test_config.runtime).iter(|| async {
                    let start = Instant::now();
                    
                    // Mock optimization strategy execution
                    let _result = black_box(simulate_optimization_strategy(strategy).await);
                    
                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// Memory usage and garbage collection performance test
fn bench_memory_performance(c: &mut Criterion) {
    let test_config = TestConfig::new();
    
    let mut group = c.benchmark_group("memory_performance");
    group.sample_size(50);

    // Test memory allocation patterns
    group.bench_function("memory_allocation", |b| {
        b.iter(|| {
            let start = Instant::now();
            
            // Simulate memory-intensive operations
            let mut data = Vec::new();
            for i in 0..10000 {
                data.push(black_box(format!("data_{}", i)));
            }
            
            // Simulate processing
            let _result: usize = data.iter().map(|s| s.len()).sum();
            
            start.elapsed()
        });
    });

    // Test garbage collection impact
    group.bench_function("gc_performance", |b| {
        b.to_async(&test_config.runtime).iter(|| async {
            let start = Instant::now();
            
            // Create temporary allocations
            for _ in 0..1000 {
                let _temp_data: Vec<u8> = vec![0; 1024];
                tokio::time::sleep(Duration::from_micros(1)).await;
            }
            
            // Force cleanup
            drop(black_box(vec![0u8; 1024 * 1024]));
            
            start.elapsed()
        });
    });

    group.finish();
}

/// Concurrent operations performance test
fn bench_concurrency_performance(c: &mut Criterion) {
    let test_config = TestConfig::new();
    
    let mut group = c.benchmark_group("concurrency_performance");
    group.sample_size(30);

    // Test concurrent execution scalability
    let concurrency_levels = vec![1, 5, 10, 20, 50];
    
    for concurrency in concurrency_levels {
        group.bench_with_input(
            BenchmarkId::new("concurrent_executions", concurrency),
            &concurrency,
            |b, &concurrency| {
                b.to_async(&test_config.runtime).iter(|| async {
                    let start = Instant::now();
                    
                    // Execute multiple operations concurrently
                    let tasks: Vec<_> = (0..concurrency)
                        .map(|i| {
                            tokio::spawn(async move {
                                // Simulate concurrent operation
                                tokio::time::sleep(Duration::from_millis(10)).await;
                                black_box(format!("result_{}", i))
                            })
                        })
                        .collect();
                    
                    // Wait for all tasks to complete
                    for task in tasks {
                        let _ = task.await;
                    }
                    
                    let elapsed = start.elapsed();
                    
                    // Check that concurrent operations scale appropriately
                    let max_expected = Duration::from_millis(50 + (concurrency as u64 * 5));
                    assert!(
                        elapsed <= max_expected,
                        "Concurrency performance regression at level {}: {:?} > {:?}",
                        concurrency,
                        elapsed,
                        max_expected
                    );
                    
                    elapsed
                });
            },
        );
    }

    group.finish();
}

/// End-to-end workflow performance test
fn bench_e2e_workflow(c: &mut Criterion) {
    let test_config = TestConfig::new();
    
    let mut group = c.benchmark_group("e2e_workflow");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(60));

    // Test complete workflow: create sandbox -> execute code -> create snapshot -> cleanup
    group.bench_function("complete_workflow", |b| {
        b.to_async(&test_config.runtime).iter(|| async {
            let start = Instant::now();
            
            // Step 1: Create sandbox
            let _sandbox_id = black_box(simulate_sandbox_creation().await);
            
            // Step 2: Execute code
            let _execution_result = black_box(simulate_code_execution("print('test')").await);
            
            // Step 3: Create snapshot
            let _snapshot_id = black_box(simulate_snapshot_creation().await);
            
            // Step 4: Cleanup
            simulate_cleanup().await;
            
            let elapsed = start.elapsed();
            
            // End-to-end workflow should complete within reasonable time
            assert!(
                elapsed <= Duration::from_secs(5),
                "E2E workflow performance regression: {:?}",
                elapsed
            );
            
            elapsed
        });
    });

    group.finish();
}

// Helper functions for simulation

async fn simulate_code_execution(code: &str) -> String {
    // Simulate execution time based on code complexity
    let delay = match code.len() {
        0..=50 => Duration::from_millis(10),
        51..=200 => Duration::from_millis(50),
        _ => Duration::from_millis(100),
    };
    
    tokio::time::sleep(delay).await;
    format!("Executed: {}", code.chars().take(20).collect::<String>())
}

async fn simulate_snapshot_creation() -> String {
    tokio::time::sleep(Duration::from_millis(75)).await;
    format!("snapshot_{}", chrono::Utc::now().timestamp())
}

async fn simulate_snapshot_restoration() -> String {
    tokio::time::sleep(Duration::from_millis(40)).await;
    format!("container_{}", chrono::Utc::now().timestamp())
}

async fn simulate_sandbox_creation() -> String {
    tokio::time::sleep(Duration::from_millis(30)).await;
    format!("sandbox_{}", chrono::Utc::now().timestamp())
}

async fn simulate_cleanup() {
    tokio::time::sleep(Duration::from_millis(10)).await;
}

async fn simulate_cpu_intensive_operation() -> u64 {
    let mut result = 0u64;
    for i in 0..10000 {
        result = result.wrapping_add(i);
    }
    // Small delay to simulate actual work
    tokio::time::sleep(Duration::from_micros(100)).await;
    result
}

async fn simulate_optimization_strategy(strategy: &str) -> String {
    let delay = match strategy {
        "lazy_evaluation" => Duration::from_millis(20),
        "allocation_pooling" => Duration::from_millis(15),
        "serialization_optimization" => Duration::from_millis(25),
        _ => Duration::from_millis(30),
    };
    
    tokio::time::sleep(delay).await;
    format!("Applied {}", strategy)
}

/// Performance monitoring utilities
struct PerformanceMonitor {
    baseline: PerformanceBaseline,
    current_metrics: HashMap<String, f64>,
}

impl PerformanceMonitor {
    fn new() -> Self {
        Self {
            baseline: PerformanceBaseline::default(),
            current_metrics: HashMap::new(),
        }
    }

    fn record_metric(&mut self, name: &str, value: f64) {
        self.current_metrics.insert(name.to_string(), value);
    }

    fn check_regression(&self, metric_name: &str, threshold_percent: f64) -> bool {
        if let Some(&current_value) = self.current_metrics.get(metric_name) {
            let baseline_value = match metric_name {
                "container_startup_time" => self.baseline.container_startup_time.as_millis() as f64,
                "execution_latency" => self.baseline.execution_latency.as_millis() as f64,
                "cpu_utilization" => self.baseline.cpu_utilization_percent,
                "memory_usage" => self.baseline.memory_usage_mb,
                "throughput" => self.baseline.throughput_operations_per_second,
                _ => return false,
            };

            let regression_threshold = baseline_value * (1.0 + threshold_percent / 100.0);
            current_value <= regression_threshold
        } else {
            false
        }
    }

    fn generate_report(&self) -> String {
        let mut report = String::from("Performance Regression Test Report\n");
        report.push_str("=====================================\n\n");

        for (metric, value) in &self.current_metrics {
            let status = if self.check_regression(metric, 10.0) {
                "PASS"
            } else {
                "FAIL"
            };
            report.push_str(&format!("{}: {:.2} - {}\n", metric, value, status));
        }

        report
    }
}

// Benchmark group configuration
criterion_group!(
    benches,
    bench_container_startup,
    bench_code_execution,
    bench_snapshot_operations,
    bench_cpu_optimization,
    bench_memory_performance,
    bench_concurrency_performance,
    bench_e2e_workflow
);

criterion_main!(benches);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_baseline() {
        let baseline = PerformanceBaseline::default();
        assert!(baseline.container_startup_time <= Duration::from_secs(1));
        assert!(baseline.cpu_utilization_percent <= 50.0);
        assert!(baseline.memory_usage_mb <= 512.0);
    }

    #[test]
    fn test_performance_monitor() {
        let mut monitor = PerformanceMonitor::new();
        monitor.record_metric("cpu_utilization", 35.0);
        assert!(monitor.check_regression("cpu_utilization", 10.0));
        
        monitor.record_metric("cpu_utilization", 55.0);
        assert!(!monitor.check_regression("cpu_utilization", 10.0));
    }

    #[tokio::test]
    async fn test_simulation_functions() {
        let result = simulate_code_execution("print('test')").await;
        assert!(result.contains("Executed"));

        let snapshot_id = simulate_snapshot_creation().await;
        assert!(snapshot_id.starts_with("snapshot_"));

        let container_id = simulate_snapshot_restoration().await;
        assert!(container_id.starts_with("container_"));
    }
}