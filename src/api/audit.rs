use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::audit::{
    models::{AuditEventType, AuditQuery, AuditResult, AuditSeverity, AuditStats},
    service::AuditService,
};
use crate::auth::{
    middleware::{AuthExtractor, AuthMiddleware},
    models::Permission,
};

/// 审计日志状态
#[derive(Clone)]
pub struct AuditState {
    pub audit_service: Arc<AuditService>,
}

impl AuditState {
    pub fn new(audit_service: Arc<AuditService>) -> Self {
        Self { audit_service }
    }
}

/// 审计日志查询参数
#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    pub event_type: Option<String>,
    pub severity: Option<String>,
    pub result: Option<String>,
    pub user_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub order_by: Option<String>,
    pub order_desc: Option<bool>,
}

impl TryFrom<AuditQueryParams> for AuditQuery {
    type Error = String;

    fn try_from(params: AuditQueryParams) -> Result<Self, Self::Error> {
        let event_type = if let Some(event_str) = params.event_type {
            Some(parse_event_type(&event_str)?)
        } else {
            None
        };

        let severity = if let Some(severity_str) = params.severity {
            Some(parse_severity(&severity_str)?)
        } else {
            None
        };

        let result = if let Some(result_str) = params.result {
            Some(parse_result(&result_str)?)
        } else {
            None
        };

        let start_time = if let Some(time_str) = params.start_time {
            Some(
                chrono::DateTime::parse_from_rfc3339(&time_str)
                    .map_err(|e| format!("Invalid start_time format: {}", e))?
                    .with_timezone(&chrono::Utc),
            )
        } else {
            None
        };

        let end_time = if let Some(time_str) = params.end_time {
            Some(
                chrono::DateTime::parse_from_rfc3339(&time_str)
                    .map_err(|e| format!("Invalid end_time format: {}", e))?
                    .with_timezone(&chrono::Utc),
            )
        } else {
            None
        };

        Ok(AuditQuery {
            event_type,
            severity,
            result,
            user_id: params.user_id,
            tenant_id: params.tenant_id,
            resource_type: params.resource_type,
            resource_id: params.resource_id,
            start_time,
            end_time,
            page: params.page,
            limit: params.limit,
            order_by: params.order_by,
            order_desc: params.order_desc,
        })
    }
}

/// 创建审计日志路由
pub fn audit_routes<S>(audit_state: AuditState) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        // 基础审计日志查询（需要审计读取权限）
        .route("/audit/logs", get(list_audit_logs))
        .route("/audit/stats", get(get_audit_stats))
        // 管理员审计功能（需要审计管理权限）
        .route("/audit/users/:user_id/logs", get(get_user_audit_logs))
        .route("/audit/security-events", get(list_security_events))
        .layer(axum::middleware::from_fn(
            AuthMiddleware::require_permission(Permission::AuditRead),
        ))
        .with_state(audit_state)
}

