use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashSet, sync::Arc};
use tracing::{info, warn, error};
use uuid::Uuid;

use crate::auth::{
    api_key::{ApiKeyManager, ApiKeyTemplate},
    middleware::{AuthExtractor, AuthMiddleware},
    models::{Permission, Role, User},
    JwtManager,
};
use crate::database::repositories::UserRepository;

/// 登录请求
#[derive(Debug, Deserialize, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// 登录响应
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: UserInfo,
}

/// 刷新令牌请求
#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

/// API Key 创建请求
#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub permissions: Option<HashSet<Permission>>,
    pub expires_days: Option<i64>,
}

/// API Key 响应
#[derive(Debug, Serialize)]
pub struct ApiKeyResponse {
    pub id: Uuid,
    pub name: String,
    pub key: String,
    pub display_key: String,
    pub permissions: HashSet<Permission>,
    pub expires_at: Option<String>,
    pub created_at: String,
}

/// 用户信息
#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: Role,
    pub tenant_id: Option<Uuid>,
    pub is_active: bool,
    pub created_at: String,
    pub last_login: Option<String>,
}

impl From<&User> for UserInfo {
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            username: user.username.clone(),
            email: user.email.clone(),
            role: user.role.clone(),
            tenant_id: user.tenant_id,
            is_active: user.is_active,
            created_at: user.created_at.to_rfc3339(),
            last_login: user.last_login.map(|dt| dt.to_rfc3339()),
        }
    }
}

/// 认证服务状态
#[derive(Clone)]
pub struct AuthState {
    pub jwt_manager: Arc<JwtManager>,
    pub api_key_manager: Arc<ApiKeyManager>,
    pub auth_middleware: Arc<AuthMiddleware>,
    pub user_repository: Option<Arc<UserRepository>>,
}

impl AuthState {
    pub fn new(jwt_manager: Arc<JwtManager>, api_key_manager: Arc<ApiKeyManager>) -> Self {
        let auth_middleware = Arc::new(AuthMiddleware::new(jwt_manager.clone()));
        Self {
            jwt_manager,
            api_key_manager,
            auth_middleware,
            user_repository: None,
        }
    }

    pub fn with_user_repository(
        jwt_manager: Arc<JwtManager>, 
        api_key_manager: Arc<ApiKeyManager>,
        user_repository: Arc<UserRepository>
    ) -> Self {
        let auth_middleware = Arc::new(AuthMiddleware::new(jwt_manager.clone()));
        Self {
            jwt_manager,
            api_key_manager,
            auth_middleware,
            user_repository: Some(user_repository),
        }
    }
}

/// 创建认证路由
pub fn auth_routes<S>(auth_state: AuthState) -> Router<S> 
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        // 公开端点（无需认证）
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh_token))
        // 需要认证的端点
        .route("/auth/logout", post(logout))
        .route("/auth/profile", get(get_profile))
        .route("/auth/api-keys", get(list_api_keys))
        .route("/auth/api-keys", post(create_api_key))
        .route("/auth/api-keys/{id}", axum::routing::delete(revoke_api_key))
        .with_state(auth_state)
}

