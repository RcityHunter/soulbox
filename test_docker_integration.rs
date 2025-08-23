use soulbox::container::{ContainerManager, ResourceLimits, NetworkConfig};
use std::sync::Arc;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!("Testing Docker integration...");
    
    // Create container manager
    let config = soulbox::config::Config::default();
    let container_manager = match ContainerManager::new(config).await {
        Ok(mgr) => Arc::new(mgr),
        Err(e) => {
            eprintln!("Failed to create container manager: {}", e);
            eprintln!("Make sure Docker is running!");
            return Err(e.into());
        }
    };
    
    println!("✓ Container manager created successfully");
    
    // Create a test sandbox
    let sandbox_id = "test-sandbox-001";
    let image = "python:3.11-slim";
    let resource_limits = ResourceLimits::default();
    let network_config = NetworkConfig::default();
    let env_vars = HashMap::new();
    
    println!("Creating sandbox container...");
    let container = match container_manager
        .create_sandbox_container(sandbox_id, image, resource_limits, network_config, env_vars)
        .await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create container: {}", e);
            return Err(e.into());
        }
    };
    
    println!("✓ Container created: {}", container.get_container_id());
    
    // Start the container
    println!("Starting container...");
    container.start().await?;
    println!("✓ Container started");
    
    // Execute a simple command
    println!("Executing test command...");
    let result = container.execute_command(vec![
        "python3".to_string(),
        "-c".to_string(),
        "print('Hello from Docker container!')".to_string()
    ]).await?;
    
    println!("✓ Command executed:");
    println!("  stdout: {}", result.stdout.trim());
    println!("  stderr: {}", result.stderr.trim());
    println!("  exit_code: {}", result.exit_code);
    
    // Get container status
    let status = container.get_status().await?;
    println!("✓ Container status: {}", status);
    
    // Clean up
    println!("Cleaning up...");
    container_manager.remove_container(sandbox_id).await?;
    println!("✓ Container removed");
    
    println!("\n🎉 All tests passed! Docker integration is working!");
    
    Ok(())
}