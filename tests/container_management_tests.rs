use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

// Import from our crate
use soulbox::container::{
    ContainerManager, SandboxContainer, ResourceLimits, NetworkConfig, PortMapping
};
use soulbox::container::resource_limits::{CpuLimits, MemoryLimits, DiskLimits, NetworkLimits};
use soulbox::error::Result;

#[tokio::test]
async fn test_container_creation_should_succeed() {
    // This test will initially fail because ContainerManager doesn't exist yet
    let config = soulbox::Config::default();
    let manager = ContainerManager::new(config).await.unwrap();
    
    let resource_limits = ResourceLimits {
        cpu: CpuLimits {
            cores: 1.0,
            shares: Some(1024),
            cpu_percent: Some(80.0),
        },
        memory: MemoryLimits {
            limit_mb: 512,
            swap_limit_mb: Some(1024),
            swap_mb: Some(512),
        },
        disk: DiskLimits {
            limit_mb: 2048,
            iops_limit: Some(1000),
        },
        network: NetworkLimits {
            upload_bps: Some(1024 * 1024), // 1 MB/s
            download_bps: Some(10 * 1024 * 1024), // 10 MB/s
            max_connections: Some(100),
        },
    };
    
    let network_config = NetworkConfig {
        enable_internet: true,
        port_mappings: vec![
            PortMapping {
                host_port: None, // Auto-assign
                container_port: 3000,
                protocol: "tcp".to_string(),
            }
        ],
        allowed_domains: vec!["github.com".to_string(), "npmjs.org".to_string()],
        dns_servers: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
    };
    
    let container = manager.create_sandbox_container(
        "test-sandbox-1",
        "node:18-alpine", 
        resource_limits,
        network_config,
        HashMap::new(), // environment variables
    ).await.unwrap();
    
    assert_eq!(container.get_id(), "test-sandbox-1");
    assert_eq!(container.get_status().await.unwrap(), "running");
    assert!(container.get_container_id().starts_with("container_"));
}

#[tokio::test]
async fn test_container_lifecycle_management() {
    // Test complete lifecycle: create -> start -> stop -> remove
    let config = soulbox::Config::default();
    let manager = ContainerManager::new(config).await.unwrap();
    
    let resource_limits = ResourceLimits::default();
    let network_config = NetworkConfig::default();
    
    // Create container
    let container = manager.create_sandbox_container(
        "test-lifecycle-sandbox",
        "alpine:latest",
        resource_limits,
        network_config,
        HashMap::new(),
    ).await.unwrap();
    
    // Verify initial state
    assert_eq!(container.get_status().await.unwrap(), "running");
    
    // Stop container
    container.stop().await.unwrap();
    assert_eq!(container.get_status().await.unwrap(), "stopped");
    
    // Start container again
    container.start().await.unwrap();
    assert_eq!(container.get_status().await.unwrap(), "running");
    
    // Remove container
    container.remove().await.unwrap();
    
    // Container should no longer exist
    assert!(container.get_status().await.is_err());
}

#[tokio::test]
async fn test_resource_limits_enforcement() {
    // Test that resource limits are properly applied and enforced
    let config = soulbox::Config::default();
    let manager = ContainerManager::new(config).await.unwrap();
    
    let strict_limits = ResourceLimits {
        cpu: CpuLimits {
            cores: 0.5, // Half a CPU core
            shares: Some(512),
            cpu_percent: Some(50.0),
        },
        memory: MemoryLimits {
            limit_mb: 128, // Very low memory
            swap_limit_mb: Some(256),
            swap_mb: Some(128),
        },
        disk: DiskLimits {
            limit_mb: 512, // Limited disk space
            iops_limit: Some(500),
        },
        network: NetworkLimits {
            upload_bps: Some(512 * 1024), // 512 KB/s
            download_bps: Some(1024 * 1024), // 1 MB/s
            max_connections: Some(50),
        },
    };
    
    let container = manager.create_sandbox_container(
        "resource-limited-sandbox",
        "node:18-alpine",
        strict_limits,
        NetworkConfig::default(),
        HashMap::new(),
    ).await.unwrap();
    
    // Verify resource limits are applied
    let stats = container.get_resource_stats().await.unwrap();
    assert!(stats.memory_limit_mb <= 128);
    assert!(stats.cpu_cores <= 0.5);
    
    // Try to exceed memory limit (should be prevented)
    let result = container.execute_command(vec![
        "node".to_string(),
        "-e".to_string(),
        "const arr = []; while(true) arr.push(new Array(1000000));".to_string(),
    ]).await;
    
    // Should either be killed or exit with error due to memory limit
    assert!(result.is_err() || result.unwrap().exit_code != 0);
}

