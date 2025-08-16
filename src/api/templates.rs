use crate::auth::middleware::AuthExtractor;
use crate::auth::models::Permission;
use crate::template::{
    TemplateManager, TemplateError,
    models::{CreateTemplateRequest, UpdateTemplateRequest},
};
use crate::database::SurrealPool;
// TODO: Re-enable when TemplateRepository is refactored
// use crate::database::TemplateRepository;
use crate::error::SoulBoxError;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put, delete},
    Router,
};
use serde::{Deserialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Clone)]
pub struct TemplateState {
    pub template_manager: Arc<TemplateManager>,
}

impl TemplateState {
    pub fn new(database: Arc<SurrealPool>) -> Self {
        // TODO: Re-enable when TemplateRepository is refactored
        // let template_repository = Arc::new(TemplateRepository::new(database));
        // let template_manager = Arc::new(TemplateManager::new(template_repository));
        // For now, create a dummy template manager
        let template_manager = Arc::new(TemplateManager::new_without_database());
        Self { template_manager }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListTemplatesQuery {
    pub runtime_type: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct CloneTemplateRequest {
    pub name: String,
    pub slug: String,
}

/// Helper function to map template errors to HTTP status codes
fn map_template_error_to_status(error: &SoulBoxError) -> StatusCode {
    let error_str = error.to_string().to_lowercase();
    
    if error_str.contains("not found") {
        StatusCode::NOT_FOUND
    } else if error_str.contains("conflict") || error_str.contains("already exists") {
        StatusCode::CONFLICT
    } else if error_str.contains("validation") || error_str.contains("invalid") {
        StatusCode::BAD_REQUEST
    } else if error_str.contains("permission") || error_str.contains("forbidden") || error_str.contains("denied") {
        StatusCode::FORBIDDEN
    } else if error_str.contains("security") {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

/// Create template routes
pub fn template_routes() -> Router<crate::server::AppState> {
    Router::new()
        // Public routes
        .route("/templates/public", get(list_public_templates))
        .route("/templates/:id_or_slug", get(get_template))
        
        // Authenticated routes
        .route("/templates", post(create_template))
        .route("/templates/my", get(list_my_templates))
        .route("/templates/:id", put(update_template))
        .route("/templates/:id", delete(delete_template))
        .route("/templates/:id/clone", post(clone_template))
        .route("/templates/:id/validate", post(validate_template))
}

/// List public templates
async fn list_public_templates(
    State(state): State<crate::server::AppState>,
    Query(params): Query<ListTemplatesQuery>,
) -> Result<Json<Value>, StatusCode> {
    let template_state = match &state.template_state {
        Some(ts) => ts,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };

    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20).min(100);
    let runtime_type = params.runtime_type.as_deref();

    match template_state.template_manager.list_public_templates(runtime_type, page, page_size).await {
        Ok(templates) => {
            let response = json!({
                "templates": templates,
                "page": page,
                "page_size": page_size,
                "runtime_type": runtime_type
            });
            Ok(Json(response))
        }
        Err(e) => {
            warn!("Failed to list public templates: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get template by ID or slug
async fn get_template(
    State(state): State<crate::server::AppState>,
    Path(id_or_slug): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let template_state = match &state.template_state {
        Some(ts) => ts,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };

    // Try to parse as UUID first
    let template = if let Ok(uuid) = Uuid::parse_str(&id_or_slug) {
        template_state.template_manager.get_template(uuid).await
    } else {
        template_state.template_manager.get_template_by_slug(&id_or_slug).await
    };

    match template {
        Ok(Some(template)) => Ok(Json(serde_json::to_value(template).unwrap())),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            warn!("Failed to get template {}: {}", id_or_slug, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Create new template
async fn create_template(
    State(state): State<crate::server::AppState>,
    auth: AuthExtractor,
    Json(request): Json<CreateTemplateRequest>,
) -> Result<Json<Value>, StatusCode> {
    let template_state = match &state.template_state {
        Some(ts) => ts,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };

    // Check permission
    if !auth.0.has_permission(&Permission::TemplateCreate) {
        return Err(StatusCode::FORBIDDEN);
    }

    info!("User {} creating template: {}", auth.0.username, request.name);

    match template_state.template_manager.create_template(
        request,
        Some(auth.0.user_id),
        auth.0.tenant_id,
    ).await {
        Ok(template) => {
            let response = json!({
                "template": template,
                "message": "Template created successfully"
            });
            Ok(Json(response))
        }
        Err(e) => {
            warn!("Failed to create template: {}", e);
            Err(map_template_error_to_status(&e))
        }
    }
}

/// List user's templates
async fn list_my_templates(
    State(state): State<crate::server::AppState>,
    auth: AuthExtractor,
    Query(params): Query<ListTemplatesQuery>,
) -> Result<Json<Value>, StatusCode> {
    let template_state = match &state.template_state {
        Some(ts) => ts,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };

    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(20).min(100);

    match template_state.template_manager.list_user_templates(auth.0.user_id, page, page_size).await {
        Ok(templates) => {
            let response = json!({
                "templates": templates,
                "page": page,
                "page_size": page_size,
                "user_id": auth.0.user_id
            });
            Ok(Json(response))
        }
        Err(e) => {
            warn!("Failed to list user templates: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Update template
async fn update_template(
    State(state): State<crate::server::AppState>,
    auth: AuthExtractor,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateTemplateRequest>,
) -> Result<Json<Value>, StatusCode> {
    let template_state = match &state.template_state {
        Some(ts) => ts,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };

    // Check permission
    if !auth.0.has_permission(&Permission::TemplateUpdate) {
        return Err(StatusCode::FORBIDDEN);
    }

    info!("User {} updating template: {}", auth.0.username, id);

    match template_state.template_manager.update_template(id, request, auth.0.user_id).await {
        Ok(template) => {
            let response = json!({
                "template": template,
                "message": "Template updated successfully"
            });
            Ok(Json(response))
        }
        Err(e) => {
            warn!("Failed to update template {}: {}", id, e);
            Err(map_template_error_to_status(&e))
        }
    }
}

/// Delete template
async fn delete_template(
    State(state): State<crate::server::AppState>,
    auth: AuthExtractor,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let template_state = match &state.template_state {
        Some(ts) => ts,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };

    // Check permission
    if !auth.0.has_permission(&Permission::TemplateDelete) {
        return Err(StatusCode::FORBIDDEN);
    }

    info!("User {} deleting template: {}", auth.0.username, id);

    match template_state.template_manager.delete_template(id, auth.0.user_id).await {
        Ok(_) => {
            let response = json!({
                "message": "Template deleted successfully",
                "deleted_id": id
            });
            Ok(Json(response))
        }
        Err(e) => {
            warn!("Failed to delete template {}: {}", id, e);
            Err(map_template_error_to_status(&e))
        }
    }
}

/// Clone template
async fn clone_template(
    State(state): State<crate::server::AppState>,
    auth: AuthExtractor,
    Path(id): Path<Uuid>,
    Json(request): Json<CloneTemplateRequest>,
) -> Result<Json<Value>, StatusCode> {
    let template_state = match &state.template_state {
        Some(ts) => ts,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };

    // Check permission
    if !auth.0.has_permission(&Permission::TemplateCreate) {
        return Err(StatusCode::FORBIDDEN);
    }

    info!("User {} cloning template: {} -> {}", auth.0.username, id, request.name);

    match template_state.template_manager.clone_template(
        id,
        request.name,
        request.slug,
        auth.0.user_id,
        auth.0.tenant_id,
    ).await {
        Ok(template) => {
            let response = json!({
                "template": template,
                "message": "Template cloned successfully"
            });
            Ok(Json(response))
        }
        Err(e) => {
            warn!("Failed to clone template {}: {}", id, e);
            Err(map_template_error_to_status(&e))
        }
    }
}

/// Validate template
async fn validate_template(
    State(state): State<crate::server::AppState>,
    auth: AuthExtractor,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    let template_state = match &state.template_state {
        Some(ts) => ts,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };

    info!("User {} validating template: {}", auth.0.username, id);

    match template_state.template_manager.get_template(id).await {
        Ok(Some(template)) => {
            let validation_result = template.validate();
            let response = json!({
                "template_id": id,
                "validation": validation_result,
                "is_valid": validation_result.is_valid
            });
            Ok(Json(response))
        }
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            warn!("Failed to validate template {}: {}", id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}