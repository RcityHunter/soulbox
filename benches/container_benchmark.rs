//! Performance benchmarks for container operations

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use soulbox::container::{ContainerManager, ResourceLimits};
use soulbox::container::NetworkConfig;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn benchmark_container_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let manager = rt.block_on(async {
        ContainerManager::new_default().unwrap()
    });
    let manager = Arc::new(manager);

    c.bench_function("container_creation", |b| {
        b.iter(|| {
            rt.block_on(async {
                let config = bollard::container::Config {
                    image: Some("alpine:latest".to_string()),
                    cmd: Some(vec!["echo".to_string(), "test".to_string()]),
                    ..Default::default()
                };
                
                let result = manager.create_container(config).await;
                black_box(result)
            })
        })
    });
}

fn benchmark_code_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    // Skip if Docker is not available
    if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
        return;
    }

    let manager = rt.block_on(async {
        ContainerManager::new_default().unwrap()
    });
    let manager = Arc::new(manager);

    // Setup: Create a test container
    let container_id = rt.block_on(async {
        let config = bollard::container::Config {
            image: Some("python:3.11-alpine".to_string()),
            cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
            ..Default::default()
        };
        
        let id = manager.create_container(config).await.unwrap();
        manager.start_container(&id).await.unwrap();
        id
    });

    let mut group = c.benchmark_group("code_execution");
    
    // Benchmark simple Python code execution
    group.bench_function("python_hello_world", |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = manager.execute_command(
                    &container_id,
                    &["python", "-c", "print('Hello World')"]
                ).await;
                black_box(result)
            })
        })
    });

    // Benchmark more complex Python code
    group.bench_function("python_math_operations", |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = manager.execute_command(
                    &container_id,
                    &["python", "-c", "result = sum(range(1000)); print(result)"]
                ).await;
                black_box(result)
            })
        })
    });

    group.finish();

    // Cleanup
    rt.block_on(async {
        let _ = manager.stop_container(&container_id).await;
        let _ = manager.remove_container_by_id(&container_id).await;
    });
}

fn benchmark_container_lifecycle(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
        return;
    }

    let manager = rt.block_on(async {
        ContainerManager::new_default().unwrap()
    });
    let manager = Arc::new(manager);

    let mut group = c.benchmark_group("container_lifecycle");

    // Benchmark full lifecycle: create -> start -> execute -> stop -> remove
    group.bench_function("full_lifecycle", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Create
                let config = bollard::container::Config {
                    image: Some("alpine:latest".to_string()),
                    cmd: Some(vec!["sh".to_string()]),
                    ..Default::default()
                };
                
                let container_id = manager.create_container(config).await.unwrap();
                
                // Start
                manager.start_container(&container_id).await.unwrap();
                
                // Execute
                let _ = manager.execute_command(
                    &container_id,
                    &["echo", "test"]
                ).await;
                
                // Stop
                manager.stop_container(&container_id).await.unwrap();
                
                // Remove
                manager.remove_container_by_id(&container_id).await.unwrap();
                
                black_box(container_id)
            })
        })
    });

    group.finish();
}

fn benchmark_concurrent_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
        return;
    }

    let manager = rt.block_on(async {
        ContainerManager::new_default().unwrap()
    });
    let manager = Arc::new(manager);

    let mut group = c.benchmark_group("concurrent_operations");

    for concurrent_count in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrent_count),
            concurrent_count,
            |b, &count| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut handles = vec![];
                        
                        for i in 0..count {
                            let mgr = Arc::clone(&manager);
                            let handle = tokio::spawn(async move {
                                let config = bollard::container::Config {
                                    image: Some("alpine:latest".to_string()),
                                    cmd: Some(vec!["echo".to_string(), format!("test{}", i)]),
                                    ..Default::default()
                                };
                                
                                let id = mgr.create_container(config).await.unwrap();
                                mgr.start_container(&id).await.unwrap();
                                let _ = mgr.execute_command(&id, &["echo", "done"]).await;
                                mgr.stop_container(&id).await.unwrap();
                                mgr.remove_container_by_id(&id).await.unwrap();
                            });
                            handles.push(handle);
                        }
                        
                        for handle in handles {
                            handle.await.unwrap();
                        }
                    })
                })
            },
        );
    }

    group.finish();
}

fn benchmark_resource_limits(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    if std::env::var("SKIP_DOCKER_TESTS").is_ok() {
        return;
    }

    let manager = rt.block_on(async {
        ContainerManager::new_default().unwrap()
    });
    let manager = Arc::new(manager);

    c.bench_function("container_with_resource_limits", |b| {
        b.iter(|| {
            rt.block_on(async {
                let resource_limits = ResourceLimits {
                    cpu_cores: 1.0,
                    memory_mb: 256,
                    disk_mb: 1024,
                    network_bandwidth_mbps: Some(10),
                    max_processes: Some(100),
                    execution_timeout: std::time::Duration::from_secs(30),
                };
                
                let network_config = NetworkConfig::default();
                
                let container = manager.create_sandbox_container(
                    "bench-test",
                    "alpine:latest",
                    resource_limits,
                    network_config,
                    HashMap::new(),
                ).await.unwrap();
                
                container.start().await.unwrap();
                let _ = container.execute(&["echo", "test"]).await;
                container.stop().await.unwrap();
                container.remove().await.unwrap();
                
                black_box(container)
            })
        })
    });
}

criterion_group!(
    benches,
    benchmark_container_creation,
    benchmark_code_execution,
    benchmark_container_lifecycle,
    benchmark_concurrent_operations,
    benchmark_resource_limits
);

criterion_main!(benches);