/// 列出审计日志
async fn list_audit_logs(
    State(audit_state): State<AuditState>,
    auth: AuthExtractor,
    Query(params): Query<AuditQueryParams>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} querying audit logs", auth.0.username);

    // 转换查询参数
    let mut query: AuditQuery = params.try_into().map_err(|e: String| {
        warn!("Invalid query parameters: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    // 如果用户不是超级管理员，只能查看自己租户的日志
    if !auth.0.has_permission(&Permission::SystemConfig) {
        if let Some(tenant_id) = auth.0.tenant_id {
            query.tenant_id = Some(tenant_id);
        }
    }

    // 查询审计日志
    let logs = audit_state.audit_service.query(query).await.map_err(|e| {
        warn!("Failed to query audit logs: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({
        "logs": logs,
        "total": logs.len(),
        "queried_by": auth.0.username,
        "tenant_id": auth.0.tenant_id
    })))
}

/// 获取审计统计信息
async fn get_audit_stats(
    State(audit_state): State<AuditState>,
    auth: AuthExtractor,
    Query(params): Query<AuditQueryParams>,
) -> Result<Json<AuditStats>, StatusCode> {
    info!("User {} requesting audit statistics", auth.0.username);

    // 转换查询参数
    let mut query: AuditQuery = params.try_into().map_err(|e: String| {
        warn!("Invalid query parameters: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    // 租户过滤
    if !auth.0.has_permission(&Permission::SystemConfig) {
        if let Some(tenant_id) = auth.0.tenant_id {
            query.tenant_id = Some(tenant_id);
        }
    }

    // 获取统计信息
    let stats = audit_state
        .audit_service
        .get_stats(Some(query))
        .map_err(|e| {
            warn!("Failed to get audit stats: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(stats))
}

/// 获取特定用户的审计日志
async fn get_user_audit_logs(
    State(audit_state): State<AuditState>,
    auth: AuthExtractor,
    axum::extract::Path(user_id): axum::extract::Path<Uuid>,
    Query(params): Query<AuditQueryParams>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} querying audit logs for user {}", auth.0.username, user_id);

    // 检查权限：只有管理员或用户本人可以查看
    if user_id != auth.0.user_id && !auth.0.has_permission(&Permission::UserRead) {
        warn!(
            "User {} attempted to view audit logs for user {} without permission",
            auth.0.username, user_id
        );
        return Err(StatusCode::FORBIDDEN);
    }

    // 转换查询参数并设置用户过滤
    let mut query: AuditQuery = params.try_into().map_err(|e: String| {
        warn!("Invalid query parameters: {}", e);
        StatusCode::BAD_REQUEST
    })?;
    query.user_id = Some(user_id);

    // 租户过滤
    if !auth.0.has_permission(&Permission::SystemConfig) {
        if let Some(tenant_id) = auth.0.tenant_id {
            query.tenant_id = Some(tenant_id);
        }
    }

    // 查询审计日志
    let logs = audit_state.audit_service.query(query).await.map_err(|e| {
        warn!("Failed to query user audit logs: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({
        "user_id": user_id,
        "logs": logs,
        "total": logs.len(),
        "queried_by": auth.0.username
    })))
}

/// 列出安全事件
async fn list_security_events(
    State(audit_state): State<AuditState>,
    auth: AuthExtractor,
    Query(params): Query<AuditQueryParams>,
) -> Result<Json<Value>, StatusCode> {
    info!("User {} querying security events", auth.0.username);

    // 转换查询参数
    let mut query: AuditQuery = params.try_into().map_err(|e: String| {
        warn!("Invalid query parameters: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    // 租户过滤
    if !auth.0.has_permission(&Permission::SystemConfig) {
        if let Some(tenant_id) = auth.0.tenant_id {
            query.tenant_id = Some(tenant_id);
        }
    }

    // 查询所有日志并过滤安全事件
    let all_logs = audit_state.audit_service.query(query).await.map_err(|e| {
        warn!("Failed to query audit logs: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let security_logs: Vec<_> = all_logs
        .into_iter()
        .filter(|log| log.is_security_event())
        .collect();

    Ok(Json(json!({
        "security_events": security_logs,
        "total": security_logs.len(),
        "queried_by": auth.0.username,
        "description": "安全相关事件列表"
    })))
}

/// 解析事件类型字符串
fn parse_event_type(event_str: &str) -> Result<AuditEventType, String> {
    match event_str.to_lowercase().as_str() {
        "userlogin" => Ok(AuditEventType::UserLogin),
        "userlogout" => Ok(AuditEventType::UserLogout),
        "userloginfailed" => Ok(AuditEventType::UserLoginFailed),
        "tokenrefresh" => Ok(AuditEventType::TokenRefresh),
        "apikeycreated" => Ok(AuditEventType::ApiKeyCreated),
        "apikeyrevoked" => Ok(AuditEventType::ApiKeyRevoked),
        "permissiongranted" => Ok(AuditEventType::PermissionGranted),
        "permissiondenied" => Ok(AuditEventType::PermissionDenied),
        "roleassigned" => Ok(AuditEventType::RoleAssigned),
        "rolerevoked" => Ok(AuditEventType::RoleRevoked),
        "sandboxcreated" => Ok(AuditEventType::SandboxCreated),
        "sandboxstarted" => Ok(AuditEventType::SandboxStarted),
        "sandboxstopped" => Ok(AuditEventType::SandboxStopped),
        "sandboxdeleted" => Ok(AuditEventType::SandboxDeleted),
        "sandboxexecuted" => Ok(AuditEventType::SandboxExecuted),
        "fileuploaded" => Ok(AuditEventType::FileUploaded),
        "filedownloaded" => Ok(AuditEventType::FileDownloaded),
        "filedeleted" => Ok(AuditEventType::FileDeleted),
        "systemconfigchanged" => Ok(AuditEventType::SystemConfigChanged),
        "maintenancestarted" => Ok(AuditEventType::MaintenanceStarted),
        "maintenancecompleted" => Ok(AuditEventType::MaintenanceCompleted),
        "securityviolation" => Ok(AuditEventType::SecurityViolation),
        "suspiciousactivity" => Ok(AuditEventType::SuspiciousActivity),
        "unauthorizedaccess" => Ok(AuditEventType::UnauthorizedAccess),
        _ => Err(format!("Unknown event type: {}", event_str)),
    }
}

/// 解析严重程度字符串
fn parse_severity(severity_str: &str) -> Result<AuditSeverity, String> {
    match severity_str.to_lowercase().as_str() {
        "info" => Ok(AuditSeverity::Info),
        "warning" => Ok(AuditSeverity::Warning),
        "error" => Ok(AuditSeverity::Error),
        "critical" => Ok(AuditSeverity::Critical),
        _ => Err(format!("Unknown severity: {}", severity_str)),
    }
}

/// 解析结果字符串
fn parse_result(result_str: &str) -> Result<AuditResult, String> {
    match result_str.to_lowercase().as_str() {
        "success" => Ok(AuditResult::Success),
        "failure" => Ok(AuditResult::Failure),
        "pending" => Ok(AuditResult::Pending),
        _ => Err(format!("Unknown result: {}", result_str)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::service::AuditConfig;

    #[test]
    fn test_parse_event_type() {
        assert!(matches!(
            parse_event_type("userlogin"),
            Ok(AuditEventType::UserLogin)
        ));
        assert!(matches!(
            parse_event_type("SANDBOXCREATED"),
            Ok(AuditEventType::SandboxCreated)
        ));
        assert!(parse_event_type("invalid").is_err());
    }

    #[test]
    fn test_parse_severity() {
        assert!(matches!(parse_severity("info"), Ok(AuditSeverity::Info)));
        assert!(matches!(
            parse_severity("CRITICAL"),
            Ok(AuditSeverity::Critical)
        ));
        assert!(parse_severity("invalid").is_err());
    }

    #[test]
    fn test_parse_result() {
        assert!(matches!(parse_result("success"), Ok(AuditResult::Success)));
        assert!(matches!(parse_result("FAILURE"), Ok(AuditResult::Failure)));
        assert!(parse_result("invalid").is_err());
    }

    #[tokio::test]
    async fn test_audit_state_creation() {
        let audit_service = AuditService::new(AuditConfig::default()).unwrap();
        let audit_state = AuditState::new(audit_service);
        assert!(std::ptr::addr_of!(audit_state.audit_service).is_aligned());
    }
}