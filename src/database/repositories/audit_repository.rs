use chrono::{DateTime, Utc};
use sqlx::{QueryBuilder, Row};
use std::sync::Arc;
use tracing::{info, error};
use uuid::Uuid;

use crate::audit::models::{AuditLog, AuditQuery, AuditEventType, AuditSeverity, AuditResult};
use crate::database::{Database, DatabaseError, DatabasePool, DatabaseResult, models::{DbAuditLog, PaginatedResult}};

/// 审计日志仓库
pub struct AuditRepository {
    db: Arc<Database>,
}

impl AuditRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
    
    /// 创建审计日志
    pub async fn create(&self, log: &AuditLog) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO audit_logs (
                        id, event_type, severity, result, timestamp,
                        user_id, username, user_role, tenant_id,
                        request_id, ip_address, user_agent, request_path, http_method,
                        resource_type, resource_id, resource_name,
                        permission, old_role, new_role,
                        message, details, error_code, error_message,
                        session_id, correlation_id, tags
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                        $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27
                    )
                    "#,
                    log.id,
                    format!("{:?}", log.event_type),
                    format!("{:?}", log.severity),
                    format!("{:?}", log.result),
                    log.timestamp,
                    log.user_id,
                    log.username,
                    log.user_role.as_ref().map(|r| format!("{:?}", r)),
                    log.tenant_id,
                    log.request_id,
                    log.ip_address,
                    log.user_agent,
                    log.request_path,
                    log.http_method,
                    log.resource_type,
                    log.resource_id,
                    log.resource_name,
                    log.permission.as_ref().map(|p| format!("{:?}", p)),
                    log.old_role.as_ref().map(|r| format!("{:?}", r)),
                    log.new_role.as_ref().map(|r| format!("{:?}", r)),
                    log.message,
                    log.details.as_ref().map(|d| serde_json::to_value(d).ok()).flatten(),
                    log.error_code,
                    log.error_message,
                    log.session_id,
                    log.correlation_id,
                    log.tags.as_ref().map(|t| serde_json::to_value(t).ok()).flatten()
                )
                .execute(pool)
                .await?;
            }
            DatabasePool::Sqlite(pool) => {
                let id = log.id.to_string();
                let event_type = format!("{:?}", log.event_type);
                let severity = format!("{:?}", log.severity);
                let result = format!("{:?}", log.result);
                let timestamp = log.timestamp.to_rfc3339();
                let user_id = log.user_id.map(|u| u.to_string());
                let user_role = log.user_role.as_ref().map(|r| format!("{:?}", r));
                let tenant_id = log.tenant_id.map(|t| t.to_string());
                let permission = log.permission.as_ref().map(|p| format!("{:?}", p));
                let old_role = log.old_role.as_ref().map(|r| format!("{:?}", r));
                let new_role = log.new_role.as_ref().map(|r| format!("{:?}", r));
                let details = log.details.as_ref().map(|d| serde_json::to_string(d).ok()).flatten();
                let tags = log.tags.as_ref().map(|t| serde_json::to_string(t).ok()).flatten();
                
                sqlx::query(
                    r#"
                    INSERT INTO audit_logs (
                        id, event_type, severity, result, timestamp,
                        user_id, username, user_role, tenant_id,
                        request_id, ip_address, user_agent, request_path, http_method,
                        resource_type, resource_id, resource_name,
                        permission, old_role, new_role,
                        message, details, error_code, error_message,
                        session_id, correlation_id, tags
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                        ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27
                    )
                    "#,
                    id,
                    event_type,
                    severity,
                    result,
                    timestamp,
                    user_id,
                    log.username,
                    user_role,
                    tenant_id,
                    log.request_id,
                    log.ip_address,
                    log.user_agent,
                    log.request_path,
                    log.http_method,
                    log.resource_type,
                    log.resource_id,
                    log.resource_name,
                    permission,
                    old_role,
                    new_role,
                    log.message,
                    details,
                    log.error_code,
                    log.error_message,
                    log.session_id,
                    log.correlation_id,
                    tags
                )
                .execute(pool)
                .await?;
            }
        }
        
        Ok(())
    }
    
    /// 查询审计日志
    pub async fn query(&self, query: AuditQuery) -> DatabaseResult<Vec<DbAuditLog>> {
        let page = query.page.unwrap_or(1).max(1);
        let limit = query.limit.unwrap_or(100).min(1000) as i64;
        let offset = ((page - 1) * limit as u32) as i64;
        
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let mut query_builder = QueryBuilder::new(
                    "SELECT * FROM audit_logs WHERE 1=1"
                );
                
                // 构建查询条件
                if let Some(event_type) = &query.event_type {
                    query_builder.push(" AND event_type = ");
                    query_builder.push_bind(format!("{:?}", event_type));
                }
                
                if let Some(severity) = &query.severity {
                    query_builder.push(" AND severity = ");
                    query_builder.push_bind(format!("{:?}", severity));
                }
                
                if let Some(result) = &query.result {
                    query_builder.push(" AND result = ");
                    query_builder.push_bind(format!("{:?}", result));
                }
                
                if let Some(user_id) = &query.user_id {
                    query_builder.push(" AND user_id = ");
                    query_builder.push_bind(user_id);
                }
                
                if let Some(tenant_id) = &query.tenant_id {
                    query_builder.push(" AND tenant_id = ");
                    query_builder.push_bind(tenant_id);
                }
                
                if let Some(resource_type) = &query.resource_type {
                    query_builder.push(" AND resource_type = ");
                    query_builder.push_bind(resource_type);
                }
                
                if let Some(resource_id) = &query.resource_id {
                    query_builder.push(" AND resource_id = ");
                    query_builder.push_bind(resource_id);
                }
                
                if let Some(start_time) = &query.start_time {
                    query_builder.push(" AND timestamp >= ");
                    query_builder.push_bind(start_time);
                }
                
                if let Some(end_time) = &query.end_time {
                    query_builder.push(" AND timestamp <= ");
                    query_builder.push_bind(end_time);
                }
                
                // 排序
                let order_by = query.order_by.as_deref().unwrap_or("timestamp");
                let order_direction = if query.order_desc.unwrap_or(true) { "DESC" } else { "ASC" };
                query_builder.push(format!(" ORDER BY {} {}", order_by, order_direction));
                
                // 分页
                query_builder.push(" LIMIT ");
                query_builder.push_bind(limit);
                query_builder.push(" OFFSET ");
                query_builder.push_bind(offset);
                
                let logs = query_builder
                    .build_query_as::<DbAuditLog>()
                    .fetch_all(pool)
                    .await?;
                
                Ok(logs)
            }
            DatabasePool::Sqlite(pool) => {
                // SQLite 版本类似，但需要调整一些语法
                let mut query_builder = QueryBuilder::new(
                    "SELECT * FROM audit_logs WHERE 1=1"
                );
                
                // 构建查询条件（与 PostgreSQL 类似）
                if let Some(event_type) = &query.event_type {
                    query_builder.push(" AND event_type = ");
                    query_builder.push_bind(format!("{:?}", event_type));
                }
                
                if let Some(severity) = &query.severity {
                    query_builder.push(" AND severity = ");
                    query_builder.push_bind(format!("{:?}", severity));
                }
                
                if let Some(result) = &query.result {
                    query_builder.push(" AND result = ");
                    query_builder.push_bind(format!("{:?}", result));
                }
                
                if let Some(user_id) = &query.user_id {
                    query_builder.push(" AND user_id = ");
                    query_builder.push_bind(user_id.to_string());
                }
                
                if let Some(tenant_id) = &query.tenant_id {
                    query_builder.push(" AND tenant_id = ");
                    query_builder.push_bind(tenant_id.to_string());
                }
                
                if let Some(resource_type) = &query.resource_type {
                    query_builder.push(" AND resource_type = ");
                    query_builder.push_bind(resource_type);
                }
                
                if let Some(resource_id) = &query.resource_id {
                    query_builder.push(" AND resource_id = ");
                    query_builder.push_bind(resource_id);
                }
                
                if let Some(start_time) = &query.start_time {
                    query_builder.push(" AND timestamp >= ");
                    query_builder.push_bind(start_time.to_rfc3339());
                }
                
                if let Some(end_time) = &query.end_time {
                    query_builder.push(" AND timestamp <= ");
                    query_builder.push_bind(end_time.to_rfc3339());
                }
                
                // 排序和分页
                let order_by = query.order_by.as_deref().unwrap_or("timestamp");
                let order_direction = if query.order_desc.unwrap_or(true) { "DESC" } else { "ASC" };
                query_builder.push(format!(" ORDER BY {} {}", order_by, order_direction));
                query_builder.push(" LIMIT ");
                query_builder.push_bind(limit);
                query_builder.push(" OFFSET ");
                query_builder.push_bind(offset);
                
                let logs = query_builder
                    .build_query_as::<DbAuditLog>()
                    .fetch_all(pool)
                    .await?;
                
                Ok(logs)
            }
        }
    }
    
    /// 统计审计日志
    pub async fn count(&self, query: Option<AuditQuery>) -> DatabaseResult<i64> {
        let query = query.unwrap_or_default();
        
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let mut query_builder = QueryBuilder::new(
                    "SELECT COUNT(*) as count FROM audit_logs WHERE 1=1"
                );
                
                // 构建查询条件（与 query 方法相同）
                if let Some(event_type) = &query.event_type {
                    query_builder.push(" AND event_type = ");
                    query_builder.push_bind(format!("{:?}", event_type));
                }
                
                if let Some(severity) = &query.severity {
                    query_builder.push(" AND severity = ");
                    query_builder.push_bind(format!("{:?}", severity));
                }
                
                if let Some(result) = &query.result {
                    query_builder.push(" AND result = ");
                    query_builder.push_bind(format!("{:?}", result));
                }
                
                if let Some(user_id) = &query.user_id {
                    query_builder.push(" AND user_id = ");
                    query_builder.push_bind(user_id);
                }
                
                if let Some(tenant_id) = &query.tenant_id {
                    query_builder.push(" AND tenant_id = ");
                    query_builder.push_bind(tenant_id);
                }
                
                let row = query_builder
                    .build()
                    .fetch_one(pool)
                    .await?;
                
                Ok(row.get::<i64, _>("count"))
            }
            DatabasePool::Sqlite(pool) => {
                let mut query_builder = QueryBuilder::new(
                    "SELECT COUNT(*) as count FROM audit_logs WHERE 1=1"
                );
                
                // 构建查询条件（与 PostgreSQL 类似，但 UUID 需要转换）
                if let Some(event_type) = &query.event_type {
                    query_builder.push(" AND event_type = ");
                    query_builder.push_bind(format!("{:?}", event_type));
                }
                
                if let Some(severity) = &query.severity {
                    query_builder.push(" AND severity = ");
                    query_builder.push_bind(format!("{:?}", severity));
                }
                
                if let Some(result) = &query.result {
                    query_builder.push(" AND result = ");
                    query_builder.push_bind(format!("{:?}", result));
                }
                
                if let Some(user_id) = &query.user_id {
                    query_builder.push(" AND user_id = ");
                    query_builder.push_bind(user_id.to_string());
                }
                
                if let Some(tenant_id) = &query.tenant_id {
                    query_builder.push(" AND tenant_id = ");
                    query_builder.push_bind(tenant_id.to_string());
                }
                
                let row = query_builder
                    .build()
                    .fetch_one(pool)
                    .await?;
                
                Ok(row.get::<i64, _>("count"))
            }
        }
    }
    
    /// 删除过期的审计日志
    pub async fn delete_old_logs(&self, before: DateTime<Utc>) -> DatabaseResult<u64> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query!(
                    "DELETE FROM audit_logs WHERE timestamp < $1",
                    before
                )
                .execute(pool)
                .await?;
                
                Ok(result.rows_affected())
            }
            DatabasePool::Sqlite(pool) => {
                let result = sqlx::query!(
                    "DELETE FROM audit_logs WHERE timestamp < ?1",
                    before.to_rfc3339()
                )
                .execute(pool)
                .await?;
                
                Ok(result.rows_affected())
            }
        }
    }
}
// DbAuditLog conversion is now handled in the models.rs file
