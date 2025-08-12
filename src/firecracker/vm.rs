use std::process::{Child, Command, Stdio};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tracing::{info, error, debug};
use uuid::Uuid;

use crate::error::Result;
use super::config::VMConfig;

#[derive(Debug, Clone)]
pub enum VMState {
    Created,
    Starting,
    Running,
    Paused,
    Stopping,
    Stopped,
    Error(String),
}

/// Represents a single Firecracker microVM instance
pub struct FirecrackerVM {
    pub id: String,
    pub config: VMConfig,
    pub state: Arc<Mutex<VMState>>,
    process: Arc<Mutex<Option<Child>>>,
    api_socket_path: PathBuf,
    metrics_socket_path: PathBuf,
    vsock_path: PathBuf,
}

impl FirecrackerVM {
    pub fn new(config: VMConfig) -> Self {
        let id = config.vm_id.clone();
        let socket_dir = PathBuf::from(format!("/tmp/firecracker/{}", id));
        
        Self {
            id: id.clone(),
            config,
            state: Arc::new(Mutex::new(VMState::Created)),
            process: Arc::new(Mutex::new(None)),
            api_socket_path: socket_dir.join("api.sock"),
            metrics_socket_path: socket_dir.join("metrics.sock"),
            vsock_path: socket_dir.join("vsock.sock"),
        }
    }

    /// Start the Firecracker VM
    pub async fn start(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        *state = VMState::Starting;
        drop(state);

        // Create socket directory
        tokio::fs::create_dir_all(&self.api_socket_path.parent().unwrap()).await?;

        // Start Firecracker process
        let mut cmd = Command::new("firecracker");
        cmd.arg("--api-sock").arg(&self.api_socket_path)
           .arg("--metrics-sock").arg(&self.metrics_socket_path)
           .arg("--config-file").arg("-")
           .stdin(Stdio::piped())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());

        let mut child = cmd.spawn()
            .map_err(|e| crate::error::SoulBoxError::internal(
                format!("Failed to start Firecracker: {}", e)
            ))?;

        // Wait for API socket to be ready
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Configure the VM via API
        self.configure_vm().await?;

        // Update process handle
        let mut process = self.process.lock().await;
        *process = Some(child);

        // Update state
        let mut state = self.state.lock().await;
        *state = VMState::Running;

        info!("Firecracker VM {} started successfully", self.id);
        Ok(())
    }

    /// Configure the VM using Firecracker's API
    async fn configure_vm(&self) -> Result<()> {
        let client = FirecrackerAPIClient::new(&self.api_socket_path).await?;

        // Set machine configuration
        client.put_machine_config(&self.config.machine_config).await?;

        // Set kernel
        client.put_kernel(&self.config.kernel_config).await?;

        // Set root drive
        client.put_drive(&self.config.rootfs_config).await?;

        // Set network interface
        client.put_network_interface(&self.config.network_config).await?;

        // Set vsock if configured
        if let Some(vsock_config) = &self.config.vsock_config {
            client.put_vsock(vsock_config).await?;
        }

        // Start the VM
        client.put_actions("InstanceStart").await?;

        Ok(())
    }

    /// Stop the VM
    pub async fn stop(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        *state = VMState::Stopping;
        drop(state);

        // Send stop action via API
        if let Ok(client) = FirecrackerAPIClient::new(&self.api_socket_path).await {
            let _ = client.put_actions("SendCtrlAltDel").await;
        }

        // Wait a bit for graceful shutdown
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Force kill if still running
        let mut process = self.process.lock().await;
        if let Some(mut child) = process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        let mut state = self.state.lock().await;
        *state = VMState::Stopped;

        info!("Firecracker VM {} stopped", self.id);
        Ok(())
    }

    /// Execute a command in the VM
    pub async fn execute_command(&self, command: Vec<String>) -> Result<(String, String, i32)> {
        // This would typically use vsock or SSH to communicate with the VM
        // For now, we'll use vsock for communication
        
        if let Some(vsock_config) = &self.config.vsock_config {
            // Connect to the VM via vsock
            let socket_path = &vsock_config.uds_path;
            
            // In a real implementation, we'd have an agent running inside the VM
            // that listens on vsock and executes commands
            
            // For now, return a placeholder
            Ok((
                format!("Command execution in VM not yet implemented: {:?}", command),
                String::new(),
                0
            ))
        } else {
            Err(crate::error::SoulBoxError::internal(
                "Vsock not configured for command execution"
            ))
        }
    }

    /// Get VM state
    pub async fn get_state(&self) -> VMState {
        self.state.lock().await.clone()
    }
}

/// Simple Firecracker API client
struct FirecrackerAPIClient {
    socket_path: PathBuf,
}

impl FirecrackerAPIClient {
    async fn new(socket_path: &PathBuf) -> Result<Self> {
        Ok(Self {
            socket_path: socket_path.clone(),
        })
    }

    async fn put_machine_config(&self, config: &super::config::MachineConfig) -> Result<()> {
        self.make_request("PUT", "/machine-config", serde_json::to_string(config)?).await
    }

    async fn put_kernel(&self, config: &super::config::KernelConfig) -> Result<()> {
        let body = serde_json::json!({
            "kernel_image_path": config.kernel_image_path,
            "boot_args": config.boot_args,
        });
        self.make_request("PUT", "/boot-source", body.to_string()).await
    }