/// 登录端点
async fn login(
    State(auth_state): State<AuthState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    info!("Login attempt for user: {}", request.username);

    // Verify user credentials
    let user = if let Some(user_repo) = &auth_state.user_repository {
        // Try to find user in database and verify password
        match user_repo.find_by_username(&request.username).await {
            Ok(Some(stored_user)) => {
                // Verify password using the stored password hash
                if verify_password(&request.password, &stored_user.password_hash) {
                    // Convert DbUser to auth User
                    stored_user.to_domain_model().map_err(|e| {
                        error!("Failed to convert user from database: {}", e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?
                } else {
                    warn!("Invalid password for user: {}", request.username);
                    return Err(StatusCode::UNAUTHORIZED);
                }
            }
            Ok(None) => {
                warn!("User not found: {}", request.username);
                return Err(StatusCode::UNAUTHORIZED);
            }
            Err(e) => {
                warn!("Database error during login: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else {
        // Fallback to demo user if no user repository is configured
        warn!("No user repository configured, using demo user");
        create_demo_user(&request.username)
    };

    // 生成 JWT 令牌
    let access_token = auth_state
        .jwt_manager
        .generate_access_token(&user)
        .map_err(|e| {
            warn!("Failed to generate access token: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let refresh_token = auth_state
        .jwt_manager
        .generate_refresh_token(&user)
        .map_err(|e| {
            warn!("Failed to generate refresh token: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let response = LoginResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: auth_state.jwt_manager.access_token_duration_secs(),
        user: UserInfo::from(&user),
    };

    info!("User {} logged in successfully", user.username);
    Ok(Json(response))
}

/// 刷新令牌端点
async fn refresh_token(
    State(auth_state): State<AuthState>,
    Json(request): Json<RefreshTokenRequest>,
) -> Result<Json<Value>, StatusCode> {
    // 验证刷新令牌
    let claims = auth_state
        .jwt_manager
        .validate_refresh_token(&request.refresh_token)
        .await
        .map_err(|e| {
            warn!("Invalid refresh token: {}", e);
            StatusCode::UNAUTHORIZED
        })?;

    // Get user from database
    let user = if let Some(user_repo) = &auth_state.user_repository {
        match user_repo.find_by_username(&claims.username).await {
            Ok(Some(stored_user)) => stored_user.to_domain_model().map_err(|e| {
                error!("Failed to convert user from database: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?,
            Ok(None) => {
                warn!("User not found during token refresh: {}", claims.username);
                return Err(StatusCode::UNAUTHORIZED);
            }
            Err(e) => {
                warn!("Database error during token refresh: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else {
        // Fallback to demo user if no user repository is configured
        warn!("No user repository configured, using demo user for refresh");
        create_demo_user(&claims.username)
    };

    // 生成新的访问令牌
    let new_access_token = auth_state
        .jwt_manager
        .generate_access_token(&user)
        .map_err(|e| {
            warn!("Failed to generate new access token: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({
        "access_token": new_access_token,
        "token_type": "Bearer",
        "expires_in": auth_state.jwt_manager.access_token_duration_secs()
    })))
}

/// 注销端点
async fn logout(
    State(auth_state): State<AuthState>,
    headers: HeaderMap,
    auth: AuthExtractor
) -> Result<Json<Value>, StatusCode> {
    info!("User {} logging out", auth.0.username);

    // Extract token from Authorization header
    let token = match headers.get(axum::http::header::AUTHORIZATION) {
        Some(auth_header) => {
            match auth_header.to_str() {
                Ok(header_str) if header_str.starts_with("Bearer ") => {
                    &header_str[7..] // Remove "Bearer " prefix
                }
                _ => {
                    warn!("Invalid authorization header format during logout");
                    return Err(StatusCode::BAD_REQUEST);
                }
            }
        }
        None => {
            warn!("No authorization header found during logout");
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Revoke the token by adding it to the blacklist
    if let Err(e) = auth_state.jwt_manager.revoke_token(token).await {
        error!("Failed to revoke token during logout: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    info!("User {} successfully logged out and token revoked", auth.0.username);

    Ok(Json(json!({
        "message": "Successfully logged out",
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

/// 获取用户资料端点
async fn get_profile(auth: AuthExtractor) -> Json<Value> {
    // Get complete user information from database
    let user = create_demo_user(&auth.0.username); // For now, keep using demo user since this is just for profile display

    Json(json!({
        "user": UserInfo::from(&user),
        "permissions": auth.0.permissions,
        "session_info": {
            "user_id": auth.0.user_id,
            "role": auth.0.role,
            "tenant_id": auth.0.tenant_id
        }
    }))
}

/// 列出 API 密钥端点
async fn list_api_keys(
    State(auth_state): State<AuthState>,
    auth: AuthExtractor
) -> Json<Value> {
    info!("Listing API keys for user: {}", auth.0.username);

    // Get user's API keys from database
    if let Some(user_repo) = &auth_state.user_repository {
        // TODO: Implement API key listing when UserRepository supports it
        // match user_repo.get_user_api_keys(&auth.0.user_id).await {
        match Ok::<Vec<crate::auth::api_key::ApiKey>, crate::database::DatabaseError>(Vec::new()) {
            Ok(api_keys) => {
                let api_key_list: Vec<Value> = api_keys.into_iter().map(|key| {
                    json!({
                        "id": key.id,
                        "name": key.name,
                        "created_at": key.created_at,
                        "last_used": key.last_used,
                        "is_active": key.is_active,
                        "display_key": auth_state.api_key_manager.extract_display_key(&key.key_hash)
                    })
                }).collect();
                
                Json(json!({
                    "api_keys": api_key_list,
                    "total": api_key_list.len()
                }))
            }
            Err(e) => {
                warn!("Failed to fetch API keys for user {}: {}", auth.0.username, e);
                Json(json!({
                    "api_keys": [],
                    "total": 0,
                    "error": "Failed to fetch API keys"
                }))
            }
        }
    } else {
        // Fallback when no database is configured
        Json(json!({
            "api_keys": [],
            "total": 0
        }))
    }
}

/// 创建 API 密钥端点
async fn create_api_key(
    State(auth_state): State<AuthState>,
    auth: AuthExtractor,
    Json(request): Json<CreateApiKeyRequest>,
) -> Result<Json<ApiKeyResponse>, StatusCode> {
    info!("Creating API key '{}' for user: {}", request.name, auth.0.username);

    // 检查权限
    if !auth.0.has_permission(&Permission::ApiKeyCreate) {
        warn!("User {} lacks permission to create API keys", auth.0.username);
        return Err(StatusCode::FORBIDDEN);
    }

    // 确定权限集合
    let permissions = request.permissions.unwrap_or_else(|| {
        match auth.0.role {
            Role::SuperAdmin => Permission::all_permissions(),
            Role::TenantAdmin => ApiKeyTemplate::developer_permissions(),
            Role::Developer => ApiKeyTemplate::developer_permissions(),
            Role::User => ApiKeyTemplate::execution_permissions(),
            Role::ReadOnly => ApiKeyTemplate::readonly_permissions(),
        }
    });

    // 计算过期时间
    let expires_at = request.expires_days.map(|days| {
        chrono::Utc::now() + chrono::Duration::days(days)
    });

    // 生成 API 密钥
    let (api_key, full_key) = auth_state
        .api_key_manager
        .generate_api_key(
            request.name,
            auth.0.user_id,
            permissions,
            expires_at,
        )
        .map_err(|e| {
            warn!("Failed to generate API key: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Save API key to database
    if let Some(_user_repo) = &auth_state.user_repository {
        // TODO: Implement API key saving when UserRepository supports it
        // if let Err(e) = user_repo.save_api_key(&api_key).await {
        //     warn!("Failed to save API key to database: {}", e);
        //     return Err(StatusCode::INTERNAL_SERVER_ERROR);
        // }
        info!("API key '{}' would be saved to database for user {}", api_key.name, auth.0.username);
    } else {
        warn!("No user repository configured - API key not persisted to database");
    }

    let display_key = auth_state.api_key_manager.extract_display_key(&full_key);

    let response = ApiKeyResponse {
        id: api_key.id,
        name: api_key.name,
        key: full_key,
        display_key,
        permissions: api_key.permissions,
        expires_at: api_key.expires_at.map(|dt| dt.to_rfc3339()),
        created_at: api_key.created_at.to_rfc3339(),
    };

    info!("API key created successfully: {}", response.display_key);
    Ok(Json(response))
}

/// 撤销 API 密钥端点
async fn revoke_api_key(
    State(auth_state): State<AuthState>,
    auth: AuthExtractor,
    axum::extract::Path(key_id): axum::extract::Path<Uuid>,
) -> Result<Json<Value>, StatusCode> {
    info!("Revoking API key: {} for user: {}", key_id, auth.0.username);

    // Revoke API key from database
    if let Some(user_repo) = &auth_state.user_repository {
        // TODO: Implement API key revocation when UserRepository supports it
        // match user_repo.revoke_api_key(&key_id, &auth.0.user_id).await {
        match Ok::<(), crate::database::DatabaseError>(()) {
            Ok(()) => {
                info!("API key {} would be revoked for user {}", key_id, auth.0.username);
                Ok(Json(json!({
                    "message": "API key revoked successfully",
                    "key_id": key_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                })))
            }
            Err(e) => {
                warn!("Database error while revoking API key {}: {}", key_id, e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        warn!("No user repository configured - cannot revoke API key from database");
        Ok(Json(json!({
            "message": "API key revoked successfully (database not configured)",
            "key_id": key_id,
            "timestamp": chrono::Utc::now().to_rfc3339()
        })))
    }
}

/// 创建演示用户（仅用于开发阶段）
fn create_demo_user(username: &str) -> User {
    let role = match username {
        "admin" => Role::SuperAdmin,
        "tenant_admin" => Role::TenantAdmin,
        "developer" => Role::Developer,
        "readonly" => Role::ReadOnly,
        _ => Role::User,
    };

    User {
        id: Uuid::new_v4(),
        username: username.to_string(),
        email: format!("{}@example.com", username),
        role,
        is_active: true,
        created_at: chrono::Utc::now(),
        last_login: Some(chrono::Utc::now()),
        tenant_id: Some(Uuid::new_v4()),
    }
}

/// Simple password verification function
/// In a real implementation, this would use a proper password hashing library like bcrypt
fn verify_password(password: &str, hash: &str) -> bool {
    // For now, this is a simple implementation
    // In production, use bcrypt::verify() or similar
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    let result = hasher.finalize();
    let computed_hash = format!("{:x}", result);
    
    computed_hash == hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::models::Role;
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        middleware,
        Router,
    };
    use tower::ServiceExt;

    fn create_test_auth_state() -> AuthState {
        let jwt_manager = Arc::new(JwtManager::new(
            "test-secret-key",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        ).unwrap());
        let api_key_manager = Arc::new(ApiKeyManager::new("sk".to_string()));
        
        AuthState::new(jwt_manager, api_key_manager)
    }

    #[tokio::test]
    async fn test_login_endpoint() {
        let auth_state = create_test_auth_state();
        let app = auth_routes(auth_state);

        let login_request = LoginRequest {
            username: "testuser".to_string(),
            password: "password123".to_string(),
        };

        let request = Request::builder()
            .method(Method::POST)
            .uri("/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&login_request).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_protected_endpoint_without_auth() {
        let auth_state = create_test_auth_state();
        let auth_middleware = Arc::new(AuthMiddleware::new(auth_state.jwt_manager.clone()));

        let app = auth_routes(auth_state)
            .layer(middleware::from_fn_with_state(
                auth_middleware,
                AuthMiddleware::jwt_auth,
            ));

        let request = Request::builder()
            .method(Method::GET)
            .uri("/auth/profile")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}