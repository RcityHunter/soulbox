use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use uuid::Uuid;

/// 用户模型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: Role,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
    pub tenant_id: Option<Uuid>,
}

/// 用户角色
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Role {
    /// 超级管理员 - 系统最高权限
    SuperAdmin,
    /// 租户管理员 - 管理特定租户
    TenantAdmin,
    /// 开发者 - 创建和管理沙箱
    Developer,
    /// 用户 - 基础使用权限
    User,
    /// 只读用户 - 仅查看权限
    ReadOnly,
}

/// 权限枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Permission {
    // 沙箱管理权限
    SandboxCreate,
    SandboxRead,
    SandboxUpdate,
    SandboxDelete,
    SandboxExecute,
    
    // 文件操作权限
    FileUpload,
    FileDownload,
    FileDelete,
    FileList,
    
    // 用户管理权限
    UserCreate,
    UserRead,
    UserUpdate,
    UserDelete,
    
    // 租户管理权限
    TenantCreate,
    TenantRead,
    TenantUpdate,
    TenantDelete,
    
    // 系统管理权限
    SystemConfig,
    SystemMonitor,
    SystemLogs,
    
    // API 密钥管理
    ApiKeyCreate,
    ApiKeyRead,
    ApiKeyRevoke,
    ApiKeyList,
    
    // 沙盒列表权限
    SandboxList,
    
    // 用户列表权限
    UserList,
    
    // 审计权限
    AuditRead,
    AuditList,
    
    // 系统维护权限
    SystemMaintenance,
}

/// 认证令牌
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// API 密钥信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub key_hash: String,
    pub user_id: Uuid,
    pub permissions: HashSet<Permission>,
    pub is_active: bool,
    pub last_used: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Role {
    /// 获取角色的默认权限
    pub fn permissions(&self) -> HashSet<Permission> {
        match self {
            Role::SuperAdmin => {
                // 超级管理员拥有所有权限
                vec![
                    Permission::SandboxCreate, Permission::SandboxRead, Permission::SandboxUpdate, Permission::SandboxDelete, Permission::SandboxExecute, Permission::SandboxList,
                    Permission::FileUpload, Permission::FileDownload, Permission::FileDelete, Permission::FileList,
                    Permission::UserCreate, Permission::UserRead, Permission::UserUpdate, Permission::UserDelete, Permission::UserList,
                    Permission::TenantCreate, Permission::TenantRead, Permission::TenantUpdate, Permission::TenantDelete,
                    Permission::SystemConfig, Permission::SystemMonitor, Permission::SystemLogs, Permission::SystemMaintenance,
                    Permission::ApiKeyCreate, Permission::ApiKeyRead, Permission::ApiKeyRevoke, Permission::ApiKeyList,
                    Permission::AuditRead, Permission::AuditList,
                ].into_iter().collect()
            }
            Role::TenantAdmin => {
                // 租户管理员管理租户内资源
                vec![
                    Permission::SandboxCreate, Permission::SandboxRead, Permission::SandboxUpdate, Permission::SandboxDelete, Permission::SandboxExecute,
                    Permission::FileUpload, Permission::FileDownload, Permission::FileDelete, Permission::FileList,
                    Permission::UserCreate, Permission::UserRead, Permission::UserUpdate, Permission::UserDelete,
                    Permission::SystemMonitor, Permission::SystemLogs,
                    Permission::ApiKeyCreate, Permission::ApiKeyRevoke, Permission::ApiKeyList,
                ].into_iter().collect()
            }
            Role::Developer => {
                // 开发者创建和管理沙箱
                vec![
                    Permission::SandboxCreate, Permission::SandboxRead, Permission::SandboxUpdate, Permission::SandboxDelete, Permission::SandboxExecute,
                    Permission::FileUpload, Permission::FileDownload, Permission::FileDelete, Permission::FileList,
                    Permission::ApiKeyCreate, Permission::ApiKeyRevoke, Permission::ApiKeyList,
                ].into_iter().collect()
            }
            Role::User => {
                // 普通用户基础操作
                vec![
                    Permission::SandboxCreate, Permission::SandboxRead, Permission::SandboxExecute,
                    Permission::FileUpload, Permission::FileDownload, Permission::FileList,
                ].into_iter().collect()
            }
            Role::ReadOnly => {
                // 只读用户仅查看
                vec![
                    Permission::SandboxRead,
                    Permission::FileList,
                ].into_iter().collect()
            }
        }
    }

    /// 检查角色是否具有指定权限
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions().contains(permission)
    }
}

impl User {
    /// 检查用户是否具有指定权限
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.is_active && self.role.has_permission(permission)
    }

    /// 检查用户是否可以访问指定租户
    pub fn can_access_tenant(&self, tenant_id: &Uuid) -> bool {
        match self.role {
            Role::SuperAdmin => true,
            _ => self.tenant_id.as_ref() == Some(tenant_id),
        }
    }
}

impl ApiKey {
    /// 检查 API 密钥是否具有指定权限
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.is_active && 
        self.expires_at.map_or(true, |exp| exp > Utc::now()) &&
        self.permissions.contains(permission)
    }

    /// 检查 API 密钥是否已过期
    pub fn is_expired(&self) -> bool {
        self.expires_at.map_or(false, |exp| exp <= Utc::now())
    }

    /// 更新最后使用时间
    pub fn update_last_used(&mut self) {
        self.last_used = Some(Utc::now());
    }
}

impl Permission {
    /// 获取所有权限的集合
    pub fn all_permissions() -> HashSet<Permission> {
        vec![
            // 沙盒权限
            Permission::SandboxCreate,
            Permission::SandboxRead,
            Permission::SandboxUpdate,
            Permission::SandboxDelete,
            Permission::SandboxExecute,
            Permission::SandboxList,
            
            // 文件权限
            Permission::FileUpload,
            Permission::FileDownload,
            Permission::FileDelete,
            Permission::FileList,
            
            // 用户管理权限
            Permission::UserCreate,
            Permission::UserRead,
            Permission::UserUpdate,
            Permission::UserDelete,
            Permission::UserList,
            
            // 租户管理权限
            Permission::TenantCreate,
            Permission::TenantRead,
            Permission::TenantUpdate,
            Permission::TenantDelete,
            
            // 系统权限
            Permission::SystemConfig,
            Permission::SystemMonitor,
            Permission::SystemLogs,
            Permission::SystemMaintenance,
            
            // API 密钥权限
            Permission::ApiKeyCreate,
            Permission::ApiKeyRead,
            Permission::ApiKeyRevoke,
            Permission::ApiKeyList,
            
            // 审计权限
            Permission::AuditRead,
            Permission::AuditList,
        ].into_iter().collect()
    }
}