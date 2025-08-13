use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::auth::models::{Permission, Role};

/// 审计事件类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AuditEventType {
    // 认证事件
    UserLogin,
    UserLogout,
    UserLoginFailed,
    TokenRefresh,
    ApiKeyCreated,
    ApiKeyRevoked,
    
    // 权限事件
    PermissionGranted,
    PermissionDenied,
    RoleAssigned,
    RoleRevoked,
    
    // 沙盒事件
    SandboxCreated,
    SandboxStarted,
    SandboxStopped,
    SandboxDeleted,
    SandboxExecuted,
    
    // 文件事件
    FileUploaded,
    FileDownloaded,
    FileDeleted,
    
    // 系统事件
    SystemConfigChanged,
    SystemMonitor,
    MaintenanceStarted,
    MaintenanceCompleted,
    
    // 安全事件
    SecurityViolation,
    SuspiciousActivity,
    UnauthorizedAccess,
}

/// 审计事件严重程度
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AuditSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// 审计事件结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AuditResult {
    Success,
    Failure,
    Pending,
}

/// 审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: Uuid,
    pub event_type: AuditEventType,
    pub severity: AuditSeverity,
    pub result: AuditResult,
    pub timestamp: DateTime<Utc>,
    
    // 用户信息
    pub user_id: Option<Uuid>,
    pub username: Option<String>,
    pub user_role: Option<Role>,
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
    pub permission: Option<Permission>,
    pub old_role: Option<Role>,
    pub new_role: Option<Role>,
    
    // 事件详情
    pub message: String,
    pub details: Option<HashMap<String, serde_json::Value>>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    
    // 元数据
    pub session_id: Option<String>,
    pub correlation_id: Option<String>,
    pub tags: Option<Vec<String>>,
}

impl AuditLog {
    /// 创建新的审计日志条目
    pub fn new(
        event_type: AuditEventType,
        severity: AuditSeverity,
        result: AuditResult,
        message: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            severity,
            result,
            timestamp: Utc::now(),
            user_id: None,
            username: None,
            user_role: None,
            tenant_id: None,
            request_id: None,
            ip_address: None,
            user_agent: None,
            request_path: None,
            http_method: None,
            resource_type: None,
            resource_id: None,
            resource_name: None,
            permission: None,
            old_role: None,
            new_role: None,
            message,
            details: None,
            error_code: None,
            error_message: None,
            session_id: None,
            correlation_id: None,
            tags: None,
        }
    }
    
    /// 设置用户信息
    pub fn with_user(mut self, user_id: Uuid, username: String, role: Role, tenant_id: Option<Uuid>) -> Self {
        self.user_id = Some(user_id);
        self.username = Some(username);
        self.user_role = Some(role);
        self.tenant_id = tenant_id;
        self
    }
    
    /// 设置请求信息
    pub fn with_request(
        mut self,
        request_id: Option<String>,
        ip_address: Option<String>,
        user_agent: Option<String>,
        path: Option<String>,
        method: Option<String>,
    ) -> Self {
        self.request_id = request_id;
        self.ip_address = ip_address;
        self.user_agent = user_agent;
        self.request_path = path;
        self.http_method = method;
        self
    }
    
    /// 设置资源信息
    pub fn with_resource(
        mut self,
        resource_type: String,
        resource_id: Option<String>,
        resource_name: Option<String>,
    ) -> Self {
        self.resource_type = Some(resource_type);
        self.resource_id = resource_id;
        self.resource_name = resource_name;
        self
    }
    
    /// 设置权限信息
    pub fn with_permission(mut self, permission: Permission) -> Self {
        self.permission = Some(permission);
        self
    }
    
    /// 设置角色变更信息
    pub fn with_role_change(mut self, old_role: Option<Role>, new_role: Role) -> Self {
        self.old_role = old_role;
        self.new_role = Some(new_role);
        self
    }
    
    /// 设置错误信息
    pub fn with_error(mut self, error_code: String, error_message: String) -> Self {
        self.error_code = Some(error_code);
        self.error_message = Some(error_message);
        self
    }
    
    /// 设置详细信息
    pub fn with_details(mut self, details: HashMap<String, serde_json::Value>) -> Self {
        self.details = Some(details);
        self
    }
    
    /// 设置元数据
    pub fn with_metadata(
        mut self,
        session_id: Option<String>,
        correlation_id: Option<String>,
        tags: Option<Vec<String>>,
    ) -> Self {
        self.session_id = session_id;
        self.correlation_id = correlation_id;
        self.tags = tags;
        self
    }
    
    /// 检查是否为安全相关事件
    pub fn is_security_event(&self) -> bool {
        matches!(
            self.event_type,
            AuditEventType::SecurityViolation
                | AuditEventType::SuspiciousActivity
                | AuditEventType::UnauthorizedAccess
                | AuditEventType::UserLoginFailed
                | AuditEventType::PermissionDenied
        )
    }
    
    /// 检查是否为高严重程度事件
    pub fn is_high_severity(&self) -> bool {
        matches!(self.severity, AuditSeverity::Error | AuditSeverity::Critical)
    }
}

