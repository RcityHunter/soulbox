#[cfg(test)]
mod integration_tests {
    use super::super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_real_docker_connection() {
        // Test that we can connect to real Docker daemon
        let result = ContainerManager::new_default();
        assert!(result.is_ok(), "Should connect to Docker daemon");
    }

    #[tokio::test]
    async fn test_container_lifecycle() {
        let manager = match ContainerManager::new_default() {
            Ok(m) => Arc::new(m),
            Err(_) => {
                eprintln!("Skipping test - Docker not available");
                return;
            }
        };

        // Create a container
        let limits = ResourceLimits::default();
        let network = NetworkConfig::default();
        let env = std::collections::HashMap::new();
        
        let container = manager
            .create_sandbox_container("test-sandbox", "alpine:latest", limits, network, env)
            .await;
        
        assert!(container.is_ok(), "Should create container");
        
        let container = container.unwrap();
        
        // Start container
        assert!(container.start().await.is_ok(), "Should start container");
        
        // Check status
        let status = container.get_status().await.unwrap();
        assert_eq!(status, "running");
        
        // Stop container
        assert!(container.stop().await.is_ok(), "Should stop container");
        
        // Remove container
        assert!(container.destroy().await.is_ok(), "Should remove container");
    }

    #[tokio::test]
    async fn test_code_execution() {
        let manager = match ContainerManager::new_default() {
            Ok(m) => Arc::new(m),
            Err(_) => {
                eprintln!("Skipping test - Docker not available");
                return;
            }
        };

        // Create Python container
        let container = manager
            .create_sandbox_container(
                "test-python",
                "python:3.11-slim",
                ResourceLimits::default(),
                NetworkConfig::default(),
                std::collections::HashMap::new(),
            )
            .await
            .expect("Should create Python container");

        container.start().await.expect("Should start container");

        // Execute Python code
        let executor = CodeExecutor::new(container.clone());
        let result = executor
            .execute_python("print('Hello from Docker!')", Duration::from_secs(5))
            .await
            .expect("Should execute Python code");

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Hello from Docker!"));
        
        // Cleanup
        container.destroy().await.expect("Should cleanup");
    }

    #[tokio::test]
    async fn test_resource_limits() {
        let manager = match ContainerManager::new_default() {
            Ok(m) => Arc::new(m),
            Err(_) => {
                eprintln!("Skipping test - Docker not available");
                return;
            }
        };

        // Create container with resource limits
        let limits = ResourceLimits {
            cpu_shares: 256,
            memory_limit: 128 * 1024 * 1024, // 128MB
            memory_swap: 256 * 1024 * 1024,  // 256MB
            cpu_quota: Some(25000),
            cpu_period: Some(100000),
            pids_limit: Some(50),
        };

        let container = manager
            .create_sandbox_container(
                "test-limited",
                "alpine:latest",
                limits,
                NetworkConfig::default(),
                std::collections::HashMap::new(),
            )
            .await
            .expect("Should create limited container");

        container.start().await.expect("Should start");

        // Get stats to verify limits are applied
        let stats = container.get_stats().await.expect("Should get stats");
        
        // Note: Actual limit verification depends on Docker daemon configuration
        // This test just ensures the stats API works
        assert!(stats.memory_stats.is_some());
        
        container.destroy().await.expect("Should cleanup");
    }

    #[tokio::test]
    async fn test_sandbox_manager_integration() {
        use crate::sandbox_manager::SandboxManager;

        let manager = match ContainerManager::new_default() {
            Ok(m) => Arc::new(m),
            Err(_) => {
                eprintln!("Skipping test - Docker not available");
                return;
            }
        };

        let sandbox_mgr = SandboxManager::new(manager);

        // Create sandbox
        let sandbox_id = sandbox_mgr
            .create_sandbox("python", None, None)
            .await
            .expect("Should create sandbox");

        // Execute code
        let result = sandbox_mgr
            .execute_code(
                &sandbox_id,
                "print(2 + 2)",
                Some(Duration::from_secs(5)),
            )
            .await
            .expect("Should execute code");

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("4"));

        // Get summary
        let summary = sandbox_mgr
            .get_sandbox_summary(&sandbox_id)
            .await
            .expect("Should get summary");
        
        assert_eq!(summary.id, sandbox_id);
        assert_eq!(summary.status, "running");

        // Cleanup
        sandbox_mgr
            .destroy_sandbox(&sandbox_id)
            .await
            .expect("Should destroy sandbox");
    }
}