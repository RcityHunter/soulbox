use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use reqwest::Client;
use soulbox::{
    Config,
    server::SoulBoxServer,
    auth::JwtManager,
    container::ContainerManager,
    monitoring::metrics::PrometheusMetrics,
    monitoring::MonitoringConfig,
};

/// Setup test server for benchmarking
async fn setup_test_server() -> (String, Arc<SoulBoxServer>) {
    let config = Config::default();
    let container_manager = Arc::new(ContainerManager::new(config.clone()).await.unwrap());
    let jwt_manager = Arc::new(JwtManager::new("test_secret".to_string()));
    let monitoring_config = MonitoringConfig::default();
    let metrics = Arc::new(PrometheusMetrics::new(monitoring_config).unwrap());
    
    let server = Arc::new(SoulBoxServer::new(
        config,
        container_manager,
        jwt_manager,
        metrics,
    ).await.unwrap());
    
    // Start server on random port
    let addr = "127.0.0.1:0";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let base_url = format!("http://127.0.0.1:{}", port);
    
    let server_clone = server.clone();
    tokio::spawn(async move {
        server_clone.serve_with_listener(listener).await.unwrap();
    });
    
    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    (base_url, server)
}

/// Generate test JWT token
fn generate_test_token() -> String {
    let jwt_manager = JwtManager::new("test_secret".to_string());
    jwt_manager.generate_token("test_user".to_string(), "user".to_string()).unwrap()
}

/// Benchmark API endpoint latency
fn bench_api_endpoints(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (base_url, _server) = rt.block_on(setup_test_server());
    let client = Client::new();
    let token = generate_test_token();

    let mut group = c.benchmark_group("api_latency");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(50);

    // Health check endpoint
    group.bench_function("health_check", |b| {
        let url = format!("{}/health", base_url);
        let client = client.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                let response = client.get(&url).send().await.unwrap();
                let latency = start.elapsed();
                
                assert!(response.status().is_success());
                black_box(latency)
            })
        });
    });

    // Authentication endpoint
    group.bench_function("auth_login", |b| {
        let url = format!("{}/api/auth/login", base_url);
        let client = client.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let payload = json!({
                    "username": "test_user",
                    "password": "test_password"
                });
                
                let start = std::time::Instant::now();
                let response = client
                    .post(&url)
                    .json(&payload)
                    .send()
                    .await
                    .unwrap();
                let latency = start.elapsed();
                
                // Note: This might return an error in real scenarios, but we're measuring latency
                black_box((latency, response.status()))
            })
        });
    });

    // Create sandbox endpoint
    group.bench_function("create_sandbox", |b| {
        let url = format!("{}/api/sandboxes", base_url);
        let client = client.clone();
        let token = token.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let payload = json!({
                    "runtime": "python",
                    "memory_limit": 512,
                    "cpu_limit": 1.0
                });
                
                let start = std::time::Instant::now();
                let response = client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .json(&payload)
                    .send()
                    .await
                    .unwrap();
                let latency = start.elapsed();
                
                black_box((latency, response.status()))
            })
        });
    });

    // Execute code endpoint
    group.bench_function("execute_code", |b| {
        let url = format!("{}/api/execute", base_url);
        let client = client.clone();
        let token = token.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let payload = json!({
                    "code": "print('Hello, World!')",
                    "runtime": "python",
                    "timeout": 10
                });
                
                let start = std::time::Instant::now();
                let response = client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .json(&payload)
                    .send()
                    .await
                    .unwrap();
                let latency = start.elapsed();
                
                black_box((latency, response.status()))
            })
        });
    });

    // List sessions endpoint
    group.bench_function("list_sessions", |b| {
        let url = format!("{}/api/sessions", base_url);
        let client = client.clone();
        let token = token.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                let response = client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await
                    .unwrap();
                let latency = start.elapsed();
                
                black_box((latency, response.status()))
            })
        });
    });

    // Metrics endpoint
    group.bench_function("metrics", |b| {
        let url = format!("{}/metrics", base_url);
        let client = client.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                let response = client.get(&url).send().await.unwrap();
                let latency = start.elapsed();
                
                black_box((latency, response.status()))
            })
        });
    });

    group.finish();
}

