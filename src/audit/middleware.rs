use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::audit::{
    models::{AuditEventType, AuditLog, AuditResult, AuditSeverity},
    service::AuditService,
};
use crate::auth::middleware::AuthExtractor;

/// 审计中间件状态
#[derive(Clone)]
pub struct AuditMiddleware {
    audit_service: Arc<AuditService>,
}

impl AuditMiddleware {
    pub fn new(audit_service: Arc<AuditService>) -> Self {
        Self { audit_service }
    }

    /// HTTP 请求审计中间件
    pub async fn audit_request(
        State(audit_middleware): State<AuditMiddleware>,
        auth: Option<AuthExtractor>,
        request: Request<Body>,
        next: Next,
    ) -> Response {
        let start_time = std::time::Instant::now();
        let method = request.method().to_string();
        let path = request.uri().path().to_string();
        let query = request.uri().query().map(|q| q.to_string());
        let headers = request.headers().clone();
        
        // 提取客户端信息
        let ip_address = extract_client_ip(&headers);
        let user_agent = headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // 生成请求ID
        let request_id = Uuid::new_v4().to_string();

        debug!("审计中间件处理请求: {} {} (ID: {})", method, path, request_id);

        // 执行请求
        let response = next.run(request).await;
        let status = response.status();
        let duration = start_time.elapsed();

        // 记录审计日志
        tokio::spawn(async move {
            if let Err(e) = audit_middleware
                .log_http_request(
                    auth,
                    &method,
                    &path,
                    query.as_deref(),
                    status,
                    duration,
                    ip_address,
                    user_agent,
                    request_id,
                )
                .await
            {
                warn!("记录HTTP请求审计日志失败: {}", e);
            }
        });

        response
    }

    /// 记录HTTP请求审计日志
    async fn log_http_request(
        &self,
        auth: Option<AuthExtractor>,
        method: &str,
        path: &str,
        query: Option<&str>,
        status: StatusCode,
        duration: std::time::Duration,
        ip_address: Option<String>,
        user_agent: Option<String>,
        request_id: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 根据状态码和路径判断事件类型和严重程度
        let (event_type, severity, result) = classify_http_request(method, path, status);

        // 构建消息
        let full_path = if let Some(q) = query {
            format!("{}?{}", path, q)
        } else {
            path.to_string()
        };

        let message = format!(
            "HTTP {} {} - {} ({:.2}ms)",
            method,
            full_path,
            status.as_u16(),
            duration.as_millis() as f64
        );

        // 创建审计日志
        let mut log = AuditLog::new(event_type, severity, result, message)
            .with_request(
                Some(request_id),
                ip_address,
                user_agent,
                Some(path.to_string()),
                Some(method.to_string()),
            );

        // 添加用户信息（如果有认证）
        if let Some(auth_info) = auth {
            log = log.with_user(
                auth_info.0.user_id,
                auth_info.0.username,
                auth_info.0.role,
                auth_info.0.tenant_id,
            );
        }

        // 添加详细信息
        let mut details = std::collections::HashMap::new();
        details.insert(
            "status_code".to_string(),
            serde_json::Value::Number(serde_json::Number::from(status.as_u16())),
        );
        details.insert(
            "duration_ms".to_string(),
            serde_json::Value::Number(serde_json::Number::from(duration.as_millis() as u64)),
        );
        if let Some(q) = query {
            details.insert("query_string".to_string(), serde_json::Value::String(q.to_string()));
        }
        log = log.with_details(details);

        // 异步记录日志
        self.audit_service.log_async(log)?;

        Ok(())
    }

