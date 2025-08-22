use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::audit::models::{AuditLog, AuditQuery, AuditStats, AuditEventType, AuditSeverity, AuditResult};
use crate::error::{Result as SoulBoxResult, SoulBoxError};
use crate::database::SurrealPool;
// Temporarily disabled: repositories::AuditRepository

/// 审计日志服务配置
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// 内存中保存的最大日志条数
    pub max_memory_logs: usize,
    /// 异步处理队列大小
    pub async_queue_size: usize,
    /// 是否启用安全事件实时告警
    pub enable_security_alerts: bool,
    /// 是否启用详细日志记录
    pub enable_detailed_logging: bool,
    /// 日志轮转大小（条数）
    pub log_rotation_size: usize,
    /// 是否启用数据库持久化
    pub enable_persistence: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            max_memory_logs: 10000,
            async_queue_size: 1000,
            enable_security_alerts: true,
            enable_detailed_logging: true,
            log_rotation_size: 50000,
            enable_persistence: true,
        }
    }
}

/// 审计日志存储（目前使用内存存储，未来可扩展到数据库）
#[derive(Debug)]
struct AuditStorage {
    logs: VecDeque<AuditLog>,
    max_size: usize,
}

impl AuditStorage {
    fn new(max_size: usize) -> Self {
        Self {
            logs: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    fn add_log(&mut self, log: AuditLog) {
        if self.logs.len() >= self.max_size {
            self.logs.pop_front();
        }
        self.logs.push_back(log);
    }

    fn query_logs(&self, query: &AuditQuery) -> Vec<&AuditLog> {
        let mut results: Vec<&AuditLog> = self.logs
            .iter()
            .filter(|log| self.matches_query(log, query))
            .collect();

        // 排序
        let desc = query.order_desc.unwrap_or(true);
        results.sort_by(|a, b| {
            if desc {
                b.timestamp.cmp(&a.timestamp)
            } else {
                a.timestamp.cmp(&b.timestamp)
            }
        });

        // 分页
        let page = query.page.unwrap_or(1);
        let limit = query.limit.unwrap_or(100);
        let start = ((page - 1) * limit) as usize;
        let end = (start + limit as usize).min(results.len());

        if start < results.len() {
            results[start..end].to_vec()
        } else {
            Vec::new()
        }
    }

    fn matches_query(&self, log: &AuditLog, query: &AuditQuery) -> bool {
        // 事件类型过滤
        if let Some(ref event_type) = query.event_type {
            if &log.event_type != event_type {
                return false;
            }
        }

        // 严重程度过滤
        if let Some(ref severity) = query.severity {
            if &log.severity != severity {
                return false;
            }
        }

        // 结果过滤
        if let Some(ref result) = query.result {
            if &log.result != result {
                return false;
            }
        }

        // 用户过滤
        if let Some(user_id) = query.user_id {
            if log.user_id != Some(user_id) {
                return false;
            }
        }

        // 租户过滤
        if let Some(tenant_id) = query.tenant_id {
            if log.tenant_id != Some(tenant_id) {
                return false;
            }
        }

        // 资源类型过滤
        if let Some(ref resource_type) = query.resource_type {
            if log.resource_type.as_ref() != Some(resource_type) {
                return false;
            }
        }

        // 资源ID过滤
        if let Some(ref resource_id) = query.resource_id {
            if log.resource_id.as_ref() != Some(resource_id) {
                return false;
            }
        }

        // 时间范围过滤
        if let Some(start_time) = query.start_time {
            if log.timestamp < start_time {
                return false;
            }
        }

        if let Some(end_time) = query.end_time {
            if log.timestamp > end_time {
                return false;
            }
        }

        true
    }

    fn get_stats(&self, query: &AuditQuery) -> AuditStats {
        let logs: Vec<&AuditLog> = self.logs
            .iter()
            .filter(|log| self.matches_query(log, query))
            .collect();

        let mut stats = AuditStats::default();
        stats.total_events = logs.len() as u64;

        // 统计按类型分组
        for log in &logs {
            *stats.events_by_type.entry(log.event_type.clone()).or_insert(0) += 1;
            *stats.events_by_severity.entry(log.severity.clone()).or_insert(0) += 1;
            *stats.events_by_result.entry(log.result.clone()).or_insert(0) += 1;

            if log.is_security_event() {
                stats.security_events += 1;
            }

            if log.result == AuditResult::Failure {
                stats.failed_events += 1;
            }
        }

        // 统计唯一用户和租户
        let unique_users: std::collections::HashSet<_> = logs
            .iter()
            .filter_map(|log| log.user_id)
            .collect();
        stats.unique_users = unique_users.len() as u64;

        let unique_tenants: std::collections::HashSet<_> = logs
            .iter()
            .filter_map(|log| log.tenant_id)
            .collect();
        stats.unique_tenants = unique_tenants.len() as u64;

        // 时间范围
        if let (Some(first), Some(last)) = (logs.first(), logs.last()) {
            stats.time_range = Some((first.timestamp, last.timestamp));
        }

        stats
    }
}

/// 审计日志服务
pub struct AuditService {
    storage: Arc<RwLock<AuditStorage>>,
    config: AuditConfig,
    sender: mpsc::UnboundedSender<AuditLog>,
    // Temporarily disabled: repository: Option<Arc<AuditRepository>>,
}

impl AuditService {
    /// 创建新的审计服务
    pub fn new(config: AuditConfig) -> SoulBoxResult<Arc<Self>> {
        let storage = Arc::new(RwLock::new(AuditStorage::new(config.max_memory_logs)));
        let (sender, receiver) = mpsc::unbounded_channel();

        let service = Arc::new(Self {
            storage: storage.clone(),
            config: config.clone(),
            sender,
            // Temporarily disabled: repository field
        });

        // 启动异步日志处理任务
        let storage_clone = storage.clone();
        let config_clone = config.clone();
        tokio::spawn(async move {
            Self::log_processing_task(receiver, storage_clone, config_clone).await;
        });

        info!("审计日志服务启动成功，配置: {:?}", config);
        Ok(service)
    }
    
    /// 创建带数据库支持的审计服务
    pub fn with_database(config: AuditConfig, database: Arc<SurrealPool>) -> SoulBoxResult<Arc<Self>> {
        let storage = Arc::new(RwLock::new(AuditStorage::new(config.max_memory_logs)));
        let (sender, receiver) = mpsc::unbounded_channel();
        // Temporarily disabled: let repository = Arc::new(AuditRepository::new(database));

        let service = Arc::new(Self {
            storage: storage.clone(),
            config: config.clone(),
            sender,
            // Temporarily disabled: repository: Some(repository.clone()),
        });

        // 启动异步日志处理任务
        let storage_clone = storage.clone();
        let config_clone = config.clone();
        // Temporarily disabled: let repo_clone = Some(repository);
        // let repo_clone: Option<Arc<AuditRepository>> = None;
        tokio::spawn(async move {
            Self::log_processing_task(receiver, storage_clone, config_clone).await;
        });

        info!("审计日志服务启动成功（带数据库支持），配置: {:?}", config);
        Ok(service)
    }

    /// 记录审计日志（异步）
    pub fn log_async(&self, log: AuditLog) -> SoulBoxResult<()> {
        if log.is_security_event() && self.config.enable_security_alerts {
            warn!("🚨 安全事件检测: {:?} - {}", log.event_type, log.message);
        }

        self.sender.send(log).map_err(|e| {
            SoulBoxError::Internal(format!("Failed to send audit log: {}", e))
        })?;

        Ok(())
    }

    /// 记录审计日志（同步）
    pub fn log_sync(&self, log: AuditLog) -> SoulBoxResult<()> {
        if log.is_security_event() && self.config.enable_security_alerts {
            warn!("🚨 安全事件检测: {:?} - {}", log.event_type, log.message);
        }

        let mut storage = self.storage.write().map_err(|e| {
            SoulBoxError::Internal(format!("Failed to acquire write lock: {}", e))
        })?;

        storage.add_log(log);
        Ok(())
    }

    /// 查询审计日志
    pub async fn query(&self, query: AuditQuery) -> SoulBoxResult<Vec<AuditLog>> {
        // Temporarily disabled: database query functionality
        if self.config.enable_persistence {
            debug!("Database persistence is temporarily disabled");
        }
        
        // 否则从内存查询
        let storage = self.storage.read().map_err(|e| {
            SoulBoxError::Internal(format!("Failed to acquire read lock: {}", e))
        })?;

        let logs = storage.query_logs(&query);
        Ok(logs.into_iter().cloned().collect())
    }

    /// 获取审计统计信息
    pub fn get_stats(&self, query: Option<AuditQuery>) -> SoulBoxResult<AuditStats> {
        let storage = self.storage.read().map_err(|e| {
            SoulBoxError::Internal(format!("Failed to acquire read lock: {}", e))
        })?;

        let query = query.unwrap_or_default();
        Ok(storage.get_stats(&query))
    }

    /// 清理旧日志
    pub fn cleanup_old_logs(&self, keep_count: usize) -> SoulBoxResult<u64> {
        let mut storage = self.storage.write().map_err(|e| {
            SoulBoxError::Internal(format!("Failed to acquire write lock: {}", e))
        })?;

        let original_count = storage.logs.len();
        while storage.logs.len() > keep_count {
            storage.logs.pop_front();
        }

        let removed_count = original_count - storage.logs.len();
        info!("清理了 {} 条旧审计日志", removed_count);
        Ok(removed_count as u64)
    }

    /// 异步日志处理任务
    async fn log_processing_task(
        mut receiver: mpsc::UnboundedReceiver<AuditLog>,
        storage: Arc<RwLock<AuditStorage>>,
        config: AuditConfig,
        // Temporarily disabled: repository: Option<Arc<AuditRepository>>,
    ) {
        info!("审计日志处理任务启动");

        while let Some(log) = receiver.recv().await {
            if config.enable_detailed_logging {
                debug!("处理审计日志: {:?}", log);
            }

            // 写入内存存储
            if let Ok(mut storage) = storage.write() {
                storage.add_log(log.clone());
            } else {
                error!("无法获取存储写锁，跳过日志: {}", log.message);
                continue;
            }

            // 写入数据库（如果启用持久化）
            if config.enable_persistence {
                // Temporarily disabled: repository functionality
                // if let Some(ref repo) = repository {
                //     if let Err(e) = repo.create(&log).await {
                //         error!("写入审计日志到数据库失败: {}", e);
                //     }
                // }
                debug!("Persistence is disabled in current build");
            }

            // 处理高严重程度事件
            if log.is_high_severity() {
                warn!("⚠️ 高严重程度事件: {:?} - {}", log.event_type, log.message);
            }

            // 发送到外部日志系统
            if let Err(e) = self.send_to_external_log_systems(&log).await {
                error!("Failed to send audit log to external systems: {}", e);
            }
            
            // 发送告警通知（针对高严重程度事件）
            if log.is_high_severity() {
                if let Err(e) = self.send_alert_notification(&log).await {
                    error!("Failed to send alert notification: {}", e);
                }
            }
            
            // 触发自动化响应（可选）
            if let Err(e) = self.trigger_automated_response(&log).await {
                error!("Failed to trigger automated response: {}", e);
            }
        }

        warn!("审计日志处理任务结束");
    }

    /// 便捷方法：记录用户登录事件
    pub fn log_user_login(
        &self,
        user_id: uuid::Uuid,
        username: String,
        role: crate::auth::models::Role,
        tenant_id: Option<uuid::Uuid>,
        ip_address: Option<String>,
        user_agent: Option<String>,
        success: bool,
    ) -> SoulBoxResult<()> {
        let (event_type, severity, result, message) = if success {
            (
                AuditEventType::UserLogin,
                AuditSeverity::Info,
                AuditResult::Success,
                format!("用户 {} 登录成功", username),
            )
        } else {
            (
                AuditEventType::UserLoginFailed,
                AuditSeverity::Warning,
                AuditResult::Failure,
                format!("用户 {} 登录失败", username),
            )
        };

        let log = AuditLog::new(event_type, severity, result, message)
            .with_user(user_id, username, role, tenant_id)
            .with_request(None, ip_address, user_agent, None, None);

        self.log_async(log)
    }

    /// 便捷方法：记录权限检查事件
    pub fn log_permission_check(
        &self,
        user_id: uuid::Uuid,
        username: String,
        permission: crate::auth::models::Permission,
        resource_type: String,
        resource_id: Option<String>,
        granted: bool,
    ) -> SoulBoxResult<()> {
        let (event_type, severity, result, message) = if granted {
            (
                AuditEventType::PermissionGranted,
                AuditSeverity::Info,
                AuditResult::Success,
                format!("用户 {} 被授予权限 {:?}", username, permission),
            )
        } else {
            (
                AuditEventType::PermissionDenied,
                AuditSeverity::Warning,
                AuditResult::Failure,
                format!("用户 {} 被拒绝权限 {:?}", username, permission),
            )
        };

        let log = AuditLog::new(event_type, severity, result, message)
            .with_user(user_id, username, crate::auth::models::Role::User, None)
            .with_permission(permission)
            .with_resource(resource_type, resource_id, None);

        self.log_async(log)
    }

    /// 便捷方法：记录沙盒操作事件
    pub fn log_sandbox_operation(
        &self,
        user_id: uuid::Uuid,
        username: String,
        operation: &str,
        sandbox_id: String,
        success: bool,
        error_message: Option<String>,
    ) -> SoulBoxResult<()> {
        let event_type = match operation {
            "create" => AuditEventType::SandboxCreated,
            "delete" => AuditEventType::SandboxDeleted,
            "execute" => AuditEventType::SandboxExecuted,
            _ => AuditEventType::SandboxExecuted,
        };

        let (severity, result, message) = if success {
            (
                AuditSeverity::Info,
                AuditResult::Success,
                format!("用户 {} {}沙盒 {} 成功", username, operation, sandbox_id),
            )
        } else {
            (
                AuditSeverity::Error,
                AuditResult::Failure,
                format!("用户 {} {}沙盒 {} 失败: {}", 
                    username, operation, sandbox_id, 
                    error_message.as_deref().unwrap_or("未知错误")),
            )
        };

        let mut log = AuditLog::new(event_type, severity, result, message)
            .with_user(user_id, username, crate::auth::models::Role::User, None)
            .with_resource("sandbox".to_string(), Some(sandbox_id), None);

        if let Some(error) = error_message {
            log = log.with_error("SANDBOX_OPERATION_FAILED".to_string(), error);
        }

        self.log_async(log)
    }

    /// 发送审计日志到外部日志系统 (ELK, Splunk)
    async fn send_to_external_log_systems(&self, log: &AuditLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // ELK Stack (Elasticsearch, Logstash, Kibana) 集成
        if let Err(e) = self.send_to_elk_stack(log).await {
            warn!("Failed to send to ELK stack: {}", e);
        }

        // Splunk 集成
        if let Err(e) = self.send_to_splunk(log).await {
            warn!("Failed to send to Splunk: {}", e);
        }

        // 其他日志聚合服务
        if let Err(e) = self.send_to_other_systems(log).await {
            warn!("Failed to send to other log systems: {}", e);
        }

        Ok(())
    }

    /// 发送到 ELK Stack
    async fn send_to_elk_stack(&self, log: &AuditLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 构造 Elasticsearch 文档
        let elk_document = serde_json::json!({
            "@timestamp": log.timestamp.to_rfc3339(),
            "service": "soulbox",
            "level": match log.severity {
                AuditSeverity::Info => "info",
                AuditSeverity::Warning => "warning", 
                AuditSeverity::Error => "error",
                AuditSeverity::Critical => "critical"
            },
            "event": {
                "type": format!("{:?}", log.event_type),
                "id": log.id.to_string(),
                "result": format!("{:?}", log.result),
                "message": log.message
            },
            "user": log.user_info.as_ref().map(|u| serde_json::json!({
                "id": u.user_id.to_string(),
                "name": u.username,
                "role": format!("{:?}", u.role),
                "tenant": u.tenant_id
            })),
            "resource": log.resource_info.as_ref().map(|r| serde_json::json!({
                "type": r.resource_type,
                "id": r.resource_id,
                "name": r.resource_name
            })),
            "network": log.network_info.as_ref().map(|n| serde_json::json!({
                "client_ip": n.client_ip,
                "user_agent": n.user_agent,
                "session_id": n.session_id
            })),
            "error": log.error_info.as_ref().map(|e| serde_json::json!({
                "code": e.error_code,
                "message": e.error_message
            })),
            "metadata": log.metadata
        });

        // 在生产环境中，这里会发送HTTP请求到Elasticsearch
        // 示例配置来自环境变量
        if let Ok(elk_endpoint) = std::env::var("ELK_ENDPOINT") {
            let client = reqwest::Client::new();
            let index_name = format!("soulbox-audit-{}", chrono::Utc::now().format("%Y.%m.%d"));
            let url = format!("{}/{}/_doc", elk_endpoint, index_name);
            
            let response = client
                .post(&url)
                .json(&elk_document)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await?;

            if response.status().is_success() {
                debug!("Successfully sent audit log to ELK: {}", log.id);
            } else {
                warn!("Failed to send to ELK, status: {}", response.status());
            }
        } else {
            debug!("ELK endpoint not configured, skipping ELK integration");
        }

        Ok(())
    }

    /// 发送到 Splunk
    async fn send_to_splunk(&self, log: &AuditLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 构造 Splunk 事件格式
        let splunk_event = serde_json::json!({
            "time": log.timestamp.timestamp(),
            "host": "soulbox-server",
            "source": "soulbox_audit",
            "sourcetype": "audit_log",
            "index": "soulbox",
            "event": {
                "id": log.id.to_string(),
                "event_type": format!("{:?}", log.event_type),
                "severity": format!("{:?}", log.severity),
                "result": format!("{:?}", log.result),
                "message": log.message,
                "user_id": log.user_info.as_ref().map(|u| u.user_id.to_string()),
                "username": log.user_info.as_ref().map(|u| u.username.clone()),
                "resource_type": log.resource_info.as_ref().map(|r| r.resource_type.clone()),
                "resource_id": log.resource_info.as_ref().map(|r| r.resource_id.clone()),
                "client_ip": log.network_info.as_ref().and_then(|n| n.client_ip.clone()),
                "error_code": log.error_info.as_ref().map(|e| e.error_code.clone()),
                "metadata": log.metadata
            }
        });

        // 在生产环境中，这里会使用 Splunk HEC (HTTP Event Collector)
        if let (Ok(splunk_hec_url), Ok(splunk_token)) = (
            std::env::var("SPLUNK_HEC_URL"),
            std::env::var("SPLUNK_HEC_TOKEN")
        ) {
            let client = reqwest::Client::new();
            
            let response = client
                .post(&splunk_hec_url)
                .header("Authorization", format!("Splunk {}", splunk_token))
                .header("Content-Type", "application/json")
                .json(&splunk_event)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await?;

            if response.status().is_success() {
                debug!("Successfully sent audit log to Splunk: {}", log.id);
            } else {
                warn!("Failed to send to Splunk, status: {}", response.status());
            }
        } else {
            debug!("Splunk HEC not configured, skipping Splunk integration");
        }

        Ok(())
    }

    /// 发送到其他日志系统 (Fluentd, Logz.io, Datadog等)
    async fn send_to_other_systems(&self, log: &AuditLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Fluentd 集成
        if let Ok(fluentd_url) = std::env::var("FLUENTD_URL") {
            let fluentd_event = serde_json::json!([
                "soulbox.audit",
                log.timestamp.timestamp(),
                {
                    "log_id": log.id.to_string(),
                    "event_type": format!("{:?}", log.event_type),
                    "severity": format!("{:?}", log.severity),
                    "message": log.message,
                    "user_id": log.user_info.as_ref().map(|u| u.user_id.to_string()),
                    "resource_type": log.resource_info.as_ref().map(|r| r.resource_type.clone()),
                    "metadata": log.metadata
                }
            ]);

            let client = reqwest::Client::new();
            if let Err(e) = client
                .post(&fluentd_url)
                .json(&fluentd_event)
                .timeout(std::time::Duration::from_secs(3))
                .send()
                .await {
                warn!("Failed to send to Fluentd: {}", e);
            }
        }

        // Datadog Logs API 集成
        if let (Ok(dd_api_key), Ok(dd_site)) = (
            std::env::var("DATADOG_API_KEY"),
            std::env::var("DATADOG_SITE").or_else(|_| Ok("datadoghq.com".to_string()))
        ) {
            let dd_log = serde_json::json!({
                "ddsource": "soulbox",
                "ddtags": format!("env:production,service:soulbox,event_type:{:?}", log.event_type),
                "hostname": "soulbox-server",
                "message": log.message,
                "level": match log.severity {
                    AuditSeverity::Info => "info",
                    AuditSeverity::Warning => "warn",
                    AuditSeverity::Error => "error",
                    AuditSeverity::Critical => "critical"
                },
                "timestamp": log.timestamp.to_rfc3339(),
                "attributes": {
                    "audit_id": log.id.to_string(),
                    "event_type": format!("{:?}", log.event_type),
                    "result": format!("{:?}", log.result),
                    "user_info": log.user_info,
                    "resource_info": log.resource_info,
                    "metadata": log.metadata
                }
            });

            let client = reqwest::Client::new();
            let url = format!("https://http-intake.logs.{}/v1/input/{}", dd_site, dd_api_key);
            
            if let Err(e) = client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&dd_log)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await {
                warn!("Failed to send to Datadog: {}", e);
            }
        }

        Ok(())
    }

    /// 发送告警通知
    async fn send_alert_notification(&self, log: &AuditLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 构造告警消息
        let alert_message = format!(
            "🚨 SoulBox Security Alert\n\n\
            Event: {:?}\n\
            Severity: {:?}\n\
            User: {}\n\
            Resource: {}\n\
            Message: {}\n\
            Time: {}\n\
            ID: {}",
            log.event_type,
            log.severity,
            log.user_info.as_ref()
                .map(|u| format!("{} ({})", u.username, u.user_id))
                .unwrap_or_else(|| "Unknown".to_string()),
            log.resource_info.as_ref()
                .map(|r| format!("{}: {}", r.resource_type, r.resource_id.as_deref().unwrap_or("N/A")))
                .unwrap_or_else(|| "Unknown".to_string()),
            log.message,
            log.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            log.id
        );

        // Slack 通知
        if let Ok(slack_webhook) = std::env::var("SLACK_ALERT_WEBHOOK") {
            let slack_payload = serde_json::json!({
                "text": alert_message,
                "username": "SoulBox Security Bot",
                "icon_emoji": ":warning:",
                "attachments": [{
                    "color": match log.severity {
                        AuditSeverity::Critical => "danger",
                        AuditSeverity::Error => "warning", 
                        AuditSeverity::Warning => "warning",
                        AuditSeverity::Info => "good"
                    },
                    "fields": [
                        {
                            "title": "Event Type",
                            "value": format!("{:?}", log.event_type),
                            "short": true
                        },
                        {
                            "title": "Severity", 
                            "value": format!("{:?}", log.severity),
                            "short": true
                        }
                    ]
                }]
            });

            let client = reqwest::Client::new();
            if let Err(e) = client
                .post(&slack_webhook)
                .json(&slack_payload)
                .send()
                .await {
                warn!("Failed to send Slack alert: {}", e);
            }
        }

        // Teams 通知
        if let Ok(teams_webhook) = std::env::var("TEAMS_ALERT_WEBHOOK") {
            let teams_payload = serde_json::json!({
                "@type": "MessageCard",
                "@context": "http://schema.org/extensions",
                "summary": "SoulBox Security Alert",
                "themeColor": match log.severity {
                    AuditSeverity::Critical => "FF0000",
                    AuditSeverity::Error => "FF6600",
                    AuditSeverity::Warning => "FFCC00", 
                    AuditSeverity::Info => "00FF00"
                },
                "sections": [{
                    "activityTitle": "SoulBox Security Alert",
                    "activitySubtitle": format!("{:?} - {:?}", log.event_type, log.severity),
                    "facts": [
                        {
                            "name": "Event Type",
                            "value": format!("{:?}", log.event_type)
                        },
                        {
                            "name": "Severity",
                            "value": format!("{:?}", log.severity)
                        },
                        {
                            "name": "Message",
                            "value": log.message
                        },
                        {
                            "name": "Time",
                            "value": log.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string()
                        }
                    ],
                    "text": alert_message
                }]
            });

            let client = reqwest::Client::new();
            if let Err(e) = client
                .post(&teams_webhook)
                .json(&teams_payload)
                .send()
                .await {
                warn!("Failed to send Teams alert: {}", e);
            }
        }

        Ok(())
    }

    /// 触发自动化响应
    async fn trigger_automated_response(&self, log: &AuditLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 根据事件类型和严重程度决定自动化响应
        match (&log.event_type, &log.severity) {
            // 关键安全事件的自动化响应
            (AuditEventType::SecurityViolation, AuditSeverity::Critical) => {
                info!("Triggering automated response for critical security violation");
                
                // 1. 自动暂停相关用户账户
                if let Some(user_info) = &log.user_info {
                    self.suspend_user_account(user_info.user_id).await?;
                }

                // 2. 自动隔离相关资源
                if let Some(resource_info) = &log.resource_info {
                    self.isolate_resource(resource_info).await?;
                }

                // 3. 触发安全团队告警
                self.trigger_security_team_alert(log).await?;
            },

            // 多次失败登录的自动化响应
            (AuditEventType::UserLogin, AuditSeverity::Error) => {
                if let Some(user_info) = &log.user_info {
                    self.check_and_handle_brute_force(user_info.user_id).await?;
                }
            },

            // 权限提升事件的自动化响应
            (AuditEventType::PermissionEscalation, _) => {
                info!("Triggering automated response for permission escalation");
                
                // 记录详细的权限变更日志
                self.log_detailed_permission_change(log).await?;
                
                // 通知管理员
                self.notify_administrators(log).await?;
            },

            _ => {
                // 对于其他事件，仅记录日志
                debug!("No automated response configured for event type: {:?}", log.event_type);
            }
        }

        Ok(())
    }

    /// 暂停用户账户（占位实现）
    async fn suspend_user_account(&self, user_id: uuid::Uuid) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        warn!("AUTO-RESPONSE: Suspending user account {}", user_id);
        // 在实际实现中，这里会调用用户管理API暂停账户
        Ok(())
    }

