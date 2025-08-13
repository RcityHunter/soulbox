use crate::api::auth::{auth_routes, AuthState};
use crate::api::permissions::permission_routes;
use crate::auth::middleware::{AuthMiddleware, AuthExtractor};
use crate::auth::models::Permission;
use crate::auth::{api_key::ApiKeyManager, JwtManager};
use crate::config::Config;
use crate::error::Result as SoulBoxResult;
use axum::{
    extract::State,
    http::StatusCode,
    middleware,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub auth_state: AuthState,
    pub auth_middleware: Arc<AuthMiddleware>,
}

pub struct Server {
    config: Config,
    app: Router,
}

impl Server {
    pub async fn new(config: Config) -> SoulBoxResult<Self> {
        // 创建认证管理器
        let jwt_secret = std::env::var("JWT_SECRET")
            .unwrap_or_else(|_| "demo-jwt-secret-change-in-production".to_string());
        
        let jwt_manager = Arc::new(JwtManager::new(
            &jwt_secret,
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        ));
        
        let api_key_manager = Arc::new(ApiKeyManager::new("sk".to_string()));
        
        let auth_state = AuthState::new(jwt_manager, api_key_manager);

        let app_state = AppState {
            config: config.clone(),
            auth_state: auth_state.clone(),
            auth_middleware: auth_state.auth_middleware.clone(),
        };

        let app = create_app(app_state);

        Ok(Self { config, app })
    }

    pub async fn run(self) -> SoulBoxResult<()> {
        let addr = format!("{}:{}", self.config.server.host, self.config.server.port);
        
        info!("🚀 SoulBox server starting on {}", addr);
        
        let listener = TcpListener::bind(&addr).await?;
        
        axum::serve(listener, self.app).await?;
        
        Ok(())
    }
}

fn create_app(state: AppState) -> Router {
    // 创建认证路由
    let auth_router = auth_routes(state.auth_state.clone());
    
    // 创建权限管理路由
    let permission_router = permission_routes(state.auth_state.clone());

    // 创建需要认证的路由 - 沙盒管理
    let sandbox_routes = Router::new()
        .route("/api/v1/sandboxes", post(create_sandbox))
        .route_layer(middleware::from_fn(AuthMiddleware::require_permission(
            Permission::SandboxCreate,
        )))
        .route("/api/v1/sandboxes/:id", get(get_sandbox))
        .route_layer(middleware::from_fn(AuthMiddleware::require_permission(
            Permission::SandboxRead,
        )))
        .route("/api/v1/sandboxes/:id", axum::routing::delete(delete_sandbox))
        .route_layer(middleware::from_fn(AuthMiddleware::require_permission(
            Permission::SandboxDelete,
        )))
        .route("/api/v1/sandboxes/:id/execute", post(execute_in_sandbox))
        .route_layer(middleware::from_fn(AuthMiddleware::require_permission(
            Permission::SandboxExecute,
        )))
        .route("/api/v1/sandboxes", get(list_sandboxes))
        .route_layer(middleware::from_fn(AuthMiddleware::require_permission(
            Permission::SandboxList,
        )))
        .layer(middleware::from_fn_with_state(
            state.auth_middleware.clone(),
            AuthMiddleware::jwt_auth,
        ));

    Router::new()
        // Health check (公开端点)
        .route("/health", get(health_check))
        .route("/ready", get(readiness_check))
        // 认证路由
        .nest("/api/v1", auth_router)
        // 权限管理路由
        .nest("/api/v1", permission_router)
        // 沙盒管理路由
        .merge(sandbox_routes)
        // 全局中间件
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive())
        )
        .with_state(state)
}

// Health check endpoint
async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "healthy",
        "service": "soulbox",
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

// Readiness check endpoint
async fn readiness_check(State(_state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    // TODO: Check database connectivity, dependencies, etc.
    
    Ok(Json(json!({
        "status": "ready",
        "service": "soulbox",
        "checks": {
            "database": "ok",
            "redis": "ok",
            "sandbox_manager": "ok"
        },
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

// Sandbox management endpoints
async fn create_sandbox(
    State(_state): State<AppState>,
    auth: AuthExtractor,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} creating new sandbox with payload: {}", auth.0.username, payload);
    
    // TODO: Implement sandbox creation logic
    
    Ok(Json(json!({
        "id": "sandbox_123",
        "status": "creating",
        "template": "python:3.11",
        "owner_id": auth.0.user_id,
        "tenant_id": auth.0.tenant_id,
        "created_at": chrono::Utc::now().to_rfc3339()
    })))
}

async fn get_sandbox(
    State(_state): State<AppState>,
    auth: AuthExtractor,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} getting sandbox: {}", auth.0.username, id);
    
    // TODO: Implement sandbox retrieval logic with tenant isolation
    
    Ok(Json(json!({
        "id": id,
        "status": "running",
        "template": "python:3.11",
        "owner_id": auth.0.user_id,
        "tenant_id": auth.0.tenant_id,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "uptime": "00:05:32"
    })))
}

async fn delete_sandbox(
    State(_state): State<AppState>,
    auth: AuthExtractor,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} deleting sandbox: {}", auth.0.username, id);
    
    // TODO: Implement sandbox deletion logic with ownership check
    
    Ok(Json(json!({
        "id": id,
        "status": "deleted",
        "deleted_by": auth.0.user_id,
        "deleted_at": chrono::Utc::now().to_rfc3339()
    })))
}

async fn execute_in_sandbox(
    State(_state): State<AppState>,
    auth: AuthExtractor,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} executing code in sandbox: {}", auth.0.username, id);
    
    // TODO: Implement code execution logic
    
    Ok(Json(json!({
        "execution_id": "exec_456",
        "sandbox_id": id,
        "status": "running",
        "executed_by": auth.0.user_id,
        "started_at": chrono::Utc::now().to_rfc3339()
    })))
}

async fn list_sandboxes(
    State(_state): State<AppState>,
    auth: AuthExtractor,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} listing sandboxes", auth.0.username);
    
    // TODO: Implement sandbox listing with tenant filtering
    
    Ok(Json(json!({
        "sandboxes": [
            {
                "id": "sandbox_123",
                "status": "running",
                "template": "python:3.11",
                "owner_id": auth.0.user_id,
                "created_at": chrono::Utc::now().to_rfc3339()
            }
        ],
        "total": 1,
        "tenant_id": auth.0.tenant_id
    })))
}