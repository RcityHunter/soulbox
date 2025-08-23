use soulbox::container::{ContainerManager, CodeExecutor, NetworkConfig};
use soulbox::container::resource_limits::{ResourceLimits, CpuLimits, MemoryLimits, DiskLimits, NetworkLimits};
use soulbox::sandbox_manager::SandboxManager;
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 Testing SoulBox Docker Integration\n");

    // Create container manager with real Docker connection
    println!("1. Connecting to Docker daemon...");
    let config = soulbox::config::Config::default();
    let container_manager = Arc::new(ContainerManager::new(config).await?);
    println!("   ✅ Docker connected successfully\n");

    // Create sandbox manager
    println!("2. Creating sandbox manager...");
    let sandbox_manager = SandboxManager::new(container_manager.clone());
    println!("   ✅ Sandbox manager ready\n");

    // Create a Python sandbox
    println!("3. Creating Python sandbox...");
    let resource_limits = ResourceLimits {
        cpu: CpuLimits {
            cores: 0.5,
            shares: Some(512),
            cpu_percent: Some(50.0),
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
            upload_bps: Some(1024 * 1024),
            download_bps: Some(2 * 1024 * 1024),
            max_connections: Some(100),
        },
    };

    let sandbox_id = sandbox_manager
        .create_sandbox("python", Some(resource_limits), None)
        .await?;
    println!("   ✅ Sandbox created: {}\n", sandbox_id);

    // Execute Python code
    println!("4. Executing Python code...");
    let python_code = r#"
import sys
import platform

print(f"Python Version: {sys.version}")
print(f"Platform: {platform.platform()}")
print(f"Machine: {platform.machine()}")

# Calculate fibonacci
def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n-1) + fibonacci(n-2)

for i in range(10):
    print(f"fibonacci({i}) = {fibonacci(i)}")

print("\n🎉 Code execution successful!")
"#;

    let result = sandbox_manager
        .execute_code(&sandbox_id, python_code, Some(Duration::from_secs(10)))
        .await?;

    println!("   📋 Execution Result:");
    println!("   Exit Code: {}", result.exit_code);
    println!("   Execution Time: {:?}", result.execution_time);
    println!("\n   📝 Output:");
    println!("{}", result.stdout);
    
    if !result.stderr.is_empty() {
        println!("\n   ⚠️ Errors:");
        println!("{}", result.stderr);
    }

    // Execute a simple calculation
    println!("\n5. Testing simple calculation...");
    let calc_code = "result = 2 ** 10\nprint(f'2^10 = {result}')";
    let calc_result = sandbox_manager
        .execute_code(&sandbox_id, calc_code, Some(Duration::from_secs(5)))
        .await?;
    println!("   📝 Output: {}", calc_result.stdout.trim());

    // Test error handling
    println!("\n6. Testing error handling...");
    let error_code = "import nonexistent_module";
    let error_result = sandbox_manager
        .execute_code(&sandbox_id, error_code, Some(Duration::from_secs(5)))
        .await?;
    println!("   ⚠️ Expected error: {}", error_result.stderr.trim());

    // Destroy sandbox
    println!("\n7. Cleaning up sandbox...");
    sandbox_manager.destroy_sandbox(&sandbox_id).await?;
    println!("   ✅ Sandbox destroyed\n");

    println!("🎉 All tests passed! Docker integration is working correctly.");

    Ok(())
}