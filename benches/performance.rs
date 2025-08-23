use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use soulbox::container::ContainerManager;
use std::time::Duration;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Benchmark container startup time
fn bench_container_startup(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("container_cold_start", |b| {
        b.iter(|| {
            rt.block_on(async {
                if let Ok(manager) = ContainerManager::new_default() {
                    let start = std::time::Instant::now();
                    
                    if let Ok(container) = manager.create_sandbox_container(
                        "bench-cold-start",
                        "python:3.11-alpine",
                        soulbox::ResourceLimits::default(),
                        soulbox::NetworkConfig::default(),
                        std::collections::HashMap::new()
                    ).await {
                        let _ = container.start().await;
                        let _ = container.remove().await;
                    }
                    
                    start.elapsed()
                } else {
                    Duration::from_millis(999)
                }
            })
        })
    });
}

/// Benchmark concurrent container creation
fn bench_concurrent_containers(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("concurrent_containers");
    
    // Test different concurrency levels
    for concurrency in [1, 2, 5].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency),
            concurrency,
            |b, &size| {
                b.iter(|| {
                    rt.block_on(async {
                        if let Ok(manager) = ContainerManager::new_default() {
                            let manager = Arc::new(manager);
                            let start = std::time::Instant::now();
                            
                            // Create containers concurrently
                            let handles: Vec<_> = (0..size)
                                .map(|i| {
                                    let mgr = manager.clone();
                                    tokio::spawn(async move {
                                        let container_id = format!("bench-concurrent-{}", i);
                                        mgr.create_sandbox_container(
                                            &container_id,
                                            "python:3.11-alpine",
                                            soulbox::ResourceLimits::default(),
                                            soulbox::NetworkConfig::default(),
                                            std::collections::HashMap::new()
                                        ).await
                                    })
                                })
                                .collect();
                            
                            // Wait for all to complete
                            for handle in handles {
                                let _ = handle.await;
                            }
                            
                            start.elapsed()
                        } else {
                            Duration::from_millis(999)
                        }
                    })
                })
            },
        );
    }
    group.finish();
}

/// Benchmark code execution
fn bench_code_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("code_execution", |b| {
        b.iter(|| {
            rt.block_on(async {
                if let Ok(manager) = ContainerManager::new_default() {
                    if let Ok(container) = manager.create_sandbox_container(
                        "bench-code-exec",
                        "python:3.11-alpine",
                        soulbox::ResourceLimits::default(),
                        soulbox::NetworkConfig::default(),
                        std::collections::HashMap::new()
                    ).await {
                        if container.start().await.is_ok() {
                            // Wait for container to be ready
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            
                            let start = std::time::Instant::now();
                            let _result = container.execute_command(vec![
                                "python3".to_string(), 
                                "-c".to_string(), 
                                "print('Hello World')".to_string()
                            ]).await;
                            let duration = start.elapsed();
                            
                            let _ = container.remove().await;
                            
                            return duration;
                        }
                    }
                }
                Duration::from_millis(999)
            })
        })
    });
}

/// Benchmark resource monitoring
fn bench_resource_monitoring(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("resource_stats", |b| {
        b.iter(|| {
            rt.block_on(async {
                if let Ok(manager) = ContainerManager::new_default() {
                    if let Ok(container) = manager.create_sandbox_container(
                        "bench-resource",
                        "python:3.11-alpine",
                        soulbox::ResourceLimits::default(),
                        soulbox::NetworkConfig::default(),
                        std::collections::HashMap::new()
                    ).await {
                        if container.start().await.is_ok() {
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            
                            let start = std::time::Instant::now();
                            let _stats = container.get_resource_stats().await;
                            let duration = start.elapsed();
                            
                            let _ = container.remove().await;
                            
                            return duration;
                        }
                    }
                }
                Duration::from_millis(999)
            })
        })
    });
}

criterion_group!(
    benches,
    bench_container_startup,
    bench_concurrent_containers,
    bench_code_execution,
    bench_resource_monitoring
);
criterion_main!(benches);