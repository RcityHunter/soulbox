//! E2B API Service - REST API endpoints compatible with E2B

use crate::e2b::{models::*, adapter::E2BAdapter, E2BError};
use crate::sandbox_manager::SandboxManager;
use crate::sandbox::SandboxConfig;
use crate::template::TemplateManager;
use crate::error::Result;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;
use tracing::{info, error, debug};

/// E2B API service state
#[derive(Clone)]
pub struct E2BApiState {
    sandbox_manager: Arc<SandboxManager>,
    template_manager: Arc<TemplateManager>,
    filesystem_manager: Arc<FilesystemManager>,
    start_time: Instant,
}

/// E2B API service
pub struct E2BApiService {
    state: E2BApiState,
}

impl E2BApiService {
    /// Create new E2B API service
    pub fn new(
        sandbox_manager: Arc<SandboxManager>,
        template_manager: Arc<TemplateManager>,
        filesystem_manager: Arc<FilesystemManager>,
    ) -> Self {
        Self {
            state: E2BApiState {
                sandbox_manager,
                template_manager,
                filesystem_manager,
                start_time: Instant::now(),
            },
        }
    }
    
    /// Build router for E2B API
    pub fn router(self) -> Router {
        Router::new()
            // Health endpoint
            .route("/health", get(Self::health))
            // Sandbox endpoints
            .route("/sandboxes", post(Self::create_sandbox))
            .route("/sandboxes/:id", get(Self::get_sandbox))
            .route("/sandboxes/:id", delete(Self::delete_sandbox))
            // Process endpoints
            .route("/sandboxes/:id/processes", post(Self::run_process))
            // File endpoints
            .route("/sandboxes/:id/files", get(Self::download_file))
            .route("/sandboxes/:id/files", post(Self::upload_file))
            .route("/sandboxes/:id/files/list", get(Self::list_files))
            // Template endpoints
            .route("/templates", get(Self::list_templates))
            .with_state(self.state)
    }
    
    /// Health check endpoint
    async fn health(State(state): State<E2BApiState>) -> impl IntoResponse {
        let uptime = state.start_time.elapsed().as_secs();
        let sandboxes_count = state.sandbox_manager.list_sandboxes().await
            .map(|list| list.len())
            .unwrap_or(0);
        
        Json(E2BHealthResponse {
            status: "healthy".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime,
            sandboxes_count,
        })
    }
    
    /// Create sandbox endpoint
    async fn create_sandbox(
        State(state): State<E2BApiState>,
        Json(request): Json<E2BCreateSandboxRequest>,
    ) -> impl IntoResponse {
        debug!("Creating sandbox with request: {:?}", request);
        
        // Convert E2B request to SoulBox config
        let config = match E2BAdapter::e2b_to_sandbox_config(&request) {
            Ok(cfg) => cfg,
            Err(e) => {
                error!("Failed to convert E2B request: {}", e);
                return (
                    StatusCode::BAD_REQUEST,
                    Json(E2BAdapter::create_error_response(&e)),
                ).into_response();
            }
        };
        
        // Get template if specified
        let template = if let Some(template_id) = &request.template_id {
            match E2BAdapter::parse_template_id(template_id) {
                Ok(runtime) => {
                    match state.template_manager.get_template_for_runtime(
                        runtime.as_str(),
                        None
                    ).await {
                        Ok(tmpl) => Some(tmpl),
                        Err(e) => {
                            error!("Failed to get template: {}", e);
                            return (
                                StatusCode::NOT_FOUND,
                                Json(E2BAdapter::create_error_response(&e)),
                            ).into_response();
                        }
                    }
                }
                Err(e) => {
                    error!("Invalid template ID: {}", e);
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(E2BAdapter::create_error_response(&e)),
                    ).into_response();
                }
            }
        } else {
            None
        };
        
        // Create sandbox
        let sandbox = match state.sandbox_manager.create_sandbox(config, template).await {
            Ok(sb) => sb,
            Err(e) => {
                error!("Failed to create sandbox: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(E2BAdapter::create_error_response(&e)),
                ).into_response();
            }
        };
        
