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
use crate::container::{ContainerManager, ResourceLimits, NetworkConfig};
use crate::container::runtime::ContainerRuntime;
use crate::container::resource_limits::{MemoryLimits, CpuLimits, DiskLimits, NetworkLimits};
use crate::database::{SurrealPool, repositories::SandboxRepository};
use crate::database::models::DbSandbox;
use crate::error::{Result as SoulBoxResult, SoulBoxError};
use crate::filesystem::FileSystemManager;
use crate::validation::InputValidator;
use tracing::{info, error, warn, debug};
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

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub auth_state: AuthState,
    pub audit_state: AuditState,
    pub template_state: Option<TemplateState>,
    pub auth_middleware: Arc<AuthMiddleware>,
    pub audit_middleware: Arc<AuditMiddleware>,
    pub database: Option<Arc<SurrealPool>>,
    pub container_manager: Arc<ContainerManager>,
    pub container_runtime: Arc<ContainerRuntime>,
    pub sandbox_repository: Option<Arc<SandboxRepository>>,
    pub filesystem_manager: Arc<FileSystemManager>,
    pub sandbox_manager: Arc<crate::SandboxManager>,
}

pub struct Server {
    config: Config,
    app: Router,
}

impl Server {
    pub async fn new(config: Config) -> SoulBoxResult<Self> {
        // 初始化 SurrealDB 数据库（如果配置了） - Disabled for MVP
        let database: Option<Arc<SurrealPool>> = if std::env::var("ENABLE_DATABASE").is_ok() && std::env::var("DATABASE_URL").is_ok() {
            info!("Initializing SurrealDB connection...");
            // Create a default SurrealDB configuration - use Memory protocol for MVP
            let surreal_config = crate::database::SurrealConfig {
                protocol: crate::database::SurrealProtocol::Memory,
                endpoint: "mem://".to_string(),
                namespace: "soulbox".to_string(),
                database: "main".to_string(),
                username: None,
                password: None,
                pool: Default::default(),
                retry: Default::default(),
                reset_on_startup: false,
                run_initialization: true,
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
            .map_err(|_| SoulBoxError::Configuration {
                message: "JWT_SECRET environment variable is required for production use. Please set a strong, randomly generated secret of at least 32 characters.".to_string(),
                parameter: "JWT_SECRET".to_string(),
                reason: "environment variable is required for production use. Please set a strong, randomly generated secret of at least 32 characters.".to_string()
            })?;
        
        let jwt_manager = Arc::new(JwtManager::new(
            &jwt_secret,
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        ).map_err(|e| SoulBoxError::Configuration {
            message: format!("Failed to initialize JWT manager with secure configuration: {}", e),
            parameter: "JWT_MANAGER".to_string(),
            reason: format!("Failed to initialize JWT manager with secure configuration: {}", e)
        })?);
        
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
        
        // Initialize container manager
        let container_manager = if std::env::var("DOCKER_HOST").is_ok() || std::env::var("DOCKER_SOCK").is_ok() {
            info!("Initializing real Docker container manager...");
            match ContainerManager::new(config.clone()).await {
                Ok(cm) => {
                    info!("Container manager initialized successfully");
                    Arc::new(cm)
                }
                Err(e) => {
                    warn!("Failed to initialize Docker container manager: {}. Using stub implementation.", e);
                    Arc::new(ContainerManager::new_default()?)
                }
            }
        } else {
            info!("No Docker environment detected. Using stub container manager.");
            Arc::new(ContainerManager::new_default()?)
        };
        
        // Initialize sandbox repository if database is available
        let sandbox_repository = database.as_ref().map(|db| Arc::new(SandboxRepository::new(db.clone())));

        // Create filesystem manager
        let filesystem_manager = Arc::new(FileSystemManager::new().await.map_err(|e| {
            error!("Failed to create filesystem manager: {}", e);
            SoulBoxError::Internal(format!("Failed to create filesystem manager: {}", e))
        })?);

        // Initialize container runtime
        let container_runtime = Arc::new(ContainerRuntime::new().await.map_err(|e| {
            error!("Failed to create container runtime: {}", e);
            SoulBoxError::Internal(format!("Failed to create container runtime: {}", e))
        })?);

        // Initialize sandbox manager
        let sandbox_manager = Arc::new(crate::SandboxManager::new(container_manager.clone()));

        let app_state = AppState {
            config: config.clone(),
            auth_state: auth_state.clone(),
            audit_state: audit_state.clone(),
            template_state,
            auth_middleware: auth_state.auth_middleware.clone(),
            audit_middleware: audit_middleware.clone(),
            database,
            container_manager,
            container_runtime,
            sandbox_repository,
            filesystem_manager,
            sandbox_manager,
        };

        let app = create_app(app_state);

        Ok(Self { config, app })
    }

    pub async fn run(self) -> SoulBoxResult<()> {
        let addr = format!("{}:{}", self.config.server.host, self.config.server.port);
        
        info!("🚀 SoulBox server starting on {}", addr);
        
        let listener = TcpListener::bind(&addr).await?;
        
        // Set up graceful shutdown
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        
        // Handle shutdown signals
        tokio::spawn(async move {
            let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Failed to create SIGTERM handler");
            let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .expect("Failed to create SIGINT handler");
                
            tokio::select! {
                _ = sigterm.recv() => info!("Received SIGTERM, initiating graceful shutdown"),
                _ = sigint.recv() => info!("Received SIGINT, initiating graceful shutdown"),
            }
            
            let _ = shutdown_tx.send(());
        });
        
        // Run server with graceful shutdown
        let server = axum::serve(listener, self.app)
            .with_graceful_shutdown(async {
                shutdown_rx.await.ok();
                info!("Shutdown signal received, stopping server...");
            });
            
        if let Err(e) = server.await {
            error!("Server error: {}", e);
            return Err(e.into());
        }
        
        info!("Server shutdown completed gracefully");
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

    // 创建需要认证的路由 - 沙盒管理 (Simplified for MVP)
    let sandbox_routes = Router::new()
        .route("/api/v1/sandboxes", post(create_sandbox))
        .route("/api/v1/sandboxes/{id}", get(get_sandbox))
        .route("/api/v1/sandboxes/{id}", axum::routing::delete(delete_sandbox))
        .route("/api/v1/sandboxes/{id}/execute", post(execute_in_sandbox))
        .route("/api/v1/sandboxes", get(list_sandboxes))
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
async fn readiness_check(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let mut checks = std::collections::HashMap::new();
    let mut all_healthy = true;
    
    // Check database connectivity
    let db_status = match &state.database {
        Some(db) => {
            match check_database_health(db).await {
                Ok(_) => "ok",
                Err(e) => {
                    warn!("Database health check failed: {}", e);
                    all_healthy = false;
                    "error"
                }
            }
        },
        None => {
            debug!("Database not configured");
            "not_configured"
        }
    };
    checks.insert("database", db_status);
    
    // Check sandbox manager
    let sandbox_status = if state.sandbox_manager.is_available() {
        "ok"
    } else {
        all_healthy = false;
        "error"
    };
    checks.insert("sandbox_manager", sandbox_status);
    
    // Check Docker connectivity (via container runtime)
    let docker_status = match check_docker_health(&state.container_runtime).await {
        Ok(_) => "ok",
        Err(e) => {
            warn!("Docker health check failed: {}", e);
            all_healthy = false;
            "error"
        }
    };
    checks.insert("docker", docker_status);
    
    // File system manager is always healthy if created
    checks.insert("filesystem", "ok");
    
    let status_code = if all_healthy { 
        StatusCode::OK 
    } else { 
        StatusCode::SERVICE_UNAVAILABLE 
    };
    
    let response = Json(json!({
        "status": if all_healthy { "ready" } else { "not_ready" },
        "service": "soulbox",
        "checks": checks,
        "timestamp": chrono::Utc::now().to_rfc3339()
    }));
    
    if all_healthy {
        Ok(response)
    } else {
        Err(status_code)
    }
}

// Health check helper functions
async fn check_database_health(db: &SurrealPool) -> SoulBoxResult<()> {
    // Simple connectivity check by getting a connection
    match db.get_connection().await {
        Ok(_) => {
            debug!("Database health check passed");
            Ok(())
        },
        Err(e) => {
            error!("Database health check failed: {}", e);
            Err(SoulBoxError::Database(format!("Health check failed: {}", e)))
        }
    }
}

async fn check_docker_health(runtime: &ContainerRuntime) -> SoulBoxResult<()> {
    match runtime.get_docker_version().await {
        Ok(version) => {
            debug!("Docker health check passed, version: {}", version);
            Ok(())
        },
        Err(e) => {
            error!("Docker health check failed: {}", e);
            Err(e)
        }
    }
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
    
    // Map template to docker image
    let docker_image = match template {
        "python:3.11" | "python" => "python:3.11-slim",
        "node:18" | "node" | "javascript" => "node:18-alpine", 
        "rust" => "rust:1.70-slim",
        "go" => "golang:1.20-alpine",
        _ => "ubuntu:22.04", // Default fallback
    };
    
    // Create resource limits
    let resource_limits = ResourceLimits {
        memory: MemoryLimits {
            limit_mb: memory_mb,
            swap_mb: Some(memory_mb / 2),
            swap_limit_mb: Some(memory_mb),
        },
        cpu: CpuLimits {
            cores: cpu_cores,
            cpu_percent: Some(80.0),
            shares: Some(1024),
        },
        disk: DiskLimits {
            limit_mb: 2048,
            iops_limit: Some(1000),
        },
        network: NetworkLimits {
            upload_bps: Some(1024 * 1024), // 1 MB/s
            download_bps: Some(10 * 1024 * 1024), // 10 MB/s
            max_connections: Some(100),
        },
    };
    
    // Create network config
    let network_config = NetworkConfig {
        enable_internet,
        port_mappings: vec![], // No port mappings by default
        allowed_domains: vec![], // No domain restrictions by default
        dns_servers: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
    };
    
    // Create environment variables
    let env_vars = std::collections::HashMap::new(); // Default empty
    
    // Create container using container manager
    match state.container_manager.create_sandbox_container(
        &sandbox_id,
        docker_image,
        resource_limits.clone(),
        network_config.clone(),
        env_vars.clone()
    ).await {
        Ok(container) => {
            // Start the container
            match container.start().await {
                Ok(_) => {
                    info!("Container started successfully for sandbox {}", sandbox_id);
                    
                    // Save to database if available
                    if let Some(ref repo) = state.sandbox_repository {
                        let db_sandbox = DbSandbox {
                            id: auth.0.user_id,
                            name: format!("Sandbox {}", &sandbox_id[3..]),
                            runtime_type: template.to_string(),
                            template: template.to_string(),
                            status: "running".to_string(),
                            owner_id: auth.0.user_id,
                            tenant_id: auth.0.tenant_id,
                            cpu_limit: Some(cpu_cores as i64),
                            memory_limit: Some(memory_mb as i64),
                            disk_limit: None,
                            container_id: Some(container.get_container_id().to_string()),
                            vm_id: None,
                            ip_address: None,
                            port_mappings: None,
                            created_at: chrono::Utc::now(),
                            updated_at: chrono::Utc::now(),
                            started_at: Some(chrono::Utc::now()),
                            stopped_at: None,
                            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(2)), // 2 hour expiry
                        };
                        
                        if let Err(e) = repo.create(&db_sandbox).await {
                            warn!("Failed to save sandbox to database: {}", e);
                        }
                    }
                    
                    Ok(Json(json!({
                        "id": sandbox_id,
                        "status": "running",
                        "template": template,
                        "memory_mb": memory_mb,
                        "cpu_cores": cpu_cores,
                        "enable_internet": enable_internet,
                        "owner_id": auth.0.user_id,
                        "tenant_id": auth.0.tenant_id,
                        "container_id": container.get_container_id(),
                        "created_at": chrono::Utc::now().to_rfc3339()
                    })))
                },
                Err(e) => {
                    error!("Failed to start container for sandbox {}: {}", sandbox_id, e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        },
        Err(e) => {
            error!("Failed to create container for sandbox {}: {}", sandbox_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_sandbox(
    State(state): State<AppState>,
    auth: AuthExtractor,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} getting sandbox: {}", auth.0.username, id);
    
    // Validate sandbox ID format
    let validated_id = InputValidator::validate_sandbox_id(&id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    // In a full implementation, would check database/container manager for sandbox
    // For now, return mock data based on sandbox ID pattern
    if validated_id.starts_with("sb_") {
        Ok(Json(json!({
            "id": validated_id,
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
    let validated_id = InputValidator::validate_sandbox_id(&id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    // Remove container using container manager
    match state.container_manager.remove_container(&validated_id).await {
        Ok(_) => {
            info!("Container removed successfully for sandbox {}", validated_id);
            
            // Update database if available
            if let Some(ref repo) = state.sandbox_repository {
                // Try to find sandbox by container_id/sandbox_id and delete
                // For MVP, we'll just log the deletion
                info!("Would delete sandbox {} from database", validated_id);
            }
            
            Ok(Json(json!({
                "id": validated_id,
                "status": "deleted", 
                "deleted_by": auth.0.user_id,
                "deleted_at": chrono::Utc::now().to_rfc3339()
            })))
        },
        Err(e) => {
            // Check if it's a "not found" error or other error
            if e.to_string().contains("not found") || e.to_string().contains("No such container") {
                warn!("Container {} not found for deletion", validated_id);
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Failed to delete container for sandbox {}: {}", validated_id, e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
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
    let validated_id = InputValidator::validate_sandbox_id(&id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
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
    
    // Get container by sandbox ID
    match state.container_manager.get_container(&validated_id).await {
        Some(container) => {
            // Create command based on language
            let command = match language {
                "python" => vec!["python3".to_string(), "-c".to_string(), code.to_string()],
                "javascript" | "node" => vec!["node".to_string(), "-e".to_string(), code.to_string()],
                "bash" => vec!["bash".to_string(), "-c".to_string(), code.to_string()],
                "rust" => {
                    // For Rust, we'd need to create a proper file and compile - simplified for MVP
                    vec!["bash".to_string(), "-c".to_string(), format!("echo '{}' > /tmp/main.rs && rustc /tmp/main.rs -o /tmp/main && /tmp/main", code)]
                },
                "go" => {
                    // Similar simplification for Go
                    vec!["bash".to_string(), "-c".to_string(), format!("echo '{}' > /tmp/main.go && cd /tmp && go run main.go", code)]
                },
                _ => vec!["bash".to_string(), "-c".to_string(), code.to_string()],
            };
            
            let start_time = chrono::Utc::now();
            
            // Execute code in the container
            match container.execute_command(command).await {
                Ok(result) => {
                    let end_time = chrono::Utc::now();
                    let duration = end_time - start_time;
                    
                    Ok(Json(json!({
                        "execution_id": execution_id,
                        "sandbox_id": validated_id,
                        "status": if result.exit_code == 0 { "completed" } else { "failed" },
                        "language": language,
                        "code_length": code.len(),
                        "executed_by": auth.0.user_id,
                        "started_at": start_time.to_rfc3339(),
                        "completed_at": end_time.to_rfc3339(),
                        "duration_ms": duration.num_milliseconds(),
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "exit_code": result.exit_code
                    })))
                },
                Err(e) => {
                    error!("Code execution failed in sandbox {}: {}", validated_id, e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        },
        None => {
            warn!("Sandbox {} not found for user {}", validated_id, auth.0.username);
            Err(StatusCode::NOT_FOUND)
        }
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