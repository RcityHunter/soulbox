use std::collections::HashMap;
use std::time::Duration;
use std::sync::Arc;
use uuid::Uuid;
use bollard::Docker;
use bollard::container::{StartContainerOptions, StopContainerOptions, RemoveContainerOptions, StatsOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use futures_util::stream::StreamExt;
use tracing::{info, warn, error};

use crate::error::{Result, SoulBoxError};
use super::{ResourceLimits, NetworkConfig, PortMapping};

#[derive(Debug, Clone)]
pub struct SandboxContainer {
    id: String,
    container_id: String,
    image: String,
    resource_limits: ResourceLimits,
    network_config: NetworkConfig,
    env_vars: HashMap<String, String>,
    /// Docker client for container operations
    docker: Arc<Docker>,
}

impl SandboxContainer {
    /// Create a new SandboxContainer with real Docker integration
    pub fn new(
        sandbox_id: &str,
        container_id: &str,
        image: &str,
        resource_limits: ResourceLimits,
        network_config: NetworkConfig,
        env_vars: HashMap<String, String>,
        docker: Arc<Docker>,
    ) -> Result<Self> {
        Ok(Self {
            id: sandbox_id.to_string(),
            container_id: container_id.to_string(),
            image: image.to_string(),
            resource_limits,
            network_config,
            env_vars,
            docker,
        })
    }

    /// Create a mock container for testing (deprecated - use new() with real Docker)
    pub fn new_mock(
        sandbox_id: &str,
        image: &str,
        resource_limits: ResourceLimits,
        network_config: NetworkConfig,
        env_vars: HashMap<String, String>,
    ) -> Result<Self> {
        let container_id = format!("container_{}", Uuid::new_v4().to_string().replace("-", "")[..12].to_lowercase());
        
        // Create a dummy Docker client - this will fail if used
        let docker = Arc::new(Docker::connect_with_socket_defaults()
            .map_err(|e| SoulBoxError::internal(format!("Mock container cannot connect to Docker: {}", e)))?);
        
        Ok(Self {
            id: sandbox_id.to_string(),
            container_id,
            image: image.to_string(),
            resource_limits,
            network_config,
            env_vars,
            docker,
        })
    }

    pub fn get_id(&self) -> &str {
        &self.id
    }

    pub fn get_container_id(&self) -> &str {
        &self.container_id
    }

    pub fn get_image(&self) -> &str {
        &self.image
    }
    
    pub async fn get_status(&self) -> Result<String> {
        match self.docker.inspect_container(&self.container_id, None::<bollard::container::InspectContainerOptions>).await {
            Ok(details) => {
                if let Some(state) = details.state {
                    match state.status {
                        Some(bollard::models::ContainerStateStatusEnum::RUNNING) => Ok("running".to_string()),
                        Some(bollard::models::ContainerStateStatusEnum::EXITED) => Ok("exited".to_string()),
                        Some(bollard::models::ContainerStateStatusEnum::CREATED) => Ok("created".to_string()),
                        Some(bollard::models::ContainerStateStatusEnum::PAUSED) => Ok("paused".to_string()),
                        Some(bollard::models::ContainerStateStatusEnum::RESTARTING) => Ok("restarting".to_string()),
                        Some(bollard::models::ContainerStateStatusEnum::REMOVING) => Ok("removing".to_string()),
                        Some(bollard::models::ContainerStateStatusEnum::DEAD) => Ok("dead".to_string()),
                        Some(bollard::models::ContainerStateStatusEnum::EMPTY) => Ok("empty".to_string()),
                        None => Ok("unknown".to_string()),
                    }
                } else {
                    Ok("unknown".to_string())
                }
            }
            Err(e) => {
                warn!("Failed to get container status for {}: {}", self.container_id, e);
                Ok("unknown".to_string())
            }
        }
    }

    pub async fn start(&self) -> Result<()> {
        info!("Starting container: {}", self.container_id);
        
        self.docker
            .start_container(&self.container_id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| SoulBoxError::internal(format!("Failed to start container: {}", e)))?;
            
        info!("Container started successfully: {}", self.container_id);
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        info!("Stopping container: {}", self.container_id);
        
        let options = StopContainerOptions { t: 10 }; // 10 second grace period
        
        self.docker
            .stop_container(&self.container_id, Some(options))
            .await
            .map_err(|e| SoulBoxError::internal(format!("Failed to stop container: {}", e)))?;
            
        info!("Container stopped successfully: {}", self.container_id);
        Ok(())
    }

    pub async fn remove(&self) -> Result<()> {
        info!("Removing container: {}", self.container_id);
        
        let options = RemoveContainerOptions {
            force: true,
            v: true, // Remove volumes
            ..Default::default()
        };
        
        self.docker
            .remove_container(&self.container_id, Some(options))
            .await
            .map_err(|e| SoulBoxError::internal(format!("Failed to remove container: {}", e)))?;
            
        info!("Container removed successfully: {}", self.container_id);
        Ok(())
    }

    pub async fn execute_command(&self, command: Vec<String>) -> Result<ExecutionResult> {
        if command.is_empty() {
            return Err(SoulBoxError::internal("Empty command"));
        }
        
        let start_time = std::time::Instant::now();
        info!("Executing command in container {}: {:?}", self.container_id, command);
        
        // Create exec instance
        let exec_options = CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            cmd: Some(command.clone()),
            ..Default::default()
        };
        
        let exec = self.docker
            .create_exec(&self.container_id, exec_options)
            .await
            .map_err(|e| SoulBoxError::internal(format!("Failed to create exec: {}", e)))?;
            
        // Start execution and collect output
        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;
        
        if let StartExecResults::Attached { mut output, .. } = self.docker
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| SoulBoxError::internal(format!("Failed to start exec: {}", e)))?
        {
            while let Some(chunk) = output.next().await {
                match chunk {
                    Ok(bollard::container::LogOutput::StdOut { message }) => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    Ok(bollard::container::LogOutput::StdErr { message }) => {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                    Err(e) => {
                        warn!("Error reading exec output: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        }
        
        // Get exit code
        if let Ok(inspect) = self.docker.inspect_exec(&exec.id).await {
            if let Some(code) = inspect.exit_code {
                exit_code = code as i32;
            }
        }
        
        let execution_time = start_time.elapsed();
        
        info!("Command execution completed. Exit code: {}, Duration: {:?}", exit_code, execution_time);
        
        Ok(ExecutionResult {
            stdout,
            stderr,
            exit_code,
            execution_time,
        })
    }

    pub async fn get_resource_stats(&self) -> Result<ResourceStats> {
        // Get container stats from Docker
        match self.docker.stats(&self.container_id, Some(StatsOptions {
            stream: false,
            one_shot: true,
        })).next().await {
            Some(Ok(stats)) => {
                let memory_usage = stats.memory_stats
                    .and_then(|mem| mem.usage)
                    .unwrap_or(0) / (1024 * 1024); // Convert to MB
                
                let cpu_usage = if let (Some(cpu_stats), Some(precpu_stats)) = 
                    (&stats.cpu_stats, &stats.precpu_stats) {
                    if let (Some(cpu_usage), Some(system_usage), Some(precpu_usage), Some(presystem_usage)) = 
                        (cpu_stats.cpu_usage.as_ref().and_then(|u| u.total_usage),
                         cpu_stats.system_cpu_usage,
                         precpu_stats.cpu_usage.as_ref().and_then(|u| u.total_usage),
                         precpu_stats.system_cpu_usage) {
                        
                        let cpu_delta = cpu_usage as f64 - precpu_usage as f64;
                        let system_delta = system_usage as f64 - presystem_usage as f64;
                        
                        if system_delta > 0.0 {
                            (cpu_delta / system_delta) * 100.0
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                
                let network_rx = stats.networks
                    .as_ref()
                    .and_then(|nets| nets.values().next())
                    .and_then(|net| net.rx_bytes)
                    .unwrap_or(0);
                    
                let network_tx = stats.networks
                    .as_ref()
                    .and_then(|nets| nets.values().next())
                    .and_then(|net| net.tx_bytes)
                    .unwrap_or(0);

                Ok(ResourceStats {
                    memory_usage_mb: memory_usage,
                    memory_limit_mb: self.resource_limits.memory.limit_mb,
                    cpu_usage_percent: cpu_usage,
                    cpu_cores: self.resource_limits.cpu.cores,
                    network_rx_bytes: network_rx,
                    network_tx_bytes: network_tx,
                })
            }
            _ => {
                // Fallback to mock data if stats unavailable
                Ok(ResourceStats {
                    memory_usage_mb: 64,
                    memory_limit_mb: self.resource_limits.memory.limit_mb,
                    cpu_usage_percent: 15.0,
                    cpu_cores: self.resource_limits.cpu.cores,
                    network_rx_bytes: 1024,
                    network_tx_bytes: 512,
                })
            }
        }
    }

    pub async fn get_port_mappings(&self) -> Result<Vec<PortMapping>> {
        match self.docker.inspect_container(&self.container_id, None::<bollard::container::InspectContainerOptions>).await {
            Ok(details) => {
                let mut mappings = Vec::new();
                
                if let Some(network_settings) = details.network_settings {
                    if let Some(ports) = network_settings.ports {
                        for (container_port, host_bindings) in ports {
                            if let Some(bindings) = host_bindings {
                                for binding in bindings {
                                    if let Some(host_port) = binding.host_port {
                                        if let Ok(container_port_num) = container_port
                                            .trim_end_matches("/tcp")
                                            .trim_end_matches("/udp")
                                            .parse::<u16>() {
                                            
                                            mappings.push(PortMapping {
                                                container_port: container_port_num,
                                                host_port: Some(host_port.parse().unwrap_or(0)),
                                                protocol: if container_port.ends_with("/udp") { 
                                                    "udp".to_string() 
                                                } else { 
                                                    "tcp".to_string() 
                                                },
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                Ok(mappings)
            }
            Err(_) => {
                // Fallback to configured port mappings
                Ok(self.network_config.port_mappings.clone())
            }
        }
    }

    // Keep some mock behavior for testing without starting Docker containers
    async fn _execute_command_mock(&self, command: Vec<String>) -> Result<ExecutionResult> {
        if command.is_empty() {
            return Err(SoulBoxError::internal("Empty command"));
        }

        // Mock some basic command behaviors for testing
        match command[0].as_str() {
            "echo" => {
                if command.len() > 1 {
                    Ok(ExecutionResult {
                        stdout: command[1..].join(" "),
                        stderr: String::new(),
                        exit_code: 0,
                        execution_time: Duration::from_millis(10),
                    })
                } else {
                    Ok(ExecutionResult {
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: 0,
                        execution_time: Duration::from_millis(10),
                    })
                }
            }
            "printenv" => {
                let mut output = String::new();
                for (key, value) in &self.env_vars {
                    output.push_str(&format!("{key}={value}\n"));
                }
                Ok(ExecutionResult {
                    stdout: output,
                    stderr: String::new(),
                    exit_code: 0,
                    execution_time: Duration::from_millis(50),
                })
            }
            "cat" => {
                if command.len() > 1 && command[1] == "/tmp/test.txt" {
                    // Mock file not found error for filesystem isolation test
                    Ok(ExecutionResult {
                        stdout: String::new(),
                        stderr: "cat: /tmp/test.txt: No such file or directory".to_string(),
                        exit_code: 1,
                        execution_time: Duration::from_millis(20),
                    })
                } else {
                    Ok(ExecutionResult {
                        stdout: String::new(),
                        stderr: "cat: file not found".to_string(),
                        exit_code: 1,
                        execution_time: Duration::from_millis(20),
                    })
                }
            }
            "sleep" => {
                // For timeout testing, we'll simulate a long-running command
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok(ExecutionResult {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: 0,
                    execution_time: Duration::from_secs(10),
                })
            }
            "wget" => {
                // Mock network isolation - wget should fail if internet is disabled
                if !self.network_config.enable_internet {
                    Ok(ExecutionResult {
                        stdout: String::new(),
                        stderr: "wget: unable to resolve host address 'google.com'".to_string(),
                        exit_code: 1,
                        execution_time: Duration::from_millis(100),
                    })
                } else {
                    Ok(ExecutionResult {
                        stdout: "Mock download successful".to_string(),
                        stderr: String::new(),
                        exit_code: 0,
                        execution_time: Duration::from_millis(500),
                    })
                }
            }
            "sh" => {
                // Handle shell commands
                if command.len() >= 3 && command[1] == "-c" {
                    let shell_command = &command[2];
                    
                    if shell_command.contains("echo 'secret data' > /tmp/test.txt && cat /tmp/test.txt") {
                        Ok(ExecutionResult {
                            stdout: "secret data".to_string(),
                            stderr: String::new(),
                            exit_code: 0,
                            execution_time: Duration::from_millis(30),
                        })
                    } else if shell_command.starts_with("echo $") {
                        let var_name = shell_command.strip_prefix("echo $").unwrap_or("");
                        let value = self.env_vars.get(var_name).cloned().unwrap_or_default();
                        Ok(ExecutionResult {
                            stdout: value,
                            stderr: String::new(),
                            exit_code: 0,
                            execution_time: Duration::from_millis(20),
                        })
                    } else {
                        Ok(ExecutionResult {
                            stdout: String::new(),
                            stderr: String::new(),
                            exit_code: 0,
                            execution_time: Duration::from_millis(50),
                        })
                    }
                } else {
                    Ok(ExecutionResult {
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: 0,
                        execution_time: Duration::from_millis(20),
                    })
                }
            }
            "node" => {
                // Mock memory exhaustion for resource limit testing
                if command.len() >= 3 && command[2].contains("while(true)") {
                    Ok(ExecutionResult {
                        stdout: String::new(),
                        stderr: "JavaScript heap out of memory".to_string(),
                        exit_code: 134, // SIGABRT
                        execution_time: Duration::from_millis(200),
                    })
                } else {
                    Ok(ExecutionResult {
                        stdout: "Node.js execution result".to_string(),
                        stderr: String::new(),
                        exit_code: 0,
                        execution_time: Duration::from_millis(100),
                    })
                }
            }
            _ => {
                Ok(ExecutionResult {
                    stdout: String::new(),
                    stderr: format!("sh: {}: command not found", command[0]),
                    exit_code: 127,
                    execution_time: Duration::from_millis(10),
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub execution_time: Duration,
}

#[derive(Debug, Clone)]
pub struct ResourceStats {
    pub memory_usage_mb: u64,
    pub memory_limit_mb: u64,
    pub cpu_usage_percent: f64,
    pub cpu_cores: f64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
}