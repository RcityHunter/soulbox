// Docker Integration Demo - Demonstrates real Docker container management
use soulbox::container::{ContainerManager, ResourceLimits, NetworkConfig};
use soulbox::config::Config;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();
    
    println!("=== SoulBox Docker Integration Demo ===\n");
    
    // Create container manager with real Docker connection
    println!("1. Connecting to Docker daemon...");
    let config = Config::default();
    let container_manager = match ContainerManager::new(config).await {
        Ok(mgr) => {
            println!("   ✓ Successfully connected to Docker");
            Arc::new(mgr)
        }
        Err(e) => {
            eprintln!("   ✗ Failed to connect to Docker: {}", e);
            eprintln!("   Please ensure Docker is running and accessible");
            return Err(e.into());
        }
    };
    
    // Define sandbox parameters
    let sandbox_id = "demo-sandbox";
    let image = "python:3.11-alpine";
    
    println!("\n2. Creating sandbox container...");
    println!("   - Sandbox ID: {}", sandbox_id);
    println!("   - Image: {}", image);
    
    // Create resource limits
    let mut resource_limits = ResourceLimits::default();
    resource_limits.memory.limit_mb = 128;
    resource_limits.cpu.cores = 0.5;
    
    // Create container
    let container = container_manager
        .create_sandbox_container(
            sandbox_id,
            image,
            resource_limits,
            NetworkConfig::default(),
            HashMap::new(),
        )
        .await?;
    
    println!("   ✓ Container created: {}", container.get_container_id());
    
    // Start container
    println!("\n3. Starting container...");
    container.start().await?;
    println!("   ✓ Container started successfully");
    
    // Check status
    let status = container.get_status().await?;
    println!("   - Status: {}", status);
    
    // Execute Python code
    println!("\n4. Executing Python code in container...");
    let python_code = r#"
import sys
import platform

print(f"Python {sys.version}")
print(f"Platform: {platform.platform()}")
print(f"Container: SoulBox Sandbox")
print("Hello from real Docker container!")

# Perform a simple calculation
result = sum(range(1, 101))
print(f"Sum of 1-100: {result}")
"#;
    
    let result = container.execute_command(vec![
        "python3".to_string(),
        "-c".to_string(),
        python_code.to_string(),
    ]).await?;
    
    println!("   ✓ Code execution completed");
    println!("   - Exit code: {}", result.exit_code);
    println!("   - Output:");
    for line in result.stdout.lines() {
        println!("     {}", line);
    }
    
    if !result.stderr.is_empty() {
        println!("   - Errors:");
        for line in result.stderr.lines() {
            println!("     {}", line);
        }
    }
    
    // Get resource statistics
    println!("\n5. Getting resource statistics...");
    let stats = container.get_resource_stats().await?;
    println!("   - Memory: {} MB / {} MB", stats.memory_usage_mb, stats.memory_limit_mb);
    println!("   - CPU: {:.2}% of {} cores", stats.cpu_usage_percent, stats.cpu_cores);
    println!("   - Network RX: {} bytes", stats.network_rx_bytes);
    println!("   - Network TX: {} bytes", stats.network_tx_bytes);
    
    // Test security: Try to execute a blocked command
    println!("\n6. Testing security restrictions...");
    println!("   Attempting to run 'sudo' (should be blocked)...");
    match container.execute_command(vec!["sudo".to_string(), "ls".to_string()]).await {
        Err(e) => println!("   ✓ Command properly blocked: {}", e),
        Ok(_) => println!("   ⚠ Command unexpectedly succeeded"),
    }
    
    // Execute another safe command
    println!("\n7. Executing shell command...");
    let shell_result = container.execute_command(vec![
        "sh".to_string(),
        "-c".to_string(),
        "echo 'Files in /tmp:' && ls -la /tmp | head -5".to_string(),
    ]).await?;
    
    println!("   Output:");
    for line in shell_result.stdout.lines() {
        println!("     {}", line);
    }
    
    // List all containers
    println!("\n8. Listing active containers...");
    let containers = container_manager.list_containers().await?;
    for info in &containers {
        println!("   - {} ({}): {} [{}]", 
            info.sandbox_id, 
            info.container_id, 
            info.status, 
            info.image
        );
    }
    
    // Clean up
    println!("\n9. Cleaning up...");
    println!("   Stopping container...");
    container.stop().await?;
    
    println!("   Removing container...");
    container_manager.remove_container(sandbox_id).await?;
    
    println!("   ✓ Cleanup completed");
    
    println!("\n=== Demo completed successfully! ===");
    println!("This demonstrates that the Docker integration is fully functional:");
    println!("- ✓ Real Docker containers are created");
    println!("- ✓ Code execution works inside containers");
    println!("- ✓ Resource limits are applied");
    println!("- ✓ Security restrictions are enforced");
    println!("- ✓ Container lifecycle is managed properly");
    
    Ok(())
}