        let response = E2BCreateSandboxResponse {
            id: sandbox.id().to_string(),
            url: None, // WebSocket URL would go here if implemented
            status: E2BAdapter::status_to_e2b(sandbox.status()),
            created_at: sandbox.created_at(),
        };
        
        info!("Created sandbox: {}", sandbox.id());
        (StatusCode::CREATED, Json(response)).into_response()
    }
    
    /// Get sandbox endpoint
    async fn get_sandbox(
        State(state): State<E2BApiState>,
        Path(id): Path<String>,
    ) -> impl IntoResponse {
        debug!("Getting sandbox: {}", id);
        
        let sandbox_id = match Uuid::parse_str(&id) {
            Ok(uuid) => uuid,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(E2BErrorResponse {
                        error: E2BErrorDetail::new("INVALID_ID", "Invalid sandbox ID format"),
                    }),
                ).into_response();
            }
        };
        
        match state.sandbox_manager.get_sandbox(sandbox_id).await {
            Ok(Some(sandbox)) => {
                let e2b_sandbox = E2BAdapter::sandbox_to_e2b(&sandbox);
                Json(e2b_sandbox).into_response()
            }
            Ok(None) => {
                (
                    StatusCode::NOT_FOUND,
                    Json(E2BErrorResponse {
                        error: E2BErrorDetail::new("NOT_FOUND", "Sandbox not found"),
                    }),
                ).into_response()
            }
            Err(e) => {
                error!("Failed to get sandbox: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(E2BAdapter::create_error_response(&e)),
                ).into_response()
            }
        }
    }
    
    /// Delete sandbox endpoint
    async fn delete_sandbox(
        State(state): State<E2BApiState>,
        Path(id): Path<String>,
    ) -> impl IntoResponse {
        debug!("Deleting sandbox: {}", id);
        
        let sandbox_id = match Uuid::parse_str(&id) {
            Ok(uuid) => uuid,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(E2BErrorResponse {
                        error: E2BErrorDetail::new("INVALID_ID", "Invalid sandbox ID format"),
                    }),
                ).into_response();
            }
        };
        
        match state.sandbox_manager.stop_sandbox(sandbox_id).await {
            Ok(()) => {
                info!("Deleted sandbox: {}", sandbox_id);
                StatusCode::NO_CONTENT.into_response()
            }
            Err(e) => {
                error!("Failed to delete sandbox: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(E2BAdapter::create_error_response(&e)),
                ).into_response()
            }
        }
    }
    
    /// Run process endpoint
    async fn run_process(
        State(state): State<E2BApiState>,
        Path(id): Path<String>,
        Json(request): Json<E2BRunProcessRequest>,
    ) -> impl IntoResponse {
        debug!("Running process in sandbox {}: {:?}", id, request);
        
        let sandbox_id = match Uuid::parse_str(&id) {
            Ok(uuid) => uuid,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(E2BErrorResponse {
                        error: E2BErrorDetail::new("INVALID_ID", "Invalid sandbox ID format"),
                    }),
                ).into_response();
            }
        };
        
        // Parse command and args
        let (command, args) = E2BAdapter::parse_e2b_command(&request);
        
        // Execute command
        let start = Instant::now();
        match state.sandbox_manager.execute_command(
            sandbox_id,
            &command,
            &args,
            request.env.clone(),
            request.cwd.clone(),
        ).await {
            Ok(output) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let response = E2BAdapter::format_process_output(
                    output.stdout,
                    output.stderr,
                    output.exit_code,
                    duration_ms,
                );
                Json(response).into_response()
            }
            Err(e) => {
                error!("Failed to run process: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(E2BAdapter::create_error_response(&e)),
                ).into_response()
            }
        }
    }
    
    /// Upload file endpoint
    async fn upload_file(
        State(state): State<E2BApiState>,
        Path(id): Path<String>,
        Json(request): Json<E2BUploadFileRequest>,
    ) -> impl IntoResponse {
        debug!("Uploading file to sandbox {}: {}", id, request.path);
        
        let sandbox_id = match Uuid::parse_str(&id) {
            Ok(uuid) => uuid,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(E2BErrorResponse {
                        error: E2BErrorDetail::new("INVALID_ID", "Invalid sandbox ID format"),
                    }),
                ).into_response();
            }
        };
        
        // Decode base64 content
        let content = match base64::decode(&request.content) {
            Ok(data) => data,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(E2BErrorResponse {
                        error: E2BErrorDetail::new("INVALID_CONTENT", &format!("Invalid base64 content: {}", e)),
                    }),
                ).into_response();
            }
        };
        
        // Write file
        match state.filesystem_manager.write_file(
            sandbox_id,
            &request.path,
            content,
        ).await {
            Ok(()) => {
                info!("Uploaded file to sandbox {}: {}", sandbox_id, request.path);
                StatusCode::NO_CONTENT.into_response()
            }
            Err(e) => {
                error!("Failed to upload file: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(E2BAdapter::create_error_response(&e)),
                ).into_response()
            }
        }
    }
    
    /// Download file endpoint
    async fn download_file(
        State(state): State<E2BApiState>,
        Path(id): Path<String>,
        Query(params): Query<FilePathQuery>,
    ) -> impl IntoResponse {
        debug!("Downloading file from sandbox {}: {}", id, params.path);
        
        let sandbox_id = match Uuid::parse_str(&id) {
            Ok(uuid) => uuid,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(E2BErrorResponse {
                        error: E2BErrorDetail::new("INVALID_ID", "Invalid sandbox ID format"),
                    }),
                ).into_response();
            }
        };
        
        // Read file
        match state.filesystem_manager.read_file(sandbox_id, &params.path).await {
            Ok(content) => {
                let response = E2BDownloadFileResponse {
                    content: base64::encode(&content),
                    size: content.len() as u64,
                    modified: chrono::Utc::now(), // TODO: Get actual modification time
                };
                Json(response).into_response()
            }
            Err(e) => {
                error!("Failed to download file: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(E2BAdapter::create_error_response(&e)),
                ).into_response()
            }
        }
    }
    
    /// List files endpoint
    async fn list_files(
        State(state): State<E2BApiState>,
        Path(id): Path<String>,
        Query(params): Query<FilePathQuery>,
    ) -> impl IntoResponse {
        debug!("Listing files in sandbox {}: {}", id, params.path);
        
        let sandbox_id = match Uuid::parse_str(&id) {
            Ok(uuid) => uuid,
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(E2BErrorResponse {
                        error: E2BErrorDetail::new("INVALID_ID", "Invalid sandbox ID format"),
                    }),
                ).into_response();
            }
        };
        
        // List files
        match state.filesystem_manager.list_files(sandbox_id, &params.path).await {
            Ok(files) => {
                let e2b_files: Vec<E2BFileInfo> = files.into_iter().map(|f| E2BFileInfo {
                    path: f.path,
                    name: f.name,
                    size: f.size,
                    is_dir: f.is_dir,
                    modified: f.modified,
                    permissions: f.permissions,
                }).collect();
                
                let response = E2BListFilesResponse {
                    total: e2b_files.len(),
                    files: e2b_files,
                };
                Json(response).into_response()
            }
            Err(e) => {
                error!("Failed to list files: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(E2BAdapter::create_error_response(&e)),
                ).into_response()
            }
        }
    }
    
    /// List templates endpoint
    async fn list_templates(
        State(state): State<E2BApiState>,
        Query(params): Query<PaginationQuery>,
    ) -> impl IntoResponse {
        debug!("Listing templates - page: {}, per_page: {}", params.page, params.per_page);
        
        let page = params.page.unwrap_or(1);
        let per_page = params.per_page.unwrap_or(20);
        
        // Get default templates
        let templates = state.template_manager.get_default_templates();
        let e2b_templates: Vec<E2BTemplate> = templates
            .values()
            .map(|t| E2BAdapter::template_to_e2b(t))
            .collect();
        
        let total = e2b_templates.len();
        
        // Apply pagination
        let start = ((page - 1) * per_page).min(total);
        let end = (start + per_page).min(total);
        let paginated = e2b_templates[start..end].to_vec();
        
        let response = E2BListTemplatesResponse {
            templates: paginated,
            total,
            page,
            per_page,
        };
        
        Json(response).into_response()
    }
}

#[derive(Deserialize)]
struct FilePathQuery {
    path: String,
}

#[derive(Deserialize)]
struct PaginationQuery {
    page: Option<usize>,
    per_page: Option<usize>,
}