#[cfg(test)]
mod integration_tests {
    use super::super::*;
    use crate::container::resource_limits::{ResourceLimits, CpuLimits, MemoryLimits, DiskLimits, NetworkLimits};
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
        assert!(container.remove().await.is_ok(), "Should remove container");
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
        container.remove().await.expect("Should cleanup");
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
            cpu: CpuLimits {
                cores: 0.5,
                shares: Some(256),
                cpu_percent: Some(50.0),
            },
            memory: MemoryLimits {
                limit_mb: 128,
                swap_limit_mb: Some(256),
                swap_mb: Some(128),
            },
            disk: DiskLimits {
                limit_mb: 1024,
                iops_limit: Some(500),
            },
            network: NetworkLimits {
                upload_bps: Some(512 * 1024),
                download_bps: Some(2 * 1024 * 1024),
                max_connections: Some(50),
            },
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
        let stats = container.get_resource_stats().await.expect("Should get stats");
        
        // Note: Actual limit verification depends on Docker daemon configuration
        // This test just ensures the stats API works
        assert!(stats.memory_usage_mb > 0);
        
        container.remove().await.expect("Should cleanup");
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
        let summaries = sandbox_mgr.list_sandboxes().await;
        let summary = summaries.iter().find(|s| s.sandbox_id == sandbox_id)
            .expect("Should find created sandbox");
        
        assert_eq!(summary.sandbox_id, sandbox_id);
        assert_eq!(summary.language, "python");

        // Cleanup
        sandbox_mgr
            .destroy_sandbox(&sandbox_id)
            .await
            .expect("Should destroy sandbox");
    }
}