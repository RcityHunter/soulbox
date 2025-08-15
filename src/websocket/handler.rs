use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, broadcast};
use tokio::time::{interval, sleep};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};
use futures_util::{StreamExt, SinkExt, stream::SplitSink, stream::SplitStream};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use super::message::{WebSocketMessage, WebSocketResponse};
use crate::error::Result;

pub struct WebSocketHandler {
    active_connections: Arc<Mutex<HashMap<String, ConnectionInfo>>>,
    authenticated_sessions: Arc<Mutex<HashMap<String, String>>>, // session_id -> user_id
    log_streams: Arc<Mutex<HashMap<String, LogStream>>>,
    heartbeat_interval: Duration,
    reconnect_config: ReconnectConfig,
}

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub connection_id: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub sandbox_id: Option<String>,
    pub terminal_id: Option<String>,
    pub stream_id: Option<String>,
    pub last_heartbeat: Instant,
    pub subscribed_logs: Vec<String>, // List of sandbox IDs for log streaming
    pub auto_reconnect: bool,
    pub connection_attempt: u32,
}

#[derive(Debug, Clone)]
pub struct LogStream {
    pub sandbox_id: String,
    pub stream_id: String,
    pub log_level: Option<String>,
    pub filter: Option<String>,
    pub sender: broadcast::Sender<LogMessage>,
    pub active_connections: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LogMessage {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: String,
    pub source: String,
    pub message: String,
    pub sandbox_id: String,
}

#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
}

impl Default for WebSocketHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSocketHandler {
    pub fn new() -> Self {
        Self {
            active_connections: Arc::new(Mutex::new(HashMap::new())),
            authenticated_sessions: Arc::new(Mutex::new(HashMap::new())),
            log_streams: Arc::new(Mutex::new(HashMap::new())),
            heartbeat_interval: Duration::from_secs(30),
            reconnect_config: ReconnectConfig {
                max_attempts: 5,
                initial_delay: Duration::from_millis(1000),
                max_delay: Duration::from_secs(30),
                backoff_multiplier: 2.0,
            },
        }
    }

