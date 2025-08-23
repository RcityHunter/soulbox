// Minimal test to verify Docker container functionality
use bollard::Docker;
use bollard::container::{Config as ContainerConfig, CreateContainerOptions, StartContainerOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing Docker Container Integration\n");
    
    // 1. Connect to Docker
    println!("Connecting to Docker...");
    let docker = match Docker::connect_with_socket_defaults() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to connect to Docker: {}", e);
            eprintln!("Please ensure Docker is running!");
            return Err(e.into());
        }
    };
    
    // 2. Verify Docker is accessible
    let version = docker.version().await?;
    println!("✓ Docker version: {}", version.version.unwrap_or_default());
    
    // 3. Create a Python container
    let container_name = format!("soulbox-test-{}", chrono::Utc::now().timestamp());
    println!("\nCreating container: {}", container_name);
    
    let config = ContainerConfig {
        image: Some("python:3.11-alpine".to_string()),
        cmd: Some(vec!["sleep".to_string(), "300".to_string()]),
        working_dir: Some("/workspace".to_string()),
        user: Some("1000:1000".to_string()),
        ..Default::default()
    };
    
    let create_options = CreateContainerOptions {
        name: container_name.clone(),
        platform: None,
    };
    
    let container = docker.create_container(Some(create_options), config).await?;
    println!("✓ Container created: {}", container.id);
    
    // 4. Start the container
    println!("\nStarting container...");
    docker.start_container(&container.id, None::<StartContainerOptions<String>>).await?;
    println!("✓ Container started");
    
    // 5. Execute Python code
    println!("\nExecuting Python code...");
    let exec_config = CreateExecOptions {
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        cmd: Some(vec![
            "python".to_string(),
            "-c".to_string(),
            r#"
print('Hello from Docker Container!')
print('Python is working correctly')
result = sum(range(1, 11))
print(f'Sum of 1-10: {result}')
"#.to_string()
        ]),
        ..Default::default()
    };
    
    let exec = docker.create_exec(&container.id, exec_config).await?;
    
    if let StartExecResults::Attached { mut output, .. } = docker.start_exec(&exec.id, None).await? {
        let mut stdout = String::new();
        let mut stderr = String::new();
        
        while let Some(chunk) = output.next().await {
            match chunk {
                Ok(bollard::container::LogOutput::StdOut { message }) => {
                    stdout.push_str(&String::from_utf8_lossy(&message));
                }
                Ok(bollard::container::LogOutput::StdErr { message }) => {
                    stderr.push_str(&String::from_utf8_lossy(&message));
                }
                _ => {}
            }
        }
        
        println!("Output:");
        for line in stdout.lines() {
            println!("  {}", line);
        }
        
        if !stderr.is_empty() {
            println!("Errors:");
            for line in stderr.lines() {
                println!("  {}", line);
            }
        }
    }
    
    // 6. Get container stats
    println!("\nGetting container stats...");
    use bollard::container::StatsOptions;
    
    let options = StatsOptions {
        stream: false,
        one_shot: true,
    };
    
    let mut stats_stream = docker.stats(&container.id, Some(options));
    
    if let Some(Ok(stats)) = stats_stream.next().await {
        let memory_usage = stats.memory_stats.usage.unwrap_or(0) / (1024 * 1024);
        println!("✓ Memory usage: {} MB", memory_usage);
    }
    
    // 7. Clean up
    println!("\nCleaning up...");
    use bollard::container::RemoveContainerOptions;
    
    docker.stop_container(&container.id, None).await?;
    println!("✓ Container stopped");
    
    docker.remove_container(&container.id, Some(RemoveContainerOptions {
        force: true,
        ..Default::default()
    })).await?;
    println!("✓ Container removed");
    
    println!("\n🎉 Docker integration test completed successfully!");
    println!("The container module is using real Docker APIs via bollard SDK.");
    
    Ok(())
}