/// 审计查询参数
#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub event_type: Option<AuditEventType>,
    pub severity: Option<AuditSeverity>,
    pub result: Option<AuditResult>,
    pub user_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub order_by: Option<String>,
    pub order_desc: Option<bool>,
}

impl Default for AuditQuery {
    fn default() -> Self {
        Self {
            event_type: None,
            severity: None,
            result: None,
            user_id: None,
            tenant_id: None,
            resource_type: None,
            resource_id: None,
            start_time: None,
            end_time: None,
            page: Some(1),
            limit: Some(100),
            order_by: Some("timestamp".to_string()),
            order_desc: Some(true),
        }
    }
}

/// 审计统计信息
#[derive(Debug, Serialize)]
pub struct AuditStats {
    pub total_events: u64,
    pub events_by_type: HashMap<AuditEventType, u64>,
    pub events_by_severity: HashMap<AuditSeverity, u64>,
    pub events_by_result: HashMap<AuditResult, u64>,
    pub security_events: u64,
    pub failed_events: u64,
    pub unique_users: u64,
    pub unique_tenants: u64,
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

impl Default for AuditStats {
    fn default() -> Self {
        Self {
            total_events: 0,
            events_by_type: HashMap::new(),
            events_by_severity: HashMap::new(),
            events_by_result: HashMap::new(),
            security_events: 0,
            failed_events: 0,
            unique_users: 0,
            unique_tenants: 0,
            time_range: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_creation() {
        let log = AuditLog::new(
            AuditEventType::UserLogin,
            AuditSeverity::Info,
            AuditResult::Success,
            "User logged in successfully".to_string(),
        );

        assert_eq!(log.event_type, AuditEventType::UserLogin);
        assert_eq!(log.severity, AuditSeverity::Info);
        assert_eq!(log.result, AuditResult::Success);
        assert_eq!(log.message, "User logged in successfully");
        assert!(log.user_id.is_none());
    }

    #[test]
    fn test_audit_log_with_user() {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        
        let log = AuditLog::new(
            AuditEventType::UserLogin,
            AuditSeverity::Info,
            AuditResult::Success,
            "User logged in".to_string(),
        )
        .with_user(user_id, "testuser".to_string(), Role::Developer, Some(tenant_id));

        assert_eq!(log.user_id, Some(user_id));
        assert_eq!(log.username, Some("testuser".to_string()));
        assert_eq!(log.user_role, Some(Role::Developer));
        assert_eq!(log.tenant_id, Some(tenant_id));
    }

    #[test]
    fn test_security_event_detection() {
        let security_log = AuditLog::new(
            AuditEventType::SecurityViolation,
            AuditSeverity::Critical,
            AuditResult::Failure,
            "Security violation detected".to_string(),
        );

        assert!(security_log.is_security_event());
        assert!(security_log.is_high_severity());

        let normal_log = AuditLog::new(
            AuditEventType::SandboxCreated,
            AuditSeverity::Info,
            AuditResult::Success,
            "Sandbox created".to_string(),
        );

        assert!(!normal_log.is_security_event());
        assert!(!normal_log.is_high_severity());
    }
}