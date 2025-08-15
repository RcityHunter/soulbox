//! Sandbox Provider Trait Abstraction
//! 
//! This module defines the core traits for sandbox providers, allowing
//! different implementations (Docker, Firecracker, etc.) while maintaining
//! a consistent interface for the rest of the system.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use crate::error::Result;

/// Core trait for sandbox providers
#[async_trait]
pub trait SandboxProvider: Send + Sync {
    /// Get the provider type (docker, firecracker, etc.)
    fn provider_type(&self) -> &'static str;

    /// Create a new sandbox instance
    async fn create_sandbox(&self, config: &SandboxConfig) -> Result<SandboxInstance>;

    /// Start an existing sandbox
    async fn start_sandbox(&self, sandbox_id: &str) -> Result<()>;

    /// Stop a running sandbox
    async fn stop_sandbox(&self, sandbox_id: &str) -> Result<()>;

    /// Remove a sandbox completely
    async fn remove_sandbox(&self, sandbox_id: &str) -> Result<()>;

    /// Get sandbox information
    async fn get_sandbox(&self, sandbox_id: &str) -> Result<SandboxInstance>;

    /// List all sandboxes managed by this provider
    async fn list_sandboxes(&self) -> Result<Vec<SandboxInstance>>;

    /// Execute code in a sandbox
    async fn execute_code(&self, sandbox_id: &str, request: &ExecutionRequest) -> Result<ExecutionResult>;

    /// Get logs from a sandbox
    async fn get_logs(&self, sandbox_id: &str, options: &LogOptions) -> Result<Vec<LogEntry>>;

    /// Stream logs from a sandbox (returns a stream)
    async fn stream_logs(&self, sandbox_id: &str, options: &LogOptions) -> Result<Box<dyn LogStream>>;

    /// Get resource usage statistics
    async fn get_resource_usage(&self, sandbox_id: &str) -> Result<ResourceUsage>;

    /// Health check for the provider
    async fn health_check(&self) -> Result<ProviderHealth>;
}

/// Storage provider trait for persistent data
#[async_trait]
pub trait StorageProvider: Send + Sync {
    /// Storage provider type
    fn storage_type(&self) -> &'static str;

    /// Create a volume for a sandbox
    async fn create_volume(&self, volume_config: &VolumeConfig) -> Result<Volume>;

    /// Mount a volume to a sandbox
    async fn mount_volume(&self, sandbox_id: &str, volume_id: &str, mount_path: &str) -> Result<()>;

    /// Unmount a volume from a sandbox
    async fn unmount_volume(&self, sandbox_id: &str, volume_id: &str) -> Result<()>;

    /// Delete a volume
    async fn delete_volume(&self, volume_id: &str) -> Result<()>;

    /// List volumes
    async fn list_volumes(&self) -> Result<Vec<Volume>>;

    /// Get volume information
    async fn get_volume(&self, volume_id: &str) -> Result<Volume>;
}

/// Network provider trait for sandbox networking
#[async_trait]
pub trait NetworkProvider: Send + Sync {
    /// Network provider type
    fn network_type(&self) -> &'static str;

    /// Create a network for sandboxes
    async fn create_network(&self, network_config: &NetworkConfig) -> Result<Network>;

    /// Connect a sandbox to a network
    async fn connect_sandbox(&self, sandbox_id: &str, network_id: &str) -> Result<()>;

    /// Disconnect a sandbox from a network
    async fn disconnect_sandbox(&self, sandbox_id: &str, network_id: &str) -> Result<()>;

    /// Delete a network
    async fn delete_network(&self, network_id: &str) -> Result<()>;

    /// List networks
    async fn list_networks(&self) -> Result<Vec<Network>>;
}

/// Log streaming trait
#[async_trait]
pub trait LogStream: Send {
    /// Get the next log entry
    async fn next(&mut self) -> Option<LogEntry>;

    /// Close the log stream
    async fn close(&mut self) -> Result<()>;
}

// Data structures

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub name: Option<String>,
    pub image: String,
    pub working_directory: String,
    pub environment: HashMap<String, String>,
    pub resource_limits: ResourceLimits,
    pub network_config: Option<NetworkConfig>,
    pub volume_mounts: Vec<VolumeMount>,
    pub timeout: Option<Duration>,
    pub auto_remove: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxInstance {
    pub id: String,
    pub name: Option<String>,
    pub status: SandboxStatus,
    pub image: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub config: SandboxConfig,
    pub provider_metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SandboxStatus {
    Creating,
    Running,
    Stopped,
    Error,
    Removing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub memory: Option<String>, // e.g., "512M", "1G"
    pub cpu: Option<String>,    // e.g., "0.5", "1.0"
    pub disk: Option<String>,   // e.g., "1G", "10G"
    pub network_bandwidth: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub code: String,
    pub language: String,
    pub timeout: Option<Duration>,
    pub environment: Option<HashMap<String, String>>,
    pub working_directory: Option<String>,
    pub input: Option<String>, // stdin
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub execution_id: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
    pub resource_usage: Option<ResourceUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogOptions {
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub level: Option<String>,
    pub source: Option<String>,
    pub tail: Option<usize>,
    pub follow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: String,
    pub source: String,
    pub message: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_used: u64,    // bytes
    pub memory_limit: u64,   // bytes
    pub cpu_percent: f64,    // 0.0 - 100.0
    pub disk_used: u64,      // bytes
    pub disk_limit: u64,     // bytes
    pub network_rx: u64,     // bytes received
    pub network_tx: u64,     // bytes transmitted
    pub uptime: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub status: HealthStatus,
    pub message: String,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub name: String,
    pub size: Option<String>,
    pub volume_type: String, // "local", "nfs", etc.
    pub options: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub id: String,
    pub name: String,
    pub size: Option<u64>,
    pub volume_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub mount_point: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    pub volume_id: String,
    pub container_path: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub name: String,
    pub driver: String, // "bridge", "overlay", etc.
    pub subnet: Option<String>,
    pub gateway: Option<String>,
    pub options: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Network {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub subnet: Option<String>,
    pub gateway: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// Default implementations

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory: Some("512M".to_string()),
            cpu: Some("1.0".to_string()),
            disk: Some("1G".to_string()),
            network_bandwidth: None,
        }
    }
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            name: None,
            image: "ubuntu:22.04".to_string(),
            working_directory: "/workspace".to_string(),
            environment: HashMap::new(),
            resource_limits: ResourceLimits::default(),
            network_config: None,
            volume_mounts: Vec::new(),
            timeout: Some(Duration::from_secs(300)),
            auto_remove: false,
        }
    }
}

impl std::fmt::Display for SandboxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxStatus::Creating => write!(f, "creating"),
            SandboxStatus::Running => write!(f, "running"),
            SandboxStatus::Stopped => write!(f, "stopped"),
            SandboxStatus::Error => write!(f, "error"),
            SandboxStatus::Removing => write!(f, "removing"),
        }
    }
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
            HealthStatus::Unknown => write!(f, "unknown"),
        }
    }
}