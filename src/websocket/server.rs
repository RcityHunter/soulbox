use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tracing::{info, error};

use super::handler::WebSocketHandler;
use crate::error::Result;

pub struct WebSocketServer {
    handler: Arc<WebSocketHandler>,
}

impl Default for WebSocketServer {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSocketServer {
    pub fn new() -> Self {
        Self {
            handler: Arc::new(WebSocketHandler::new()),
        }
    }

    pub async fn start(&self, addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(&addr).await?;
        info!("WebSocket server listening on: {}", addr);

        while let Ok((stream, peer_addr)) = listener.accept().await {
            info!("New WebSocket connection from: {}", peer_addr);
            
            let handler = Arc::clone(&self.handler);
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(handler, stream, peer_addr).await {
                    error!("Error handling WebSocket connection from {}: {}", peer_addr, e);
                }
            });
        }

        Ok(())
    }

    async fn handle_connection(
        handler: Arc<WebSocketHandler>,
        stream: TcpStream,
        peer_addr: SocketAddr,
    ) -> Result<()> {
        // Set up WebSocket
        let ws_stream = match accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                error!("Failed to accept WebSocket connection from {}: {}", peer_addr, e);
                return Err(e.into());
            }
        };

        info!("WebSocket handshake completed for: {}", peer_addr);

        // Handle the WebSocket connection
        if let Err(e) = handler.handle_connection(ws_stream).await {
            error!("WebSocket connection error for {}: {}", peer_addr, e);
        }

        info!("WebSocket connection closed for: {}", peer_addr);
        Ok(())
    }

    pub async fn get_active_connection_count(&self) -> usize {
        self.handler.get_active_connection_count().await
    }

    pub async fn get_authenticated_session_count(&self) -> usize {
        self.handler.get_authenticated_session_count().await
    }

    // Helper method to create a test WebSocket server on a random port
    #[cfg(test)]
    pub async fn start_test_server() -> Result<(Self, SocketAddr)> {
        let server = Self::new();
        let addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();
        let listener = TcpListener::bind(&addr).await?;
        let actual_addr = listener.local_addr()?;
        
        let handler = Arc::clone(&server.handler);
        tokio::spawn(async move {
            while let Ok((stream, peer_addr)) = listener.accept().await {
                let handler = Arc::clone(&handler);
                tokio::spawn(async move {
                    if let Err(e) = Self::handle_connection(handler, stream, peer_addr).await {
                        error!("Test server connection error: {}", e);
                    }
                });
            }
        });

        Ok((server, actual_addr))
    }
}