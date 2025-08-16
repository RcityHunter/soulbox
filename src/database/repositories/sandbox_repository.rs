use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, error, debug};
use uuid::Uuid;

use crate::database::surrealdb::{
    SurrealPool, SurrealOperations, QueryBuilder as SurrealQueryBuilder, UpdateBuilder, 
    uuid_to_record_id, record_id_to_uuid, SurrealResult, SurrealConnectionError, PaginationResult
};
use crate::database::{DatabaseError, DatabaseResult, models::{DbSandbox, PaginatedResult}};

/// SurrealDB 沙盒模型
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SurrealSandbox {
    pub id: String, // SurrealDB record ID
    pub name: String,
    pub runtime_type: String,
    pub template: String,
    pub status: String,
    pub owner_id: String,
    pub tenant_id: Option<String>,
    pub cpu_limit: Option<i64>,
    pub memory_limit: Option<i64>,
    pub disk_limit: Option<i64>,
    pub container_id: Option<String>,
    pub vm_id: Option<String>,
    pub ip_address: Option<String>,
    pub port_mappings: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// 沙盒仓库
pub struct SandboxRepository {
    pool: Arc<SurrealPool>,
}

impl SandboxRepository {
    pub fn new(pool: Arc<SurrealPool>) -> Self {
        Self { pool }
    }
    
    /// 创建沙盒
    pub async fn create(&self, sandbox: &DbSandbox) -> DatabaseResult<()> {
        debug!("创建沙盒: {} ({})", sandbox.name, sandbox.id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let surreal_sandbox = SurrealSandbox {
            id: uuid_to_record_id("sandboxes", sandbox.id),
            name: sandbox.name.clone(),
            runtime_type: sandbox.runtime_type.clone(),
            template: sandbox.template.clone(),
            status: sandbox.status.clone(),
            owner_id: uuid_to_record_id("users", sandbox.owner_id),
            tenant_id: sandbox.tenant_id.map(|id| uuid_to_record_id("tenants", id)),
            cpu_limit: sandbox.cpu_limit,
            memory_limit: sandbox.memory_limit,
            disk_limit: sandbox.disk_limit,
            container_id: sandbox.container_id.clone(),
            vm_id: sandbox.vm_id.clone(),
            ip_address: sandbox.ip_address.clone(),
            port_mappings: sandbox.port_mappings.clone(),
            created_at: sandbox.created_at,
            updated_at: sandbox.updated_at,
            started_at: sandbox.started_at,
            stopped_at: sandbox.stopped_at,
            expires_at: sandbox.expires_at,
        };
        
        ops.create("sandboxes", &surreal_sandbox).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        info!("Created sandbox: {} ({})", sandbox.name, sandbox.id);
        Ok(())
    }
    
    /// 根据ID查找沙盒
    pub async fn find_by_id(&self, id: Uuid) -> DatabaseResult<Option<DbSandbox>> {
        debug!("根据ID查找沙盒: {}", id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        match ops.find_by_id::<SurrealSandbox>("sandboxes", &id.to_string()).await {
            Ok(Some(surreal_sandbox)) => {
                let db_sandbox = self.surreal_to_db_sandbox(surreal_sandbox)?;
                Ok(Some(db_sandbox))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(DatabaseError::Query(e.to_string())),
        }
    }
    
    /// 根据容器ID查找沙盒
    pub async fn find_by_container_id(&self, container_id: &str) -> DatabaseResult<Option<DbSandbox>> {
        debug!("根据容器ID查找沙盒: {}", container_id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let query = SurrealQueryBuilder::new("sandboxes")
            .where_clause(format!("container_id = '{}'", container_id));
        
        match ops.find_where::<SurrealSandbox>(query).await {
            Ok(mut sandboxes) => {
                if let Some(surreal_sandbox) = sandboxes.pop() {
                    let db_sandbox = self.surreal_to_db_sandbox(surreal_sandbox)?;
                    Ok(Some(db_sandbox))
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(DatabaseError::Query(e.to_string())),
        }
    }
    
    /// 根据VM ID查找沙盒
    pub async fn find_by_vm_id(&self, vm_id: &str) -> DatabaseResult<Option<DbSandbox>> {
        debug!("根据VM ID查找沙盒: {}", vm_id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let query = SurrealQueryBuilder::new("sandboxes")
            .where_clause(format!("vm_id = '{}'", vm_id));
        
        match ops.find_where::<SurrealSandbox>(query).await {
            Ok(mut sandboxes) => {
                if let Some(surreal_sandbox) = sandboxes.pop() {
                    let db_sandbox = self.surreal_to_db_sandbox(surreal_sandbox)?;
                    Ok(Some(db_sandbox))
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(DatabaseError::Query(e.to_string())),
        }
    }
    
    /// 更新沙盒
    pub async fn update(&self, sandbox: &DbSandbox) -> DatabaseResult<()> {
        debug!("更新沙盒: {} ({})", sandbox.name, sandbox.id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let mut update = UpdateBuilder::new("sandboxes")
            .set(format!("name = '{}'", sandbox.name))
            .set(format!("runtime_type = '{}'", sandbox.runtime_type))
            .set(format!("template = '{}'", sandbox.template))
            .set(format!("status = '{}'", sandbox.status))
            .set("updated_at = time::now()")
            .where_clause(format!("id = {}", uuid_to_record_id("sandboxes", sandbox.id)));
        
        // 可选字段更新
        if let Some(tenant_id) = sandbox.tenant_id {
            update = update.set(format!("tenant_id = {}", uuid_to_record_id("tenants", tenant_id)));
        }
        
        if let Some(cpu_limit) = sandbox.cpu_limit {
            update = update.set(format!("cpu_limit = {}", cpu_limit));
        }
        
        if let Some(memory_limit) = sandbox.memory_limit {
            update = update.set(format!("memory_limit = {}", memory_limit));
        }
        
        if let Some(disk_limit) = sandbox.disk_limit {
            update = update.set(format!("disk_limit = {}", disk_limit));
        }
        
        if let Some(ref container_id) = sandbox.container_id {
            update = update.set(format!("container_id = '{}'", container_id));
        }
        
        if let Some(ref vm_id) = sandbox.vm_id {
            update = update.set(format!("vm_id = '{}'", vm_id));
        }
        
        if let Some(ref ip_address) = sandbox.ip_address {
            update = update.set(format!("ip_address = '{}'", ip_address));
        }
        
        if let Some(started_at) = sandbox.started_at {
            update = update.set(format!("started_at = '{}'", started_at.to_rfc3339()));
        }
        
        if let Some(stopped_at) = sandbox.stopped_at {
            update = update.set(format!("stopped_at = '{}'", stopped_at.to_rfc3339()));
        }
        
        if let Some(expires_at) = sandbox.expires_at {
            update = update.set(format!("expires_at = '{}'", expires_at.to_rfc3339()));
        }
        
        let results: Vec<SurrealSandbox> = ops.update_where(update).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        if results.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        
        info!("Updated sandbox: {} ({})", sandbox.name, sandbox.id);
        Ok(())
    }
    
    /// 更新沙盒状态
    pub async fn update_status(&self, id: Uuid, status: &str) -> DatabaseResult<()> {
        debug!("更新沙盒状态: {} -> {}", id, status);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let update = UpdateBuilder::new("sandboxes")
            .set(format!("status = '{}'", status))
            .set("updated_at = time::now()")
            .where_clause(format!("id = {}", uuid_to_record_id("sandboxes", id)));
        
        let results: Vec<SurrealSandbox> = ops.update_where(update).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        if results.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        
        Ok(())
    }
    
    /// 删除沙盒
    pub async fn delete(&self, id: Uuid) -> DatabaseResult<()> {
        debug!("删除沙盒: {}", id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let deleted = ops.delete("sandboxes", &id.to_string()).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        if !deleted {
            return Err(DatabaseError::NotFound);
        }
        
        info!("Deleted sandbox: {}", id);
        Ok(())
    }
    
    /// 列出用户的沙盒
    pub async fn list_by_owner(
        &self,
        owner_id: Uuid,
        status: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> DatabaseResult<PaginatedResult<DbSandbox>> {
        debug!("列出用户 {} 的沙盒，状态: {:?}, 页码: {}, 大小: {}", owner_id, status, page, page_size);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let mut query = SurrealQueryBuilder::new("sandboxes")
            .where_clause(format!("owner_id = {}", uuid_to_record_id("users", owner_id)))
            .order_by("created_at DESC");
        
        if let Some(status) = status {
            query = query.where_clause(format!("status = '{}'", status));
        }
        
        let pagination_result = ops.find_paginated::<SurrealSandbox>(query, page as usize, page_size as usize).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        let mut db_sandboxes = Vec::new();
        for surreal_sandbox in pagination_result.items {
            db_sandboxes.push(self.surreal_to_db_sandbox(surreal_sandbox)?);
        }
        
        Ok(PaginatedResult::new(
            db_sandboxes,
            pagination_result.total,
            pagination_result.page as u32,
            pagination_result.page_size as u32,
        ))
    }
    
    /// 列出租户的沙盒
    pub async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        status: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> DatabaseResult<PaginatedResult<DbSandbox>> {
        debug!("列出租户 {} 的沙盒，状态: {:?}, 页码: {}, 大小: {}", tenant_id, status, page, page_size);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let mut query = SurrealQueryBuilder::new("sandboxes")
            .where_clause(format!("tenant_id = {}", uuid_to_record_id("tenants", tenant_id)))
            .order_by("created_at DESC");
        
        if let Some(status) = status {
            query = query.where_clause(format!("status = '{}'", status));
        }
        
        let pagination_result = ops.find_paginated::<SurrealSandbox>(query, page as usize, page_size as usize).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        let mut db_sandboxes = Vec::new();
        for surreal_sandbox in pagination_result.items {
            db_sandboxes.push(self.surreal_to_db_sandbox(surreal_sandbox)?);
        }
        
        Ok(PaginatedResult::new(
            db_sandboxes,
            pagination_result.total,
            pagination_result.page as u32,
            pagination_result.page_size as u32,
        ))
    }
    
    /// 统计用户的沙盒数量
    pub async fn count_by_owner(&self, owner_id: Uuid, status: Option<&str>) -> DatabaseResult<i64> {
        debug!("统计用户 {} 的沙盒数量，状态: {:?}", owner_id, status);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let mut query = SurrealQueryBuilder::new("sandboxes")
            .select("count()")
            .where_clause(format!("owner_id = {}", uuid_to_record_id("users", owner_id)));
        
        if let Some(status) = status {
            query = query.where_clause(format!("status = '{}'", status));
        }
        
        let results: Vec<surrealdb::Value> = ops.find_where(query).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        let count = if let Some(count_value) = results.first() {
            count_value.as_int() as i64
        } else {
            0
        };
        
        Ok(count)
    }
    
    /// 清理过期沙盒
    pub async fn cleanup_expired(&self) -> DatabaseResult<i64> {
        debug!("清理过期沙盒");
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        // 查找过期沙盒
        let query = SurrealQueryBuilder::new("sandboxes")
            .where_clause("expires_at < time::now()")
            .where_clause("status != 'stopped'");
        
        let expired_sandboxes: Vec<SurrealSandbox> = ops.find_where(query).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        let count = expired_sandboxes.len() as i64;
        
        if count > 0 {
            // 更新过期沙盒状态
            let update = UpdateBuilder::new("sandboxes")
                .set("status = 'stopped'")
                .set("stopped_at = time::now()")
                .set("updated_at = time::now()")
                .where_clause("expires_at < time::now()")
                .where_clause("status != 'stopped'");
            
            ops.update_where::<SurrealSandbox>(update).await
                .map_err(|e| DatabaseError::Query(e.to_string()))?;
            
            info!("清理了 {} 个过期沙盒", count);
        }
        
        Ok(count)
    }
    
    /// 将 SurrealSandbox 转换为 DbSandbox
    fn surreal_to_db_sandbox(&self, surreal_sandbox: SurrealSandbox) -> DatabaseResult<DbSandbox> {
        let id_str = surreal_sandbox.id.split(':').last()
            .ok_or_else(|| DatabaseError::Other("Invalid sandbox record ID format".to_string()))?;
        
        let id = id_str.parse::<Uuid>()
            .map_err(|e| DatabaseError::Other(format!("Failed to parse sandbox UUID: {}", e)))?;
        
        let owner_id_str = surreal_sandbox.owner_id.split(':').last()
            .ok_or_else(|| DatabaseError::Other("Invalid owner record ID format".to_string()))?;
        
        let owner_id = owner_id_str.parse::<Uuid>()
            .map_err(|e| DatabaseError::Other(format!("Failed to parse owner UUID: {}", e)))?;
        
        let tenant_id = if let Some(tenant_record_id) = &surreal_sandbox.tenant_id {
            let tenant_id_str = tenant_record_id.split(':').last()
                .ok_or_else(|| DatabaseError::Other("Invalid tenant record ID format".to_string()))?;
            Some(tenant_id_str.parse::<Uuid>()
                .map_err(|e| DatabaseError::Other(format!("Failed to parse tenant UUID: {}", e)))?)
        } else {
            None
        };
        
        Ok(DbSandbox {
            id,
            name: surreal_sandbox.name,
            runtime_type: surreal_sandbox.runtime_type,
            template: surreal_sandbox.template,
            status: surreal_sandbox.status,
            owner_id,
            tenant_id,
            cpu_limit: surreal_sandbox.cpu_limit,
            memory_limit: surreal_sandbox.memory_limit,
            disk_limit: surreal_sandbox.disk_limit,
            container_id: surreal_sandbox.container_id,
            vm_id: surreal_sandbox.vm_id,
            ip_address: surreal_sandbox.ip_address,
            port_mappings: surreal_sandbox.port_mappings,
            created_at: surreal_sandbox.created_at,
            updated_at: surreal_sandbox.updated_at,
            started_at: surreal_sandbox.started_at,
            stopped_at: surreal_sandbox.stopped_at,
            expires_at: surreal_sandbox.expires_at,
        })
    }
}

// Convert DatabaseError from SurrealConnectionError
impl From<SurrealConnectionError> for DatabaseError {
    fn from(err: SurrealConnectionError) -> Self {
        match err {
            SurrealConnectionError::Connection(msg) => DatabaseError::Connection(msg),
            SurrealConnectionError::Query(msg) => DatabaseError::Query(msg),
            SurrealConnectionError::PoolExhausted => DatabaseError::Connection("Connection pool exhausted".to_string()),
            SurrealConnectionError::HealthCheck(msg) => DatabaseError::Connection(format!("Health check failed: {}", msg)),
            SurrealConnectionError::Auth(msg) => DatabaseError::Connection(format!("Authentication failed: {}", msg)),
            SurrealConnectionError::Config(msg) => DatabaseError::Connection(format!("Configuration error: {}", msg)),
            SurrealConnectionError::Surreal(e) => DatabaseError::Other(e.to_string()),
        }
    }
}