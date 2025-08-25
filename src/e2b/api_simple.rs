//! Simplified E2B API Service - Basic REST API endpoints compatible with E2B

use crate::e2b::{models::*, adapter::E2BAdapter};
use crate::template::TemplateManager;
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
use std::collections::HashMap;
use uuid::Uuid;
use tracing::{info, debug};

/// Simplified E2B API service state
#[derive(Clone)]
pub struct E2BApiState {
    template_manager: Arc<TemplateManager>,
    start_time: Instant,
    // Store sandbox info in memory for demo purposes
    sandboxes: Arc<tokio::sync::RwLock<HashMap<String, E2BSandbox>>>,
}

/// Simplified E2B API service
pub struct E2BApiService {
    state: E2BApiState,
}

impl E2BApiService {
    /// Create new E2B API service
    pub fn new(template_manager: Arc<TemplateManager>) -> Self {
        Self {
            state: E2BApiState {
                template_manager,
                start_time: Instant::now(),
                sandboxes: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
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
            // Template endpoints
            .route("/templates", get(Self::list_templates))
            .with_state(self.state)
    }
    
    /// Health check endpoint
    async fn health(State(state): State<E2BApiState>) -> impl IntoResponse {
        let uptime = state.start_time.elapsed().as_secs();
        let sandboxes_count = state.sandboxes.read().await.len();
        
        Json(E2BHealthResponse {
            status: "healthy".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime,
            sandboxes_count,
        })
    }
    
    /// Create sandbox endpoint (simplified - just creates a mock sandbox)
    async fn create_sandbox(
        State(state): State<E2BApiState>,
        Json(request): Json<E2BCreateSandboxRequest>,
    ) -> impl IntoResponse {
        debug!("Creating sandbox with request: {:?}", request);
        
        let sandbox_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        
        // Create mock sandbox
        let sandbox = E2BSandbox {
            id: sandbox_id.clone(),
            template_id: request.template_id.clone(),
            status: E2BSandboxStatus::Running,
            created_at: now,
            metadata: request.metadata.unwrap_or_default(),
            resources: request.resources.unwrap_or(E2BResources {
                cpu: 1.0,
                memory_mb: 512,
                disk_mb: 1024,
                network_enabled: true,
            }),
            network: E2BNetwork {
                ports: Vec::new(),
                dns: vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
                proxy: None,
            },
        };
        
        // Store sandbox
        state.sandboxes.write().await.insert(sandbox_id.clone(), sandbox.clone());
        
        let response = E2BCreateSandboxResponse {
            id: sandbox_id,
            url: None,
            status: E2BSandboxStatus::Running,
            created_at: now,
        };
        
        info!("Created mock sandbox: {}", response.id);
        (StatusCode::CREATED, Json(response)).into_response()
    }
    
    /// Get sandbox endpoint
    async fn get_sandbox(
        State(state): State<E2BApiState>,
        Path(id): Path<String>,
    ) -> impl IntoResponse {
        debug!("Getting sandbox: {}", id);
        
        let sandboxes = state.sandboxes.read().await;
        match sandboxes.get(&id) {
            Some(sandbox) => {
                Json(sandbox.clone()).into_response()
            }
            None => {
                (
                    StatusCode::NOT_FOUND,
                    Json(E2BErrorResponse {
                        error: E2BErrorDetail::new("NOT_FOUND", "Sandbox not found"),
                    }),
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
        
        let removed = state.sandboxes.write().await.remove(&id);
        
        if removed.is_some() {
            info!("Deleted sandbox: {}", id);
            StatusCode::NO_CONTENT.into_response()
        } else {
            (
                StatusCode::NOT_FOUND,
                Json(E2BErrorResponse {
                    error: E2BErrorDetail::new("NOT_FOUND", "Sandbox not found"),
                }),
            ).into_response()
        }
    }
    
    /// List templates endpoint
    async fn list_templates(
        State(state): State<E2BApiState>,
        Query(params): Query<PaginationQuery>,
    ) -> impl IntoResponse {
        debug!("Listing templates - page: {:?}, per_page: {:?}", params.page, params.per_page);
        
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
        let paginated = if start < total {
            e2b_templates[start..end].to_vec()
        } else {
            Vec::new()
        };
        
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
struct PaginationQuery {
    page: Option<usize>,
    per_page: Option<usize>,
}