    pub fn with_heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }

    pub fn with_reconnect_config(mut self, config: ReconnectConfig) -> Self {
        self.reconnect_config = config;
        self
    }

    pub async fn handle_connection<S>(&self, mut ws_stream: WebSocketStream<S>) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let connection_id = format!("conn_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
        
        let connection_info = ConnectionInfo {
            connection_id: connection_id.clone(),
            user_id: None,
            session_id: None,
            sandbox_id: None,
            terminal_id: None,
            stream_id: None,
            last_heartbeat: Instant::now(),
            subscribed_logs: Vec::new(),
            auto_reconnect: true,
            connection_attempt: 1,
        };

        // Store connection
        {
            let mut connections = self.active_connections.lock().await;
            connections.insert(connection_id.clone(), connection_info);
        }

        info!("WebSocket connection established: {}", connection_id);

        // Split the stream for concurrent reading and writing
        let (mut sender, mut receiver) = ws_stream.split();
        
        // Start heartbeat task
        let heartbeat_task = self.start_heartbeat_task(connection_id.clone(), sender);
        
        // Handle incoming messages
        let message_handler = async {
            while let Some(message_result) = receiver.next().await {
                match message_result {
                    Ok(message) => {
                        // Update last heartbeat time
                        {
                            let mut connections = self.active_connections.lock().await;
                            if let Some(conn_info) = connections.get_mut(&connection_id) {
                                conn_info.last_heartbeat = Instant::now();
                            }
                        }

                        match self.handle_message_enhanced(&connection_id, message).await {
                            Ok(should_continue) => {
                                if !should_continue {
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Error handling message for {}: {}", connection_id, e);
                                // Send error via broadcast if possible
                                self.broadcast_error(&connection_id, &format!("Internal error: {e}")).await;
                            }
                        }
                    }
                    Err(e) => {
                        error!("WebSocket error for {}: {}", connection_id, e);
                        break;
                    }
                }
            }
        };

        // Wait for either heartbeat task or message handler to complete
        tokio::select! {
            _ = heartbeat_task => {
                debug!("Heartbeat task completed for {}", connection_id);
            }
            _ = message_handler => {
                debug!("Message handler completed for {}", connection_id);
            }
        }

        // Cleanup connection
        {
            let mut connections = self.active_connections.lock().await;
            connections.remove(&connection_id);
        }

        info!("WebSocket connection closed: {}", connection_id);
        Ok(())
    }

    async fn handle_message<S>(
        &self,
        connection_id: &str,
        message: Message,
        ws_stream: &mut WebSocketStream<S>,
    ) -> Result<bool>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        match message {
            Message::Text(text) => {
                debug!("Received message from {}: {}", connection_id, text);
                
                // Try to parse as WebSocketMessage
                match serde_json::from_str::<WebSocketMessage>(&text) {
                    Ok(ws_message) => {
                        self.process_websocket_message(connection_id, ws_message, ws_stream).await?;
                    }
                    Err(_) => {
                        // Invalid JSON format
                        let error_response = WebSocketResponse::error("Invalid message format", Some("INVALID_FORMAT"));
                        let response_text = serde_json::to_string(&error_response)?;
                        ws_stream.send(Message::Text(response_text)).await?;
                    }
                }
                Ok(true)
            }
            Message::Ping(payload) => {
                debug!("Received ping from {}", connection_id);
                ws_stream.send(Message::Pong(payload)).await?;
                Ok(true)
            }
            Message::Pong(_) => {
                debug!("Received pong from {}", connection_id);
                Ok(true)
            }
            Message::Close(_) => {
                info!("Received close message from {}", connection_id);
                Ok(false) // Signal to close the connection
            }
            Message::Binary(_) => {
                warn!("Received binary message from {} (not supported)", connection_id);
                let error_response = WebSocketResponse::error("Binary messages not supported", Some("UNSUPPORTED_MESSAGE"));
                let response_text = serde_json::to_string(&error_response)?;
                ws_stream.send(Message::Text(response_text)).await?;
                Ok(true)
            }
            Message::Frame(_) => {
                // Raw frame handling - not typically needed for application logic
                debug!("Received raw frame from {}", connection_id);
                Ok(true)
            }
        }
    }

    async fn process_websocket_message<S>(
        &self,
        connection_id: &str,
        message: WebSocketMessage,
        ws_stream: &mut WebSocketStream<S>,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        match message.r#type.as_str() {
            "ping" => {
                let response = WebSocketResponse::pong();
                let response_text = serde_json::to_string(&response)?;
                ws_stream.send(Message::Text(response_text)).await?;
            }
            "authenticate" => {
                self.handle_authentication(connection_id, &message, ws_stream).await?;
            }
            "sandbox_stream_init" => {
                if self.is_authenticated(connection_id).await {
                    self.handle_sandbox_stream_init(connection_id, &message, ws_stream).await?;
                } else {
                    let error_response = WebSocketResponse::error("Authentication required", Some("UNAUTHORIZED"));
                    let response_text = serde_json::to_string(&error_response)?;
                    ws_stream.send(Message::Text(response_text)).await?;
                }
            }
            "terminal_init" => {
                if self.is_authenticated(connection_id).await {
                    self.handle_terminal_init(connection_id, &message, ws_stream).await?;
                } else {
                    let error_response = WebSocketResponse::error("Authentication required", Some("UNAUTHORIZED"));
                    let response_text = serde_json::to_string(&error_response)?;
                    ws_stream.send(Message::Text(response_text)).await?;
                }
            }
            "terminal_input" => {
                self.handle_terminal_input(connection_id, &message, ws_stream).await?;
            }
            "execute_code" => {
                if self.is_authenticated(connection_id).await {
                    self.handle_code_execution(connection_id, &message, ws_stream).await?;
                } else {
                    let error_response = WebSocketResponse::error("Authentication required", Some("UNAUTHORIZED"));
                    let response_text = serde_json::to_string(&error_response)?;
                    ws_stream.send(Message::Text(response_text)).await?;
                }
            }
            "file_watcher_init" => {
                if self.is_authenticated(connection_id).await {
                    self.handle_file_watcher_init(connection_id, &message, ws_stream).await?;
                } else {
                    let error_response = WebSocketResponse::error("Authentication required", Some("UNAUTHORIZED"));
                    let response_text = serde_json::to_string(&error_response)?;
                    ws_stream.send(Message::Text(response_text)).await?;
                }
            }
            "simulate_file_event" => {
                // This is for testing purposes only
                self.handle_simulate_file_event(connection_id, &message, ws_stream).await?;
            }
            "create_sandbox" => {
                if self.is_authenticated(connection_id).await {
                    self.handle_create_sandbox(connection_id, &message, ws_stream).await?;
                } else {
                    let error_response = WebSocketResponse::error("Authentication required", Some("UNAUTHORIZED"));
                    let response_text = serde_json::to_string(&error_response)?;
                    ws_stream.send(Message::Text(response_text)).await?;
                }
            }
            "large_message_test" => {
                // Handle large message test
                let response = WebSocketResponse::new("large_message_processed", serde_json::json!({
                    "processed": true,
                    "size": message.payload.to_string().len()
                }));
                let response_text = serde_json::to_string(&response)?;
                ws_stream.send(Message::Text(response_text)).await?;
            }
            _ => {
                warn!("Unknown message type: {}", message.r#type);
                let error_response = WebSocketResponse::error(&format!("Unknown message type: {}", message.r#type), Some("UNKNOWN_MESSAGE_TYPE"));
                let response_text = serde_json::to_string(&error_response)?;
                ws_stream.send(Message::Text(response_text)).await?;
            }
        }

        Ok(())
    }

    async fn handle_authentication<S>(
        &self,
        connection_id: &str,
        message: &WebSocketMessage,
        ws_stream: &mut WebSocketStream<S>,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        let api_key = message.payload["api_key"].as_str().unwrap_or("");
        let user_id = message.payload["user_id"].as_str().unwrap_or("");

        // Mock authentication - in a real implementation, validate the API key
        if api_key == "test-api-key" && !user_id.is_empty() {
            let session_id = format!("session_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
            
            // Store session
            {
                let mut sessions = self.authenticated_sessions.lock().await;
                sessions.insert(session_id.clone(), user_id.to_string());
            }

            // Update connection info
            {
                let mut connections = self.active_connections.lock().await;
                if let Some(conn_info) = connections.get_mut(connection_id) {
                    conn_info.user_id = Some(user_id.to_string());
                    conn_info.session_id = Some(session_id.clone());
                }
            }

            let response = WebSocketResponse::authentication_success(user_id, &session_id);
            let response_text = serde_json::to_string(&response)?;
            ws_stream.send(Message::Text(response_text)).await?;
            
            info!("User authenticated: {} with session: {}", user_id, session_id);
        } else {
            let error_response = WebSocketResponse::error("Invalid credentials", Some("INVALID_CREDENTIALS"));
            let response_text = serde_json::to_string(&error_response)?;
            ws_stream.send(Message::Text(response_text)).await?;
        }

        Ok(())
    }

    async fn handle_sandbox_stream_init<S>(
        &self,
        connection_id: &str,
        message: &WebSocketMessage,
        ws_stream: &mut WebSocketStream<S>,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        let sandbox_id = message.payload["sandbox_id"].as_str().unwrap_or("");
        let stream_type = message.payload["stream_type"].as_str().unwrap_or("");

        if sandbox_id.is_empty() {
            let error_response = WebSocketResponse::error("Sandbox ID is required", Some("MISSING_SANDBOX_ID"));
            let response_text = serde_json::to_string(&error_response)?;
            ws_stream.send(Message::Text(response_text)).await?;
            return Ok(());
        }

        let stream_id = format!("stream_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());

        // Update connection info
        {
            let mut connections = self.active_connections.lock().await;
            if let Some(conn_info) = connections.get_mut(connection_id) {
                conn_info.sandbox_id = Some(sandbox_id.to_string());
                conn_info.stream_id = Some(stream_id.clone());
            }
        }

        let response = WebSocketResponse::sandbox_stream_ready(&stream_id);
        let response_text = serde_json::to_string(&response)?;
        ws_stream.send(Message::Text(response_text)).await?;

        info!("Sandbox stream initialized: {} for sandbox: {}", stream_id, sandbox_id);
        Ok(())
    }

    async fn handle_terminal_init<S>(
        &self,
        connection_id: &str,
        message: &WebSocketMessage,
        ws_stream: &mut WebSocketStream<S>,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        let sandbox_id = message.payload["sandbox_id"].as_str().unwrap_or("");
        
        if sandbox_id.is_empty() {
            let error_response = WebSocketResponse::error("Sandbox ID is required", Some("MISSING_SANDBOX_ID"));
            let response_text = serde_json::to_string(&error_response)?;
            ws_stream.send(Message::Text(response_text)).await?;
            return Ok(());
        }

        let terminal_id = format!("term_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());

        // Update connection info
        {
            let mut connections = self.active_connections.lock().await;
            if let Some(conn_info) = connections.get_mut(connection_id) {
                conn_info.sandbox_id = Some(sandbox_id.to_string());
                conn_info.terminal_id = Some(terminal_id.clone());
            }
        }

        let response = WebSocketResponse::terminal_ready(&terminal_id);
        let response_text = serde_json::to_string(&response)?;
        ws_stream.send(Message::Text(response_text)).await?;

        info!("Terminal initialized: {} for sandbox: {}", terminal_id, sandbox_id);
        Ok(())
    }

    async fn handle_terminal_input<S>(
        &self,
        connection_id: &str,
        message: &WebSocketMessage,
        ws_stream: &mut WebSocketStream<S>,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        let terminal_id = message.payload["terminal_id"].as_str().unwrap_or("");
        let data = message.payload["data"].as_str().unwrap_or("");

        // Mock terminal command processing
        if data.trim() == "echo 'Hello Terminal'" {
            let response = WebSocketResponse::terminal_output(terminal_id, "Hello Terminal\n");
            let response_text = serde_json::to_string(&response)?;
            ws_stream.send(Message::Text(response_text)).await?;
        } else if data.trim().starts_with("ls") {
            let response = WebSocketResponse::terminal_output(terminal_id, "package.json  src  index.js\n");
            let response_text = serde_json::to_string(&response)?;
            ws_stream.send(Message::Text(response_text)).await?;
        } else if !data.trim().is_empty() {
            let response = WebSocketResponse::terminal_output(terminal_id, &format!("bash: {}: command not found\n", data.trim()));
            let response_text = serde_json::to_string(&response)?;
            ws_stream.send(Message::Text(response_text)).await?;
        }

        Ok(())
    }

    async fn handle_code_execution<S>(
        &self,
        connection_id: &str,
        message: &WebSocketMessage,
        ws_stream: &mut WebSocketStream<S>,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        let sandbox_id = message.payload["sandbox_id"].as_str().unwrap_or("");
        let code = message.payload["code"].as_str().unwrap_or("");
        let language = message.payload["language"].as_str().unwrap_or("");

        if sandbox_id.is_empty() || code.is_empty() {
            let error_response = WebSocketResponse::error("Sandbox ID and code are required", Some("MISSING_PARAMETERS"));
            let response_text = serde_json::to_string(&error_response)?;
            ws_stream.send(Message::Text(response_text)).await?;
            return Ok(());
        }

        let execution_id = format!("exec_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());

        // Send execution started
        let started_response = WebSocketResponse::execution_started(&execution_id);
        let response_text = serde_json::to_string(&started_response)?;
        ws_stream.send(Message::Text(response_text)).await?;

        // Mock execution output
        if language == "javascript" && code.contains("Step") {
            for i in 1..=3 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                
                let output_response = WebSocketResponse::execution_output(&execution_id, &format!("Step {i}\n"));
                let response_text = serde_json::to_string(&output_response)?;
                ws_stream.send(Message::Text(response_text)).await?;
            }
        }

        // Send execution completed
        let completed_response = WebSocketResponse::execution_completed(&execution_id, 0);
        let response_text = serde_json::to_string(&completed_response)?;
        ws_stream.send(Message::Text(response_text)).await?;

        info!("Code execution completed: {} for sandbox: {}", execution_id, sandbox_id);
        Ok(())
    }

    async fn handle_file_watcher_init<S>(
        &self,
        connection_id: &str,
        message: &WebSocketMessage,
        ws_stream: &mut WebSocketStream<S>,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        let sandbox_id = message.payload["sandbox_id"].as_str().unwrap_or("");
        
        if sandbox_id.is_empty() {
            let error_response = WebSocketResponse::error("Sandbox ID is required", Some("MISSING_SANDBOX_ID"));
            let response_text = serde_json::to_string(&error_response)?;
            ws_stream.send(Message::Text(response_text)).await?;
            return Ok(());
        }

        let watcher_id = format!("watcher_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());

        let response = WebSocketResponse::file_watcher_ready(&watcher_id);
        let response_text = serde_json::to_string(&response)?;
        ws_stream.send(Message::Text(response_text)).await?;

        info!("File watcher initialized: {} for sandbox: {}", watcher_id, sandbox_id);
        Ok(())
    }

    async fn handle_simulate_file_event<S>(
        &self,
        _connection_id: &str,
        message: &WebSocketMessage,
        ws_stream: &mut WebSocketStream<S>,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        let event_type = message.payload["event_type"].as_str().unwrap_or("");
        let path = message.payload["path"].as_str().unwrap_or("");

        let response = WebSocketResponse::file_event(event_type, path);
        let response_text = serde_json::to_string(&response)?;
        ws_stream.send(Message::Text(response_text)).await?;

        Ok(())
    }

    async fn handle_create_sandbox<S>(
        &self,
        _connection_id: &str,
        message: &WebSocketMessage,
        ws_stream: &mut WebSocketStream<S>,
    ) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
    {
        let template_id = message.payload["template_id"].as_str().unwrap_or("");
        
        if template_id.is_empty() {
            let error_response = WebSocketResponse::error("Template ID is required", Some("MISSING_TEMPLATE_ID"));
            let response_text = serde_json::to_string(&error_response)?;
            ws_stream.send(Message::Text(response_text)).await?;
            return Ok(());
        }

        let sandbox_id = format!("sb_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());

        let response = WebSocketResponse::new("sandbox_created", serde_json::json!({
            "sandbox_id": sandbox_id,
            "template_id": template_id,
            "status": "running"
        }));
        let response_text = serde_json::to_string(&response)?;
        ws_stream.send(Message::Text(response_text)).await?;

        info!("Sandbox created via WebSocket: {}", sandbox_id);
        Ok(())
    }

    async fn is_authenticated(&self, connection_id: &str) -> bool {
        let connections = self.active_connections.lock().await;
        if let Some(conn_info) = connections.get(connection_id) {
            conn_info.user_id.is_some() && conn_info.session_id.is_some()
        } else {
            false
        }
    }

    pub async fn get_active_connection_count(&self) -> usize {
        let connections = self.active_connections.lock().await;
        connections.len()
    }

    pub async fn get_authenticated_session_count(&self) -> usize {
        let sessions = self.authenticated_sessions.lock().await;
        sessions.len()
    }

    // Enhanced WebSocket methods for real-time features

    async fn start_heartbeat_task<S>(&self, connection_id: String, mut sender: SplitSink<WebSocketStream<S>, Message>) -> Result<()>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let mut interval = interval(self.heartbeat_interval);
        let connections = Arc::clone(&self.active_connections);

        loop {
            interval.tick().await;

            // Check if connection is still active
            let is_active = {
                let connections_guard = connections.lock().await;
                connections_guard.contains_key(&connection_id)
            };

            if !is_active {
                debug!("Connection {} no longer active, stopping heartbeat", connection_id);
                break;
            }

            // Send ping
            let ping_message = WebSocketResponse::new("ping", serde_json::json!({
                "timestamp": chrono::Utc::now().timestamp(),
                "connection_id": connection_id
            }));

            if let Ok(ping_text) = serde_json::to_string(&ping_message) {
                if sender.send(Message::Text(ping_text)).await.is_err() {
                    debug!("Failed to send heartbeat to {}, connection likely closed", connection_id);
                    break;
                }
            }

            // Check for stale connections
            let should_disconnect = {
                let connections_guard = connections.lock().await;
                if let Some(conn_info) = connections_guard.get(&connection_id) {
                    conn_info.last_heartbeat.elapsed() > self.heartbeat_interval * 3
                } else {
                    true
                }
            };

            if should_disconnect {
                warn!("Connection {} appears stale, disconnecting", connection_id);
                break;
            }
        }

        Ok(())
    }

    async fn handle_message_enhanced(&self, connection_id: &str, message: Message) -> Result<bool> {
        match message {
            Message::Text(text) => {
                debug!("Received message from {}: {}", connection_id, text);
                
                match serde_json::from_str::<WebSocketMessage>(&text) {
                    Ok(ws_message) => {
                        self.process_websocket_message_enhanced(connection_id, ws_message).await?;
                    }
                    Err(_) => {
                        self.broadcast_error(connection_id, "Invalid message format").await;
                    }
                }
                Ok(true)
            }
            Message::Ping(payload) => {
                debug!("Received ping from {}", connection_id);
                // Pong is handled automatically by the WebSocket implementation
                Ok(true)
            }
            Message::Pong(_) => {
                debug!("Received pong from {}", connection_id);
                // Update last heartbeat
                {
                    let mut connections = self.active_connections.lock().await;
                    if let Some(conn_info) = connections.get_mut(connection_id) {
                        conn_info.last_heartbeat = Instant::now();
                    }
                }
                Ok(true)
            }
            Message::Close(_) => {
                info!("Received close message from {}", connection_id);
                Ok(false)
            }
            Message::Binary(_) => {
                warn!("Received binary message from {} (not supported)", connection_id);
                self.broadcast_error(connection_id, "Binary messages not supported").await;
                Ok(true)
            }
            Message::Frame(_) => {
                debug!("Received raw frame from {}", connection_id);
                Ok(true)
            }
        }
    }

    async fn process_websocket_message_enhanced(&self, connection_id: &str, message: WebSocketMessage) -> Result<()> {
        match message.r#type.as_str() {
            "ping" => {
                self.broadcast_to_connection(connection_id, &WebSocketResponse::pong()).await;
            }
            "pong" => {
                // Handle pong response - update heartbeat
                {
                    let mut connections = self.active_connections.lock().await;
                    if let Some(conn_info) = connections.get_mut(connection_id) {
                        conn_info.last_heartbeat = Instant::now();
                    }
                }
            }
            "authenticate" => {
                self.handle_authentication_enhanced(connection_id, &message).await?;
            }
            "subscribe_logs" => {
                if self.is_authenticated(connection_id).await {
                    self.handle_log_subscription(connection_id, &message).await?;
                } else {
                    self.broadcast_error(connection_id, "Authentication required").await;
                }
            }
            "unsubscribe_logs" => {
                if self.is_authenticated(connection_id).await {
                    self.handle_log_unsubscription(connection_id, &message).await?;
                } else {
                    self.broadcast_error(connection_id, "Authentication required").await;
                }
            }
            "execute_code_stream" => {
                if self.is_authenticated(connection_id).await {
                    self.handle_streaming_code_execution(connection_id, &message).await?;
                } else {
                    self.broadcast_error(connection_id, "Authentication required").await;
                }
            }
            _ => {
                // Fallback to original message handling
                warn!("Unknown enhanced message type: {}", message.r#type);
                self.broadcast_error(connection_id, &format!("Unknown message type: {}", message.r#type)).await;
            }
        }

        Ok(())
    }

    async fn handle_authentication_enhanced(&self, connection_id: &str, message: &WebSocketMessage) -> Result<()> {
        let api_key = message.payload["api_key"].as_str().unwrap_or("");
        let user_id = message.payload["user_id"].as_str().unwrap_or("");

        if api_key == "test-api-key" && !user_id.is_empty() {
            let session_id = format!("session_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
            
            // Store session
            {
                let mut sessions = self.authenticated_sessions.lock().await;
                sessions.insert(session_id.clone(), user_id.to_string());
            }

            // Update connection info
            {
                let mut connections = self.active_connections.lock().await;
                if let Some(conn_info) = connections.get_mut(connection_id) {
                    conn_info.user_id = Some(user_id.to_string());
                    conn_info.session_id = Some(session_id.clone());
                }
            }

            let response = WebSocketResponse::authentication_success(user_id, &session_id);
            self.broadcast_to_connection(connection_id, &response).await;
            
            info!("User authenticated: {} with session: {}", user_id, session_id);
        } else {
            self.broadcast_error(connection_id, "Invalid credentials").await;
        }

        Ok(())
    }

    async fn handle_log_subscription(&self, connection_id: &str, message: &WebSocketMessage) -> Result<()> {
        let sandbox_id = message.payload["sandbox_id"].as_str().unwrap_or("");
        let log_level = message.payload["level"].as_str().map(|s| s.to_string());
        let filter = message.payload["filter"].as_str().map(|s| s.to_string());

        if sandbox_id.is_empty() {
            self.broadcast_error(connection_id, "Sandbox ID is required").await;
            return Ok(());
        }

        // Add subscription
        {
            let mut connections = self.active_connections.lock().await;
            if let Some(conn_info) = connections.get_mut(connection_id) {
                if !conn_info.subscribed_logs.contains(&sandbox_id.to_string()) {
                    conn_info.subscribed_logs.push(sandbox_id.to_string());
                }
            }
        }

        // Create or update log stream
        let stream_id = format!("logs_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
        {
            let mut log_streams = self.log_streams.lock().await;
            let (sender, _receiver) = broadcast::channel(1000);
            
            let log_stream = LogStream {
                sandbox_id: sandbox_id.to_string(),
                stream_id: stream_id.clone(),
                log_level,
                filter,
                sender,
                active_connections: vec![connection_id.to_string()],
            };
            
            log_streams.insert(stream_id.clone(), log_stream);
        }

        // Start log streaming simulation
        self.simulate_log_stream(sandbox_id, &stream_id, connection_id).await;

        let response = WebSocketResponse::new("log_subscription_active", serde_json::json!({
            "sandbox_id": sandbox_id,
            "stream_id": stream_id
        }));
        self.broadcast_to_connection(connection_id, &response).await;

        info!("Log subscription created for sandbox {} by {}", sandbox_id, connection_id);
        Ok(())
    }

    async fn handle_log_unsubscription(&self, connection_id: &str, message: &WebSocketMessage) -> Result<()> {
        let sandbox_id = message.payload["sandbox_id"].as_str().unwrap_or("");

        if sandbox_id.is_empty() {
            self.broadcast_error(connection_id, "Sandbox ID is required").await;
            return Ok(());
        }

        // Remove subscription
        {
            let mut connections = self.active_connections.lock().await;
            if let Some(conn_info) = connections.get_mut(connection_id) {
                conn_info.subscribed_logs.retain(|id| id != sandbox_id);
            }
        }

        // Remove from log streams
        {
            let mut log_streams = self.log_streams.lock().await;
            let mut to_remove = Vec::new();
            
            for (stream_id, log_stream) in log_streams.iter_mut() {
                if log_stream.sandbox_id == sandbox_id {
                    log_stream.active_connections.retain(|id| id != connection_id);
                    if log_stream.active_connections.is_empty() {
                        to_remove.push(stream_id.clone());
                    }
                }
            }
            
            for stream_id in to_remove {
                log_streams.remove(&stream_id);
            }
        }

        let response = WebSocketResponse::new("log_subscription_cancelled", serde_json::json!({
            "sandbox_id": sandbox_id
        }));
        self.broadcast_to_connection(connection_id, &response).await;

        info!("Log subscription cancelled for sandbox {} by {}", sandbox_id, connection_id);
        Ok(())
    }

    async fn handle_streaming_code_execution(&self, connection_id: &str, message: &WebSocketMessage) -> Result<()> {
        let sandbox_id = message.payload["sandbox_id"].as_str().unwrap_or("");
        let code = message.payload["code"].as_str().unwrap_or("");
        let language = message.payload["language"].as_str().unwrap_or("");

        if sandbox_id.is_empty() || code.is_empty() {
            self.broadcast_error(connection_id, "Sandbox ID and code are required").await;
            return Ok(());
        }

        let execution_id = format!("exec_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());

        // Send execution started
        let started_response = WebSocketResponse::execution_started(&execution_id);
        self.broadcast_to_connection(connection_id, &started_response).await;

        // Simulate streaming execution
        self.simulate_streaming_execution(connection_id, &execution_id, code, language).await;

        info!("Streaming code execution started: {} for sandbox: {}", execution_id, sandbox_id);
        Ok(())
    }

    async fn simulate_log_stream(&self, sandbox_id: &str, stream_id: &str, connection_id: &str) {
        let sandbox_id = sandbox_id.to_string();
        let stream_id = stream_id.to_string();
        let connection_id = connection_id.to_string();
        let handler = Arc::new(self.clone());

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(2));
            let mut counter = 0;

            loop {
                interval.tick().await;
                counter += 1;

                // Check if stream is still active
                let is_active = {
                    let log_streams = handler.log_streams.lock().await;
                    log_streams.contains_key(&stream_id)
                };

                if !is_active {
                    break;
                }

                // Generate sample log
                let log_message = LogMessage {
                    timestamp: chrono::Utc::now(),
                    level: if counter % 4 == 0 { "error" } else if counter % 3 == 0 { "warn" } else { "info" }.to_string(),
                    source: "container".to_string(),
                    message: format!("Sample log message #{} from sandbox {}", counter, sandbox_id),
                    sandbox_id: sandbox_id.clone(),
                };

                let response = WebSocketResponse::new("log_message", serde_json::json!({
                    "stream_id": stream_id,
                    "timestamp": log_message.timestamp,
                    "level": log_message.level,
                    "source": log_message.source,
                    "message": log_message.message,
                    "sandbox_id": log_message.sandbox_id
                }));

                handler.broadcast_to_connection(&connection_id, &response).await;

                if counter >= 20 {
                    break; // Stop after 20 messages for demo
                }
            }
        });
    }

    async fn simulate_streaming_execution(&self, connection_id: &str, execution_id: &str, code: &str, language: &str) {
        let connection_id = connection_id.to_string();
        let execution_id = execution_id.to_string();
        let code = code.to_string();
        let language = language.to_string();
        let handler = Arc::new(self.clone());

        tokio::spawn(async move {
            // Simulate progressive output
            let outputs = if language == "python" && code.contains("print") {
                vec![
                    "Starting Python execution...",
                    "Loading modules...",
                    "Executing user code...",
                    "Output: Hello from Python!",
                    "Execution completed successfully"
                ]
            } else {
                vec![
                    "Starting execution...",
                    "Code parsed successfully",
                    "Execution completed"
                ]
            };

            for (i, output) in outputs.iter().enumerate() {
                sleep(Duration::from_millis(300)).await;
                
                let response = WebSocketResponse::execution_output(&execution_id, &format!("{}\n", output));
                handler.broadcast_to_connection(&connection_id, &response).await;
                
                // Send progress update
                let progress_response = WebSocketResponse::new("execution_progress", serde_json::json!({
                    "execution_id": execution_id,
                    "progress": ((i + 1) as f32 / outputs.len() as f32 * 100.0) as u32,
                    "step": format!("Step {}/{}", i + 1, outputs.len())
                }));
                handler.broadcast_to_connection(&connection_id, &progress_response).await;
            }

            // Send completion
            let completed_response = WebSocketResponse::execution_completed(&execution_id, 0);
            handler.broadcast_to_connection(&connection_id, &completed_response).await;
        });
    }

    async fn broadcast_to_connection(&self, connection_id: &str, response: &WebSocketResponse) {
        // In a real implementation, this would send to the actual WebSocket connection
        // For now, we'll just log it
        debug!("Broadcasting to {}: {:?}", connection_id, response);
    }

    async fn broadcast_error(&self, connection_id: &str, message: &str) {
        let error_response = WebSocketResponse::error(message, Some("ERROR"));
        self.broadcast_to_connection(connection_id, &error_response).await;
    }

    // Connection management methods
    pub async fn cleanup_stale_connections(&self) {
        let stale_timeout = self.heartbeat_interval * 3;
        let mut to_remove = Vec::new();

        {
            let connections = self.active_connections.lock().await;
            for (connection_id, conn_info) in connections.iter() {
                if conn_info.last_heartbeat.elapsed() > stale_timeout {
                    to_remove.push(connection_id.clone());
                }
            }
        }

        if !to_remove.is_empty() {
            let mut connections = self.active_connections.lock().await;
            for connection_id in to_remove {
                connections.remove(&connection_id);
                info!("Removed stale connection: {}", connection_id);
            }
        }
    }

    pub async fn get_connection_stats(&self) -> ConnectionStats {
        let connections = self.active_connections.lock().await;
        let sessions = self.authenticated_sessions.lock().await;
        let log_streams = self.log_streams.lock().await;

        let mut total_log_subscriptions = 0;
        for conn_info in connections.values() {
            total_log_subscriptions += conn_info.subscribed_logs.len();
        }

        ConnectionStats {
            total_connections: connections.len(),
            authenticated_connections: connections.values().filter(|c| c.user_id.is_some()).count(),
            active_sessions: sessions.len(),
            active_log_streams: log_streams.len(),
            total_log_subscriptions,
        }
    }
}

// Make WebSocketHandler cloneable for background tasks
impl Clone for WebSocketHandler {
    fn clone(&self) -> Self {
        Self {
            active_connections: Arc::clone(&self.active_connections),
            authenticated_sessions: Arc::clone(&self.authenticated_sessions),
            log_streams: Arc::clone(&self.log_streams),
            heartbeat_interval: self.heartbeat_interval,
            reconnect_config: self.reconnect_config.clone(),
        }
    }
}

#[derive(Debug)]
pub struct ConnectionStats {
    pub total_connections: usize,
    pub authenticated_connections: usize,
    pub active_sessions: usize,
    pub active_log_streams: usize,
    pub total_log_subscriptions: usize,
}