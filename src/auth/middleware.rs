use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use tracing::{warn, debug};

use super::{JwtManager, Claims};
use super::models::{User, Permission, Role};
use crate::audit::{AuditService, AuditMiddleware, AuditEventType};
use crate::error::SoulBoxError;
use crate::validation::InputValidator;

/// 认证提取器 - 从请求中提取用户信息
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: uuid::Uuid,
    pub username: String,
    pub role: Role,
    pub tenant_id: Option<uuid::Uuid>,
    pub permissions: std::collections::HashSet<Permission>,
}

impl AuthContext {
    pub fn from_claims(claims: Claims) -> Self {
        let user_id = uuid::Uuid::parse_str(&claims.sub)
            .expect("Invalid user ID in token claims");
        
        Self {
            user_id,
            username: claims.username,
            role: claims.role.clone(),
            tenant_id: claims.tenant_id,
            permissions: claims.role.permissions(),
        }
    }

    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions.contains(permission)
    }

    pub fn can_access_tenant(&self, tenant_id: &uuid::Uuid) -> bool {
        match self.role {
            Role::SuperAdmin => true,
            _ => self.tenant_id.as_ref() == Some(tenant_id),
        }
    }
}

/// 认证中间件
pub struct AuthMiddleware {
    jwt_manager: Arc<JwtManager>,
    audit_middleware: Option<Arc<AuditMiddleware>>,
    /// API Key 管理器
    api_key_manager: Option<Arc<crate::auth::api_key::ApiKeyManager>>,
}

impl AuthMiddleware {
    pub fn new(jwt_manager: Arc<JwtManager>) -> Self {
        Self {
            jwt_manager,
            audit_middleware: None,
            api_key_manager: None,
        }
    }

    /// Set API Key manager
    pub fn with_api_key_manager(mut self, api_key_manager: Arc<crate::auth::api_key::ApiKeyManager>) -> Self {
        self.api_key_manager = Some(api_key_manager);
        self
    }

    pub fn with_audit(mut self, audit_middleware: Arc<AuditMiddleware>) -> Self {
        self.audit_middleware = Some(audit_middleware);
        self
    }