    async fn put_drive(&self, config: &super::config::RootfsConfig) -> Result<()> {
        let body = serde_json::json!({
            "drive_id": config.drive_id,
            "path_on_host": config.path_on_host,
            "is_root_device": config.is_root_device,
            "is_read_only": config.is_read_only,
        });
        let path = format!("/drives/{}", config.drive_id);
        self.make_request("PUT", &path, body.to_string()).await
    }

    async fn put_network_interface(&self, config: &super::config::NetworkConfig) -> Result<()> {
        let body = serde_json::json!({
            "iface_id": config.iface_id,
            "host_dev_name": config.host_dev_name,
            "guest_mac": config.guest_mac,
        });
        let path = format!("/network-interfaces/{}", config.iface_id);
        self.make_request("PUT", &path, body.to_string()).await
    }

    async fn put_vsock(&self, config: &super::config::VsockConfig) -> Result<()> {
        let body = serde_json::json!({
            "vsock_id": config.vsock_id,
            "guest_cid": config.guest_cid,
            "uds_path": config.uds_path,
        });
        self.make_request("PUT", "/vsock", body.to_string()).await
    }

    async fn put_actions(&self, action: &str) -> Result<()> {
        let body = serde_json::json!({
            "action_type": action,
        });
        self.make_request("PUT", "/actions", body.to_string()).await
    }

    async fn make_request(&self, method: &str, path: &str, body: String) -> Result<()> {
        // Connect to Unix socket
        let mut stream = UnixStream::connect(&self.socket_path).await
            .map_err(|e| crate::error::SoulBoxError::internal(
                format!("Failed to connect to Firecracker API: {}", e)
            ))?;

        // Send HTTP request
        let request = format!(
            "{} {} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            method, path, body.len(), body
        );

        stream.write_all(request.as_bytes()).await?;
        stream.flush().await?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut response = String::new();
        reader.read_line(&mut response).await?;

        if !response.contains("204") && !response.contains("200") {
            return Err(crate::error::SoulBoxError::internal(
                format!("Firecracker API request failed: {}", response)
            ));
        }

        Ok(())
    }
}

impl FirecrackerVM {
    /// Execute code in the VM via vsock
    pub async fn execute_code(
        &self,
        language: String,
        code: String,
        timeout_seconds: u64,
        env_vars: std::collections::HashMap<String, String>,
    ) -> Result<(String, String, i32)> {
        use super::agent::{VsockClient, ExecutionRequest};
        
        // Check VM is running
        let state = self.state.lock().await;
        match &*state {
            VMState::Running => {},
            _ => return Err(crate::error::SoulBoxError::invalid_state(
                "VM must be running to execute code"
            )),
        }
        drop(state);
        
        // Create vsock client
        let client = VsockClient::new(self.id.clone(), self.vsock_path.to_string_lossy().to_string());
        
        // Create execution request
        let request = ExecutionRequest {
            execution_id: format!("exec_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase()),
            language,
            code,
            timeout_seconds,
            env_vars,
            working_dir: Some("/workspace".to_string()),
            stream_output: false,
        };
        
        // Execute via vsock
        let response = client.execute(request).await
            .map_err(|e| crate::error::SoulBoxError::internal(
                format!("Failed to execute code via vsock: {}", e)
            ))?;
        
        Ok((response.stdout, response.stderr, response.exit_code.unwrap_or(-1)))
    }
    
    /// Execute code with streaming output
    pub async fn execute_code_stream<F>(
        &self,
        language: String,
        code: String,
        timeout_seconds: u64,
        env_vars: std::collections::HashMap<String, String>,
        callback: F,
    ) -> Result<(String, String, i32)> 
    where
        F: FnMut(super::agent::protocol::StreamEvent) + Send + 'static,
    {
        use super::agent::{VsockClient, ExecutionRequest};
        
        // Check VM is running
        let state = self.state.lock().await;
        match &*state {
            VMState::Running => {},
            _ => return Err(crate::error::SoulBoxError::invalid_state(
                "VM must be running to execute code"
            )),
        }
        drop(state);
        
        // Create vsock client
        let client = VsockClient::new(self.id.clone(), self.vsock_path.to_string_lossy().to_string());
        
        // Create execution request
        let request = ExecutionRequest {
            execution_id: format!("exec_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase()),
            language,
            code,
            timeout_seconds,
            env_vars,
            working_dir: Some("/workspace".to_string()),
            stream_output: true,
        };
        
        // Execute with streaming
        let response = client.execute_stream(request, callback).await
            .map_err(|e| crate::error::SoulBoxError::internal(
                format!("Failed to execute code via vsock: {}", e)
            ))?;
        
        Ok((response.stdout, response.stderr, response.exit_code.unwrap_or(-1)))
    }
}

impl Clone for FirecrackerVM {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            config: self.config.clone(),
            state: Arc::clone(&self.state),
            process: Arc::clone(&self.process),
            api_socket_path: self.api_socket_path.clone(),
            metrics_socket_path: self.metrics_socket_path.clone(),
            vsock_path: self.vsock_path.clone(),
        }
    }
}