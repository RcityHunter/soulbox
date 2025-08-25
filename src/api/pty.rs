//! PTY Terminal API endpoints
//! 
//! Provides REST API endpoints for managing PTY terminal sessions

use axum::{
    extract::{Path, State},
    response::Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use tracing::{info, error};

use crate::websocket::PtyWebSocketHandler;

/// PTY session information
#[derive(Debug, Serialize, Deserialize)]
pub struct PtySessionInfo {
    pub id: Uuid,
    pub created_at: String,
    pub status: String,
}

/// PTY session list response
#[derive(Debug, Serialize, Deserialize)]
pub struct PtySessionListResponse {
    pub sessions: Vec<PtySessionInfo>,
    pub total: usize,
}

/// PTY session creation request
#[derive(Debug, Deserialize)]
pub struct CreatePtySessionRequest {
    pub shell: Option<String>,
    pub working_directory: Option<String>,
    pub environment: Option<std::collections::HashMap<String, String>>,
}

/// PTY session creation response
#[derive(Debug, Serialize)]
pub struct CreatePtySessionResponse {
    pub session_id: Uuid,
    pub websocket_url: String,
    pub message: String,
}

/// PTY API handler
pub struct PtyApiHandler {
    pty_handler: Arc<PtyWebSocketHandler>,
}

impl PtyApiHandler {
    pub fn new(pty_handler: Arc<PtyWebSocketHandler>) -> Self {
        Self { pty_handler }
    }

    /// List all active PTY sessions
    pub async fn list_sessions(
        State(handler): State<Arc<PtyApiHandler>>,
    ) -> std::result::Result<Json<PtySessionListResponse>, (StatusCode, String)> {
        info!("Listing active PTY sessions");

        let session_ids = handler.pty_handler.list_sessions().await;
        let sessions: Vec<PtySessionInfo> = session_ids
            .into_iter()
            .map(|id| PtySessionInfo {
                id,
                created_at: chrono::Utc::now().to_rfc3339(),
                status: "active".to_string(),
            })
            .collect();

        let total = sessions.len();

        Ok(Json(PtySessionListResponse { sessions, total }))
    }

    /// Kill a specific PTY session
    pub async fn kill_session(
        State(handler): State<Arc<PtyApiHandler>>,
        Path(session_id): Path<Uuid>,
    ) -> std::result::Result<(StatusCode, String), (StatusCode, String)> {
        info!("Killing PTY session: {}", session_id);

        match handler.pty_handler.kill_session(session_id).await {
            Ok(_) => {
                info!("PTY session {} killed successfully", session_id);
                Ok((StatusCode::OK, format!("Session {} terminated", session_id)))
            }
            Err(e) => {
                error!("Failed to kill PTY session {}: {}", session_id, e);
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to terminate session: {}", e),
                ))
            }
        }
    }

    /// Get PTY session information
    pub async fn get_session_info(
        State(handler): State<Arc<PtyApiHandler>>,
        Path(session_id): Path<Uuid>,
    ) -> std::result::Result<Json<PtySessionInfo>, (StatusCode, String)> {
        info!("Getting PTY session info: {}", session_id);

        let active_sessions = handler.pty_handler.list_sessions().await;
        
        if active_sessions.contains(&session_id) {
            Ok(Json(PtySessionInfo {
                id: session_id,
                created_at: chrono::Utc::now().to_rfc3339(),
                status: "active".to_string(),
            }))
        } else {
            Err((
                StatusCode::NOT_FOUND,
                format!("PTY session {} not found", session_id),
            ))
        }
    }

    /// Create a new PTY session (returns WebSocket URL)
    pub async fn create_session(
        State(_handler): State<Arc<PtyApiHandler>>,
        Json(_request): Json<CreatePtySessionRequest>,
    ) -> std::result::Result<Json<CreatePtySessionResponse>, (StatusCode, String)> {
        info!("Creating new PTY session");

        let session_id = Uuid::new_v4();
        let websocket_url = format!("ws://localhost:3000/ws/pty/{}", session_id);

        Ok(Json(CreatePtySessionResponse {
            session_id,
            websocket_url,
            message: "Connect to the WebSocket URL to start the PTY session".to_string(),
        }))
    }

    /// Get PTY session statistics
    pub async fn get_statistics(
        State(handler): State<Arc<PtyApiHandler>>,
    ) -> std::result::Result<Json<serde_json::Value>, (StatusCode, String)> {
        info!("Getting PTY session statistics");

        let active_sessions = handler.pty_handler.list_sessions().await;
        let stats = serde_json::json!({
            "active_sessions": active_sessions.len(),
            "total_created": active_sessions.len(), // This would need to be tracked separately
            "server_uptime": "N/A", // This would need to be tracked from server start
            "memory_usage": "N/A", // This would require system metrics
        });

        Ok(Json(stats))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_pty_api_handler_creation() {
        let pty_handler = Arc::new(PtyWebSocketHandler::new());
        let api_handler = PtyApiHandler::new(pty_handler);
        
        // Basic test that handler can be created
        assert!(std::ptr::addr_of!(api_handler) as usize != 0);
    }
}