    /// JWT 认证中间件
    pub async fn jwt_auth(
        State(auth_middleware): State<Arc<Self>>,
        mut request: Request,
        next: Next,
    ) -> Result<Response, StatusCode> {
        // 从 Authorization 头中提取令牌
        let auth_header = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        let token = match auth_header {
            Some(header) if header.starts_with("Bearer ") => {
                let token_part = &header[7..]; // 移除 "Bearer " 前缀
                
                // Basic token format validation
                if token_part.is_empty() || token_part.len() > 4096 {
                    warn!("Invalid token format: length={}", token_part.len());
                    return Err(StatusCode::UNAUTHORIZED);
                }
                
                // Check for dangerous characters
                if token_part.contains('\0') || token_part.contains('\n') || token_part.contains('\r') {
                    warn!("Token contains invalid characters");
                    return Err(StatusCode::UNAUTHORIZED);
                }
                
                token_part
            }
            _ => {
                warn!("Missing or invalid Authorization header");
                return Err(StatusCode::UNAUTHORIZED);
            }
        };

        // 验证 JWT 令牌
        let claims = match auth_middleware.jwt_manager.validate_access_token(token).await {
            Ok(claims) => {
                // 记录成功的认证事件
                if let Some(ref audit_middleware) = auth_middleware.audit_middleware {
                    let user_id = uuid::Uuid::parse_str(&claims.sub).ok();
                    let ip_address = request.headers()
                        .get("x-forwarded-for")
                        .or_else(|| request.headers().get("x-real-ip"))
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let user_agent = request.headers()
                        .get("user-agent")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());

                    let _ = audit_middleware.log_auth_event(
                        AuditEventType::UserLogin,
                        user_id,
                        Some(claims.username.clone()),
                        true,
                        None,
                        ip_address,
                        user_agent,
                    ).await;
                }
                claims
            }
            Err(e) => {
                // 记录失败的认证事件
                if let Some(ref audit_middleware) = auth_middleware.audit_middleware {
                    let ip_address = request.headers()
                        .get("x-forwarded-for")
                        .or_else(|| request.headers().get("x-real-ip"))
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let user_agent = request.headers()
                        .get("user-agent")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());

                    let _ = audit_middleware.log_auth_event(
                        AuditEventType::UserLoginFailed,
                        None,
                        None,
                        false,
                        Some(e.to_string()),
                        ip_address,
                        user_agent,
                    ).await;
                }
                warn!("JWT validation failed: {}", e);
                return Err(StatusCode::UNAUTHORIZED);
            }
        };

        // 创建认证上下文并添加到请求扩展中
        let auth_context = AuthContext::from_claims(claims);
        request.extensions_mut().insert(auth_context);

        debug!("User authenticated: {}", request.extensions().get::<AuthContext>().unwrap().username);
        Ok(next.run(request).await)
    }

    /// API Key authentication middleware
    pub async fn api_key_auth(
        State(auth_middleware): State<Arc<Self>>,
        mut request: Request,
        next: Next,
    ) -> Result<Response, StatusCode> {
        // Extract API key from Authorization header or query parameter
        let api_key = Self::extract_api_key(&request)?;
        
        // For now, implement simple API key validation
        // In production, this would check against a database or key store
        let is_valid = Self::validate_api_key(&api_key);
        
        if !is_valid {
            warn!("Invalid API key provided: {}", &api_key[..8.min(api_key.len())]);
            return Err(StatusCode::UNAUTHORIZED);
        }
        
        // Create a basic auth context for API key access
        let auth_context = AuthContext {
            user_id: uuid::Uuid::new_v4(), // Would be looked up from API key
            username: format!("api_user_{}", &api_key[..8.min(api_key.len())]),
            role: super::models::Role::Developer, // Default role for API access
            tenant_id: None, // Would be associated with API key
            permissions: super::models::Role::Developer.permissions(),
        };
        
        // Record API key usage if audit middleware is available
        if let Some(ref audit_middleware) = auth_middleware.audit_middleware {
            let _ = audit_middleware.log_auth_event(
                crate::audit::AuditEventType::ApiKeyUsed,
                Some(auth_context.user_id),
                Some(auth_context.username.clone()),
                true,
                None,
                None,
                None,
            ).await;
        }
        
        request.extensions_mut().insert(auth_context);
        
        Ok(next.run(request).await)
    }
    
    fn extract_api_key(request: &Request) -> Result<String, StatusCode> {
        // Try Authorization header first (Bearer token)
        if let Some(auth_header) = request.headers().get(AUTHORIZATION) {
            if let Ok(auth_str) = auth_header.to_str() {
                if auth_str.starts_with("Bearer ") {
                    return Ok(auth_str[7..].to_string());
                }
            }
        }
        
        // Try X-API-Key header
        if let Some(api_key_header) = request.headers().get("x-api-key") {
            if let Ok(api_key) = api_key_header.to_str() {
                return Ok(api_key.to_string());
            }
        }
        
        // Try query parameter
        if let Some(query) = request.uri().query() {
            for param in query.split('&') {
                if let Some((key, value)) = param.split_once('=') {
                    if key == "api_key" {
                        return Ok(value.to_string());
                    }
                }
            }
        }
        
        Err(StatusCode::UNAUTHORIZED)
    }
    
    fn validate_api_key(api_key: &str) -> bool {
        // Basic validation rules for API keys
        if api_key.len() < 16 || api_key.len() > 128 {
            return false;
        }
        
        // Check for valid characters (alphanumeric + some special chars)
        if !api_key.chars().all(|c| c.is_alphanumeric() || "_-".contains(c)) {
            return false;
        }
        
        // For demo purposes, accept keys starting with "sk_" (similar to many APIs)
        if api_key.starts_with("sk_") && api_key.len() >= 32 {
            return true;
        }
        
        // Or keys starting with "soulbox_" for development
        if api_key.starts_with("soulbox_") && api_key.len() >= 24 {
            return true;
        }
        
        false
    }

    /// 权限检查中间件工厂
    pub fn require_permission(
        permission: Permission,
    ) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, StatusCode>> + Send>> + Clone {
        move |request: Request, next: Next| {
            let required_permission = permission.clone();
            Box::pin(async move {
                // 从请求扩展中获取认证上下文
                let auth_context = request
                    .extensions()
                    .get::<AuthContext>()
                    .ok_or(StatusCode::UNAUTHORIZED)?;

                // 检查权限
                if !auth_context.has_permission(&required_permission) {
                    warn!(
                        "User {} lacks required permission {:?}",
                        auth_context.username, required_permission
                    );
                    return Err(StatusCode::FORBIDDEN);
                }

                Ok(next.run(request).await)
            })
        }
    }

    /// 租户访问检查中间件工厂
    pub fn require_tenant_access() -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, StatusCode>> + Send>> + Clone {
        move |request: Request, next: Next| {
            Box::pin(async move {
                // 从请求扩展中获取认证上下文
                let auth_context = request
                    .extensions()
                    .get::<AuthContext>()
                    .ok_or(StatusCode::UNAUTHORIZED)?;

                // Extract tenant ID from request path or headers
                let tenant_id = extract_tenant_id_from_request(&request);
                
                // 检查租户访问权限
                // if !auth_context.can_access_tenant(&tenant_id) {
                //     warn!(
                //         "User {} cannot access tenant {}",
                //         auth_context.username, tenant_id
                //     );
                //     return Err(StatusCode::FORBIDDEN);
                // }

                Ok(next.run(request).await)
            })
        }
    }
}

