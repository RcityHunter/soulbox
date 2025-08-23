use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, debug};
use uuid::Uuid;

use crate::database::surrealdb::{
    SurrealPool, uuid_to_record_id
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
        
        // Use direct SurrealQL CREATE query
        let sql = "CREATE $record_id CONTENT $sandbox";
        
        conn.db()
            .query(sql)
            .bind(("record_id", &surreal_sandbox.id))
            .bind(("sandbox", &surreal_sandbox))
            .await
            .map_err(|e| DatabaseError::Query(format!("创建沙盒失败: {}", e)))?;
        
        info!("Created sandbox: {} ({})", sandbox.name, sandbox.id);
        Ok(())
    }
    
    /// 根据ID查找沙盒
    pub async fn find_by_id(&self, id: Uuid) -> DatabaseResult<Option<DbSandbox>> {
        debug!("根据ID查找沙盒: {}", id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let record_id = uuid_to_record_id("sandboxes", id);
        
        // Use direct SurrealQL SELECT query
        let sql = "SELECT * FROM $record_id";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("record_id", &record_id))
            .await
            .map_err(|e| DatabaseError::Query(format!("查询沙盒失败: {}", e)))?;
        
        let sandboxes: Vec<SurrealSandbox> = response
            .take::<Vec<SurrealSandbox>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("解析查询结果失败: {}", e)))?;
        
        if let Some(surreal_sandbox) = sandboxes.into_iter().next() {
            let db_sandbox = self.surreal_to_db_sandbox(surreal_sandbox)?;
            Ok(Some(db_sandbox))
        } else {
            Ok(None)
        }
    }
    
    /// 根据容器ID查找沙盒
    pub async fn find_by_container_id(&self, container_id: &str) -> DatabaseResult<Option<DbSandbox>> {
        debug!("根据容器ID查找沙盒: {}", container_id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        // Use direct SurrealQL SELECT query
        let sql = "SELECT * FROM sandboxes WHERE container_id = $container_id";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("container_id", container_id))
            .await
            .map_err(|e| DatabaseError::Query(format!("查询沙盒失败: {}", e)))?;
        
        let sandboxes: Vec<SurrealSandbox> = response
            .take::<Vec<SurrealSandbox>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("解析查询结果失败: {}", e)))?;
        
        if let Some(surreal_sandbox) = sandboxes.into_iter().next() {
            let db_sandbox = self.surreal_to_db_sandbox(surreal_sandbox)?;
            Ok(Some(db_sandbox))
        } else {
            Ok(None)
        }
    }
    
    /// 根据VM ID查找沙盒
    pub async fn find_by_vm_id(&self, vm_id: &str) -> DatabaseResult<Option<DbSandbox>> {
        debug!("根据VM ID查找沙盒: {}", vm_id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        // Use direct SurrealQL SELECT query
        let sql = "SELECT * FROM sandboxes WHERE vm_id = $vm_id";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("vm_id", vm_id))
            .await
            .map_err(|e| DatabaseError::Query(format!("查询沙盒失败: {}", e)))?;
        
        let sandboxes: Vec<SurrealSandbox> = response
            .take::<Vec<SurrealSandbox>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("解析查询结果失败: {}", e)))?;
        
        if let Some(surreal_sandbox) = sandboxes.into_iter().next() {
            let db_sandbox = self.surreal_to_db_sandbox(surreal_sandbox)?;
            Ok(Some(db_sandbox))
        } else {
            Ok(None)
        }
    }
    
    /// 更新沙盒
    pub async fn update(&self, sandbox: &DbSandbox) -> DatabaseResult<()> {
        debug!("更新沙盒: {} ({})", sandbox.name, sandbox.id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let record_id = uuid_to_record_id("sandboxes", sandbox.id);
        
        // Use direct SurrealQL UPDATE query - build the update fields
        let mut sql = format!(
            "UPDATE $record_id SET name = $name, runtime_type = $runtime_type, template = $template, status = $status, updated_at = time::now()"
        );
        
        // Add optional fields to the update
        if sandbox.tenant_id.is_some() {
            sql.push_str(", tenant_id = $tenant_id");
        }
        if sandbox.cpu_limit.is_some() {
            sql.push_str(", cpu_limit = $cpu_limit");
        }
        if sandbox.memory_limit.is_some() {
            sql.push_str(", memory_limit = $memory_limit");
        }
        if sandbox.disk_limit.is_some() {
            sql.push_str(", disk_limit = $disk_limit");
        }
        if sandbox.container_id.is_some() {
            sql.push_str(", container_id = $container_id");
        }
        if sandbox.vm_id.is_some() {
            sql.push_str(", vm_id = $vm_id");
        }
        if sandbox.ip_address.is_some() {
            sql.push_str(", ip_address = $ip_address");
        }
        
        let mut query = conn.db()
            .query(&sql)
            .bind(("record_id", &record_id))
            .bind(("name", &sandbox.name))
            .bind(("runtime_type", &sandbox.runtime_type))
            .bind(("template", &sandbox.template))
            .bind(("status", &sandbox.status));
        
        // Bind optional fields
        if let Some(tenant_id) = sandbox.tenant_id {
            let tenant_record_id = uuid_to_record_id("tenants", tenant_id);
            query = query.bind(("tenant_id", tenant_record_id));
        }
        if let Some(cpu_limit) = sandbox.cpu_limit {
            query = query.bind(("cpu_limit", cpu_limit));
        }
        if let Some(memory_limit) = sandbox.memory_limit {
            query = query.bind(("memory_limit", memory_limit));
        }
        if let Some(disk_limit) = sandbox.disk_limit {
            query = query.bind(("disk_limit", disk_limit));
        }
        if let Some(ref container_id) = sandbox.container_id {
            query = query.bind(("container_id", container_id));
        }
        if let Some(ref vm_id) = sandbox.vm_id {
            query = query.bind(("vm_id", vm_id));
        }
        if let Some(ref ip_address) = sandbox.ip_address {
            query = query.bind(("ip_address", ip_address));
        }
        
        let mut response = query.await
            .map_err(|e| DatabaseError::Query(format!("更新沙盒失败: {}", e)))?;
        
        // Check if update was successful
        let result: Vec<serde_json::Value> = response.take::<Vec<serde_json::Value>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("更新失败: {}", e)))?;
        
        if result.is_empty() {
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
        
        let record_id = uuid_to_record_id("sandboxes", id);
        
        // Use direct SurrealQL UPDATE query
        let sql = "UPDATE $record_id SET status = $status, updated_at = time::now()";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("record_id", &record_id))
            .bind(("status", status))
            .await
            .map_err(|e| DatabaseError::Query(format!("更新沙盒状态失败: {}", e)))?;
        
        // Check if update was successful
        let result: Vec<serde_json::Value> = response.take::<Vec<serde_json::Value>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("更新失败: {}", e)))?;
        
        if result.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        
        Ok(())
    }
    
    /// 删除沙盒
    pub async fn delete(&self, id: Uuid) -> DatabaseResult<()> {
        debug!("删除沙盒: {}", id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let record_id = uuid_to_record_id("sandboxes", id);
        
        // Use direct SurrealQL DELETE query
        let sql = "DELETE $record_id";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("record_id", &record_id))
            .await
            .map_err(|e| DatabaseError::Query(format!("删除沙盒失败: {}", e)))?;
        
        // Check if deletion was successful
        let result: Vec<serde_json::Value> = response.take::<Vec<serde_json::Value>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("删除失败: {}", e)))?;
        
        if result.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        
        info!("Deleted sandbox: {}", id);
        Ok(())
    }
    
    /// 列出用户的沙盒 (简化版本 for MVP)
    pub async fn list_by_owner(
        &self,
        owner_id: Uuid,
        status: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> DatabaseResult<PaginatedResult<DbSandbox>> {
        debug!("列出用户 {} 的沙盒，状态: {:?}", owner_id, status);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let owner_record_id = uuid_to_record_id("users", owner_id);
        
        // Simple query without complex pagination for MVP
        let sql = if let Some(status) = status {
            "SELECT * FROM sandboxes WHERE owner_id = $owner_id AND status = $status ORDER BY created_at DESC LIMIT $limit"
        } else {
            "SELECT * FROM sandboxes WHERE owner_id = $owner_id ORDER BY created_at DESC LIMIT $limit"
        };
        
        let mut query = conn.db()
            .query(sql)
            .bind(("owner_id", &owner_record_id))
            .bind(("limit", page_size));
        
        if let Some(status) = status {
            query = query.bind(("status", status));
        }
        
        let mut response = query.await
            .map_err(|e| DatabaseError::Query(format!("查询用户沙盒失败: {}", e)))?;
        
        let sandboxes: Vec<SurrealSandbox> = response
            .take::<Vec<SurrealSandbox>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("解析查询结果失败: {}", e)))?;
        
        let mut db_sandboxes = Vec::new();
        for surreal_sandbox in sandboxes {
            db_sandboxes.push(self.surreal_to_db_sandbox(surreal_sandbox)?);
        }
        
        // Simple pagination for MVP - just return what we have
        Ok(PaginatedResult::new(
            db_sandboxes,
            0, // total count not implemented for MVP
            page,
            page_size,
        ))
    }
    
    /// 简单的按用户统计沙盒数量 (MVP版本)
    pub async fn count_by_owner(&self, owner_id: Uuid, _status: Option<&str>) -> DatabaseResult<i64> {
        debug!("统计用户 {} 的沙盒数量", owner_id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let owner_record_id = uuid_to_record_id("users", owner_id);
        
        // Simple count query for MVP
        let sql = "SELECT VALUE count() FROM sandboxes WHERE owner_id = $owner_id";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("owner_id", &owner_record_id))
            .await
            .map_err(|e| DatabaseError::Query(format!("统计沙盒数量失败: {}", e)))?;
        
        let count: Option<i64> = response
            .take::<Option<i64>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("解析统计结果失败: {}", e)))?;
        
        Ok(count.unwrap_or(0))
    }
    
    /// 清理过期的沙盒
    pub async fn cleanup_expired_sandboxes(&self) -> DatabaseResult<i64> {
        debug!("清理过期的沙盒");
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        // 删除已过期的沙盒
        let sql = "DELETE FROM sandboxes WHERE expires_at IS NOT NONE AND expires_at < time::now() RETURN count()";
        
        let mut response = conn.db()
            .query(sql)
            .await
            .map_err(|e| DatabaseError::Query(format!("清理过期沙盒失败: {}", e)))?;
        
        let result: Vec<serde_json::Value> = response
            .take(0usize)
            .map_err(|e| DatabaseError::Query(format!("解析清理结果失败: {}", e)))?;
        
        let count = result.first()
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as i64;
        
        if count > 0 {
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
// Note: From<SurrealConnectionError> implementation is in database/mod.rs to avoid duplicates