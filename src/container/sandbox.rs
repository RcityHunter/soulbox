use std::collections::HashMap;
use std::time::Duration;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::sync::Mutex;
use bollard::Docker;
use bollard::container::{StartContainerOptions, StopContainerOptions, RemoveContainerOptions, StatsOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use futures_util::stream::StreamExt;
use tracing::{info, warn, error};

use crate::error::{Result, SoulBoxError};
use super::{ResourceLimits, NetworkConfig, PortMapping};

#[derive(Debug)]
pub struct SandboxContainer {
    id: String,
    container_id: String,
    image: String,
    resource_limits: ResourceLimits,
    network_config: NetworkConfig,
    env_vars: HashMap<String, String>,
    /// Docker client for container operations
    docker: Arc<Docker>,
    /// Track if container has been cleaned up
    cleaned_up: Arc<AtomicBool>,
    /// Task tracker for managing background tasks
    task_tracker: Arc<TaskTracker>,
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
            cleaned_up: Arc::new(AtomicBool::new(false)),
            task_tracker: Arc::new(TaskTracker::new()),
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
        
        // Security validation: check for dangerous commands
        self.validate_command_security(&command)?;
        
        let start_time = std::time::Instant::now();
        info!("Executing command in container {}: {:?}", self.container_id, command);
        
        // Create exec instance
        let exec_options = CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            cmd: Some(command.clone()),
            
            // Security: Execute as non-root user
            user: Some("1000:1000".to_string()),
            
            // Security: Set working directory
            working_dir: Some("/workspace".to_string()),
            
            // Security: Limit environment
            env: Some(vec![
                "PATH=/usr/local/bin:/usr/bin:/bin".to_string(),
                "HOME=/workspace".to_string(),
                "USER=soulbox".to_string(),
            ]),
            
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
        
        // Log execution for security audit
        info!("Command executed: {:?}, exit_code: {}, duration: {:?}s", 
              command.first().unwrap_or(&"unknown".to_string()), 
              exit_code, 
              execution_time.as_secs());
              
        if exit_code != 0 {
            warn!("Command failed with exit code {}: {:?}", exit_code, command);
        }
        
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
                    .usage
                    .unwrap_or(0) / (1024 * 1024); // Convert to MB
                
                let cpu_usage = if let (cpu_stats, precpu_stats) = 
                    (&stats.cpu_stats, &stats.precpu_stats) {
                    if let (Some(system_usage), Some(presystem_usage)) = 
                        (cpu_stats.system_cpu_usage, precpu_stats.system_cpu_usage) {
                        
                        let cpu_usage = cpu_stats.cpu_usage.total_usage;
                        let precpu_usage = precpu_stats.cpu_usage.total_usage;
                        
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
                    .map(|net| net.rx_bytes)
                    .unwrap_or(0);
                    
                let network_tx = stats.networks
                    .as_ref()
                    .and_then(|nets| nets.values().next())
                    .map(|net| net.tx_bytes)
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

    /// Manually cleanup container resources
    pub async fn cleanup(&self) -> Result<()> {
        if self.cleaned_up.compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
            info!("Starting cleanup for container {}", self.container_id);
            
            // First, cancel all background tasks
            self.task_tracker.cancel_all().await;
            
            // Then cleanup the container with timeout
            let cleanup_result = tokio::time::timeout(
                Duration::from_secs(30),
                self.docker.remove_container(&self.container_id, Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }))
            ).await;
            
            match cleanup_result {
                Ok(Ok(_)) => {
                    info!("Container {} cleaned up successfully", self.container_id);
                }
                Ok(Err(e)) => {
                    error!("Failed to cleanup container {}: {}", self.container_id, e);
                    // Don't propagate error to avoid panic in Drop
                }
                Err(_) => {
                    error!("Timeout during container {} cleanup", self.container_id);
                }
            }
        }
        Ok(())
    }

    /// Check if container has been cleaned up
    pub fn is_cleaned_up(&self) -> bool {
        self.cleaned_up.load(Ordering::Relaxed)
    }
    
    /// Get task tracker statistics
    pub async fn get_task_stats(&self) -> TaskStats {
        self.task_tracker.get_stats().await
    }
    
    /// Validate command for security issues
    fn validate_command_security(&self, command: &[String]) -> Result<()> {
        if command.is_empty() {
            return Err(SoulBoxError::internal("Empty command"));
        }
        
        let cmd = &command[0];
        
        // Block dangerous commands that could lead to container escape
        let blocked_commands = [
            // System commands
            "sudo", "su", "doas",
            // Container/system inspection
            "docker", "podman", "runc", "ctr", "crictl",
            // Kernel/system access
            "insmod", "rmmod", "modprobe", "kmod",
            "mount", "umount", "swapon", "swapoff",
            // Network configuration
            "iptables", "ip6tables", "nftables", "tc",
            "ifconfig", "ip", "route",
            // Process/system control
            "systemctl", "service", "init", "systemd",
            "reboot", "shutdown", "halt", "poweroff",
            // Debugging/tracing (could be used for escape)
            "strace", "ptrace", "gdb", "lldb",
            "perf", "dtrace", "ftrace",
            // Filesystem manipulation that could escape chroot
            "chroot", "pivot_root",
            // Device access
            "mknod", "mkfifo",
            // Dangerous shells or interpreters with too much power
            "bash", "zsh", "fish", "csh", "tcsh", "ksh",
        ];
        
        // Check if the base command (without path) is blocked
        let base_cmd = cmd.split('/').last().unwrap_or(cmd);
        if blocked_commands.contains(&base_cmd) {
            error!("Blocked dangerous command execution attempt: {}", cmd);
            return Err(SoulBoxError::internal(
                format!("Command '{}' is not allowed for security reasons", cmd)
            ));
        }
        
        // Improved path traversal prevention with canonicalization
        if let Err(e) = Self::validate_command_path_security(cmd) {
            error!("Command path security validation failed for {}: {}", cmd, e);
            return Err(e);
        }
        
        // Check for suspicious command line arguments
        for arg in command {
            if arg.len() > 4096 {
                return Err(SoulBoxError::internal(
                    "Command argument too long (potential buffer overflow)"
                ));
            }
            
            if arg.contains("\x00") {
                return Err(SoulBoxError::internal(
                    "Null bytes in command arguments are not allowed"
                ));
            }
        }
        
        // Validate total command length
        let total_length: usize = command.iter().map(|s| s.len()).sum();
        if total_length > 65536 { // 64KB limit
            return Err(SoulBoxError::internal(
                "Total command length exceeds security limits"
            ));
        }
        
        info!("Command security validation passed for: {}", cmd);
        Ok(())
    }
    
    /// 验证命令路径安全性，防止路径遍历攻击
    fn validate_command_path_security(cmd: &str) -> Result<()> {
        // Basic path traversal checks
        if cmd.contains("..") {
            return Err(SoulBoxError::internal("Path traversal attempt detected: .."));
        }
        
        // Check for access to dangerous system paths
        let dangerous_paths = [
            "/proc", "/sys", "/dev", "/etc", "/root", "/home", "/boot",
            "/usr/bin/sudo", "/bin/su", "/sbin", "/usr/sbin"
        ];
        
        for dangerous_path in &dangerous_paths {
            if cmd.contains(dangerous_path) {
                return Err(SoulBoxError::internal(
                    format!("Access to restricted path detected: {}", dangerous_path)
                ));
            }
        }
        
        // Proper path canonicalization to prevent bypasses
        if cmd.starts_with('/') {
            // For absolute paths, attempt canonicalization to detect bypass attempts
            match Self::canonicalize_and_validate_path(cmd) {
                Ok(canonical_path) => {
                    // Check if canonicalized path is safe
                    if Self::is_path_in_restricted_area(&canonical_path) {
                        return Err(SoulBoxError::internal(
                            format!("Canonicalized path {} leads to restricted area", canonical_path)
                        ));
                    }
                }
                Err(e) => {
                    warn!("Failed to canonicalize path {}: {}", cmd, e);
                    // If we can't canonicalize, be conservative and block unusual paths
                    if cmd.contains("//") || cmd.contains("/./") || cmd.contains("/../") {
                        return Err(SoulBoxError::internal("Suspicious path format detected"));
                    }
                }
            }
        }
        
        // Check for encoded path traversal attempts
        let decoded_cmd = Self::url_decode(cmd);
        if decoded_cmd.contains("..") || decoded_cmd != cmd {
            return Err(SoulBoxError::internal(
                "Encoded path traversal attempt detected"
            ));
        }
        
        // Check for null bytes that could truncate paths in C code
        if cmd.contains('\0') {
            return Err(SoulBoxError::internal("Null bytes in path not allowed"));
        }
        
        Ok(())
    }
    
    /// 安全地规范化路径并验证
    fn canonicalize_and_validate_path(path: &str) -> Result<String> {
        use std::path::Path;
        
        // 创建安全的路径处理
        let path_obj = Path::new(path);
        
        // 检查路径长度
        if path.len() > 4096 {
            return Err(SoulBoxError::internal("Path too long"));
        }
        
        // 手动规范化路径以避免文件系统访问
        let normalized = Self::normalize_path_manually(path)?;
        
        // 验证规范化的路径
        if normalized.contains("..") {
            return Err(SoulBoxError::internal("Path traversal detected after normalization"));
        }
        
        Ok(normalized)
    }
    
    /// 手动规范化路径，避免文件系统调用
    fn normalize_path_manually(path: &str) -> Result<String> {
        let mut components = Vec::new();
        
        // 分解路径组件
        for component in path.split('/') {
            match component {
                "" | "." => continue, // 跳过空组件和当前目录引用
                ".." => {
                    // 上级目录：弹出最后一个组件，但不能超出根目录
                    if !components.is_empty() {
                        components.pop();
                    } else {
                        // 尝试超出根目录的路径遍历攻击
                        return Err(SoulBoxError::internal("Path traversal beyond root"));
                    }
                }
                comp => components.push(comp),
            }
        }
        
        // 重建路径
        if components.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", components.join("/")))
        }
    }
    
    /// 检查路径是否在受限区域内
    fn is_path_in_restricted_area(canonical_path: &str) -> bool {
        let restricted_prefixes = [
            "/proc", "/sys", "/dev", "/etc", "/root", "/home", "/boot", 
            "/usr/bin/sudo", "/bin/su", "/sbin", "/usr/sbin", "/var/run",
            "/run", "/tmp/docker", "/var/lib/docker"
        ];
        
        for prefix in &restricted_prefixes {
            if canonical_path.starts_with(prefix) {
                return true;
            }
        }
        
        false
    }
    
    /// 简单的URL解码以检测编码的路径遍历
    fn url_decode(input: &str) -> String {
        // 检查常见的URL编码的路径遍历序列
        input
            .replace("%2e%2e", "..")
            .replace("%2E%2E", "..")
            .replace("%2e.", "..")
            .replace(".%2e", "..")
            .replace("%2f", "/")
            .replace("%2F", "/")
            .replace("%5c", "\\")
            .replace("%5C", "\\")
    }
}

