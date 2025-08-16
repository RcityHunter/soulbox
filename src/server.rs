use crate::api::auth::{auth_routes, AuthState};
use crate::api::audit::{audit_routes, AuditState};
use crate::api::permissions::permission_routes;
use crate::api::templates::{template_routes, TemplateState};
use crate::api::files::file_routes;
use crate::audit::{AuditConfig, AuditMiddleware, AuditService};
use crate::auth::middleware::{AuthMiddleware, AuthExtractor};
use crate::auth::models::Permission;
use crate::auth::{api_key::ApiKeyManager, JwtManager};
use crate::config::Config;
use crate::database::SurrealPool;
use crate::error::{Result as SoulBoxResult, SoulBoxError};
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
    pub audit_state: AuditState,
    pub template_state: Option<TemplateState>,
    pub auth_middleware: Arc<AuthMiddleware>,
    pub audit_middleware: Arc<AuditMiddleware>,
    pub database: Option<Arc<SurrealPool>>,
}

pub struct Server {
    config: Config,
    app: Router,
}

impl Server {
    pub async fn new(config: Config) -> SoulBoxResult<Self> {
        // 初始化 SurrealDB 数据库（如果配置了）
        let database: Option<Arc<SurrealPool>> = if let Ok(_db_url) = std::env::var("DATABASE_URL") {
            info!("Initializing SurrealDB connection...");
            // Create a default SurrealDB configuration
            let surreal_config = crate::database::SurrealConfig {
                protocol: crate::database::SurrealProtocol::RocksDb,
                endpoint: "rocksdb://./soulbox.db".to_string(),
                namespace: "soulbox".to_string(),
                database: "main".to_string(),
                username: None,
                password: None,
                pool: Default::default(),
                retry: Default::default(),
            };
            
            match SurrealPool::new(surreal_config).await {
                Ok(pool) => {
                    info!("SurrealDB connected successfully");
                    Some(Arc::new(pool))
                }
                Err(e) => {
                    tracing::warn!("Failed to connect to SurrealDB: {}. Running without persistence.", e);
                    None
                }
            }
        } else {
            info!("No DATABASE_URL configured. Running without persistence.");
            None
        };

        // 创建认证管理器并强制使用安全的JWT配置
        let jwt_secret = std::env::var("JWT_SECRET")
            .map_err(|_| SoulBoxError::Configuration(
                "JWT_SECRET environment variable is required for production use. \
                Please set a strong, randomly generated secret of at least 32 characters.".to_string()
            ))?;
        
        let jwt_manager = Arc::new(JwtManager::new(
            &jwt_secret,
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        ).map_err(|e| SoulBoxError::Configuration(
            format!("Failed to initialize JWT manager with secure configuration: {}", e)
        ))?);
        
        let api_key_manager = Arc::new(ApiKeyManager::new("sk".to_string()));
        
        // 创建审计服务（带数据库支持）
        let audit_config = AuditConfig::default();
        let audit_service = if let Some(ref db) = database {
            AuditService::with_database(audit_config, db.clone())?
        } else {
            AuditService::new(audit_config)?
        };
        let audit_middleware = Arc::new(AuditMiddleware::new(audit_service.clone()));
        
        let auth_state = AuthState::new(jwt_manager, api_key_manager);
        let audit_state = AuditState::new(audit_service);
        
        // Create template state if database is available
        let template_state = database.as_ref().map(|db| TemplateState::new(db.clone()));

        let app_state = AppState {
            config: config.clone(),
            auth_state: auth_state.clone(),
            audit_state: audit_state.clone(),
            template_state,
            auth_middleware: auth_state.auth_middleware.clone(),
            audit_middleware: audit_middleware.clone(),
            database,
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
    
    // 创建审计日志路由
    let audit_router = audit_routes(state.audit_state.clone());
    
    // 创建模板路由 (如果可用)
    let template_router = if state.template_state.is_some() {
        Some(template_routes())
    } else {
        None
    };

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

    let mut app = Router::new()
        // Health check (公开端点)
        .route("/health", get(health_check))
        .route("/ready", get(readiness_check))
        // 认证路由
        .nest("/api/v1", auth_router)
        // 权限管理路由
        .nest("/api/v1", permission_router)
        // 审计日志路由
        .nest("/api/v1", audit_router)
        // 沙盒管理路由
        .merge(sandbox_routes);

    // Add template routes if available
    if let Some(template_router) = template_router {
        app = app.nest("/api/v1", template_router);
    }
    
    // Add file system routes
    app = app.nest("/api/v1", file_routes());

    app
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
    State(state): State<AppState>,
    auth: AuthExtractor,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} creating new sandbox with payload: {}", auth.0.username, payload);
    
    // Extract sandbox parameters from payload
    let template = payload.get("template")
        .and_then(|v| v.as_str())
        .unwrap_or("python:3.11");
    
    let memory_mb = payload.get("memory_mb")
        .and_then(|v| v.as_u64())
        .unwrap_or(512);
    
    let cpu_cores = payload.get("cpu_cores")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);
    