/// Benchmark concurrent API requests
fn bench_concurrent_requests(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (base_url, _server) = rt.block_on(setup_test_server());
    let client = Client::new();
    let token = generate_test_token();

    let mut group = c.benchmark_group("concurrent_api");
    group.measurement_time(Duration::from_secs(45));
    group.sample_size(10);

    let concurrency_levels = vec![1, 5, 10, 20, 50];

    for concurrency in concurrency_levels {
        group.bench_with_input(
            BenchmarkId::new("health_check", concurrency),
            &concurrency,
            |b, &concurrency_level| {
                let url = format!("{}/health", base_url);
                let client = client.clone();
                
                b.iter(|| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();
                        
                        let tasks: Vec<_> = (0..concurrency_level)
                            .map(|_| {
                                let client = client.clone();
                                let url = url.clone();
                                tokio::spawn(async move {
                                    client.get(&url).send().await.unwrap()
                                })
                            })
                            .collect();

                        // Wait for all requests to complete
                        for task in tasks {
                            let response = task.await.unwrap();
                            assert!(response.status().is_success());
                        }
                        
                        let total_time = start.elapsed();
                        black_box(total_time)
                    })
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("execute_code", concurrency),
            &concurrency,
            |b, &concurrency_level| {
                let url = format!("{}/api/execute", base_url);
                let client = client.clone();
                let token = token.clone();
                
                b.iter(|| {
                    rt.block_on(async {
                        let start = std::time::Instant::now();
                        
                        let tasks: Vec<_> = (0..concurrency_level)
                            .map(|i| {
                                let client = client.clone();
                                let url = url.clone();
                                let token = token.clone();
                                tokio::spawn(async move {
                                    let payload = json!({
                                        "code": format!("print('Hello from request {}')", i),
                                        "runtime": "python",
                                        "timeout": 10
                                    });
                                    
                                    client
                                        .post(&url)
                                        .header("Authorization", format!("Bearer {}", token))
                                        .json(&payload)
                                        .send()
                                        .await
                                        .unwrap()
                                })
                            })
                            .collect();

                        // Wait for all requests to complete
                        for task in tasks {
                            let _response = task.await.unwrap();
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

/// Benchmark request payload sizes
fn bench_payload_sizes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (base_url, _server) = rt.block_on(setup_test_server());
    let client = Client::new();
    let token = generate_test_token();

    let mut group = c.benchmark_group("payload_sizes");
    group.measurement_time(Duration::from_secs(30));

    // Test different code payload sizes
    let payload_sizes = vec![
        ("small", "print('hello')"),
        ("medium", &"print('hello')\n".repeat(100)),
        ("large", &"print('hello')\n".repeat(1000)),
        ("very_large", &"print('hello')\n".repeat(10000)),
    ];

    for (size_name, code) in payload_sizes {
        group.bench_function(size_name, |b| {
            let url = format!("{}/api/execute", base_url);
            let client = client.clone();
            let token = token.clone();
            let code = code.to_string();
            
            b.iter(|| {
                rt.block_on(async {
                    let payload = json!({
                        "code": code,
                        "runtime": "python",
                        "timeout": 10
                    });
                    
                    let start = std::time::Instant::now();
                    let response = client
                        .post(&url)
                        .header("Authorization", format!("Bearer {}", token))
                        .json(&payload)
                        .send()
                        .await
                        .unwrap();
                    let latency = start.elapsed();
                    
                    black_box((latency, response.status()))
                })
            });
        });
    }

    group.finish();
}

/// Benchmark authentication overhead
fn bench_auth_overhead(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (base_url, _server) = rt.block_on(setup_test_server());
    let client = Client::new();
    let token = generate_test_token();

    let mut group = c.benchmark_group("auth_overhead");
    group.measurement_time(Duration::from_secs(20));

    // Compare authenticated vs unauthenticated requests
    group.bench_function("without_auth", |b| {
        let url = format!("{}/health", base_url);
        let client = client.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                let response = client.get(&url).send().await.unwrap();
                let latency = start.elapsed();
                
                black_box((latency, response.status()))
            })
        });
    });

    group.bench_function("with_auth", |b| {
        let url = format!("{}/api/sessions", base_url);
        let client = client.clone();
        let token = token.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                let response = client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await
                    .unwrap();
                let latency = start.elapsed();
                
                black_box((latency, response.status()))
            })
        });
    });

    // Test JWT token validation overhead
    group.bench_function("token_validation", |b| {
        let jwt_manager = JwtManager::new("test_secret".to_string());
        let token = token.clone();
        
        b.iter(|| {
            let start = std::time::Instant::now();
            let result = jwt_manager.validate_token(&token);
            let validation_time = start.elapsed();
            
            black_box((validation_time, result))
        });
    });

    group.finish();
}

