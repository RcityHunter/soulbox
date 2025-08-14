use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::audit::models::{AuditLog, AuditQuery, AuditStats, AuditEventType, AuditSeverity, AuditResult};
use crate::error::{Result as SoulBoxResult, SoulBoxError};
use crate::database::Database;
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
    pub fn with_database(config: AuditConfig, database: Arc<Database>) -> SoulBoxResult<Arc<Self>> {
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

            // TODO: 未来可在此处添加：
            // - 发送到外部日志系统 (ELK, Splunk)
            // - 发送告警通知
            // - 触发自动化响应
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
        let all_logs = service.query(AuditQuery::default()).unwrap();
        assert_eq!(all_logs.len(), 2);

        // 查询特定事件类型
        let login_query = AuditQuery {
            event_type: Some(AuditEventType::UserLogin),
            ..Default::default()
        };
        let login_logs = service.query(login_query).unwrap();
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

        let logs = service.query(AuditQuery::default()).unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].event_type, AuditEventType::UserLogin);
    }
}