use axum::{
    extract::State,
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
    models::{Permission, Role},
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
        .route("/permissions/users/{user_id}", get(get_user_permissions))
        .route("/permissions/check", post(check_permission));
        
    // 权限管理端点（需要管理员权限） - Simplified for MVP
    let admin_routes = Router::new()
        .route("/permissions/assign-role", put(assign_role));
        
    // 系统权限端点（需要超级管理员权限） - Simplified for MVP  
    let system_routes = Router::new()
        .route("/permissions/system/all", get(list_all_permissions));
    
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
    State(auth_state): State<AuthState>,
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

    // Get user information from database
    let (user_permissions, actual_user, actual_role) = if let Some(user_repo) = &auth_state.user_repository {
        match user_repo.find_by_id(user_id).await {
            Ok(Some(db_user)) => {
                let domain_user = db_user.to_domain_model().map_err(|e| {
                    warn!("Failed to convert user from database: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                
                let permissions: Vec<Permission> = domain_user.role.permissions().into_iter().collect();
                (permissions, domain_user.username, domain_user.role)
            },
            Ok(None) => {
                warn!("User {} not found in database", user_id);
                return Err(StatusCode::NOT_FOUND);
            },
            Err(e) => {
                warn!("Database error getting user {}: {}", user_id, e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else {
        // Fallback to current auth user info
        let user_permissions: Vec<Permission> = auth.0.role.permissions().into_iter().collect();
        (user_permissions, auth.0.username.clone(), auth.0.role.clone())
    };
    
    let response = UserPermissionsResponse {
        user_id,
        username: actual_user,
        role: actual_role,
        permissions: user_permissions.clone(),
        tenant_id: auth.0.tenant_id, // This could also come from the database user
        effective_permissions: user_permissions, // Future: might include dynamic permissions
    };

    Ok(Json(response))
}

/// 检查用户权限
async fn check_permission(
    State(auth_state): State<AuthState>,
    auth: AuthExtractor,
    Json(request): Json<PermissionCheckRequest>,
) -> Result<Json<PermissionCheckResponse>, StatusCode> {
    info!("User {} checking permission {:?} for user {}", 
          auth.0.username, request.permission, request.user_id);

    // 检查是否有权限查看其他用户的权限
    if request.user_id != auth.0.user_id && !auth.0.has_permission(&Permission::UserRead) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Get user information from database for permission check
    let (has_permission, user_role) = if let Some(user_repo) = &auth_state.user_repository {
        match user_repo.find_by_id(request.user_id).await {
            Ok(Some(db_user)) => {
                let domain_user = db_user.to_domain_model().map_err(|e| {
                    warn!("Failed to convert user from database: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                
                let has_perm = domain_user.has_permission(&request.permission);
                (has_perm, domain_user.role)
            },
            Ok(None) => {
                warn!("User {} not found in database", request.user_id);
                return Err(StatusCode::NOT_FOUND);
            },
            Err(e) => {
                warn!("Database error getting user {}: {}", request.user_id, e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else {
        // Fallback to current auth user if checking own permissions
        if request.user_id == auth.0.user_id {
            (auth.0.has_permission(&request.permission), auth.0.role.clone())
        } else {
            return Err(StatusCode::INTERNAL_SERVER_ERROR); // Can't check other users without database
        }
    };
    
    let reason = if has_permission {
        format!("User has role {:?} which includes this permission", user_role)
    } else {
        format!("User role {:?} does not include this permission", user_role)
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
    State(auth_state): State<AuthState>,
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

    // Implement actual role assignment logic
    if let Some(user_repo) = &auth_state.user_repository {
        match user_repo.find_by_id(request.user_id).await {
            Ok(Some(db_user)) => {
                // Update user role in the database
                let mut domain_user = db_user.to_domain_model().map_err(|e| {
                    warn!("Failed to convert user from database: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
                
                domain_user.role = request.role.clone();
                
                // Save the updated user back to database
                if let Err(e) = user_repo.update(&domain_user).await {
                    warn!("Failed to update user role: {}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
                
                info!("Successfully assigned role {:?} to user {}", request.role, request.user_id);
            },
            Ok(None) => {
                warn!("User {} not found for role assignment", request.user_id);
                return Err(StatusCode::NOT_FOUND);
            },
            Err(e) => {
                warn!("Database error during role assignment: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else {
        warn!("No user repository configured - role assignment not possible");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

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