/// Benchmark response time under load
fn bench_load_response_time(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (base_url, _server) = rt.block_on(setup_test_server());
    let client = Client::new();

    let mut group = c.benchmark_group("load_response_time");
    group.measurement_time(Duration::from_secs(60));
    group.sample_size(5);

    // Simulate sustained load
    group.bench_function("sustained_load", |b| {
        let url = format!("{}/health", base_url);
        let client = client.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                
                // Send 100 requests rapidly
                let tasks: Vec<_> = (0..100)
                    .map(|_| {
                        let client = client.clone();
                        let url = url.clone();
                        tokio::spawn(async move {
                            let request_start = std::time::Instant::now();
                            let response = client.get(&url).send().await.unwrap();
                            let request_latency = request_start.elapsed();
                            (response.status(), request_latency)
                        })
                    })
                    .collect();

                let mut latencies = Vec::new();
                for task in tasks {
                    let (status, latency) = task.await.unwrap();
                    assert!(status.is_success());
                    latencies.push(latency);
                }
                
                let total_time = start.elapsed();
                let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
                let max_latency = latencies.iter().max().unwrap();
                let min_latency = latencies.iter().min().unwrap();
                
                black_box((total_time, avg_latency, *min_latency, *max_latency))
            })
        });
    });

    group.finish();
}

/// Benchmark error handling latency
fn bench_error_handling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (base_url, _server) = rt.block_on(setup_test_server());
    let client = Client::new();

    let mut group = c.benchmark_group("error_handling");
    group.measurement_time(Duration::from_secs(20));

    // Test various error scenarios
    group.bench_function("invalid_endpoint", |b| {
        let url = format!("{}/api/nonexistent", base_url);
        let client = client.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                let response = client.get(&url).send().await.unwrap();
                let latency = start.elapsed();
                
                black_box((latency, response.status()))
            })
        });
    });

    group.bench_function("invalid_auth", |b| {
        let url = format!("{}/api/sessions", base_url);
        let client = client.clone();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                let response = client
                    .get(&url)
                    .header("Authorization", "Bearer invalid_token")
                    .send()
                    .await
                    .unwrap();
                let latency = start.elapsed();
                
                black_box((latency, response.status()))
            })
        });
    });

    group.bench_function("invalid_json", |b| {
        let url = format!("{}/api/execute", base_url);
        let client = client.clone();
        let token = generate_test_token();
        
        b.iter(|| {
            rt.block_on(async {
                let start = std::time::Instant::now();
                let response = client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body("invalid json")
                    .send()
                    .await
                    .unwrap();
                let latency = start.elapsed();
                
                black_box((latency, response.status()))
            })
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_api_endpoints,
    bench_concurrent_requests,
    bench_payload_sizes,
    bench_auth_overhead,
    bench_load_response_time,
    bench_error_handling
);

criterion_main!(benches);