    /// 隔离资源（占位实现）
    async fn isolate_resource(&self, resource_info: &crate::audit::models::ResourceInfo) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        warn!("AUTO-RESPONSE: Isolating resource {} ({})", 
              resource_info.resource_type, 
              resource_info.resource_id.as_deref().unwrap_or("unknown"));
        // 在实际实现中，这里会调用相应的资源管理API进行隔离
        Ok(())
    }

    /// 触发安全团队告警（占位实现）
    async fn trigger_security_team_alert(&self, log: &AuditLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        error!("SECURITY ALERT: Critical security violation detected - {}", log.id);
        // 在实际实现中，这里会发送高优先级告警给安全团队
        Ok(())
    }

    /// 检查和处理暴力破解攻击（占位实现）
    async fn check_and_handle_brute_force(&self, user_id: uuid::Uuid) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        warn!("Checking for potential brute force attack against user {}", user_id);
        // 在实际实现中，这里会检查失败登录次数并采取相应措施
        Ok(())
    }

    /// 记录详细的权限变更日志（占位实现）
    async fn log_detailed_permission_change(&self, log: &AuditLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Logging detailed permission change for audit ID: {}", log.id);
        // 在实际实现中，这里会记录权限变更的详细信息
        Ok(())
    }

    /// 通知管理员（占位实现）
    async fn notify_administrators(&self, log: &AuditLog) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        warn!("Notifying administrators about audit event: {}", log.id);
        // 在实际实现中，这里会向管理员发送通知
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_audit_service_creation() {
        let config = AuditConfig::default();
        let service = AuditService::new(config).unwrap();
        assert!(service.storage.read().unwrap().logs.is_empty());
    }

    #[tokio::test]
    async fn test_sync_logging() {
        let config = AuditConfig::default();
        let service = AuditService::new(config).unwrap();

        let log = AuditLog::new(
            AuditEventType::UserLogin,
            AuditSeverity::Info,
            AuditResult::Success,
            "Test log".to_string(),
        );

        service.log_sync(log).unwrap();

        let storage = service.storage.read().unwrap();
        assert_eq!(storage.logs.len(), 1);
    }

    #[tokio::test]
    async fn test_query_functionality() {
        let config = AuditConfig::default();
        let service = AuditService::new(config).unwrap();

        // 添加测试数据
        service.log_sync(AuditLog::new(
            AuditEventType::UserLogin,
            AuditSeverity::Info,
            AuditResult::Success,
            "Login 1".to_string(),
        )).unwrap();

        service.log_sync(AuditLog::new(
            AuditEventType::UserLogout,
            AuditSeverity::Info,
            AuditResult::Success,
            "Logout 1".to_string(),
        )).unwrap();

        // 查询所有日志
        let all_logs = service.query(AuditQuery::default()).await.unwrap();
        assert_eq!(all_logs.len(), 2);

        // 查询特定事件类型
        let login_query = AuditQuery {
            event_type: Some(AuditEventType::UserLogin),
            ..Default::default()
        };
        let login_logs = service.query(login_query).await.unwrap();
        assert_eq!(login_logs.len(), 1);
    }

    #[tokio::test]
    async fn test_convenience_methods() {
        let config = AuditConfig::default();
        let service = AuditService::new(config).unwrap();

        let user_id = Uuid::new_v4();

        // 测试用户登录日志
        service.log_user_login(
            user_id,
            "testuser".to_string(),
            crate::auth::models::Role::Developer,
            None,
            Some("127.0.0.1".to_string()),
            Some("test-agent".to_string()),
            true,
        ).unwrap();

        // 等待异步处理
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let logs = service.query(AuditQuery::default()).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].event_type, AuditEventType::UserLogin);
    }
}