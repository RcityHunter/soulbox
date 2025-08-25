//! E2B API Models
//! 
//! Data structures compatible with E2B's API

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// E2B Sandbox representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BSandbox {
    pub id: String,
    pub template_id: Option<String>,
    pub status: E2BSandboxStatus,
    pub created_at: DateTime<Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub resources: E2BResources,
    pub network: E2BNetwork,
}

/// E2B Sandbox status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum E2BSandboxStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}

/// E2B Resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BResources {
    pub cpu: f64,
    pub memory_mb: u64,
    pub disk_mb: u64,
    pub network_enabled: bool,
}

/// E2B Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BNetwork {
    pub ports: Vec<E2BPortMapping>,
    pub dns: Vec<String>,
    pub proxy: Option<E2BProxy>,
}

/// E2B Port mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BPortMapping {
    pub container: u16,
    pub host: Option<u16>,
    pub protocol: String,
}

/// E2B Proxy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BProxy {
    pub http: Option<String>,
    pub https: Option<String>,
    pub no_proxy: Vec<String>,
}

/// E2B Process representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BProcess {
    pub id: String,
    pub sandbox_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: String,
    pub status: E2BProcessStatus,
    pub exit_code: Option<i32>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// E2B Process status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum E2BProcessStatus {
    Running,
    Exited,
    Failed,
    Killed,
}

/// E2B Process output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BProcessOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

/// E2B Filesystem operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BFilesystem {
    pub sandbox_id: String,
    pub operations: Vec<E2BFileOperation>,
}

/// E2B File operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum E2BFileOperation {
    Read { path: String },
    Write { path: String, content: String },
    Delete { path: String },
    List { path: String },
    Mkdir { path: String },
    Move { from: String, to: String },
    Copy { from: String, to: String },
}

/// E2B File info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BFileInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: DateTime<Utc>,
    pub permissions: String,
}

/// E2B Create sandbox request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BCreateSandboxRequest {
    pub template_id: Option<String>,
    pub timeout: Option<u64>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub resources: Option<E2BResources>,
    pub env: Option<HashMap<String, String>>,
    pub cwd: Option<String>,
}

/// E2B Create sandbox response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BCreateSandboxResponse {
    pub id: String,
    pub url: Option<String>,
    pub status: E2BSandboxStatus,
    pub created_at: DateTime<Utc>,
}

/// E2B Run process request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BRunProcessRequest {
    pub command: String,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub cwd: Option<String>,
    pub timeout: Option<u64>,
    pub stdin: Option<String>,
}

/// E2B Run process response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BRunProcessResponse {
    pub id: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

/// E2B Upload file request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BUploadFileRequest {
    pub path: String,
    pub content: String, // Base64 encoded
    pub permissions: Option<String>,
}

/// E2B Download file response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BDownloadFileResponse {
    pub content: String, // Base64 encoded
    pub size: u64,
    pub modified: DateTime<Utc>,
}

/// E2B List files response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BListFilesResponse {
    pub files: Vec<E2BFileInfo>,
    pub total: usize,
}

/// E2B WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum E2BWebSocketMessage {
    Input { data: String },
    Output { data: String, stream: String }, // stream: "stdout" or "stderr"
    Exit { code: i32 },
    Error { message: String },
    Resize { cols: u16, rows: u16 },
    Ping,
    Pong,
}

/// E2B Template info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BTemplate {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub runtime: String,
    pub version: String,
    pub public: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// E2B List templates response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BListTemplatesResponse {
    pub templates: Vec<E2BTemplate>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
}

/// E2B Error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BErrorResponse {
    pub error: E2BErrorDetail,
}

/// E2B Error detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BErrorDetail {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl E2BErrorDetail {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }
}

/// E2B Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2BHealthResponse {
    pub status: String,
    pub version: String,
    pub uptime: u64,
    pub sandboxes_count: usize,
}