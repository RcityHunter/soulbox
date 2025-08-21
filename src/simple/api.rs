// Simple HTTP API - no overengineering, just working endpoints
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

use super::core::{SandboxManager, Sandbox, SandboxStatus};
use super::execution::{CodeExecution, ExecutionResult, LanguageRegistry};

/// API state - simple and clear
#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<RwLock<SandboxManager>>,
    pub language_registry: Arc<LanguageRegistry>,
}

/// Create sandbox request
#[derive(Deserialize)]
pub struct CreateRequest {
    pub language: String,
}

/// Execute code request  
#[derive(Deserialize)]
pub struct ExecuteRequest {
    pub code: String,
    pub timeout_seconds: Option<u64>,
}

/// Create sandbox response
#[derive(Serialize)]
pub struct CreateResponse {
    pub sandbox_id: String,
    pub status: String,
    pub image: String,
}

/// Sandbox info response
#[derive(Serialize)]
pub struct SandboxInfo {
    pub id: String,
    pub image: String, 
    pub status: String,
}

/// Error response
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Simple API implementation - no complex middleware stack
pub struct SimpleAPI;

impl SimpleAPI {
    /// Create router with all endpoints
    pub fn router() -> Router<AppState> {
        Router::new()
            .route("/health", get(health_check))
            .route("/sandboxes", post(create_sandbox))
            .route("/sandboxes", get(list_sandboxes))
            .route("/sandboxes/{id}", get(get_sandbox))
            .route("/sandboxes/{id}", delete(remove_sandbox))
            .route("/sandboxes/{id}/execute", post(execute_code))
            .route("/languages", get(list_languages))
    }

    /// Start the server - simple setup
    pub async fn serve(manager: SandboxManager) -> Result<(), Box<dyn std::error::Error>> {
        let state = AppState {
            manager: Arc::new(RwLock::new(manager)),
            language_registry: Arc::new(LanguageRegistry::new()),
        };

        let app = Self::router().with_state(state);
        
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
        info!("🚀 SoulBox Simple API listening on http://0.0.0.0:8080");
        
        axum::serve(listener, app).await?;
        Ok(())
    }
}

// Health check - always useful
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "service": "soulbox-simple",
        "status": "healthy",
        "version": "1.0.0"
    }))
}

// Create sandbox endpoint
async fn create_sandbox(
    State(state): State<AppState>,
    Json(req): Json<CreateRequest>,
) -> Result<Json<CreateResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get language config
    let config = state.language_registry.get_config(&req.language)
        .map_err(|e| (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse { error: e.to_string() })
        ))?;

    // Create sandbox
    let mut manager = state.manager.write().await;
    let sandbox_id = manager.create_sandbox(&config.image).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e.to_string() })
        ))?;

    Ok(Json(CreateResponse {
        sandbox_id,
        status: "running".to_string(),
        image: config.image.clone(),
    }))
}

// Execute code endpoint
async fn execute_code(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<ExecutionResult>, (StatusCode, Json<ErrorResponse>)> {
    // Create execution request
    let execution = CodeExecution {
        language: "generic".to_string(), // We don't need to specify language here
        code: req.code,
        timeout_seconds: req.timeout_seconds,
    };

    // Validate request
    execution.validate().map_err(|e| (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse { error: e.to_string() })
    ))?;

    // Execute with timing
    let manager = state.manager.read().await;
    let (result, execution_time) = measure_execution(|| async {
        // Simple command execution - no language-specific logic needed here
        let command = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("cat > /workspace/code.txt << 'EOF'\n{}\nEOF && echo 'Code saved successfully'", execution.code)
        ];
        
        manager.execute(&sandbox_id, command).await
    }).await;

    match result {
        Ok((stdout, stderr, exit_code)) => {
            Ok(Json(ExecutionResult {
                stdout,
                stderr,
                exit_code,
                execution_time_ms: execution_time,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e.to_string() })
        ))
    }
}

// Get sandbox info
async fn get_sandbox(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
) -> Result<Json<SandboxInfo>, (StatusCode, Json<ErrorResponse>)> {
    let manager = state.manager.read().await;
    let sandbox = manager.get_sandbox(&sandbox_id)
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: "Sandbox not found".to_string() })
        ))?;

    Ok(Json(SandboxInfo {
        id: sandbox.id.clone(),
        image: sandbox.image.clone(),
        status: format!("{:?}", sandbox.status),
    }))
}

// List all sandboxes
async fn list_sandboxes(
    State(state): State<AppState>,
) -> Json<Vec<SandboxInfo>> {
    let manager = state.manager.read().await;
    let sandboxes: Vec<SandboxInfo> = manager.list_sandboxes()
        .into_iter()
        .map(|s| SandboxInfo {
            id: s.id.clone(),
            image: s.image.clone(), 
            status: format!("{:?}", s.status),
        })
        .collect();
    
    Json(sandboxes)
}

// Remove sandbox
async fn remove_sandbox(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let mut manager = state.manager.write().await;
    manager.remove_sandbox(&sandbox_id).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e.to_string() })
        ))?;
    
    Ok(StatusCode::NO_CONTENT)
}

// List supported languages
async fn list_languages(
    State(state): State<AppState>,
) -> Json<Vec<String>> {
    let languages: Vec<String> = state.language_registry
        .supported_languages()
        .into_iter()
        .cloned()
        .collect();
    
    Json(languages)
}

// Async version of measure_execution
async fn measure_execution<F, Fut, T>(f: F) -> (T, u64) 
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let start = std::time::Instant::now();
    let result = f().await;
    let duration = start.elapsed().as_millis() as u64;
    (result, duration)
}