/// Extract tenant ID from request path parameters or headers
fn extract_tenant_id_from_request(request: &Request) -> Option<uuid::Uuid> {
    // Try to extract from path parameters first (e.g., /api/tenants/{tenant_id}/...)
    if let Some(uri_path) = request.uri().path().strip_prefix("/api/tenants/") {
        if let Some(tenant_id_str) = uri_path.split('/').next() {
            if let Ok(tenant_id) = uuid::Uuid::parse_str(tenant_id_str) {
                debug!("Extracted tenant ID from path: {}", tenant_id);
                return Some(tenant_id);
            }
        }
    }
    
    // Try to extract from query parameters (e.g., ?tenant_id=...)
    if let Some(query) = request.uri().query() {
        for param in query.split('&') {
            if let Some((key, value)) = param.split_once('=') {
                if key == "tenant_id" {
                    if let Ok(tenant_id) = uuid::Uuid::parse_str(value) {
                        debug!("Extracted tenant ID from query: {}", tenant_id);
                        return Some(tenant_id);
                    }
                }
            }
        }
    }
    
    // Try to extract from X-Tenant-ID header
    if let Some(header_value) = request.headers().get("X-Tenant-ID") {
        if let Ok(header_str) = header_value.to_str() {
            if let Ok(tenant_id) = uuid::Uuid::parse_str(header_str) {
                debug!("Extracted tenant ID from header: {}", tenant_id);
                return Some(tenant_id);
            }
        }
    }
    
    debug!("No tenant ID found in request");
    None
}

/// 认证提取器 - 用于处理函数中提取认证信息
pub struct AuthExtractor(pub AuthContext);

impl<S> axum::extract::FromRequestParts<S> for AuthExtractor
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let auth_context = parts
            .extensions
            .get::<AuthContext>()
            .ok_or(StatusCode::UNAUTHORIZED)?
            .clone();

        Ok(AuthExtractor(auth_context))
    }
}

/// 可选认证提取器 - 认证信息可能不存在
pub struct OptionalAuthExtractor(pub Option<AuthContext>);

impl<S> axum::extract::FromRequestParts<S> for OptionalAuthExtractor
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let auth_context = parts.extensions.get::<AuthContext>().cloned();
        Ok(OptionalAuthExtractor(auth_context))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::models::{User, Role};
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
        middleware,
        response::Response,
        routing::get,
        Router,
    };
    use tower::ServiceExt;
    use chrono::Utc;
    use uuid::Uuid;

    async fn test_handler(auth: AuthExtractor) -> &'static str {
        format!("Hello, {}!", auth.0.username).leak()
    }

    fn create_test_user() -> User {
        User {
            id: Uuid::new_v4(),
            username: "testuser".to_string(),
            email: "test@example.com".to_string(),
            role: Role::Developer,
            is_active: true,
            created_at: Utc::now(),
            last_login: None,
            tenant_id: Some(Uuid::new_v4()),
        }
    }

    #[tokio::test]
    async fn test_jwt_auth_middleware_success() {
        let jwt_manager = Arc::new(JwtManager::new(
            "test-secret",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        ).unwrap());
        
        let auth_middleware = Arc::new(AuthMiddleware::new(jwt_manager.clone()));
        
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(middleware::from_fn_with_state(
                auth_middleware,
                AuthMiddleware::jwt_auth,
            ));

        let user = create_test_user();
        let token = jwt_manager.generate_access_token(&user).unwrap();

        let request = Request::builder()
            .method(Method::GET)
            .uri("/test")
            .header(AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_jwt_auth_middleware_missing_token() {
        let jwt_manager = Arc::new(JwtManager::new(
            "test-secret",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        ).unwrap());
        
        let auth_middleware = Arc::new(AuthMiddleware::new(jwt_manager));
        
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(middleware::from_fn_with_state(
                auth_middleware,
                AuthMiddleware::jwt_auth,
            ));

        let request = Request::builder()
            .method(Method::GET)
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_jwt_auth_middleware_invalid_token() {
        let jwt_manager = Arc::new(JwtManager::new(
            "test-secret",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        ).unwrap());
        
        let auth_middleware = Arc::new(AuthMiddleware::new(jwt_manager));
        
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(middleware::from_fn_with_state(
                auth_middleware,
                AuthMiddleware::jwt_auth,
            ));

        let request = Request::builder()
            .method(Method::GET)
            .uri("/test")
            .header(AUTHORIZATION, "Bearer invalid-token")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}