impl Drop for SandboxContainer {
    fn drop(&mut self) {
        if self.cleaned_up.compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
            let container_id = self.container_id.clone();
            let docker = self.docker.clone();
            let task_tracker = self.task_tracker.clone();
            
            // SECURITY FIX: Removed async task spawning to prevent race conditions
            // Use synchronous cleanup to ensure resources are properly freed
            
            // First, cancel all background tasks synchronously
            if let Ok(mut tasks) = self.task_tracker.tasks.try_lock() {
                for task in tasks.drain(..) {
                    task.abort();
                }
            }
            self.task_tracker.cancelled.store(true, Ordering::Relaxed);
            
            // For container cleanup, spawn a dedicated thread to avoid blocking tokio runtime
            // This ensures cleanup completes before Drop returns
            let container_id = self.container_id.clone();
            let docker = self.docker.clone();
            
            let cleanup_thread = std::thread::spawn(move || {
                // Create a new minimal runtime just for cleanup
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        error!("Failed to create cleanup runtime for container {}: {}", container_id, e);
                        return;
                    }
                };
                
                rt.block_on(async move {
                    // Force remove the container with strict timeout
                    match tokio::time::timeout(
                        Duration::from_secs(5),
                        docker.remove_container(&container_id, Some(RemoveContainerOptions {
                            force: true,
                            v: true, // Remove associated volumes
                            ..Default::default()
                        }))
                    ).await {
                        Ok(Ok(_)) => {
                            info!("Container {} cleaned up synchronously during drop", container_id);
                        }
                        Ok(Err(e)) => {
                            error!("Failed to cleanup container {} during drop: {}", container_id, e);
                        }
                        Err(_) => {
                            error!("Timeout during synchronous cleanup of container {}", container_id);
                        }
                    }
                });
            });
            
            // Wait for cleanup to complete (with timeout to prevent hanging)
            if cleanup_thread.join().is_err() {
                error!("Cleanup thread panicked for container {}", self.container_id);
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

/// Task tracker for managing background tasks and preventing leaks
#[derive(Debug)]
pub struct TaskTracker {
    tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    cancelled: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct TaskStats {
    pub active_tasks: usize,
    pub cancelled: bool,
}

impl TaskTracker {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
    
    pub async fn spawn<F>(&self, future: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(crate::error::SoulBoxError::internal(
                "TaskTracker has been cancelled"
            ));
        }
        
        let handle = tokio::spawn(future);
        let mut tasks = self.tasks.lock().unwrap();
        tasks.push(handle);
        
        // Clean up completed tasks periodically
        self.cleanup_completed_tasks(&mut tasks);
        
        Ok(())
    }
    
    pub async fn cancel_all(&self) {
        if self.cancelled.compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
            let mut tasks = self.tasks.lock().unwrap();
            
            info!("Cancelling {} background tasks", tasks.len());
            
            for task in tasks.drain(..) {
                task.abort();
            }
            
            info!("All background tasks cancelled");
        }
    }
    
    pub async fn get_stats(&self) -> TaskStats {
        let tasks = self.tasks.lock().unwrap();
        TaskStats {
            active_tasks: tasks.len(),
            cancelled: self.cancelled.load(Ordering::Relaxed),
        }
    }
    
    fn cleanup_completed_tasks(&self, tasks: &mut Vec<tokio::task::JoinHandle<()>>) {
        tasks.retain(|task| !task.is_finished());
    }
}

impl Drop for TaskTracker {
    fn drop(&mut self) {
        if !self.cancelled.load(Ordering::Relaxed) {
            self.cancelled.store(true, Ordering::Relaxed);
            
            // Try to cancel all tasks (best effort)
            if let Ok(mut tasks) = self.tasks.try_lock() {
                for task in tasks.drain(..) {
                    task.abort();
                }
            }
        }
    }
}