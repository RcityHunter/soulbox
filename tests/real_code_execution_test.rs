//! Real code execution tests - verifies actual Docker container execution

use soulbox::{SandboxManager, ContainerManager, ResourceLimits};
use soulbox::container::resource_limits::{MemoryLimits, CpuLimits, DiskLimits, NetworkLimits};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
#[ignore] // Requires Docker to be running
async fn test_real_python_execution() {
    // Create container manager
    let container_manager = ContainerManager::new_default()
        .expect("Failed to create container manager");
    
    // Create sandbox manager
    let sandbox_manager = SandboxManager::new(Arc::new(container_manager));
    
    // Python code to execute
    let python_code = r#"
print("Hello from Python!")
x = 5
y = 10
result = x + y
print(f"Result: {result}")
"#;
    
    // Execute code in a new sandbox
    let result = sandbox_manager.execute_in_new_sandbox(
        python_code,
        "python",
        None,
        Some(Duration::from_secs(10))
    ).await;
    
    // Verify execution
    assert!(result.is_ok(), "Python execution failed: {:?}", result);
    
    let execution_result = result.unwrap();
    assert_eq!(execution_result.exit_code, 0, "Python code should execute successfully");
    assert!(execution_result.stdout.contains("Hello from Python!"));
    assert!(execution_result.stdout.contains("Result: 15"));
}

#[tokio::test]
#[ignore] // Requires Docker to be running
async fn test_real_nodejs_execution() {
    // Create container manager
    let container_manager = ContainerManager::new_default()
        .expect("Failed to create container manager");
    
    // Create sandbox manager
    let sandbox_manager = SandboxManager::new(Arc::new(container_manager));
    
    // JavaScript code to execute
    let js_code = r#"
console.log("Hello from Node.js!");
const x = 5;
const y = 10;
const result = x + y;
console.log(`Result: ${result}`);
"#;
    
    // Execute code in a new sandbox
    let result = sandbox_manager.execute_in_new_sandbox(
        js_code,
        "nodejs",
        None,
        Some(Duration::from_secs(10))
    ).await;
    
    // Verify execution
    assert!(result.is_ok(), "Node.js execution failed: {:?}", result);
    
    let execution_result = result.unwrap();
    assert_eq!(execution_result.exit_code, 0, "JavaScript code should execute successfully");
    assert!(execution_result.stdout.contains("Hello from Node.js!"));
    assert!(execution_result.stdout.contains("Result: 15"));
}

#[tokio::test]
#[ignore] // Requires Docker to be running
async fn test_code_with_error() {
    // Create container manager
    let container_manager = ContainerManager::new_default()
        .expect("Failed to create container manager");
    
    // Create sandbox manager
    let sandbox_manager = SandboxManager::new(Arc::new(container_manager));
    
    // Python code with error
    let python_code = r#"
print("Before error")
raise ValueError("This is an intentional error")
print("This should not be printed")
"#;
    
    // Execute code
    let result = sandbox_manager.execute_in_new_sandbox(
        python_code,
        "python",
        None,
        Some(Duration::from_secs(10))
    ).await;
    
    // Verify execution
    assert!(result.is_ok(), "Execution should complete even with error");
    
    let execution_result = result.unwrap();
    assert_ne!(execution_result.exit_code, 0, "Code with error should have non-zero exit code");
    assert!(execution_result.stdout.contains("Before error"));
    assert!(execution_result.stderr.contains("ValueError"));
}

