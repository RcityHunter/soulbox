use std::time::{Duration, Instant};
use soulbox::container::ContainerManager;
use tokio;

/// Performance validation test - verify basic performance metrics
#[tokio::test]
async fn test_performance_metrics_validation() {
    println!("🚀 Running SoulBox Performance Validation Tests");
    
    // Test Docker availability first
    let manager = match ContainerManager::new_default() {
        Ok(manager) => manager,
        Err(e) => {
            println!("⚠️  Docker not available, skipping performance tests: {}", e);
            return;
        }
    };

    // Test 1: Container cold start time
    println!("📊 Testing container cold start performance...");
    let cold_start_time = test_cold_start(&manager).await;
    println!("   Cold start time: {:?}", cold_start_time);
    
    // Architecture target: <500ms for cold start
    if cold_start_time < Duration::from_millis(500) {
        println!("   ✅ Cold start performance: EXCELLENT (<500ms target)");
    } else if cold_start_time < Duration::from_millis(2000) {
        println!("   ⚠️  Cold start performance: ACCEPTABLE but needs improvement");
    } else {
        println!("   ❌ Cold start performance: NEEDS OPTIMIZATION (>2s)");
    }

    // Test 2: Code execution overhead
    println!("📊 Testing code execution performance...");
    let exec_time = test_code_execution(&manager).await;
    println!("   Code execution time: {:?}", exec_time);
    
    // Architecture target: <10ms overhead
    if exec_time < Duration::from_millis(10) {
        println!("   ✅ Execution performance: EXCELLENT (<10ms target)");
    } else if exec_time < Duration::from_millis(100) {
        println!("   ⚠️  Execution performance: ACCEPTABLE but needs improvement");
    } else {
        println!("   ❌ Execution performance: NEEDS OPTIMIZATION (>100ms)");
    }

    // Test 3: Memory efficiency
    println!("📊 Testing memory usage...");
    let memory_usage = test_memory_usage(&manager).await;
    println!("   Memory usage: {} MB", memory_usage);
    
    // Architecture target: <50MB
    if memory_usage < 50.0 {
        println!("   ✅ Memory efficiency: EXCELLENT (<50MB target)");
    } else if memory_usage < 100.0 {
        println!("   ⚠️  Memory efficiency: ACCEPTABLE but needs improvement");
    } else {
        println!("   ❌ Memory efficiency: NEEDS OPTIMIZATION (>100MB)");
    }

    // Test 4: Concurrent performance
    println!("📊 Testing concurrent container creation...");
    let concurrent_time = test_concurrent_performance(&manager).await;
    println!("   Concurrent creation (5 containers): {:?}", concurrent_time);
    
    // Architecture target: good scalability
    if concurrent_time < Duration::from_secs(10) {
        println!("   ✅ Concurrent performance: EXCELLENT (<10s for 5 containers)");
    } else if concurrent_time < Duration::from_secs(30) {
        println!("   ⚠️  Concurrent performance: ACCEPTABLE but needs improvement");
    } else {
        println!("   ❌ Concurrent performance: NEEDS OPTIMIZATION (>30s)");
    }

    println!("🏁 Performance validation completed!");
}

async fn test_cold_start(manager: &ContainerManager) -> Duration {
    let start = Instant::now();
    
    match manager.create_sandbox_container(
        "perf-test-cold", 
        "python:3.11-alpine",
        soulbox::ResourceLimits::default(),
        soulbox::NetworkConfig::default(),
        std::collections::HashMap::new()
    ).await {
        Ok(container) => {
            let _ = container.start().await;
            let duration = start.elapsed();
            let _ = container.remove().await;
            duration
        }
        Err(_) => Duration::from_secs(999) // Failed
    }
}

async fn test_code_execution(manager: &ContainerManager) -> Duration {
    if let Ok(container) = manager.create_sandbox_container(
        "perf-test-exec",
        "python:3.11-alpine", 
        soulbox::ResourceLimits::default(),
        soulbox::NetworkConfig::default(),
        std::collections::HashMap::new()
    ).await {
        if container.start().await.is_ok() {
            tokio::time::sleep(Duration::from_millis(1000)).await; // Let container stabilize
            
            let start = Instant::now();
            let result = container.execute_command(vec![
                "python3".to_string(),
                "-c".to_string(), 
                "print('Hello Performance Test')".to_string()
            ]).await;
            let duration = start.elapsed();
            
            let _ = container.remove().await;
            
            if result.is_ok() {
                return duration;
            }
        }
    }
    Duration::from_secs(999) // Failed
}

async fn test_memory_usage(manager: &ContainerManager) -> f64 {
    if let Ok(container) = manager.create_sandbox_container(
        "perf-test-mem",
        "python:3.11-alpine",
        soulbox::ResourceLimits::default(), 
        soulbox::NetworkConfig::default(),
        std::collections::HashMap::new()
    ).await {
        if container.start().await.is_ok() {
            tokio::time::sleep(Duration::from_millis(2000)).await; // Let container stabilize
            
            if let Ok(stats) = container.get_resource_stats().await {
                let _ = container.remove().await;
                return stats.memory_usage_mb as f64;
            }
        }
        let _ = container.remove().await;
    }
    999.0 // Failed
}

async fn test_concurrent_performance(manager: &ContainerManager) -> Duration {
    let start = Instant::now();
    
    // Create 5 containers concurrently
    let handles: Vec<_> = (0..5).map(|i| {
        let mgr = manager.clone();
        tokio::spawn(async move {
            let container_id = format!("perf-test-concurrent-{}", i);
            mgr.create_sandbox_container(
                &container_id,
                "python:3.11-alpine",
                soulbox::ResourceLimits::default(),
                soulbox::NetworkConfig::default(), 
                std::collections::HashMap::new()
            ).await
        })
    }).collect();
    
    // Wait for all to complete
    let mut created_containers = Vec::new();
    for handle in handles {
        if let Ok(Ok(container)) = handle.await {
            created_containers.push(container);
        }
    }
    
    let duration = start.elapsed();
    
    // Cleanup
    for container in created_containers {
        let _ = container.remove().await;
    }
    
    duration
}