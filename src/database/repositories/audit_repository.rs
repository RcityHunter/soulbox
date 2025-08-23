use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, debug};
use uuid::Uuid;

use crate::audit::models::{AuditLog, AuditEventType};
use crate::database::surrealdb::{
    SurrealPool, SurrealOperations, 
    uuid_to_record_id
};
use crate::database::{DatabaseError, DatabaseResult, models::{DbAuditLog, PaginatedResult}};

/// SurrealDB 审计日志模型
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SurrealAuditLog {
    pub id: String,
    pub event_type: String,
    pub severity: String,
    pub result: String,
    pub timestamp: DateTime<Utc>,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub user_role: Option<String>,
    pub tenant_id: Option<String>,
    pub request_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub request_path: Option<String>,
    pub http_method: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub resource_name: Option<String>,
    pub permission: Option<String>,
    pub old_role: Option<String>,
    pub new_role: Option<String>,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub session_id: Option<String>,
    pub correlation_id: Option<String>,
    pub tags: Option<serde_json::Value>,
}

/// 审计日志仓库
pub struct AuditRepository {
    pool: Arc<SurrealPool>,
}

impl AuditRepository {
    pub fn new(pool: Arc<SurrealPool>) -> Self {
        Self { pool }
    }
    
    /// 创建审计日志
    pub async fn create(&self, audit_log: &AuditLog) -> DatabaseResult<()> {
        debug!("创建审计日志: {:?} - {}", audit_log.event_type, audit_log.message);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let surreal_audit_log = SurrealAuditLog {
            id: uuid_to_record_id("audit_logs", audit_log.id),
            event_type: format!("{:?}", audit_log.event_type),
            severity: format!("{:?}", audit_log.severity),
            result: format!("{:?}", audit_log.result),
            timestamp: audit_log.timestamp,
            user_id: audit_log.user_id.map(|id| uuid_to_record_id("users", id)),
            username: audit_log.username.clone(),
            user_role: audit_log.user_role.as_ref().map(|r| format!("{:?}", r)),
            tenant_id: audit_log.tenant_id.map(|id| uuid_to_record_id("tenants", id)),
            request_id: audit_log.request_id.clone(),
            ip_address: audit_log.ip_address.clone(),
            user_agent: audit_log.user_agent.clone(),
            request_path: audit_log.request_path.clone(),
            http_method: audit_log.http_method.clone(),
            resource_type: audit_log.resource_type.clone(),
            resource_id: audit_log.resource_id.clone(),
            resource_name: audit_log.resource_name.clone(),
            permission: audit_log.permission.as_ref().map(|p| format!("{:?}", p)),
            old_role: audit_log.old_role.as_ref().map(|r| format!("{:?}", r)),
            new_role: audit_log.new_role.as_ref().map(|r| format!("{:?}", r)),
            message: audit_log.message.clone(),
            details: audit_log.details.as_ref().map(|d| serde_json::to_value(d).unwrap_or(serde_json::Value::Null)),
            error_code: audit_log.error_code.clone(),
            error_message: audit_log.error_message.clone(),
            session_id: audit_log.session_id.clone(),
            correlation_id: audit_log.correlation_id.clone(),
            tags: audit_log.tags.as_ref().map(|t| serde_json::to_value(t).unwrap_or(serde_json::Value::Null)),
        };
        
        ops.create("audit_logs", &surreal_audit_log).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        Ok(())
    }
    
