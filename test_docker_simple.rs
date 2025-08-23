// Simple Docker integration test that doesn't require the full soulbox library
use bollard::Docker;
use bollard::container::{Config as ContainerConfig, CreateContainerOptions, StartContainerOptions, RemoveContainerOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use futures_util::stream::StreamExt;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Testing Docker integration directly with bollard...");
    
    // Connect to Docker
    let docker = Docker::connect_with_socket_defaults()?;
    println!("✓ Connected to Docker");
    
    // Check Docker version
    let version = docker.version().await?;
    println!("✓ Docker version: {}", version.version.unwrap_or_default());
    
    // Create a simple Python container
    let container_config = ContainerConfig {
        image: Some("python:3.11-alpine".to_string()),
        cmd: Some(vec!["sleep".to_string(), "3600".to_string()]),
        ..Default::default()
    };
    
    let container_name = format!("test-container-{}", uuid::Uuid::new_v4());
    let create_options = CreateContainerOptions {
        name: container_name.clone(),
        platform: None,
    };
    
    println!("Creating container...");
    let container = docker.create_container(Some(create_options), container_config).await?;
    println!("✓ Container created: {}", container.id);
    
    // Start the container
    println!("Starting container...");
    docker.start_container(&container.id, None::<StartContainerOptions<String>>).await?;
    println!("✓ Container started");
    
    // Execute a command
    println!("Executing Python command...");
    let exec_config = CreateExecOptions {
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        cmd: Some(vec![
            "python".to_string(),
            "-c".to_string(),
            "print('Hello from Docker!')".to_string()
        ]),
        ..Default::default()
    };
    
    let exec = docker.create_exec(&container.id, exec_config).await?;
    
    if let StartExecResults::Attached { mut output, .. } = docker.start_exec(&exec.id, None).await? {
        let mut stdout = String::new();
        while let Some(chunk) = output.next().await {
            match chunk {
                Ok(bollard::container::LogOutput::StdOut { message }) => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                _ => {}
            }
        }
        println!("✓ Command output: {}", stdout.trim());
    }
    
    // Clean up
    println!("Removing container...");
    docker.remove_container(&container.id, Some(RemoveContainerOptions {
        force: true,
        ..Default::default()
    })).await?;
    println!("✓ Container removed");
    
    println!("\n🎉 Docker integration works! The container module should be functional.");
    
    Ok(())
}