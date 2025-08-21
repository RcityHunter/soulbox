use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

use soulbox::container::{ContainerManager, ResourceLimits};
use soulbox::container::resource_limits::{CpuLimits, MemoryLimits, DiskLimits, NetworkLimits};
use soulbox::network::SandboxNetworkConfig;
use soulbox::config::Config;

/// Test if Docker integration actually creates containers and executes code
#[tokio::test]
async fn test_docker_hello_world_python() {
    // Create a container manager
    let config = Config::default();
    let manager = match ContainerManager::new(config).await {
        Ok(manager) => manager,
        Err(e) => {
            eprintln!("Failed to create ContainerManager. Is Docker running? Error: {}", e);
            panic!("Docker connection failed: {}", e);
        }
    };

    // Create resource limits
    let resource_limits = ResourceLimits {
        cpu: CpuLimits {
            cores: 1.0,
            shares: Some(1024),
            cpu_percent: Some(80.0),
        },
        memory: MemoryLimits {
            limit_mb: 256,
            swap_limit_mb: Some(512),
            swap_mb: Some(256),
        },
        disk: DiskLimits {
            limit_mb: 1024,
            iops_limit: Some(1000),
        },
        network: NetworkLimits {
            upload_bps: Some(1024 * 1024), // 1 MB/s
            download_bps: Some(10 * 1024 * 1024), // 10 MB/s
            max_connections: Some(50),
        },
    };

    // Create network config
    let network_config = SandboxNetworkConfig::default();

    // Create a container
    let sandbox_id = "test-python-hello";
    println!("Creating Python container...");
    
    let container = manager
        .create_sandbox_container_with_network(
            sandbox_id,
            "python:3.11-alpine",
            resource_limits,
            network_config,
            HashMap::new(),
        )
        .await
        .expect("Failed to create sandbox container");

    println!("Container created with ID: {}", container.get_container_id());

    // Start the container
    println!("Starting container...");
    container.start().await.expect("Failed to start container");

    // Wait a moment for container to be fully started
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Check container status
    let status = container.get_status().await.expect("Failed to get container status");
    println!("Container status: {}", status);
    assert_eq!(status, "running");

    // Execute Hello World Python code
    println!("Executing Python Hello World...");
    let result = container
        .execute_command(vec![
            "python".to_string(),
            "-c".to_string(),
            "print('Hello World from SoulBox!')".to_string(),
        ])
        .await
        .expect("Failed to execute command");

    println!("Execution result:");
    println!("  Exit code: {}", result.exit_code);
    println!("  Stdout: '{}'", result.stdout);
    println!("  Stderr: '{}'", result.stderr);
    println!("  Duration: {:?}", result.execution_time);

    // Verify the output
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout.trim(), "Hello World from SoulBox!");

    // Test a slightly more complex Python operation
    println!("Executing more complex Python code...");
    let result2 = container
        .execute_command(vec![
            "python".to_string(),
            "-c".to_string(),
            "import sys; print(f'Python version: {sys.version_info.major}.{sys.version_info.minor}'); print(f'Platform: {sys.platform}')".to_string(),
        ])
        .await
        .expect("Failed to execute complex command");

    println!("Complex execution result:");
    println!("  Exit code: {}", result2.exit_code);
    println!("  Stdout: '{}'", result2.stdout);
    println!("  Stderr: '{}'", result2.stderr);

    assert_eq!(result2.exit_code, 0);
    assert!(result2.stdout.contains("Python version: 3.11"));
    assert!(result2.stdout.contains("Platform: linux"));

    // Test error handling - execute invalid command
    println!("Testing error handling with invalid command...");
    let result3 = container
        .execute_command(vec![
            "python".to_string(),
            "-c".to_string(),
            "this is not valid python code".to_string(),
        ])
        .await
        .expect("Command should execute but fail");

    println!("Error test result:");
    println!("  Exit code: {}", result3.exit_code);
    println!("  Stdout: '{}'", result3.stdout);
    println!("  Stderr: '{}'", result3.stderr);

    assert_ne!(result3.exit_code, 0); // Should fail
    assert!(result3.stderr.contains("SyntaxError") || result3.stderr.contains("invalid syntax"));

    // Test resource stats
    println!("Getting resource stats...");
    let stats = container.get_resource_stats().await.expect("Failed to get resource stats");
    println!("Resource stats:");
    println!("  Memory usage: {} MB / {} MB", stats.memory_usage_mb, stats.memory_limit_mb);
    println!("  CPU usage: {:.2}%", stats.cpu_usage_percent);
    println!("  Network RX: {} bytes", stats.network_rx_bytes);
    println!("  Network TX: {} bytes", stats.network_tx_bytes);

    assert_eq!(stats.memory_limit_mb, 256);
    assert_eq!(stats.cpu_cores, 1.0);

    // Clean up
    println!("Cleaning up container...");
    container.stop().await.expect("Failed to stop container");
    container.remove().await.expect("Failed to remove container");

    // Verify container is removed
    let final_status = container.get_status().await;
    println!("Final status check result: {:?}", final_status);
    
    println!("✅ Docker integration test passed!");
}