#[tokio::test]
#[ignore] // Requires Docker to be running
async fn test_resource_limits() {
    // Create container manager
    let container_manager = ContainerManager::new_default()
        .expect("Failed to create container manager");
    
    // Create sandbox manager
    let sandbox_manager = SandboxManager::new(Arc::new(container_manager));
    
    // Create strict resource limits
    let resource_limits = ResourceLimits {
        memory: MemoryLimits {
            limit_mb: 64,
            swap_limit_mb: Some(0),
            swap_mb: Some(0),
        },
        cpu: CpuLimits {
            cores: 0.5,
            shares: Some(512),
            cpu_percent: Some(50.0),
        },
        disk: DiskLimits {
            limit_mb: 100,
            iops_limit: Some(100),
        },
        network: NetworkLimits {
            upload_bps: Some(1000 * 1024),
            download_bps: Some(1000 * 1024),
            max_connections: Some(100),
        },
    };
    
    // Python code that uses some resources
    let python_code = r#"
import time
print("Starting resource test")
# Allocate some memory
data = [i for i in range(100000)]
print(f"Allocated list with {len(data)} items")
time.sleep(1)
print("Resource test completed")
"#;
    
    // Execute with resource limits
    let result = sandbox_manager.execute_in_new_sandbox(
        python_code,
        "python",
        Some(resource_limits),
        Some(Duration::from_secs(10))
    ).await;
    
    // Verify execution
    assert!(result.is_ok(), "Execution with resource limits failed: {:?}", result);
    
    let execution_result = result.unwrap();
    assert_eq!(execution_result.exit_code, 0, "Code should execute within resource limits");
    assert!(execution_result.stdout.contains("Resource test completed"));
}

#[tokio::test]
#[ignore] // Requires Docker to be running
async fn test_timeout() {
    // Create container manager
    let container_manager = ContainerManager::new_default()
        .expect("Failed to create container manager");
    
    // Create sandbox manager
    let sandbox_manager = SandboxManager::new(Arc::new(container_manager));
    
    // Python code that runs for too long
    let python_code = r#"
import time
print("Starting long-running process")
time.sleep(10)  # Sleep for 10 seconds
print("This should not be printed due to timeout")
"#;
    
    // Execute with short timeout
    let result = sandbox_manager.execute_in_new_sandbox(
        python_code,
        "python",
        None,
        Some(Duration::from_secs(2)) // 2 second timeout
    ).await;
    
    // Verify execution
    assert!(result.is_ok(), "Execution should complete even with timeout");
    
    let execution_result = result.unwrap();
    assert_ne!(execution_result.exit_code, 0, "Code should be terminated due to timeout");
    assert!(execution_result.stdout.contains("Starting long-running process"));
    assert!(!execution_result.stdout.contains("This should not be printed"));
}

#[tokio::test]
#[ignore] // Requires Docker to be running
async fn test_sandbox_isolation() {
    // Create container manager
    let container_manager = ContainerManager::new_default()
        .expect("Failed to create container manager");
    
    // Create sandbox manager
    let sandbox_manager = Arc::new(SandboxManager::new(Arc::new(container_manager)));
    
    // Create two sandboxes
    let sandbox1 = sandbox_manager.create_sandbox("python", None, None).await
        .expect("Failed to create sandbox 1");
    
    let sandbox2 = sandbox_manager.create_sandbox("python", None, None).await
        .expect("Failed to create sandbox 2");
    
    // Write file in sandbox 1
    let code1 = r#"
with open('/tmp/test.txt', 'w') as f:
    f.write('Sandbox 1')
print("File written in sandbox 1")
"#;
    
    let result1 = sandbox_manager.execute_code(&sandbox1, code1, None).await
        .expect("Failed to execute in sandbox 1");
    assert_eq!(result1.exit_code, 0);
    
    // Try to read file in sandbox 2 (should not exist)
    let code2 = r#"
try:
    with open('/tmp/test.txt', 'r') as f:
        content = f.read()
        print(f"File content: {content}")
except FileNotFoundError:
    print("File not found - sandboxes are isolated")
"#;
    
    let result2 = sandbox_manager.execute_code(&sandbox2, code2, None).await
        .expect("Failed to execute in sandbox 2");
    assert_eq!(result2.exit_code, 0);
    assert!(result2.stdout.contains("File not found - sandboxes are isolated"));
    
    // Clean up
    sandbox_manager.destroy_sandbox(&sandbox1).await.expect("Failed to destroy sandbox 1");
    sandbox_manager.destroy_sandbox(&sandbox2).await.expect("Failed to destroy sandbox 2");
}