    /// 记录认证事件
    pub async fn log_auth_event(
        &self,
        event_type: AuditEventType,
        user_id: Option<Uuid>,
        username: Option<String>,
        success: bool,
        error_message: Option<String>,
        ip_address: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (severity, result) = if success {
            (AuditSeverity::Info, AuditResult::Success)
        } else {
            (AuditSeverity::Warning, AuditResult::Failure)
        };

        let message = match event_type {
            AuditEventType::UserLogin => {
                if success {
                    format!("用户 {} 登录成功", username.as_deref().unwrap_or("未知"))
                } else {
                    format!("用户 {} 登录失败", username.as_deref().unwrap_or("未知"))
                }
            }
            AuditEventType::UserLogout => {
                format!("用户 {} 注销", username.as_deref().unwrap_or("未知"))
            }
            AuditEventType::TokenRefresh => {
                if success {
                    format!("用户 {} 刷新令牌成功", username.as_deref().unwrap_or("未知"))
                } else {
                    format!("用户 {} 刷新令牌失败", username.as_deref().unwrap_or("未知"))
                }
            }
            _ => format!("认证事件: {:?}", event_type),
        };

        let mut log = AuditLog::new(event_type, severity, result, message)
            .with_request(None, ip_address, user_agent, None, None);

        if let (Some(uid), Some(uname)) = (user_id, username) {
            log = log.with_user(uid, uname, crate::auth::models::Role::User, None);
        }

        if let Some(error) = error_message {
            log = log.with_error("AUTH_ERROR".to_string(), error);
        }

        self.audit_service.log_async(log)?;
        Ok(())
    }

    /// 记录权限检查事件
    pub async fn log_permission_event(
        &self,
        user_id: Uuid,
        username: String,
        permission: crate::auth::models::Permission,
        resource_type: String,
        resource_id: Option<String>,
        granted: bool,
        request_path: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (event_type, severity, result) = if granted {
            (
                AuditEventType::PermissionGranted,
                AuditSeverity::Info,
                AuditResult::Success,
            )
        } else {
            (
                AuditEventType::PermissionDenied,
                AuditSeverity::Warning,
                AuditResult::Failure,
            )
        };

        let message = if granted {
            format!("用户 {} 被授予权限 {:?}", username, permission)
        } else {
            format!("用户 {} 被拒绝权限 {:?}", username, permission)
        };

        let log = AuditLog::new(event_type, severity, result, message)
            .with_user(user_id, username, crate::auth::models::Role::User, None)
            .with_permission(permission)
            .with_resource(resource_type, resource_id, None)
            .with_request(None, None, None, request_path, None);

        self.audit_service.log_async(log)?;
        Ok(())
    }

    /// 记录系统事件
    pub async fn log_system_event(
        &self,
        event_type: AuditEventType,
        message: String,
        severity: AuditSeverity,
        details: Option<std::collections::HashMap<String, serde_json::Value>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut log = AuditLog::new(event_type, severity, AuditResult::Success, message);

        if let Some(d) = details {
            log = log.with_details(d);
        }

        self.audit_service.log_async(log)?;
        Ok(())
    }
}

/// 根据HTTP请求分类审计事件
fn classify_http_request(
    method: &str,
    path: &str,
    status: StatusCode,
) -> (AuditEventType, AuditSeverity, AuditResult) {
    let is_success = status.is_success();
    let is_client_error = status.is_client_error();
    let is_server_error = status.is_server_error();

    // 根据路径判断事件类型
    let event_type = if path.contains("/auth/login") {
        if is_success {
            AuditEventType::UserLogin
        } else {
            AuditEventType::UserLoginFailed
        }
    } else if path.contains("/auth/logout") {
        AuditEventType::UserLogout
    } else if path.contains("/sandboxes") {
        match method {
            "POST" => AuditEventType::SandboxCreated,
            "DELETE" => AuditEventType::SandboxDeleted,
            _ => AuditEventType::SandboxExecuted, // 默认为执行操作
        }
    } else if path.contains("/permissions") {
        if is_success {
            AuditEventType::PermissionGranted
        } else {
            AuditEventType::PermissionDenied
        }
    } else if path.contains("/files") {
        match method {
            "POST" => AuditEventType::FileUploaded,
            "GET" => AuditEventType::FileDownloaded,
            "DELETE" => AuditEventType::FileDeleted,
            _ => AuditEventType::FileDownloaded,
        }
    } else {
        // 根据状态码判断是否为安全事件
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            AuditEventType::UnauthorizedAccess
        } else {
            AuditEventType::SystemMonitor // 通用系统事件
        }
    };

    // 根据状态码判断严重程度和结果
    let (severity, result) = if is_success {
        (AuditSeverity::Info, AuditResult::Success)
    } else if is_client_error {
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                (AuditSeverity::Warning, AuditResult::Failure)
            }
            StatusCode::TOO_MANY_REQUESTS => (AuditSeverity::Warning, AuditResult::Failure),
            _ => (AuditSeverity::Info, AuditResult::Failure),
        }
    } else if is_server_error {
        (AuditSeverity::Error, AuditResult::Failure)
    } else {
        (AuditSeverity::Info, AuditResult::Pending)
    };

    (event_type, severity, result)
}

