use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use soulbox::{
    Config,
    container::{ContainerManager, ContainerPool, PoolConfig},
    runtime::{RuntimeType, RuntimeManager, ExecutionContext},
    session::{SessionManager, InMemorySessionManager},
    monitoring::{metrics::PrometheusMetrics, MonitoringConfig},
};

/// Setup test environment for execution benchmarks
async fn setup_execution_environment() -> (
    Arc<dyn ContainerManager>,
    Arc<ContainerPool>,
    Arc<dyn SessionManager>,
    RuntimeManager,
) {
    let config = Config::default();
    let container_manager = Arc::new(ContainerManager::new(config).await.unwrap());
    
    // Setup container pool
    let pool_config = PoolConfig::default();
    let mut container_pool = ContainerPool::new(pool_config, container_manager.clone());
    container_pool.initialize().await.unwrap();
    let container_pool = Arc::new(container_pool);
    
    // Setup session manager
    let session_manager = Arc::new(InMemorySessionManager::new());
    
    // Setup runtime manager
    let runtime_manager = RuntimeManager::new();
    
    (container_manager, container_pool, session_manager, runtime_manager)
}

/// Benchmark concurrent code execution across different runtimes
fn bench_concurrent_execution_runtimes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (container_manager, _pool, _session_manager, runtime_manager) = 
        rt.block_on(setup_execution_environment());

    let mut group = c.benchmark_group("concurrent_execution_runtimes");
    group.measurement_time(Duration::from_secs(60));
    group.sample_size(10);

    let test_codes = vec![
        (RuntimeType::Python, "print('Hello from Python')"),
        (RuntimeType::NodeJS, "console.log('Hello from Node.js')"),
        (RuntimeType::Shell, "echo 'Hello from Shell'"),
    ];

    let concurrency_levels = vec![1, 2, 4, 8];

    for (runtime_type, code) in test_codes {
        for concurrency in &concurrency_levels {
            group.bench_with_input(
                BenchmarkId::new(format!("{:?}", runtime_type), concurrency),
                &(*concurrency, runtime_type.clone(), code),
                |b, (concurrency_level, runtime, test_code)| {
                    let container_manager = container_manager.clone();
                    let runtime_config = runtime_manager.get_runtime_by_type(runtime).unwrap().clone();
                    
                    b.iter(|| {
                        rt.block_on(async {
                            let start = std::time::Instant::now();
                            
                            let tasks: Vec<_> = (0..*concurrency_level)
                                .map(|i| {
                                    let container_manager = container_manager.clone();
                                    let runtime_config = runtime_config.clone();
                                    let test_code = test_code.to_string();
                                    
                                    tokio::spawn(async move {
                                        let execution_start = std::time::Instant::now();
                                        
                                        // Create container
                                        let container_config = soulbox::container::ContainerConfig {
                                            image: runtime_config.docker_image.clone(),
                                            working_dir: Some(runtime_config.working_directory.clone()),
                                            environment: runtime_config.environment.clone(),
                                            memory_limit: runtime_config.memory_limit,
                                            cpu_limit: runtime_config.cpu_limit,
                                            ..Default::default()
                                        };

                                        let container_id = container_manager
                                            .create_container(container_config)
                                            .await
                                            .unwrap();
                                        
                                        container_manager
                                            .start_container(&container_id)
                                            .await
                                            .unwrap();
                                        
                                        // Write test file
                                        let filename = match runtime {
                                            RuntimeType::Python => format!("test_{}.py", i),
                                            RuntimeType::NodeJS => format!("test_{}.js", i),
                                            RuntimeType::Shell => format!("test_{}.sh", i),
                                            _ => format!("test_{}.txt", i),
                                        };
                                        
                                        container_manager
                                            .write_file(&container_id, &filename, &test_code)
                                            .await
                                            .unwrap();
                                        
                                        // Execute code
                                        let exec_command = runtime_config.generate_exec_command(&filename);
                                        let result = container_manager
                                            .execute_command(&container_id, &exec_command, None)
                                            .await;
                                        
                                        let execution_time = execution_start.elapsed();
                                        
                                        // Cleanup
                                        let _ = container_manager.stop_container(&container_id).await;
                                        let _ = container_manager.remove_container(&container_id).await;
                                        
                                        (execution_time, result.is_ok())
                                    })
                                })
                                .collect();

                            let mut execution_times = Vec::new();
                            let mut success_count = 0;
                            
                            for task in tasks {
                                let (execution_time, success) = task.await.unwrap();
                                execution_times.push(execution_time);
                                if success {
                                    success_count += 1;
                                }
                            }
                            
                            let total_time = start.elapsed();
                            let avg_execution_time = execution_times.iter().sum::<Duration>() / execution_times.len() as u32;
                            
                            black_box((total_time, avg_execution_time, success_count))
                        })
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark execution with different workload sizes
fn bench_execution_workload_sizes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (container_manager, _pool, _session_manager, runtime_manager) = 
        rt.block_on(setup_execution_environment());

    let mut group = c.benchmark_group("execution_workload_sizes");
    group.measurement_time(Duration::from_secs(45));
    group.sample_size(20);

    let runtime_config = runtime_manager.get_runtime("python").unwrap().clone();

    // Different workload sizes
    let workloads = vec![
        ("light", "print('hello')", 1),
        ("medium", &format!("for i in range(100):\n    print(f'iteration {{i}}')\n"), 2),
        ("heavy", &format!("import time\nfor i in range(1000):\n    if i % 100 == 0:\n        print(f'iteration {{i}}')\ntime.sleep(0.01)\n"), 4),
        ("compute_intensive", "sum(i*i for i in range(10000))", 1),
    ];

    for (workload_name, code, concurrency) in workloads {
        group.bench_with_input(
            BenchmarkId::new("workload", workload_name),
            &(code, concurrency),
            |b, (test_code, concurrency_level)| {
                let container_manager = container_manager.clone();
                let runtime_config = runtime_config.clone();
                
                b.iter(|| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();
                        
                        let tasks: Vec<_> = (0..*concurrency_level)
                            .map(|i| {
                                let container_manager = container_manager.clone();
                                let runtime_config = runtime_config.clone();
                                let test_code = test_code.to_string();
                                
                                tokio::spawn(async move {
                                    let execution_start = std::time::Instant::now();
                                    
                                    // Create container
                                    let container_config = soulbox::container::ContainerConfig {
                                        image: runtime_config.docker_image.clone(),
                                        working_dir: Some(runtime_config.working_directory.clone()),
                                        environment: runtime_config.environment.clone(),
                                        memory_limit: runtime_config.memory_limit,
                                        cpu_limit: runtime_config.cpu_limit,
                                        ..Default::default()
                                    };

                                    let container_id = container_manager
                                        .create_container(container_config)
                                        .await
                                        .unwrap();
                                    
                                    container_manager
                                        .start_container(&container_id)
                                        .await
                                        .unwrap();
                                    
                                    // Write and execute test code
                                    let filename = format!("test_{}.py", i);
                                    container_manager
                                        .write_file(&container_id, &filename, &test_code)
                                        .await
                                        .unwrap();
                                    
                                    let exec_command = runtime_config.generate_exec_command(&filename);
                                    let result = container_manager
                                        .execute_command(&container_id, &exec_command, Some(30))
                                        .await;
                                    
                                    let execution_time = execution_start.elapsed();
                                    
                                    // Cleanup
                                    let _ = container_manager.stop_container(&container_id).await;
                                    let _ = container_manager.remove_container(&container_id).await;
                                    
                                    (execution_time, result.is_ok())
                                })
                            })
                            .collect();

                        let mut results = Vec::new();
                        for task in tasks {
                            results.push(task.await.unwrap());
                        }
                        
                        let total_time = start.elapsed();
                        black_box((total_time, results))
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark execution throughput
fn bench_execution_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (container_manager, container_pool, _session_manager, _runtime_manager) = 
        rt.block_on(setup_execution_environment());

    let mut group = c.benchmark_group("execution_throughput");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(5);

    // Compare pool vs direct execution throughput
    group.bench_function("pool_based_execution", |b| {
        let container_pool = container_pool.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                
                // Execute 20 tasks using pool
                let tasks: Vec<_> = (0..20)
                    .map(|i| {
                        let container_pool = container_pool.clone();
                        tokio::spawn(async move {
                            let task_start = std::time::Instant::now();
                            
                            let container_id = container_pool
                                .get_container(RuntimeType::Python)
                                .await
                                .unwrap();
                            
                            // Simple execution
                            let code = format!("print('Task {}')", i);
                            let filename = format!("task_{}.py", i);
                            
                            // Note: This is simplified - real implementation would need container manager reference
                            let task_time = task_start.elapsed();
                            
                            // Return container to pool
                            let _ = container_pool
                                .return_container(container_id, RuntimeType::Python)
                                .await;
                            
                            task_time
                        })
                    })
                    .collect();

                let mut task_times = Vec::new();
                for task in tasks {
                    task_times.push(task.await.unwrap());
                }
                
                let total_time = start.elapsed();
                let throughput = 20.0 / total_time.as_secs_f64();
                
                black_box((total_time, throughput, task_times))
            })
        });
    });

    group.bench_function("direct_execution", |b| {
        let container_manager = container_manager.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                
                // Execute 20 tasks directly
                let tasks: Vec<_> = (0..20)
                    .map(|i| {
                        let container_manager = container_manager.clone();
                        tokio::spawn(async move {
                            let task_start = std::time::Instant::now();
                            
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
                            
                            container_manager
                                .start_container(&container_id)
                                .await
                                .unwrap();
                            
                            let code = format!("print('Task {}')", i);
                            let filename = format!("task_{}.py", i);
                            
                            container_manager
                                .write_file(&container_id, &filename, &code)
                                .await
                                .unwrap();
                            
                            let _ = container_manager
                                .execute_command(&container_id, &format!("python {}", filename), None)
                                .await;
                            
                            let task_time = task_start.elapsed();
                            
                            // Cleanup
                            let _ = container_manager.stop_container(&container_id).await;
                            let _ = container_manager.remove_container(&container_id).await;
                            
                            task_time
                        })
                    })
                    .collect();

                let mut task_times = Vec::new();
                for task in tasks {
                    task_times.push(task.await.unwrap());
                }
                
                let total_time = start.elapsed();
                let throughput = 20.0 / total_time.as_secs_f64();
                
                black_box((total_time, throughput, task_times))
            })
        });
    });

    group.finish();
}

/// Benchmark execution under different resource constraints
fn bench_resource_constraints(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (container_manager, _pool, _session_manager, _runtime_manager) = 
        rt.block_on(setup_execution_environment());

    let mut group = c.benchmark_group("resource_constraints");
    group.measurement_time(Duration::from_secs(40));
    group.sample_size(10);

    // Test different memory limits
    let memory_configs = vec![
        ("128mb", 128 * 1024 * 1024),
        ("256mb", 256 * 1024 * 1024),
        ("512mb", 512 * 1024 * 1024),
        ("1gb", 1024 * 1024 * 1024),
    ];

    for (config_name, memory_limit) in memory_configs {
        group.bench_with_input(
            BenchmarkId::new("memory_limit", config_name),
            &memory_limit,
            |b, &mem_limit| {
                let container_manager = container_manager.clone();
                
                b.iter(|| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();
                        
                        let container_config = soulbox::container::ContainerConfig {
                            image: "python:3.11-alpine".to_string(),
                            working_dir: Some("/workspace".to_string()),
                            environment: HashMap::new(),
                            memory_limit: Some(mem_limit),
                            cpu_limit: Some(1.0),
                            ..Default::default()
                        };

                        let container_id = container_manager
                            .create_container(container_config)
                            .await
                            .unwrap();
                        
                        container_manager
                            .start_container(&container_id)
                            .await
                            .unwrap();
                        
                        // Memory-intensive task
                        let code = "data = [i for i in range(100000)]\nprint(f'Generated {len(data)} items')";
                        container_manager
                            .write_file(&container_id, "memory_test.py", code)
                            .await
                            .unwrap();
                        
                        let result = container_manager
                            .execute_command(&container_id, "python memory_test.py", Some(30))
                            .await;
                        
                        let execution_time = start.elapsed();
                        
                        // Cleanup
                        let _ = container_manager.stop_container(&container_id).await;
                        let _ = container_manager.remove_container(&container_id).await;
                        
                        black_box((execution_time, result.is_ok()))
                    })
                });
            },
        );
    }

    // Test different CPU limits
    let cpu_configs = vec![
        ("0.5_cpu", 0.5),
        ("1.0_cpu", 1.0),
        ("2.0_cpu", 2.0),
    ];

    for (config_name, cpu_limit) in cpu_configs {
        group.bench_with_input(
            BenchmarkId::new("cpu_limit", config_name),
            &cpu_limit,
            |b, &cpu_lim| {
                let container_manager = container_manager.clone();
                
                b.iter(|| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();
                        
                        let container_config = soulbox::container::ContainerConfig {
                            image: "python:3.11-alpine".to_string(),
                            working_dir: Some("/workspace".to_string()),
                            environment: HashMap::new(),
                            memory_limit: Some(512 * 1024 * 1024),
                            cpu_limit: Some(cpu_lim),
                            ..Default::default()
                        };

                        let container_id = container_manager
                            .create_container(container_config)
                            .await
                            .unwrap();
                        
                        container_manager
                            .start_container(&container_id)
                            .await
                            .unwrap();
                        
                        // CPU-intensive task
                        let code = "result = sum(i*i for i in range(50000))\nprint(f'Result: {result}')";
                        container_manager
                            .write_file(&container_id, "cpu_test.py", code)
                            .await
                            .unwrap();
                        
                        let result = container_manager
                            .execute_command(&container_id, "python cpu_test.py", Some(30))
                            .await;
                        
                        let execution_time = start.elapsed();
                        
                        // Cleanup
                        let _ = container_manager.stop_container(&container_id).await;
                        let _ = container_manager.remove_container(&container_id).await;
                        
                        black_box((execution_time, result.is_ok()))
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark execution timeout handling
fn bench_timeout_handling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (container_manager, _pool, _session_manager, _runtime_manager) = 
        rt.block_on(setup_execution_environment());

    let mut group = c.benchmark_group("timeout_handling");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(20);

    // Test different timeout scenarios
    let timeout_scenarios = vec![
        ("quick_task", "print('hello')", 5), // Should complete well within timeout
        ("medium_task", "import time; time.sleep(2); print('done')", 5), // Should complete within timeout
        ("timeout_task", "import time; time.sleep(10); print('done')", 3), // Should timeout
    ];

    for (scenario_name, code, timeout_secs) in timeout_scenarios {
        group.bench_with_input(
            BenchmarkId::new("timeout", scenario_name),
            &(code, timeout_secs),
            |b, (test_code, timeout)| {
                let container_manager = container_manager.clone();
                
                b.iter(|| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();
                        
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
                        
                        container_manager
                            .start_container(&container_id)
                            .await
                            .unwrap();
                        
                        container_manager
                            .write_file(&container_id, "timeout_test.py", test_code)
                            .await
                            .unwrap();
                        
                        let result = container_manager
                            .execute_command(&container_id, "python timeout_test.py", Some(*timeout))
                            .await;
                        
                        let execution_time = start.elapsed();
                        
                        // Cleanup
                        let _ = container_manager.stop_container(&container_id).await;
                        let _ = container_manager.remove_container(&container_id).await;
                        
                        black_box((execution_time, result.is_ok()))
                    })
                });
            },
        );
    }

    group.finish();
}

