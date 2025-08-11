use crate::config::Config;
use crate::error::Result as SoulBoxResult;
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    // Add other shared state here
}

pub struct Server {
    config: Config,
    app: Router,
}

impl Server {
    pub async fn new(config: Config) -> SoulBoxResult<Self> {
        let app_state = AppState {
            config: config.clone(),
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
    Router::new()
        // Health check
        .route("/health", get(health_check))
        .route("/ready", get(readiness_check))
        // API routes
        .route("/api/v1/sandboxes", post(create_sandbox))
        .route("/api/v1/sandboxes/:id", get(get_sandbox))
        // Add middleware
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
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    info!("Creating new sandbox with payload: {}", payload);
    
    // TODO: Implement sandbox creation logic
    
    Ok(Json(json!({
        "id": "sandbox_123",
        "status": "creating",
        "template": "python:3.11",
        "created_at": chrono::Utc::now().to_rfc3339()
    })))
}

async fn get_sandbox(
    State(_state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("Getting sandbox: {}", id);
    
    // TODO: Implement sandbox retrieval logic
    
    Ok(Json(json!({
        "id": id,
        "status": "running",
        "template": "python:3.11",
        "created_at": chrono::Utc::now().to_rfc3339(),
        "uptime": "00:05:32"
    })))
}