/// 提取客户端IP地址
fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    // 尝试多个可能的头部字段
    let ip_headers = [
        "x-forwarded-for",
        "x-real-ip",
        "x-client-ip",
        "cf-connecting-ip",
        "x-forwarded",
        "forwarded-for",
        "forwarded",
    ];

    for header_name in &ip_headers {
        if let Some(header_value) = headers.get(*header_name) {
            if let Ok(ip_str) = header_value.to_str() {
                // x-forwarded-for 可能包含多个IP，取第一个
                let first_ip = ip_str.split(',').next().unwrap_or("").trim();
                if !first_ip.is_empty() && first_ip != "unknown" {
                    return Some(first_ip.to_string());
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::service::AuditConfig;

    #[tokio::test]
    async fn test_audit_middleware_creation() {
        let audit_service = AuditService::new(AuditConfig::default()).unwrap();
        let middleware = AuditMiddleware::new(audit_service);
        
        // 测试中间件创建成功
        assert!(std::ptr::addr_of!(middleware.audit_service).is_aligned());
    }

    #[test]
    fn test_classify_http_request() {
        // 测试成功的登录请求
        let (event_type, severity, result) = classify_http_request(
            "POST",
            "/auth/login",
            StatusCode::OK,
        );
        assert_eq!(event_type, AuditEventType::UserLogin);
        assert_eq!(severity, AuditSeverity::Info);
        assert_eq!(result, AuditResult::Success);

        // 测试失败的登录请求
        let (event_type, severity, result) = classify_http_request(
            "POST",
            "/auth/login",
            StatusCode::UNAUTHORIZED,
        );
        assert_eq!(event_type, AuditEventType::UserLoginFailed);
        assert_eq!(severity, AuditSeverity::Warning);
        assert_eq!(result, AuditResult::Failure);

        // 测试未授权访问
        let (event_type, severity, result) = classify_http_request(
            "GET",
            "/api/v1/admin",
            StatusCode::FORBIDDEN,
        );
        assert_eq!(event_type, AuditEventType::UnauthorizedAccess);
        assert_eq!(severity, AuditSeverity::Warning);
        assert_eq!(result, AuditResult::Failure);
    }

    #[test]
    fn test_extract_client_ip() {
        let mut headers = HeaderMap::new();
        
        // 测试 x-forwarded-for
        headers.insert("x-forwarded-for", "192.168.1.1, 10.0.0.1".parse().unwrap());
        let ip = extract_client_ip(&headers);
        assert_eq!(ip, Some("192.168.1.1".to_string()));

        // 测试 x-real-ip
        headers.clear();
        headers.insert("x-real-ip", "192.168.1.2".parse().unwrap());
        let ip = extract_client_ip(&headers);
        assert_eq!(ip, Some("192.168.1.2".to_string()));

        // 测试无IP头部
        headers.clear();
        let ip = extract_client_ip(&headers);
        assert_eq!(ip, None);
    }
}