    /// 根据ID查找审计日志
    pub async fn find_by_id(&self, id: Uuid) -> DatabaseResult<Option<DbAuditLog>> {
        debug!("根据ID查找审计日志: {}", id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        match ops.find_by_id::<SurrealAuditLog>("audit_logs", &id.to_string()).await {
            Ok(Some(surreal_audit_log)) => {
                let db_audit_log = self.surreal_to_db_audit_log(surreal_audit_log)?;
                Ok(Some(db_audit_log))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(DatabaseError::Query(e.to_string())),
        }
    }
    
    /// 列出用户的审计日志
    pub async fn list_by_user(
        &self,
        user_id: Uuid,
        page: u32,
        page_size: u32,
    ) -> DatabaseResult<PaginatedResult<DbAuditLog>> {
        debug!("列出用户 {} 的审计日志，页码: {}, 大小: {}", user_id, page, page_size);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let user_record_id = uuid_to_record_id("users", user_id);
        let data_sql = format!(
            "SELECT * FROM audit_logs WHERE user_id = {} ORDER BY timestamp DESC LIMIT {} START {}", 
            user_record_id, page_size, (page - 1) * page_size
        );
        let count_sql = format!("SELECT count() as total FROM audit_logs WHERE user_id = {}", user_record_id);
        
        let pagination_result = ops.query_paginated::<SurrealAuditLog>(&data_sql, &count_sql, page as usize, page_size as usize).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        let mut db_audit_logs = Vec::new();
        for surreal_audit_log in pagination_result.items {
            db_audit_logs.push(self.surreal_to_db_audit_log(surreal_audit_log)?);
        }
        
        Ok(PaginatedResult::new(
            db_audit_logs,
            pagination_result.total,
            pagination_result.page as u32,
            pagination_result.page_size as u32,
        ))
    }
    
    /// 列出租户的审计日志
    pub async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        page: u32,
        page_size: u32,
    ) -> DatabaseResult<PaginatedResult<DbAuditLog>> {
        debug!("列出租户 {} 的审计日志，页码: {}, 大小: {}", tenant_id, page, page_size);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let tenant_id_str = uuid_to_record_id("tenants", tenant_id);
        let offset = page * page_size;
        
        // Get audit logs with pagination
        let mut response = conn.db()
            .query("SELECT * FROM audit_logs WHERE tenant_id = $tenant_id ORDER BY timestamp DESC LIMIT $limit START $start")
            .bind(("tenant_id", tenant_id_str.clone()))
            .bind(("limit", page_size))
            .bind(("start", offset))
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
            
        let logs: Vec<SurrealAuditLog> = response
            .take(0)
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        // Get total count
        let mut count_response = conn.db()
            .query("SELECT count() FROM audit_logs WHERE tenant_id = $tenant_id GROUP ALL")
            .bind(("tenant_id", tenant_id_str))
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
            
        let count_result: Vec<serde_json::Value> = count_response
            .take(0)
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
            
        let total = count_result.first()
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        
        let mut db_audit_logs = Vec::new();
        for surreal_audit_log in logs {
            db_audit_logs.push(self.surreal_to_db_audit_log(surreal_audit_log)?);
        }
        
        Ok(PaginatedResult::new(
            db_audit_logs,
            total as i64,
            page,
            page_size,
        ))
    }
    
    /// 根据事件类型查询审计日志
    pub async fn list_by_event_type(
        &self,
        event_type: &AuditEventType,
        page: u32,
        page_size: u32,
    ) -> DatabaseResult<PaginatedResult<DbAuditLog>> {
        debug!("根据事件类型查询审计日志: {:?}, 页码: {}, 大小: {}", event_type, page, page_size);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let offset = page * page_size;
        
        // Get audit logs with pagination
        let mut response = conn.db()
            .query("SELECT * FROM audit_logs WHERE event_type = $event_type ORDER BY timestamp DESC LIMIT $limit START $start")
            .bind(("event_type", format!("{:?}", event_type)))
            .bind(("limit", page_size))
            .bind(("start", offset))
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
            
        let logs: Vec<SurrealAuditLog> = response
            .take(0)
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        // Get total count
        let mut count_response = conn.db()
            .query("SELECT count() FROM audit_logs WHERE event_type = $event_type GROUP ALL")
            .bind(("event_type", format!("{:?}", event_type)))
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
            
        let count_result: Vec<serde_json::Value> = count_response
            .take(0)
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
            
        let total = count_result.first()
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        
        let mut db_audit_logs = Vec::new();
        for surreal_audit_log in logs {
            db_audit_logs.push(self.surreal_to_db_audit_log(surreal_audit_log)?);
        }
        
        Ok(PaginatedResult::new(
            db_audit_logs,
            total as i64,
            page,
            page_size,
        ))
    }
    
    /// 清理旧的审计日志
    pub async fn cleanup_old_logs(&self, days: u32) -> DatabaseResult<i64> {
        debug!("清理 {} 天前的审计日志", days);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        // 删除过期日志并返回删除数量
        let mut response = conn.db()
            .query("DELETE audit_logs WHERE timestamp < time::now() - $days RETURN count()")
            .bind(("days", format!("{}d", days)))
            .await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
            
        let result: Vec<serde_json::Value> = response
            .take(0)
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        let count = result.first()
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as i64;
        
        if count > 0 {
            info!("清理了 {} 条过期审计日志", count);
        }
        
        Ok(count)
    }
    
    /// 将 SurrealAuditLog 转换为 DbAuditLog
    fn surreal_to_db_audit_log(&self, surreal_audit_log: SurrealAuditLog) -> DatabaseResult<DbAuditLog> {
        let id_str = surreal_audit_log.id.split(':').last()
            .ok_or_else(|| DatabaseError::Other("Invalid audit log record ID format".to_string()))?;
        
        let id = id_str.parse::<Uuid>()
            .map_err(|e| DatabaseError::Other(format!("Failed to parse audit log UUID: {}", e)))?;
        
        let user_id = if let Some(user_record_id) = &surreal_audit_log.user_id {
            let user_id_str = user_record_id.split(':').last()
                .ok_or_else(|| DatabaseError::Other("Invalid user record ID format".to_string()))?;
            Some(user_id_str.parse::<Uuid>()
                .map_err(|e| DatabaseError::Other(format!("Failed to parse user UUID: {}", e)))?)
        } else {
            None
        };
        
        let tenant_id = if let Some(tenant_record_id) = &surreal_audit_log.tenant_id {
            let tenant_id_str = tenant_record_id.split(':').last()
                .ok_or_else(|| DatabaseError::Other("Invalid tenant record ID format".to_string()))?;
            Some(tenant_id_str.parse::<Uuid>()
                .map_err(|e| DatabaseError::Other(format!("Failed to parse tenant UUID: {}", e)))?)
        } else {
            None
        };
        
        Ok(DbAuditLog {
            id,
            event_type: surreal_audit_log.event_type,
            severity: surreal_audit_log.severity,
            result: surreal_audit_log.result,
            timestamp: surreal_audit_log.timestamp,
            user_id,
            username: surreal_audit_log.username,
            user_role: surreal_audit_log.user_role,
            tenant_id,
            request_id: surreal_audit_log.request_id,
            ip_address: surreal_audit_log.ip_address,
            user_agent: surreal_audit_log.user_agent,
            request_path: surreal_audit_log.request_path,
            http_method: surreal_audit_log.http_method,
            resource_type: surreal_audit_log.resource_type,
            resource_id: surreal_audit_log.resource_id,
            resource_name: surreal_audit_log.resource_name,
            permission: surreal_audit_log.permission,
            old_role: surreal_audit_log.old_role,
            new_role: surreal_audit_log.new_role,
            message: surreal_audit_log.message,
            details: surreal_audit_log.details,
            error_code: surreal_audit_log.error_code,
            error_message: surreal_audit_log.error_message,
            session_id: surreal_audit_log.session_id,
            correlation_id: surreal_audit_log.correlation_id,
            tags: surreal_audit_log.tags,
        })
    }
}

// Note: From<SurrealConnectionError> implementation is in database/mod.rs to avoid duplicates