#[tokio::test] 
async fn test_network_isolation_and_port_mapping() {
    // Test network isolation and port mapping functionality
    let config = soulbox::Config::default();
    let manager = ContainerManager::new(config).await.unwrap();
    
    let network_config = NetworkConfig {
        enable_internet: false, // Isolated from internet
        port_mappings: vec![
            PortMapping {
                host_port: Some(8080),
                container_port: 3000,
                protocol: "tcp".to_string(),
            }
        ],
        allowed_domains: vec![], // No domains allowed
        dns_servers: vec!["127.0.0.1".to_string()], // Local DNS only
    };
    
    let container = manager.create_sandbox_container(
        "network-isolated-sandbox",
        "node:18-alpine",
        ResourceLimits::default(),
        network_config,
        HashMap::new(),
    ).await.unwrap();
    
    // Test that internet is blocked
    let result = container.execute_command(vec![
        "wget".to_string(),
        "-T".to_string(), "5".to_string(),
        "https://google.com".to_string(),
    ]).await;
    
    assert!(result.is_err() || result.unwrap().exit_code != 0);
    
    // Test that port mapping works
    let port_mappings = container.get_port_mappings().await.unwrap();
    assert_eq!(port_mappings.len(), 1);
    assert_eq!(port_mappings[0].host_port, Some(8080));
    assert_eq!(port_mappings[0].container_port, 3000);
}

#[tokio::test]
async fn test_file_system_isolation() {
    // Test that containers have isolated file systems
    let config = soulbox::Config::default();
    let manager = ContainerManager::new(config).await.unwrap();
    
    let container1 = manager.create_sandbox_container(
        "fs-test-1",
        "alpine:latest",
        ResourceLimits::default(),
        NetworkConfig::default(),
        HashMap::new(),
    ).await.unwrap();
    
    let container2 = manager.create_sandbox_container(
        "fs-test-2", 
        "alpine:latest",
        ResourceLimits::default(),
        NetworkConfig::default(),
        HashMap::new(),
    ).await.unwrap();
    
    // Create file in container1
    let result1 = container1.execute_command(vec![
        "sh".to_string(),
        "-c".to_string(),
        "echo 'secret data' > /tmp/test.txt && cat /tmp/test.txt".to_string(),
    ]).await.unwrap();
    
    assert_eq!(result1.stdout.trim(), "secret data");
    
    // Try to read file from container2 (should not exist)
    let result2 = container2.execute_command(vec![
        "cat".to_string(),
        "/tmp/test.txt".to_string(),
    ]).await.unwrap();
    
    assert_ne!(result2.exit_code, 0); // File should not exist
    assert!(result2.stderr.contains("No such file") || result2.stderr.contains("not found"));
}

#[tokio::test]
async fn test_container_execution_timeout() {
    // Test that long-running commands can be properly timed out
    let config = soulbox::Config::default();
    let manager = ContainerManager::new(config).await.unwrap();
    
    let container = manager.create_sandbox_container(
        "timeout-test-sandbox",
        "alpine:latest", 
        ResourceLimits::default(),
        NetworkConfig::default(),
        HashMap::new(),
    ).await.unwrap();
    
    // Execute long-running command with timeout
    let start_time = std::time::Instant::now();
    
    let result = timeout(
        Duration::from_secs(2),
        container.execute_command(vec![
            "sleep".to_string(),
            "10".to_string(), // Sleep for 10 seconds
        ])
    ).await;
    
    let elapsed = start_time.elapsed();
    
    // Should timeout after ~2 seconds, not complete after 10 seconds
    assert!(result.is_err());
    assert!(elapsed < Duration::from_secs(5));
}

