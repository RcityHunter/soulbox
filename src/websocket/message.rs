use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessage {
    pub r#type: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketResponse {
    pub r#type: String,
    pub payload: serde_json::Value,
}

// Sandbox Stream Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxStreamInit {
    pub sandbox_id: String,
    pub stream_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxStreamReady {
    pub stream_id: String,
}

// Terminal Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInit {
    pub sandbox_id: String,
    pub config: TerminalConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    pub rows: u32,
    pub cols: u32,
    pub shell: String,
    pub working_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalReady {
    pub terminal_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInput {
    pub terminal_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalOutput {
    pub terminal_id: String,
    pub data: String,
}

// Code Execution Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteCodeRequest {
    pub sandbox_id: String,
    pub code: String,
    pub language: String,
    pub timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStarted {
    pub execution_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOutput {
    pub execution_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCompleted {
    pub execution_id: String,
    pub exit_code: i32,
}

// File Watcher Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWatcherInit {
    pub sandbox_id: String,
    pub watch_paths: Vec<String>,
    pub recursive: bool,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWatcherReady {
    pub watcher_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEvent {
    pub event_type: String,
    pub path: String,
}

// Authentication Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequest {
    pub api_key: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSuccess {
    pub user_id: String,
    pub session_id: String,
}

// Error Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub message: String,
    pub code: Option<String>,
}

impl WebSocketMessage {
    pub fn new(message_type: &str, payload: serde_json::Value) -> Self {
        Self {
            r#type: message_type.to_string(),
            payload,
        }
    }

    pub fn ping(connection_id: i32) -> Self {
        Self::new("ping", serde_json::json!({ "connection_id": connection_id }))
    }

    pub fn sandbox_stream_init(sandbox_id: &str, stream_type: &str) -> Self {
        Self::new("sandbox_stream_init", serde_json::json!({
            "sandbox_id": sandbox_id,
            "stream_type": stream_type
        }))
    }

    pub fn terminal_init(sandbox_id: &str, config: TerminalConfig) -> Self {
        Self::new("terminal_init", serde_json::json!({
            "sandbox_id": sandbox_id,
            "config": config
        }))
    }

    pub fn terminal_input(terminal_id: &str, data: &str) -> Self {
        Self::new("terminal_input", serde_json::json!({
            "terminal_id": terminal_id,
            "data": data
        }))
    }

    pub fn execute_code(sandbox_id: &str, code: &str, language: &str, timeout: u64) -> Self {
        Self::new("execute_code", serde_json::json!({
            "sandbox_id": sandbox_id,
            "code": code,
            "language": language,
            "timeout": timeout
        }))
    }

    pub fn file_watcher_init(sandbox_id: &str, watch_paths: Vec<String>, recursive: bool, events: Vec<String>) -> Self {
        Self::new("file_watcher_init", serde_json::json!({
            "sandbox_id": sandbox_id,
            "watch_paths": watch_paths,
            "recursive": recursive,
            "events": events
        }))
    }

    pub fn authenticate(api_key: &str, user_id: &str) -> Self {
        Self::new("authenticate", serde_json::json!({
            "api_key": api_key,
            "user_id": user_id
        }))
    }
}

impl WebSocketResponse {
    pub fn new(message_type: &str, payload: serde_json::Value) -> Self {
        Self {
            r#type: message_type.to_string(),
            payload,
        }
    }

    pub fn pong() -> Self {
        Self::new("pong", serde_json::json!({}))
    }

    pub fn sandbox_stream_ready(stream_id: &str) -> Self {
        Self::new("sandbox_stream_ready", serde_json::json!({
            "stream_id": stream_id
        }))
    }

    pub fn terminal_ready(terminal_id: &str) -> Self {
        Self::new("terminal_ready", serde_json::json!({
            "terminal_id": terminal_id
        }))
    }

    pub fn terminal_output(terminal_id: &str, data: &str) -> Self {
        Self::new("terminal_output", serde_json::json!({
            "terminal_id": terminal_id,
            "data": data
        }))
    }

    pub fn execution_started(execution_id: &str) -> Self {
        Self::new("execution_started", serde_json::json!({
            "execution_id": execution_id
        }))
    }

    pub fn execution_output(execution_id: &str, data: &str) -> Self {
        Self::new("execution_output", serde_json::json!({
            "execution_id": execution_id,
            "data": data
        }))
    }

    pub fn execution_completed(execution_id: &str, exit_code: i32) -> Self {
        Self::new("execution_completed", serde_json::json!({
            "execution_id": execution_id,
            "exit_code": exit_code
        }))
    }

    pub fn file_watcher_ready(watcher_id: &str) -> Self {
        Self::new("file_watcher_ready", serde_json::json!({
            "watcher_id": watcher_id
        }))
    }

    pub fn file_event(event_type: &str, path: &str) -> Self {
        Self::new("file_event", serde_json::json!({
            "event_type": event_type,
            "path": path
        }))
    }

    pub fn authentication_success(user_id: &str, session_id: &str) -> Self {
        Self::new("authentication_success", serde_json::json!({
            "user_id": user_id,
            "session_id": session_id
        }))
    }

    pub fn error(message: &str, code: Option<&str>) -> Self {
        let mut payload = serde_json::json!({ "message": message });
        if let Some(code) = code {
            payload["code"] = serde_json::Value::String(code.to_string());
        }
        Self::new("error", payload)
    }
}