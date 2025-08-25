//! WebSocket PTY Terminal Implementation
//! 
//! Provides full terminal emulation over WebSocket for interactive shell sessions

use tokio::process::{Command, Child};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, error, debug};
use uuid::Uuid;

/// PTY message types for WebSocket communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PtyMessage {
    /// Input from client to PTY
    Input { data: String },
    /// Output from PTY to client
    Output { data: String },
    /// Resize terminal
    Resize { cols: u16, rows: u16 },
    /// Terminal closed
    Close { code: i32 },
    /// Error occurred
    Error { message: String },
    /// Heartbeat
    Ping,
    Pong,
}

/// PTY session state
pub struct PtySession {
    id: Uuid,
    process: Option<Child>,
    stdin_tx: Option<mpsc::Sender<Vec<u8>>>,
    dimensions: (u16, u16), // (cols, rows)
    created_at: std::time::Instant,
}

impl PtySession {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            process: None,
            stdin_tx: None,
            dimensions: (80, 24), // Default terminal size
            created_at: std::time::Instant::now(),
        }
    }
    
    /// Start PTY process
    pub async fn start(&mut self, shell: Option<String>) -> Result<(), String> {
        let shell = shell.unwrap_or_else(|| {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
        });
        
        debug!("Starting PTY session {} with shell: {}", self.id, shell);
        
        // Create PTY process
        let mut child = Command::new(&shell)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("TERM", "xterm-256color")
            .env("COLUMNS", self.dimensions.0.to_string())
            .env("LINES", self.dimensions.1.to_string())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to spawn shell: {}", e))?;
        
        // Setup stdin channel
        let stdin = child.stdin.take().ok_or("Failed to get stdin")?;
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(100);
        
        // Spawn stdin writer task
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(data) = stdin_rx.recv().await {
                if let Err(e) = stdin.write_all(&data).await {
                    error!("Failed to write to stdin: {}", e);
                    break;
                }
                let _ = stdin.flush().await;
            }
        });
        
        self.process = Some(child);
        self.stdin_tx = Some(stdin_tx);
        
        info!("PTY session {} started successfully", self.id);
        Ok(())
    }
    
    /// Send input to PTY
    pub async fn send_input(&self, data: &[u8]) -> Result<(), String> {
        if let Some(tx) = &self.stdin_tx {
            tx.send(data.to_vec()).await
                .map_err(|e| format!("Failed to send input: {}", e))?;
        }
        Ok(())
    }
    
    /// Resize PTY
    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), String> {
        self.dimensions = (cols, rows);
        
        // Send resize signal to process if using real PTY
        // Note: This requires platform-specific implementation
        #[cfg(unix)]
        {
            if let Some(process) = &self.process {
                if let Some(pid) = process.id() {
                    unsafe {
                        // Send SIGWINCH signal
                        libc::kill(pid as i32, libc::SIGWINCH);
                    }
                }
            }
        }
        
        debug!("PTY session {} resized to {}x{}", self.id, cols, rows);
        Ok(())
    }
    
    /// Kill PTY process
    pub async fn kill(&mut self) -> Result<(), String> {
        if let Some(mut process) = self.process.take() {
            process.kill().await
                .map_err(|e| format!("Failed to kill process: {}", e))?;
        }
        info!("PTY session {} terminated", self.id);
        Ok(())
    }
}

/// PTY WebSocket handler
pub struct PtyWebSocketHandler {
    sessions: Arc<RwLock<HashMap<Uuid, Arc<RwLock<PtySession>>>>>,
}

impl PtyWebSocketHandler {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Handle WebSocket connection
    pub async fn handle_connection<S>(&self, ws_stream: WebSocketStream<S>)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let (output_tx, mut output_rx) = mpsc::channel::<PtyMessage>(100);
        
        // Create new PTY session
        let mut session = PtySession::new();
        let session_id = session.id;
        
        // Start PTY
        if let Err(e) = session.start(None).await {
            let _ = ws_sender.send(Message::Text(
                serde_json::to_string(&PtyMessage::Error { 
                    message: format!("Failed to start PTY: {}", e) 
                }).unwrap()
            )).await;
            return;
        }
        
        let session = Arc::new(RwLock::new(session));
        self.sessions.write().await.insert(session_id, session.clone());
        
        // Spawn output reader task
        let session_clone = session.clone();
        let output_tx_clone = output_tx.clone();
        tokio::spawn(async move {
            let mut session = session_clone.write().await;
            if let Some(process) = &mut session.process {
                if let Some(stdout) = process.stdout.take() {
                    let mut stdout = stdout;
                    let mut buffer = vec![0; 4096];
                    
                    loop {
                        match stdout.read(&mut buffer).await {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                                let _ = output_tx_clone.send(PtyMessage::Output { data }).await;
                            }
                            Err(e) => {
                                error!("Failed to read stdout: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        });
        
        // Spawn stderr reader task
        let session_clone = session.clone();
        let output_tx_clone = output_tx.clone();
        tokio::spawn(async move {
            let mut session = session_clone.write().await;
            if let Some(process) = &mut session.process {
                if let Some(stderr) = process.stderr.take() {
                    let mut stderr = stderr;
                    let mut buffer = vec![0; 4096];
                    
                    loop {
                        match stderr.read(&mut buffer).await {
                            Ok(0) => break,
                            Ok(n) => {
                                let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                                let _ = output_tx_clone.send(PtyMessage::Output { data }).await;
                            }
                            Err(e) => {
                                error!("Failed to read stderr: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        });
        
        // Handle WebSocket messages
        let session_clone = session.clone();
        let output_tx_clone = output_tx.clone();
        tokio::spawn(async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(pty_msg) = serde_json::from_str::<PtyMessage>(&text) {
                            match pty_msg {
                                PtyMessage::Input { data } => {
                                    let session = session_clone.read().await;
                                    if let Err(e) = session.send_input(data.as_bytes()).await {
                                        let _ = output_tx_clone.send(PtyMessage::Error { 
                                            message: e 
                                        }).await;
                                    }
                                }
                                PtyMessage::Resize { cols, rows } => {
                                    let mut session = session_clone.write().await;
                                    if let Err(e) = session.resize(cols, rows).await {
                                        let _ = output_tx_clone.send(PtyMessage::Error { 
                                            message: e 
                                        }).await;
                                    }
                                }
                                PtyMessage::Ping => {
                                    let _ = output_tx_clone.send(PtyMessage::Pong).await;
                                }
                                _ => {}
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            
            // Clean up session
            let mut session = session_clone.write().await;
            let _ = session.kill().await;
        });
        
        // Send output messages to WebSocket
        while let Some(msg) = output_rx.recv().await {
            let json = serde_json::to_string(&msg).unwrap();
            if ws_sender.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
        
        // Remove session
        self.sessions.write().await.remove(&session_id);
        info!("PTY session {} closed", session_id);
    }
    
    /// List active sessions
    pub async fn list_sessions(&self) -> Vec<Uuid> {
        self.sessions.read().await.keys().cloned().collect()
    }
    
    /// Kill specific session
    pub async fn kill_session(&self, id: Uuid) -> Result<(), String> {
        if let Some(session) = self.sessions.write().await.remove(&id) {
            let mut session = session.write().await;
            session.kill().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_pty_session() {
        let mut session = PtySession::new();
        
        // Start session
        assert!(session.start(Some("/bin/bash".to_string())).await.is_ok());
        
        // Send command
        assert!(session.send_input(b"echo hello\n").await.is_ok());
        
        // Resize
        assert!(session.resize(100, 40).await.is_ok());
        
        // Kill
        assert!(session.kill().await.is_ok());
    }
}