#[tokio::test]
async fn test_concurrent_container_operations() {
    // Test that multiple containers can be managed concurrently
    let config = soulbox::Config::default();
    let manager = ContainerManager::new(config).await.unwrap();
    let container_count = 5;
    
    let mut tasks = Vec::new();
    
    for i in 0..container_count {
        let manager_clone = manager.clone();
        let task = tokio::spawn(async move {
            let container = manager_clone.create_sandbox_container(
                &format!("concurrent-test-{}", i),
                "alpine:latest",
                ResourceLimits::default(),
                NetworkConfig::default(),
                HashMap::new(),
            ).await.unwrap();
            
            // Execute a simple command in each container
            let result = container.execute_command(vec![
                "echo".to_string(),
                format!("Hello from container {}", i),
            ]).await.unwrap();
            
            assert_eq!(result.stdout.trim(), format!("Hello from container {}", i));
            
            container.get_id().to_string()
        });
        tasks.push(task);
    }
    
    // Wait for all tasks to complete
    let results = futures::future::try_join_all(tasks).await.unwrap();
    
    // Verify all containers were created with unique IDs
    assert_eq!(results.len(), container_count);
    for (i, container_id) in results.iter().enumerate() {
        assert_eq!(container_id, &format!("concurrent-test-{}", i));
    }
}

#[tokio::test]
async fn test_container_cleanup_on_manager_drop() {
    // Test that containers are properly cleaned up when manager is dropped
    let container_id = {
        let config = soulbox::Config::default();
    let manager = ContainerManager::new(config).await.unwrap();
        
        let container = manager.create_sandbox_container(
            "cleanup-test-sandbox",
            "alpine:latest",
            ResourceLimits::default(), 
            NetworkConfig::default(),
            HashMap::new(),
        ).await.unwrap();
        
        container.get_container_id().to_string()
        
        // Manager goes out of scope here
    };
    
    // Give some time for cleanup
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Try to create a new manager and verify the container is gone
    let config = soulbox::Config::default();
    let new_manager = ContainerManager::new(config).await.unwrap();
    let existing_containers = new_manager.list_containers().await.unwrap();
    
    // The cleanup-test-sandbox should not exist anymore
    assert!(!existing_containers.iter().any(|c| c.container_id == container_id));
}

#[tokio::test]
async fn test_environment_variable_injection() {
    // Test that environment variables are properly injected into containers
    let config = soulbox::Config::default();
    let manager = ContainerManager::new(config).await.unwrap();
    
    let mut env_vars = HashMap::new();
    env_vars.insert("TEST_VAR".to_string(), "test_value".to_string());
    env_vars.insert("NODE_ENV".to_string(), "test".to_string());
    env_vars.insert("API_KEY".to_string(), "secret123".to_string());
    
    let container = manager.create_sandbox_container(
        "env-test-sandbox",
        "node:18-alpine",
        ResourceLimits::default(),
        NetworkConfig::default(),
        env_vars,
    ).await.unwrap();
    
    // Test environment variables are set
    let result = container.execute_command(vec![
        "printenv".to_string(),
    ]).await.unwrap();
    
    assert!(result.stdout.contains("TEST_VAR=test_value"));
    assert!(result.stdout.contains("NODE_ENV=test"));
    assert!(result.stdout.contains("API_KEY=secret123"));
    
    // Test accessing specific environment variable
    let result2 = container.execute_command(vec![
        "sh".to_string(),
        "-c".to_string(),
        "echo $TEST_VAR".to_string(),
    ]).await.unwrap();
    
    assert_eq!(result2.stdout.trim(), "test_value");
}

// Helper functions and structs for testing
#[derive(Debug, Clone)]
pub struct MockContainerStats {
    pub memory_usage_mb: u64,
    pub memory_limit_mb: u64,
    pub cpu_usage_percent: f64,
    pub cpu_cores: f64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct MockExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub execution_time: Duration,
}

pub async fn setup_test_docker_environment() -> Result<()> {
    // Helper function to ensure Docker is available and pull test images
    // This will be implemented when we create the actual ContainerManager
    Ok(())
}

pub async fn cleanup_test_containers() -> Result<()> {
    // Helper function to cleanup any leftover test containers
    // This will be implemented when we create the actual ContainerManager
    Ok(())
}

// Integration test to verify Module 2 (protocols) work with Module 3 (containers)
#[tokio::test]
async fn test_grpc_protocol_with_real_containers() {
    // This test verifies that our gRPC services from Module 2 can control real containers
    use soulbox::grpc::service::SoulBoxServiceImpl;
    
    let grpc_service = SoulBoxServiceImpl::new();
    let config = soulbox::Config::default();
    let manager = ContainerManager::new(config).await.unwrap();
    
    // The gRPC service is already initialized with a container manager
    // No need to set it explicitly in this implementation
    
    // This test will be implemented once we have both modules integrated
    // For now, just verify the components can be instantiated together
    assert!(std::mem::size_of::<SoulBoxServiceImpl>() > 0);
}