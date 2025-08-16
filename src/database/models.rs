use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
// Removed sqlx::FromRow since we're using SurrealDB now
use uuid::Uuid;

/// 数据库用户模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbUser {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub role: String, // Role enum as string
    pub is_active: bool,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
}

/// 数据库API密钥模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbApiKey {
    pub id: Uuid,
    pub key_hash: String,
    pub prefix: String,
    pub name: String,
    pub user_id: Uuid,
    pub permissions: serde_json::Value, // JSON array of permissions
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

use crate::audit::models::{AuditLog, AuditEventType, AuditSeverity, AuditResult};
use crate::auth::models::Role;

/// 数据库审计日志模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbAuditLog {
    pub id: Uuid,
    pub event_type: String,
    pub severity: String,
    pub result: String,
    pub timestamp: DateTime<Utc>,
    
    // 用户信息
    pub user_id: Option<Uuid>,
    pub username: Option<String>,
    pub user_role: Option<String>,
    pub tenant_id: Option<Uuid>,
    
    // 请求信息
    pub request_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub request_path: Option<String>,
    pub http_method: Option<String>,
    
    // 资源信息
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub resource_name: Option<String>,
    
    // 权限信息
    pub permission: Option<String>,
    pub old_role: Option<String>,
    pub new_role: Option<String>,
    
    // 事件详情
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    
    // 元数据
    pub session_id: Option<String>,
    pub correlation_id: Option<String>,
    pub tags: Option<serde_json::Value>,
}

/// 数据库沙盒模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSandbox {
    pub id: Uuid,
    pub name: String,
    pub runtime_type: String, // docker or firecracker
    pub template: String,
    pub status: String,
    pub owner_id: Uuid,
    pub tenant_id: Option<Uuid>,
    
    // 资源限制
    pub cpu_limit: Option<i64>,
    pub memory_limit: Option<i64>,
    pub disk_limit: Option<i64>,
    
    // 运行时信息
    pub container_id: Option<String>,
    pub vm_id: Option<String>,
    pub ip_address: Option<String>,
    pub port_mappings: Option<serde_json::Value>,
    
    // 时间戳
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// 数据库会话模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub refresh_token_hash: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: DateTime<Utc>,
    pub is_active: bool,
}

/// 数据库租户模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTenant {
    pub id: Uuid,
    pub name: String,
    pub slug: String, // URL-friendly identifier
    pub description: Option<String>,
    pub settings: Option<serde_json::Value>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 数据库模板模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTemplate {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub runtime_type: String,
    pub base_image: String,
    pub default_command: Option<String>,
    pub environment_vars: Option<serde_json::Value>,
    pub resource_limits: Option<serde_json::Value>,
    pub is_public: bool,
    pub owner_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// 查询结果辅助结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
}

impl<T> PaginatedResult<T> {
    pub fn new(items: Vec<T>, total: i64, page: u32, page_size: u32) -> Self {
        let total_pages = ((total as f64) / (page_size as f64)).ceil() as u32;
        Self {
            items,
            total,
            page,
            page_size,
            total_pages,
        }
    }
}

// 转换实现
impl DbAuditLog {
    /// 从领域模型转换为数据库模型
    pub fn from_domain_model(log: &AuditLog) -> Self {
        DbAuditLog {
            id: log.id,
            event_type: format!("{:?}", log.event_type),
            severity: format!("{:?}", log.severity),
            result: format!("{:?}", log.result),
            timestamp: log.timestamp,
            user_id: log.user_id,
            username: log.username.clone(),
            user_role: log.user_role.as_ref().map(|r| format!("{:?}", r)),
            tenant_id: log.tenant_id,
            request_id: log.request_id.clone(),
            ip_address: log.ip_address.clone(),
            user_agent: log.user_agent.clone(),
            request_path: log.request_path.clone(),
            http_method: log.http_method.clone(),
            resource_type: log.resource_type.clone(),
            resource_id: log.resource_id.clone(),
            resource_name: log.resource_name.clone(),
            permission: log.permission.as_ref().map(|p| format!("{:?}", p)),
            old_role: log.old_role.as_ref().map(|r| format!("{:?}", r)),
            new_role: log.new_role.as_ref().map(|r| format!("{:?}", r)),
            message: log.message.clone(),
            details: log.details.as_ref().map(|d| serde_json::to_value(d).unwrap_or(serde_json::Value::Null)),
            error_code: log.error_code.clone(),
            error_message: log.error_message.clone(),
            session_id: log.session_id.clone(),
            correlation_id: log.correlation_id.clone(),
            tags: log.tags.as_ref().map(|t| serde_json::to_value(t).unwrap_or(serde_json::Value::Null)),
        }
    }
    
    /// 转换为领域模型
    pub fn to_domain_model(&self) -> Result<AuditLog, String> {
        use std::str::FromStr;
        
        // 解析枚举类型
        let event_type = parse_enum(&self.event_type, "AuditEventType")?;
        let severity = parse_enum(&self.severity, "AuditSeverity")?;
        let result = parse_enum(&self.result, "AuditResult")?;
        let user_role = self.user_role.as_ref()
            .map(|r| parse_enum::<Role>(r, "Role"))
            .transpose()?;
        let permission = self.permission.as_ref()
            .map(|p| parse_enum::<crate::auth::models::Permission>(p, "Permission"))
            .transpose()?;
        let old_role = self.old_role.as_ref()
            .map(|r| parse_enum::<Role>(r, "Role"))
            .transpose()?;
        let new_role = self.new_role.as_ref()
            .map(|r| parse_enum::<Role>(r, "Role"))
            .transpose()?;
        
        Ok(AuditLog {
            id: self.id,
            event_type,
            severity,
            result,
            timestamp: self.timestamp,
            user_id: self.user_id,
            username: self.username.clone(),
            user_role,
            tenant_id: self.tenant_id,
            request_id: self.request_id.clone(),
            ip_address: self.ip_address.clone(),
            user_agent: self.user_agent.clone(),
            request_path: self.request_path.clone(),
            http_method: self.http_method.clone(),
            resource_type: self.resource_type.clone(),
            resource_id: self.resource_id.clone(),
            resource_name: self.resource_name.clone(),
            permission,
            old_role,
            new_role,
            message: self.message.clone(),
            details: self.details.as_ref()
                .and_then(|d| d.as_object())
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
            error_code: self.error_code.clone(),
            error_message: self.error_message.clone(),
            session_id: self.session_id.clone(),
            correlation_id: self.correlation_id.clone(),
            tags: self.tags.as_ref()
                .and_then(|t| t.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
        })
    }
}

// 辅助函数：解析枚举字符串
fn parse_enum<T: std::str::FromStr>(s: &str, type_name: &str) -> Result<T, String> {
    s.parse::<T>()
        .map_err(|_| format!("Failed to parse {} from string: {}", type_name, s))
}