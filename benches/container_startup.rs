use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use soulbox::{
    Config,
    container::{ContainerManager, ContainerPool, PoolConfig},
    runtime::RuntimeType,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

/// Benchmark container startup times
fn bench_container_startup(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = Config::default();
    
    let container_manager = rt.block_on(async {
        Arc::new(ContainerManager::new(config).await.unwrap())
    });

    let mut group = c.benchmark_group("container_startup");
    group.measurement_time(Duration::from_secs(60));
    group.sample_size(10);

    // Benchmark different runtime startup times
    let runtimes = vec![
        (RuntimeType::Python, "python:3.11-slim"),
        (RuntimeType::NodeJS, "node:20-alpine"),
        (RuntimeType::Rust, "rust:1.75-slim"),
    ];

    for (runtime, image) in runtimes {
        group.bench_with_input(
            BenchmarkId::new("startup", format!("{:?}", runtime)),
            &(runtime, image),
            |b, (runtime_type, image_name)| {
                b.iter(|| {
                    rt.block_on(async {
                        let container_config = soulbox::container::ContainerConfig {
                            image: image_name.to_string(),
                            working_dir: Some("/workspace".to_string()),
                            environment: HashMap::new(),
                            memory_limit: Some(512 * 1024 * 1024), // 512MB
                            cpu_limit: Some(1.0),
                            ..Default::default()
                        };

                        let start = std::time::Instant::now();
                        
                        // Create container
                        let container_id = container_manager
                            .create_container(container_config)
                            .await
                            .unwrap();
                        
                        // Start container
                        container_manager
                            .start_container(&container_id)
                            .await
                            .unwrap();
                        
                        let startup_time = start.elapsed();
                        
                        // Clean up
                        let _ = container_manager.stop_container(&container_id).await;
                        let _ = container_manager.remove_container(&container_id).await;
                        
                        black_box(startup_time)
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark container pool performance
fn bench_container_pool(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = Config::default();
    
    let container_manager = rt.block_on(async {
        Arc::new(ContainerManager::new(config).await.unwrap())
    });

    let mut group = c.benchmark_group("container_pool");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(20);

    // Create pool configuration
    let pool_config = PoolConfig {
        min_containers: 2,
        max_containers: 10,
        max_idle_time: 300,
        maintenance_interval: 60,
        warmup_timeout: 30,
        enable_prewarming: true,
        runtime_configs: {
            let mut configs = HashMap::new();
            configs.insert(RuntimeType::Python, soulbox::container::pool::RuntimePoolConfig {
                min_containers: 2,
                max_containers: 5,
                warmup_command: Some("python -c 'print(\"ready\")'".to_string()),
                base_image: "python:3.11-slim".to_string(),
                memory_limit: Some(512 * 1024 * 1024),
                cpu_limit: Some(1.0),
            });
            configs
        },
    };

    let container_pool = rt.block_on(async {
        let mut pool = ContainerPool::new(pool_config, container_manager);
        pool.initialize().await.unwrap();
        Arc::new(pool)
    });

    // Benchmark pool vs direct container creation
    group.bench_function("pool_get_container", |b| {
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                
                let container_id = container_pool
                    .get_container(RuntimeType::Python)
                    .await
                    .unwrap();
                
                let get_time = start.elapsed();
                
                // Return container to pool
                let _ = container_pool
                    .return_container(container_id, RuntimeType::Python)
                    .await;
                
                black_box(get_time)
            })
        });
    });

    group.bench_function("direct_container_create", |b| {
        let manager = container_manager.clone();
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                
                let container_config = soulbox::container::ContainerConfig {
                    image: "python:3.11-slim".to_string(),
                    working_dir: Some("/workspace".to_string()),
                    environment: HashMap::new(),
                    memory_limit: Some(512 * 1024 * 1024),
                    cpu_limit: Some(1.0),
                    ..Default::default()
                };

                let container_id = manager
                    .create_container(container_config)
                    .await
                    .unwrap();
                
                manager.start_container(&container_id).await.unwrap();
                
                let create_time = start.elapsed();
                
                // Clean up
                let _ = manager.stop_container(&container_id).await;
                let _ = manager.remove_container(&container_id).await;
                
                black_box(create_time)
            })
        });
    });

    group.finish();
}

/// Benchmark concurrent container operations
fn bench_concurrent_containers(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = Config::default();
    
    let container_manager = rt.block_on(async {
        Arc::new(ContainerManager::new(config).await.unwrap())
    });

    let mut group = c.benchmark_group("concurrent_containers");
    group.measurement_time(Duration::from_secs(45));
    group.sample_size(5);

    // Test different levels of concurrency
    let concurrency_levels = vec![1, 2, 4, 8];

    for concurrency in concurrency_levels {
        group.bench_with_input(
            BenchmarkId::new("concurrent_startup", concurrency),
            &concurrency,
            |b, &concurrency_level| {
                b.iter(|| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();
                        
                        let tasks: Vec<_> = (0..concurrency_level)
                            .map(|_| {
                                let manager = container_manager.clone();
                                tokio::spawn(async move {
                                    let container_config = soulbox::container::ContainerConfig {
                                        image: "python:3.11-slim".to_string(),
                                        working_dir: Some("/workspace".to_string()),
                                        environment: HashMap::new(),
                                        memory_limit: Some(256 * 1024 * 1024), // 256MB for concurrency test
                                        cpu_limit: Some(0.5),
                                        ..Default::default()
                                    };

                                    let container_id = manager
                                        .create_container(container_config)
                                        .await
                                        .unwrap();
                                    
                                    manager.start_container(&container_id).await.unwrap();
                                    
                                    // Clean up
                                    let _ = manager.stop_container(&container_id).await;
                                    let _ = manager.remove_container(&container_id).await;
                                    
                                    container_id
                                })
                            })
                            .collect();

                        // Wait for all tasks to complete
                        for task in tasks {
                            let _ = task.await;
                        }
                        
                        let total_time = start.elapsed();
                        black_box(total_time)
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark container resource overhead
fn bench_container_overhead(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = Config::default();
    
    let container_manager = rt.block_on(async {
        Arc::new(ContainerManager::new(config).await.unwrap())
    });

    let mut group = c.benchmark_group("container_overhead");
    group.measurement_time(Duration::from_secs(30));

    // Test memory overhead of different runtimes
    let runtime_configs = vec![
        ("python_minimal", "python:3.11-alpine", 128 * 1024 * 1024), // 128MB
        ("python_standard", "python:3.11-slim", 512 * 1024 * 1024),  // 512MB
        ("node_minimal", "node:20-alpine", 128 * 1024 * 1024),       // 128MB
        ("node_standard", "node:20", 512 * 1024 * 1024),             // 512MB
    ];

    for (name, image, memory_limit) in runtime_configs {
        group.bench_function(name, |b| {
            b.iter(|| {
                rt.block_on(async {
                    let start = std::time::Instant::now();
                    
                    let container_config = soulbox::container::ContainerConfig {
                        image: image.to_string(),
                        working_dir: Some("/workspace".to_string()),
                        environment: HashMap::new(),
                        memory_limit: Some(memory_limit),
                        cpu_limit: Some(0.5),
                        ..Default::default()
                    };

                    let container_id = container_manager
                        .create_container(container_config)
                        .await
                        .unwrap();
                    
                    container_manager.start_container(&container_id).await.unwrap();
                    
                    // Measure actual startup time
                    let startup_time = start.elapsed();
                    
                    // Get container stats
                    let stats = container_manager
                        .get_container_stats(&container_id)
                        .await
                        .unwrap();
                    
                    // Clean up
                    let _ = container_manager.stop_container(&container_id).await;
                    let _ = container_manager.remove_container(&container_id).await;
                    
                    black_box((startup_time, stats))
                })
            });
        });
    }

    group.finish();
}

/// Benchmark container lifecycle operations
fn bench_container_lifecycle(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = Config::default();
    
    let container_manager = rt.block_on(async {
        Arc::new(ContainerManager::new(config).await.unwrap())
    });

    let mut group = c.benchmark_group("container_lifecycle");
    group.measurement_time(Duration::from_secs(20));

    // Benchmark full lifecycle: create -> start -> execute -> stop -> remove
    group.bench_function("full_lifecycle", |b| {
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                
                // Create
                let container_config = soulbox::container::ContainerConfig {
                    image: "python:3.11-alpine".to_string(),
                    working_dir: Some("/workspace".to_string()),
                    environment: HashMap::new(),
                    memory_limit: Some(256 * 1024 * 1024),
                    cpu_limit: Some(0.5),
                    ..Default::default()
                };

                let container_id = container_manager
                    .create_container(container_config)
                    .await
                    .unwrap();
                
                // Start
                container_manager.start_container(&container_id).await.unwrap();
                
                // Execute simple command
                let _ = container_manager
                    .execute_command(&container_id, "python -c 'print(\"hello\")'", None)
                    .await;
                
                // Stop
                container_manager.stop_container(&container_id).await.unwrap();
                
                // Remove
                container_manager.remove_container(&container_id).await.unwrap();
                
                let total_time = start.elapsed();
                black_box(total_time)
            })
        });
    });

    // Benchmark individual operations
    group.bench_function("create_only", |b| {
        b.iter(|| {
            rt.block_on(async {
                let container_config = soulbox::container::ContainerConfig {
                    image: "python:3.11-alpine".to_string(),
                    working_dir: Some("/workspace".to_string()),
                    environment: HashMap::new(),
                    memory_limit: Some(256 * 1024 * 1024),
                    cpu_limit: Some(0.5),
                    ..Default::default()
                };

                let start = std::time::Instant::now();
                let container_id = container_manager
                    .create_container(container_config)
                    .await
                    .unwrap();
                let create_time = start.elapsed();
                
                // Clean up
                let _ = container_manager.remove_container(&container_id).await;
                
                black_box(create_time)
            })
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_container_startup,
    bench_container_pool,
    bench_concurrent_containers,
    bench_container_overhead,
    bench_container_lifecycle
);

criterion_main!(benches);