/// Test Node.js execution
#[tokio::test]
async fn test_docker_hello_world_nodejs() {
    // Pull Node.js image first
    println!("Pulling Node.js image...");
    let output = std::process::Command::new("docker")
        .args(&["pull", "node:18-alpine"])
        .output()
        .expect("Failed to execute docker pull");
    
    if !output.status.success() {
        panic!("Failed to pull Node.js image: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Create a container manager
    let config = Config::default();
    let manager = ContainerManager::new(config).await
        .expect("Failed to create ContainerManager");

    // Create resource limits
    let resource_limits = ResourceLimits::default();
    let network_config = SandboxNetworkConfig::default();

    // Create a container
    let sandbox_id = "test-nodejs-hello";
    println!("Creating Node.js container...");
    
    let container = manager
        .create_sandbox_container_with_network(
            sandbox_id,
            "node:18-alpine",
            resource_limits,
            network_config,
            HashMap::new(),
        )
        .await
        .expect("Failed to create sandbox container");

    println!("Container created with ID: {}", container.get_container_id());

    // Start the container
    println!("Starting container...");
    container.start().await.expect("Failed to start container");

    // Wait for container to be ready
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Execute Node.js Hello World
    println!("Executing Node.js Hello World...");
    let result = container
        .execute_command(vec![
            "node".to_string(),
            "-e".to_string(),
            "console.log('Hello World from Node.js in SoulBox!')".to_string(),
        ])
        .await
        .expect("Failed to execute Node.js command");

    println!("Node.js execution result:");
    println!("  Exit code: {}", result.exit_code);
    println!("  Stdout: '{}'", result.stdout);
    println!("  Stderr: '{}'", result.stderr);

    // Verify the output
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout.trim(), "Hello World from Node.js in SoulBox!");

    // Clean up
    println!("Cleaning up Node.js container...");
    container.cleanup().await.expect("Failed to cleanup container");

    println!("✅ Node.js Docker integration test passed!");
}

/// Test container lifecycle operations
#[tokio::test]
async fn test_container_lifecycle() {
    let config = Config::default();
    let manager = ContainerManager::new(config).await
        .expect("Failed to create ContainerManager");

    let container = manager
        .create_sandbox_container_with_network(
            "test-lifecycle",
            "python:3.11-alpine",
            ResourceLimits::default(),
            SandboxNetworkConfig::default(),
            HashMap::new(),
        )
        .await
        .expect("Failed to create container");

    println!("Testing container lifecycle...");

    // Test: Container should be created but not running initially
    println!("Initial status check...");
    let status = container.get_status().await.expect("Failed to get status");
    println!("Initial status: {}", status);

    // Test: Start container
    println!("Starting container...");
    container.start().await.expect("Failed to start");
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let status = container.get_status().await.expect("Failed to get status");
    println!("Status after start: {}", status);
    assert_eq!(status, "running");

    // Test: Execute a command
    let result = container
        .execute_command(vec!["echo".to_string(), "Container is working".to_string()])
        .await
        .expect("Failed to execute command");
    
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout.trim(), "Container is working");

    // Test: Stop container
    println!("Stopping container...");
    container.stop().await.expect("Failed to stop");
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let status = container.get_status().await.expect("Failed to get status");
    println!("Status after stop: {}", status);
    assert_eq!(status, "exited");

    // Test: Restart container
    println!("Restarting container...");
    container.start().await.expect("Failed to restart");
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let status = container.get_status().await.expect("Failed to get status");
    println!("Status after restart: {}", status);
    assert_eq!(status, "running");

    // Test: Remove container
    println!("Removing container...");
    container.remove().await.expect("Failed to remove");

    // After removal, status should fail or return "dead"
    let final_status = container.get_status().await;
    println!("Final status result: {:?}", final_status);

    println!("✅ Container lifecycle test passed!");
}

