use serde::{Deserialize, Serialize};

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

// Enhanced Real-time Messages

// Log Streaming Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSubscriptionRequest {
    pub sandbox_id: String,
    pub level: Option<String>,
    pub filter: Option<String>,
    pub buffer_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSubscriptionResponse {
    pub sandbox_id: String,
    pub stream_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    pub stream_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: String,
    pub source: String,
    pub message: String,
    pub sandbox_id: String,
}

// Streaming Code Execution Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingExecuteRequest {
    pub sandbox_id: String,
    pub code: String,
    pub language: String,
    pub timeout: Option<u64>,
    pub stream_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionProgress {
    pub execution_id: String,
    pub progress: u32, // 0-100
    pub step: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingExecutionOutput {
    pub execution_id: String,
    pub output_type: String, // "stdout", "stderr", "system"
    pub data: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// Heartbeat Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPing {
    pub connection_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub server_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPong {
    pub connection_id: String,
    pub client_time: i64,
    pub server_time: i64,
}

// Connection Management Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStats {
    pub connection_id: String,
    pub uptime: String,
    pub message_count: u64,
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectInfo {
    pub attempt: u32,
    pub delay_ms: u64,
    pub max_attempts: u32,
    pub next_attempt_at: chrono::DateTime<chrono::Utc>,
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

    // Enhanced message constructors
    pub fn subscribe_logs(sandbox_id: &str, level: Option<&str>, filter: Option<&str>) -> Self {
        let mut payload = serde_json::json!({
            "sandbox_id": sandbox_id
        });
        if let Some(level) = level {
            payload["level"] = serde_json::Value::String(level.to_string());
        }
        if let Some(filter) = filter {
            payload["filter"] = serde_json::Value::String(filter.to_string());
        }
        Self::new("subscribe_logs", payload)
    }

    pub fn unsubscribe_logs(sandbox_id: &str) -> Self {
        Self::new("unsubscribe_logs", serde_json::json!({
            "sandbox_id": sandbox_id
        }))
    }

    pub fn execute_code_stream(sandbox_id: &str, code: &str, language: &str) -> Self {
        Self::new("execute_code_stream", serde_json::json!({
            "sandbox_id": sandbox_id,
            "code": code,
            "language": language,
            "stream_output": true
        }))
    }

    pub fn heartbeat_pong(connection_id: &str, client_time: i64) -> Self {
        Self::new("pong", serde_json::json!({
            "connection_id": connection_id,
            "client_time": client_time,
            "server_time": chrono::Utc::now().timestamp()
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

    // Enhanced response constructors
    pub fn log_message(stream_id: &str, timestamp: chrono::DateTime<chrono::Utc>, level: &str, source: &str, message: &str, sandbox_id: &str) -> Self {
        Self::new("log_message", serde_json::json!({
            "stream_id": stream_id,
            "timestamp": timestamp,
            "level": level,
            "source": source,
            "message": message,
            "sandbox_id": sandbox_id
        }))
    }

    pub fn execution_progress(execution_id: &str, progress: u32, step: &str) -> Self {
        Self::new("execution_progress", serde_json::json!({
            "execution_id": execution_id,
            "progress": progress,
            "step": step,
            "timestamp": chrono::Utc::now()
        }))
    }

    pub fn streaming_output(execution_id: &str, output_type: &str, data: &str) -> Self {
        Self::new("streaming_output", serde_json::json!({
            "execution_id": execution_id,
            "output_type": output_type,
            "data": data,
            "timestamp": chrono::Utc::now()
        }))
    }

    pub fn heartbeat_ping(connection_id: &str) -> Self {
        Self::new("ping", serde_json::json!({
            "connection_id": connection_id,
            "timestamp": chrono::Utc::now(),
            "server_time": chrono::Utc::now().timestamp()
        }))
    }

    pub fn connection_stats(connection_id: &str, uptime: &str, message_count: u64) -> Self {
        Self::new("connection_stats", serde_json::json!({
            "connection_id": connection_id,
            "uptime": uptime,
            "message_count": message_count,
            "last_activity": chrono::Utc::now()
        }))
    }

    pub fn reconnect_info(attempt: u32, delay_ms: u64, max_attempts: u32) -> Self {
        Self::new("reconnect_info", serde_json::json!({
            "attempt": attempt,
            "delay_ms": delay_ms,
            "max_attempts": max_attempts,
            "next_attempt_at": chrono::Utc::now() + chrono::Duration::milliseconds(delay_ms as i64)
        }))
    }
}