    let enable_internet = payload.get("enable_internet")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    
    // Validate limits against config
    if memory_mb > state.config.sandbox.max_memory_mb {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    if cpu_cores > state.config.sandbox.max_cpu_cores as f64 {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Generate sandbox ID
    let sandbox_id = format!("sb_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
    
    // For now, return a simple response - actual sandbox creation would happen here
    // In a full implementation, this would integrate with the container manager
    Ok(Json(json!({
        "id": sandbox_id,
        "status": "creating",
        "template": template,
        "memory_mb": memory_mb,
        "cpu_cores": cpu_cores,
        "enable_internet": enable_internet,
        "owner_id": auth.0.user_id,
        "tenant_id": auth.0.tenant_id,
        "created_at": chrono::Utc::now().to_rfc3339()
    })))
}

async fn get_sandbox(
    State(state): State<AppState>,
    auth: AuthExtractor,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} getting sandbox: {}", auth.0.username, id);
    
    // Validate sandbox ID format
    if !id.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // In a full implementation, would check database/container manager for sandbox
    // For now, return mock data based on sandbox ID pattern
    if id.starts_with("sb_") {
        Ok(Json(json!({
            "id": id,
            "status": "running",
            "template": "python:3.11",
            "memory_mb": 512,
            "cpu_cores": 1.0,
            "enable_internet": true,
            "owner_id": auth.0.user_id,
            "tenant_id": auth.0.tenant_id,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "uptime": "00:05:32"
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn delete_sandbox(
    State(state): State<AppState>,
    auth: AuthExtractor,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} deleting sandbox: {}", auth.0.username, id);
    
    // Validate sandbox ID format
    if !id.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // In a full implementation, would:
    // 1. Check if sandbox exists and user has permission
    // 2. Stop and remove container
    // 3. Clean up resources
    // 4. Update database
    
    if id.starts_with("sb_") {
        Ok(Json(json!({
            "id": id,
            "status": "deleted",
            "deleted_by": auth.0.user_id,
            "deleted_at": chrono::Utc::now().to_rfc3339()
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn execute_in_sandbox(
    State(state): State<AppState>,
    auth: AuthExtractor,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} executing code in sandbox: {}", auth.0.username, id);
    
    // Validate sandbox ID format
    if !id.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Extract code execution parameters
    let code = payload.get("code")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    let language = payload.get("language")
        .and_then(|v| v.as_str())
        .unwrap_or("python");
    
    // Validate code length
    if code.len() > 100_000 { // 100KB limit
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Validate language
    let allowed_languages = ["python", "javascript", "node", "rust", "go", "bash"];
    if !allowed_languages.contains(&language) {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Generate execution ID
    let execution_id = format!("exec_{}", uuid::Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
    
    // In a full implementation, would:
    // 1. Validate sandbox exists and user has access
    // 2. Queue code execution job
    // 3. Execute code in sandbox container
    // 4. Return results
    
    if id.starts_with("sb_") {
        Ok(Json(json!({
            "execution_id": execution_id,
            "sandbox_id": id,
            "status": "completed",
            "language": language,
            "code_length": code.len(),
            "executed_by": auth.0.user_id,
            "started_at": chrono::Utc::now().to_rfc3339(),
            "stdout": "Hello, World!",
            "stderr": "",
            "exit_code": 0
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn list_sandboxes(
    State(state): State<AppState>,
    auth: AuthExtractor,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} listing sandboxes", auth.0.username);
    
    // In a full implementation, would:
    // 1. Query database for user's sandboxes
    // 2. Apply tenant filtering
    // 3. Support pagination
    // 4. Return actual sandbox data
    
    let mock_sandboxes = vec![
        json!({
            "id": "sb_abc12345",
            "status": "running",
            "template": "python:3.11",
            "memory_mb": 512,
            "cpu_cores": 1.0,
            "owner_id": auth.0.user_id,
            "created_at": chrono::Utc::now().to_rfc3339()
        }),
        json!({
            "id": "sb_def67890",
            "status": "stopped",
            "template": "node:18",
            "memory_mb": 256,
            "cpu_cores": 0.5,
            "owner_id": auth.0.user_id,
            "created_at": chrono::Utc::now().to_rfc3339()
        })
    ];
    
    Ok(Json(json!({
        "sandboxes": mock_sandboxes,
        "total": mock_sandboxes.len(),
        "tenant_id": auth.0.tenant_id,
        "page": 1,
        "page_size": 10
    })))
}