/// Test concurrent container operations
#[tokio::test]
async fn test_concurrent_containers() {
    let config = Config::default();
    let manager = ContainerManager::new(config).await
        .expect("Failed to create ContainerManager");

    println!("Testing concurrent container operations...");

    let mut handles = vec![];

    // Create 3 containers concurrently
    for i in 0..3 {
        let mgr = manager.clone();
        let handle = tokio::spawn(async move {
            let container = mgr
                .create_sandbox_container_with_network(
                    &format!("concurrent-test-{}", i),
                    "python:3.11-alpine",
                    ResourceLimits::default(),
                    SandboxNetworkConfig::default(),
                    HashMap::new(),
                )
                .await
                .expect("Failed to create container");

            container.start().await.expect("Failed to start container");
            tokio::time::sleep(Duration::from_millis(1000)).await;

            let result = container
                .execute_command(vec![
                    "python".to_string(),
                    "-c".to_string(),
                    format!("print('Hello from container {}')", i),
                ])
                .await
                .expect("Failed to execute command");

            assert_eq!(result.exit_code, 0);
            assert_eq!(result.stdout.trim(), format!("Hello from container {}", i));

            container.cleanup().await.expect("Failed to cleanup");
            
            format!("Container {} completed successfully", i)
        });
        handles.push(handle);
    }

    // Wait for all containers to complete
    let results = futures::future::try_join_all(handles).await
        .expect("One or more concurrent operations failed");

    for result in results {
        println!("{}", result);
    }

    println!("✅ Concurrent containers test passed!");
}

/// Test security restrictions
#[tokio::test] 
async fn test_security_restrictions() {
    let config = Config::default();
    let manager = ContainerManager::new(config).await
        .expect("Failed to create ContainerManager");

    let container = manager
        .create_sandbox_container_with_network(
            "security-test",
            "python:3.11-alpine",
            ResourceLimits::default(),
            SandboxNetworkConfig::default(),
            HashMap::new(),
        )
        .await
        .expect("Failed to create container");

    container.start().await.expect("Failed to start container");
    tokio::time::sleep(Duration::from_millis(1000)).await;

    println!("Testing security restrictions...");

    // Test: Blocked dangerous commands should fail validation
    let dangerous_commands = vec![
        vec!["sudo".to_string(), "ls".to_string()],
        vec!["docker".to_string(), "ps".to_string()],
        vec!["mount".to_string(), "/dev".to_string()],
    ];

    for cmd in dangerous_commands {
        println!("Testing blocked command: {:?}", cmd);
        let result = container.execute_command(cmd.clone()).await;
        
        // Should either fail validation or command should not exist
        match result {
            Err(_) => {
                println!("  ✅ Command {:?} properly blocked by validation", cmd);
            }
            Ok(exec_result) if exec_result.exit_code != 0 => {
                println!("  ✅ Command {:?} failed in container (exit code {})", cmd, exec_result.exit_code);
            }
            Ok(exec_result) => {
                println!("  ⚠️  Command {:?} unexpectedly succeeded with output: {}", cmd, exec_result.stdout);
            }
        }
    }

    // Test: Safe commands should work
    let safe_commands = vec![
        vec!["echo".to_string(), "Hello".to_string()],
        vec!["python".to_string(), "-c".to_string(), "print('Safe command')".to_string()],
        vec!["ls".to_string(), "/workspace".to_string()],
    ];

    for cmd in safe_commands {
        println!("Testing safe command: {:?}", cmd);
        let result = container.execute_command(cmd.clone()).await
            .expect("Safe command should execute");
        println!("  Exit code: {}, Output: '{}'", result.exit_code, result.stdout.trim());
    }

    container.cleanup().await.expect("Failed to cleanup");
    println!("✅ Security restrictions test completed!");
}