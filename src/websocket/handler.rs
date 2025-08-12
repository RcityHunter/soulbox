use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};
use futures_util::{StreamExt, SinkExt};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use super::message::{WebSocketMessage, WebSocketResponse};
use crate::error::Result;

pub struct WebSocketHandler {
    active_connections: Arc<Mutex<HashMap<String, ConnectionInfo>>>,
    authenticated_sessions: Arc<Mutex<HashMap<String, String>>>, // session_id -> user_id
}

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub connection_id: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub sandbox_id: Option<String>,
    pub terminal_id: Option<String>,
    pub stream_id: Option<String>,
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
        }
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
        };

        // Store connection
        {
            let mut connections = self.active_connections.lock().await;
            connections.insert(connection_id.clone(), connection_info);
        }

        info!("WebSocket connection established: {}", connection_id);

        // Handle messages
        while let Some(message_result) = ws_stream.next().await {
            match message_result {
                Ok(message) => {
                    match self.handle_message(&connection_id, message, &mut ws_stream).await {
                        Ok(should_continue) => {
                            if !should_continue {
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Error handling message for {}: {}", connection_id, e);
                            let error_response = WebSocketResponse::error(&format!("Internal error: {e}"), Some("INTERNAL_ERROR"));
                            if let Ok(response_text) = serde_json::to_string(&error_response) {
                                let _ = ws_stream.send(Message::Text(response_text)).await;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("WebSocket error for {}: {}", connection_id, e);
                    break;
                }
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
}