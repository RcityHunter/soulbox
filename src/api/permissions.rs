use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

use crate::auth::{
    middleware::{AuthExtractor, AuthMiddleware},
    models::{Permission, Role, User},
};
use crate::api::auth::AuthState;
use tracing::{info, warn};

/// 权限查询参数
#[derive(Debug, Deserialize)]
pub struct PermissionQuery {
    pub user_id: Option<Uuid>,
    pub role: Option<Role>,
}

/// 角色权限响应
#[derive(Debug, Serialize)]
pub struct RolePermissionsResponse {
    pub role: Role,
    pub permissions: Vec<Permission>,
    pub permission_count: usize,
}

/// 用户权限响应
#[derive(Debug, Serialize)]
pub struct UserPermissionsResponse {
    pub user_id: Uuid,
    pub username: String,
    pub role: Role,
    pub permissions: Vec<Permission>,
    pub tenant_id: Option<Uuid>,
    pub effective_permissions: Vec<Permission>,
}

/// 权限检查请求
#[derive(Debug, Deserialize)]
pub struct PermissionCheckRequest {
    pub user_id: Uuid,
    pub permission: Permission,
    pub resource_id: Option<String>,
}

/// 权限检查响应
#[derive(Debug, Serialize)]
pub struct PermissionCheckResponse {
    pub has_permission: bool,
    pub user_id: Uuid,
    pub permission: Permission,
    pub reason: String,
}

/// 角色分配请求
#[derive(Debug, Deserialize)]
pub struct AssignRoleRequest {
    pub user_id: Uuid,
    pub role: Role,
    pub reason: Option<String>,
}

/// 创建权限管理路由
pub fn permission_routes<S>(auth_state: AuthState) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    // 基础权限查询端点
    let basic_routes = Router::new()
        .route("/permissions/roles", get(list_role_permissions))
        .route("/permissions/users/:user_id", get(get_user_permissions))
        .route("/permissions/check", post(check_permission));
        
    // 权限管理端点（需要管理员权限）
    let admin_routes = Router::new()
        .route("/permissions/assign-role", put(assign_role))
        .layer(axum::middleware::from_fn(
            AuthMiddleware::require_permission(Permission::UserUpdate),
        ));
        
    // 系统权限端点（需要超级管理员权限）
    let system_routes = Router::new()
        .route("/permissions/system/all", get(list_all_permissions))
        .layer(axum::middleware::from_fn(
            AuthMiddleware::require_permission(Permission::SystemConfig),
        ));
    
    // 合并所有路由并添加认证
    basic_routes
        .merge(admin_routes)
        .merge(system_routes)
        .layer(axum::middleware::from_fn_with_state(
            auth_state.auth_middleware.clone(),
            AuthMiddleware::jwt_auth,
        ))
        .with_state(auth_state)
}

/// 列出所有角色及其权限
async fn list_role_permissions(
    auth: AuthExtractor,
) -> Result<Json<Vec<RolePermissionsResponse>>, StatusCode> {
    info!("User {} listing role permissions", auth.0.username);

    let roles = vec![
        Role::SuperAdmin,
        Role::TenantAdmin,
        Role::Developer,
        Role::User,
        Role::ReadOnly,
    ];

    let response: Vec<RolePermissionsResponse> = roles
        .into_iter()
        .map(|role| {
            let permissions: Vec<Permission> = role.permissions().into_iter().collect();
            RolePermissionsResponse {
                permission_count: permissions.len(),
                role,
                permissions,
            }
        })
        .collect();

    Ok(Json(response))
}

/// 获取用户权限
async fn get_user_permissions(
    auth: AuthExtractor,
    axum::extract::Path(user_id): axum::extract::Path<Uuid>,
) -> Result<Json<UserPermissionsResponse>, StatusCode> {
    info!("User {} getting permissions for user {}", auth.0.username, user_id);

    // 检查是否有权限查看其他用户的权限
    if user_id != auth.0.user_id && !auth.0.has_permission(&Permission::UserRead) {
        warn!("User {} attempted to view permissions for user {} without permission", 
              auth.0.username, user_id);
        return Err(StatusCode::FORBIDDEN);
    }

    // TODO: 从数据库获取用户信息
    // 目前使用当前认证用户的信息作为演示
    let user_permissions: Vec<Permission> = auth.0.role.permissions().into_iter().collect();
    
    let response = UserPermissionsResponse {
        user_id,
        username: auth.0.username.clone(),
        role: auth.0.role.clone(),
        permissions: user_permissions.clone(),
        tenant_id: auth.0.tenant_id,
        effective_permissions: user_permissions, // 未来可能包含动态权限
    };

    Ok(Json(response))
}