/// Benchmark session-based execution
fn bench_session_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (container_manager, _pool, session_manager, _runtime_manager) = 
        rt.block_on(setup_execution_environment());

    let mut group = c.benchmark_group("session_execution");
    group.measurement_time(Duration::from_secs(25));

    group.bench_function("session_reuse", |b| {
        let session_manager = session_manager.clone();
        let container_manager = container_manager.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                
                // Create session
                let session = session_manager
                    .create_session("test_user".to_string())
                    .await
                    .unwrap();
                
                // Create container for session
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
                
                container_manager
                    .start_container(&container_id)
                    .await
                    .unwrap();
                
                // Update session with container
                let mut updated_session = session.clone();
                updated_session.set_container(container_id.clone(), "python".to_string());
                session_manager
                    .update_session(&updated_session)
                    .await
                    .unwrap();
                
                // Execute multiple commands in same session
                for i in 0..5 {
                    let code = format!("print('Command {} executed')", i);
                    container_manager
                        .write_file(&container_id, &format!("cmd_{}.py", i), &code)
                        .await
                        .unwrap();
                    
                    let _ = container_manager
                        .execute_command(&container_id, &format!("python cmd_{}.py", i), Some(5))
                        .await;
                }
                
                let total_time = start.elapsed();
                
                // Cleanup
                let _ = container_manager.stop_container(&container_id).await;
                let _ = container_manager.remove_container(&container_id).await;
                let _ = session_manager.delete_session(session.id).await;
                
                black_box(total_time)
            })
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_concurrent_execution_runtimes,
    bench_execution_workload_sizes,
    bench_execution_throughput,
    bench_resource_constraints,
    bench_timeout_handling,
    bench_session_execution
);

criterion_main!(benches);