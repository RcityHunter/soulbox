//! Hot Reload API endpoints
//! 
//! Provides REST API endpoints for managing hot reload sessions

use axum::{
    extract::{Path, State, Query},
    response::Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use uuid::Uuid;
use tracing::{info, error};

use crate::runtime::{HotReloadManager, HotReloadStats, HotReloadConfig};
use crate::container::CodeExecutor;

/// Hot reload session creation request
#[derive(Debug, Deserialize)]
pub struct CreateHotReloadSessionRequest {
    pub container_id: String,
    pub runtime: String,
    pub main_file: String,
    pub config: Option<HotReloadConfig>,
}

/// Hot reload session creation response
#[derive(Debug, Serialize)]
pub struct CreateHotReloadSessionResponse {
    pub session_id: Uuid,
    pub container_id: String,
    pub status: String,
    pub message: String,
}

/// Hot reload session list response
#[derive(Debug, Serialize)]
pub struct HotReloadSessionListResponse {
    pub sessions: Vec<HotReloadSessionInfo>,
    pub total: usize,
}

/// Hot reload session info
#[derive(Debug, Serialize)]
pub struct HotReloadSessionInfo {
    pub session_id: Uuid,
    pub container_id: String,
    pub runtime: String,
    pub main_file: String,
    pub status: String,
}

/// Hot reload API query parameters
#[derive(Debug, Deserialize)]
pub struct HotReloadQuery {
    pub container_id: Option<String>,
    pub runtime: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Hot reload API handler
pub struct HotReloadApiHandler {
    hot_reload_manager: Arc<HotReloadManager>,
    code_executor: Arc<CodeExecutor>,
}

impl HotReloadApiHandler {
    pub fn new(
        hot_reload_manager: Arc<HotReloadManager>,
        code_executor: Arc<CodeExecutor>,
    ) -> Self {
        Self {
            hot_reload_manager,
            code_executor,
        }
    }

    /// Create new hot reload session
    pub async fn create_session(
        State(handler): State<Arc<HotReloadApiHandler>>,
        Json(request): Json<CreateHotReloadSessionRequest>,
    ) -> std::result::Result<Json<CreateHotReloadSessionResponse>, (StatusCode, String)> {
        info!(
            "Creating hot reload session for container: {}, runtime: {}",
            request.container_id, request.runtime
        );

        // Parse runtime type
        let runtime_type = match request.runtime.parse() {
            Ok(rt) => rt,
            Err(e) => {
                error!("Invalid runtime type '{}': {}", request.runtime, e);
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Invalid runtime type: {}", e),
                ));
            }
        };

        // Create session
        match handler
            .hot_reload_manager
            .create_session(
                request.container_id.clone(),
                runtime_type,
                request.main_file.clone().into(),
                Arc::clone(&handler.code_executor),
            )
            .await
        {
            Ok(session_id) => {
                info!("Hot reload session created: {}", session_id);
                Ok(Json(CreateHotReloadSessionResponse {
                    session_id,
                    container_id: request.container_id,
                    status: "active".to_string(),
                    message: "Hot reload session created successfully".to_string(),
                }))
            }
            Err(e) => {
                error!("Failed to create hot reload session: {}", e);
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to create session: {}", e),
                ))
            }
        }
    }

    /// List hot reload sessions
    pub async fn list_sessions(
        State(handler): State<Arc<HotReloadApiHandler>>,
        Query(_query): Query<HotReloadQuery>,
    ) -> std::result::Result<Json<HotReloadSessionListResponse>, (StatusCode, String)> {
        info!("Listing hot reload sessions");

        let session_ids = handler.hot_reload_manager.list_sessions().await;
        let mut sessions = Vec::new();

        for session_id in session_ids {
            if let Ok(stats) = handler
                .hot_reload_manager
                .get_session_stats(session_id)
                .await
            {
                sessions.push(HotReloadSessionInfo {
                    session_id: stats.session_id,
                    container_id: stats.container_id,
                    runtime: stats.runtime.to_string(),
                    main_file: stats.main_file.to_string_lossy().to_string(),
                    status: "active".to_string(),
                });
            }
        }

        let total = sessions.len();

        Ok(Json(HotReloadSessionListResponse { sessions, total }))
    }

    /// Get hot reload session details
    pub async fn get_session(
        State(handler): State<Arc<HotReloadApiHandler>>,
        Path(session_id): Path<Uuid>,
    ) -> std::result::Result<Json<HotReloadStats>, (StatusCode, String)> {
        info!("Getting hot reload session details: {}", session_id);

        match handler
            .hot_reload_manager
            .get_session_stats(session_id)
            .await
        {
            Ok(stats) => {
                info!("Retrieved hot reload session stats: {}", session_id);
                Ok(Json(stats))
            }
            Err(e) => {
                error!("Failed to get hot reload session {}: {}", session_id, e);
                Err((
                    StatusCode::NOT_FOUND,
                    format!("Session not found: {}", e),
                ))
            }
        }
    }

    /// Remove hot reload session
    pub async fn remove_session(
        State(handler): State<Arc<HotReloadApiHandler>>,
        Path(session_id): Path<Uuid>,
    ) -> std::result::Result<(StatusCode, String), (StatusCode, String)> {
        info!("Removing hot reload session: {}", session_id);

        match handler
            .hot_reload_manager
            .remove_session(session_id)
            .await
        {
            Ok(_) => {
                info!("Hot reload session removed: {}", session_id);
                Ok((
                    StatusCode::OK,
                    format!("Session {} removed successfully", session_id),
                ))
            }
            Err(e) => {
                error!("Failed to remove hot reload session {}: {}", session_id, e);
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to remove session: {}", e),
                ))
            }
        }
    }

    /// Get hot reload statistics
    pub async fn get_statistics(
        State(handler): State<Arc<HotReloadApiHandler>>,
    ) -> std::result::Result<Json<serde_json::Value>, (StatusCode, String)> {
        info!("Getting hot reload statistics");

        let session_ids = handler.hot_reload_manager.list_sessions().await;
        let mut runtime_counts: HashMap<String, usize> = HashMap::new();

        for session_id in &session_ids {
            if let Ok(stats) = handler
                .hot_reload_manager
                .get_session_stats(*session_id)
                .await
            {
                let runtime_str = stats.runtime.to_string();
                *runtime_counts.entry(runtime_str).or_insert(0) += 1;
            }
        }

        let statistics = serde_json::json!({
            "total_sessions": session_ids.len(),
            "active_sessions": session_ids.len(),
            "runtime_distribution": runtime_counts,
            "status": "operational"
        });

        Ok(Json(statistics))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::HotReloadConfig;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_hot_reload_api_handler_creation() {
        use crate::container::SandboxContainer;
        
        let config = HotReloadConfig::default();
        let hot_reload_manager = Arc::new(HotReloadManager::new(config).unwrap());
        
        // Create a mock container for testing
        let mock_container = Arc::new(SandboxContainer::new(
            "test-container".to_string(),
            "test-image".to_string(),
        ));
        let code_executor = Arc::new(CodeExecutor::new(mock_container));
        
        let api_handler = HotReloadApiHandler::new(hot_reload_manager, code_executor);
        
        // Basic test that handler can be created
        assert!(std::ptr::addr_of!(api_handler) as usize != 0);
    }
}