/// 检查用户权限
async fn check_permission(
    auth: AuthExtractor,
    Json(request): Json<PermissionCheckRequest>,
) -> Result<Json<PermissionCheckResponse>, StatusCode> {
    info!("User {} checking permission {:?} for user {}", 
          auth.0.username, request.permission, request.user_id);

    // 检查是否有权限查看其他用户的权限
    if request.user_id != auth.0.user_id && !auth.0.has_permission(&Permission::UserRead) {
        return Err(StatusCode::FORBIDDEN);
    }

    // TODO: 从数据库获取用户信息进行权限检查
    // 目前使用当前认证用户进行演示
    let has_permission = auth.0.has_permission(&request.permission);
    let reason = if has_permission {
        format!("User has role {:?} which includes this permission", auth.0.role)
    } else {
        format!("User role {:?} does not include this permission", auth.0.role)
    };

    let response = PermissionCheckResponse {
        has_permission,
        user_id: request.user_id,
        permission: request.permission,
        reason,
    };

    Ok(Json(response))
}

/// 分配角色给用户
async fn assign_role(
    auth: AuthExtractor,
    Json(request): Json<AssignRoleRequest>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} assigning role {:?} to user {}", 
          auth.0.username, request.role, request.user_id);

    // 检查权限：只有管理员可以分配角色
    if !auth.0.has_permission(&Permission::UserUpdate) {
        warn!("User {} attempted to assign role without permission", auth.0.username);
        return Err(StatusCode::FORBIDDEN);
    }

    // 检查是否尝试分配超级管理员角色
    if request.role == Role::SuperAdmin && auth.0.role != Role::SuperAdmin {
        warn!("Non-superadmin user {} attempted to assign SuperAdmin role", auth.0.username);
        return Err(StatusCode::FORBIDDEN);
    }

    // TODO: 实现实际的角色分配逻辑
    // 目前返回成功响应作为演示

    Ok(Json(json!({
        "success": true,
        "user_id": request.user_id,
        "new_role": request.role,
        "assigned_by": auth.0.user_id,
        "reason": request.reason.unwrap_or_else(|| "Role assignment".to_string()),
        "assigned_at": chrono::Utc::now().to_rfc3339()
    })))
}

/// 列出所有系统权限
async fn list_all_permissions(
    auth: AuthExtractor,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} listing all system permissions", auth.0.username);

    // 获取所有权限
    let all_permissions: Vec<Permission> = Permission::all_permissions().into_iter().collect();
    
    // 按类别分组权限
    let mut permission_categories: HashMap<String, Vec<Permission>> = HashMap::new();
    
    for permission in &all_permissions {
        let category = match permission {
            Permission::SandboxCreate | Permission::SandboxRead | Permission::SandboxUpdate 
            | Permission::SandboxDelete | Permission::SandboxExecute | Permission::SandboxList => "Sandbox",
            
            Permission::FileUpload | Permission::FileDownload | Permission::FileDelete 
            | Permission::FileList => "File",
            
            Permission::UserCreate | Permission::UserRead | Permission::UserUpdate 
            | Permission::UserDelete | Permission::UserList => "User Management",
            
            Permission::TenantCreate | Permission::TenantRead | Permission::TenantUpdate 
            | Permission::TenantDelete => "Tenant Management",
            
            Permission::SystemConfig | Permission::SystemMonitor | Permission::SystemLogs 
            | Permission::SystemMaintenance => "System",
            
            Permission::ApiKeyCreate | Permission::ApiKeyRead | Permission::ApiKeyRevoke 
            | Permission::ApiKeyList => "API Key",
            
            Permission::AuditRead | Permission::AuditList => "Audit",
            
            Permission::TemplateCreate | Permission::TemplateRead | Permission::TemplateUpdate 
            | Permission::TemplateDelete | Permission::TemplateList => "Template",
        };
        
        permission_categories
            .entry(category.to_string())
            .or_insert_with(Vec::new)
            .push(permission.clone());
    }

    Ok(Json(json!({
        "total_permissions": all_permissions.len(),
        "categories": permission_categories,
        "all_permissions": all_permissions
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{api_key::ApiKeyManager, JwtManager};
    use std::sync::Arc;

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
    async fn test_permission_routes_creation() {
        let auth_state = create_test_auth_state();
        let _router: axum::Router = permission_routes(auth_state);
        // 路由创建成功
    }
}