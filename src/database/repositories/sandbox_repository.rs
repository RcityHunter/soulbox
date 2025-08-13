use chrono::{DateTime, Utc};
use sqlx::{QueryBuilder, Row};
use std::sync::Arc;
use tracing::{info, error};
use uuid::Uuid;

use crate::database::{Database, DatabaseError, DatabasePool, DatabaseResult, models::{DbSandbox, PaginatedResult}};

/// 沙盒仓库
pub struct SandboxRepository {
    db: Arc<Database>,
}

impl SandboxRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
    
    /// 创建沙盒
    pub async fn create(&self, sandbox: &DbSandbox) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                sqlx::query!(
                    r#"
                    INSERT INTO sandboxes (
                        id, name, runtime_type, template, status, owner_id, tenant_id,
                        cpu_limit, memory_limit, disk_limit,
                        container_id, vm_id, ip_address, port_mappings,
                        created_at, updated_at, expires_at
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17
                    )
                    "#,
                    sandbox.id,
                    sandbox.name,
                    sandbox.runtime_type,
                    sandbox.template,
                    sandbox.status,
                    sandbox.owner_id,
                    sandbox.tenant_id,
                    sandbox.cpu_limit,
                    sandbox.memory_limit,
                    sandbox.disk_limit,
                    sandbox.container_id,
                    sandbox.vm_id,
                    sandbox.ip_address,
                    sandbox.port_mappings,
                    sandbox.created_at,
                    sandbox.updated_at,
                    sandbox.expires_at
                )
                .execute(pool)
                .await?;
            }
            DatabasePool::Sqlite(pool) => {
                let id = sandbox.id.to_string();
                let owner_id = sandbox.owner_id.to_string();
                let tenant_id = sandbox.tenant_id.map(|t| t.to_string());
                let created_at = sandbox.created_at.to_rfc3339();
                let updated_at = sandbox.updated_at.to_rfc3339();
                let expires_at = sandbox.expires_at.map(|e| e.to_rfc3339());
                let port_mappings = sandbox.port_mappings.as_ref().map(|p| p.to_string());
                
                sqlx::query!(
                    r#"
                    INSERT INTO sandboxes (
                        id, name, runtime_type, template, status, owner_id, tenant_id,
                        cpu_limit, memory_limit, disk_limit,
                        container_id, vm_id, ip_address, port_mappings,
                        created_at, updated_at, expires_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17
                    )
                    "#,
                    id,
                    sandbox.name,
                    sandbox.runtime_type,
                    sandbox.template,
                    sandbox.status,
                    owner_id,
                    tenant_id,
                    sandbox.cpu_limit,
                    sandbox.memory_limit,
                    sandbox.disk_limit,
                    sandbox.container_id,
                    sandbox.vm_id,
                    sandbox.ip_address,
                    port_mappings,
                    created_at,
                    updated_at,
                    expires_at
                )
                .execute(pool)
                .await?;
            }
        }
        
        info!("Created sandbox: {} ({})", sandbox.name, sandbox.id);
        Ok(())
    }
    
    /// 根据ID查找沙盒
    pub async fn find_by_id(&self, id: Uuid) -> DatabaseResult<Option<DbSandbox>> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let sandbox = sqlx::query_as!(
                    DbSandbox,
                    "SELECT * FROM sandboxes WHERE id = $1",
                    id
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(sandbox)
            }
            DatabasePool::Sqlite(pool) => {
                let id_str = id.to_string();
                let sandbox = sqlx::query_as!(
                    DbSandbox,
                    "SELECT * FROM sandboxes WHERE id = ?1",
                    id_str
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(sandbox)
            }
        }
    }
    
    /// 更新沙盒状态
    pub async fn update_status(&self, id: Uuid, status: &str) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query!(
                    "UPDATE sandboxes SET status = $2, updated_at = NOW() WHERE id = $1",
                    id,
                    status
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
            DatabasePool::Sqlite(pool) => {
                let id_str = id.to_string();
                let result = sqlx::query!(
                    "UPDATE sandboxes SET status = ?2, updated_at = datetime('now') WHERE id = ?1",
                    id_str,
                    status
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
        }
        
        Ok(())
    }
    
    /// 更新沙盒运行时信息
    pub async fn update_runtime_info(
        &self,
        id: Uuid,
        container_id: Option<&str>,
        vm_id: Option<&str>,
        ip_address: Option<&str>,
    ) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query!(
                    r#"
                    UPDATE sandboxes 
                    SET container_id = $2, vm_id = $3, ip_address = $4, 
                        started_at = NOW(), updated_at = NOW()
                    WHERE id = $1
                    "#,
                    id,
                    container_id,
                    vm_id,
                    ip_address
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
            DatabasePool::Sqlite(pool) => {
                let id_str = id.to_string();
                let result = sqlx::query!(
                    r#"
                    UPDATE sandboxes 
                    SET container_id = ?2, vm_id = ?3, ip_address = ?4, 
                        started_at = datetime('now'), updated_at = datetime('now')
                    WHERE id = ?1
                    "#,
                    id_str,
                    container_id,
                    vm_id,
                    ip_address
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
        }
        
        Ok(())
    }
    
    /// 记录沙盒停止时间
    pub async fn mark_stopped(&self, id: Uuid) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                sqlx::query!(
                    "UPDATE sandboxes SET stopped_at = NOW(), updated_at = NOW() WHERE id = $1",
                    id
                )
                .execute(pool)
                .await?;
            }
            DatabasePool::Sqlite(pool) => {
                let id_str = id.to_string();
                sqlx::query!(
                    "UPDATE sandboxes SET stopped_at = datetime('now'), updated_at = datetime('now') WHERE id = ?1",
                    id_str
                )
                .execute(pool)
                .await?;
            }
        }
        
        Ok(())
    }
    
    /// 删除沙盒
    pub async fn delete(&self, id: Uuid) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query!(
                    "DELETE FROM sandboxes WHERE id = $1",
                    id
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
            DatabasePool::Sqlite(pool) => {
                let id_str = id.to_string();
                let result = sqlx::query!(
                    "DELETE FROM sandboxes WHERE id = ?1",
                    id_str
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
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
        let offset = ((page - 1) * page_size) as i64;
        let limit = page_size as i64;
        
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let mut query_builder = QueryBuilder::new(
                    "SELECT * FROM sandboxes WHERE owner_id = "
                );
                query_builder.push_bind(owner_id);
                
                if let Some(status) = status {
                    query_builder.push(" AND status = ");
                    query_builder.push_bind(status);
                }
                
                query_builder.push(" ORDER BY created_at DESC LIMIT ");
                query_builder.push_bind(limit);
                query_builder.push(" OFFSET ");
                query_builder.push_bind(offset);
                
                let sandboxes = query_builder
                    .build_query_as::<DbSandbox>()
                    .fetch_all(pool)
                    .await?;
                
                // 计算总数
                let mut count_builder = QueryBuilder::new(
                    "SELECT COUNT(*) as count FROM sandboxes WHERE owner_id = "
                );
                count_builder.push_bind(owner_id);
                
                if let Some(status) = status {
                    count_builder.push(" AND status = ");
                    count_builder.push_bind(status);
                }
                
                let total = count_builder
                    .build()
                    .fetch_one(pool)
                    .await?
                    .get::<i64, _>("count");
                
                Ok(PaginatedResult::new(sandboxes, total, page, page_size))
            }
            DatabasePool::Sqlite(pool) => {
                let owner_id_str = owner_id.to_string();
                let mut query_builder = QueryBuilder::new(
                    "SELECT * FROM sandboxes WHERE owner_id = "
                );
                query_builder.push_bind(&owner_id_str);
                
                if let Some(status) = status {
                    query_builder.push(" AND status = ");
                    query_builder.push_bind(status);
                }
                
                query_builder.push(" ORDER BY created_at DESC LIMIT ");
                query_builder.push_bind(limit);
                query_builder.push(" OFFSET ");
                query_builder.push_bind(offset);
                
                let sandboxes = query_builder
                    .build_query_as::<DbSandbox>()
                    .fetch_all(pool)
                    .await?;
                
                // 计算总数
                let mut count_builder = QueryBuilder::new(
                    "SELECT COUNT(*) as count FROM sandboxes WHERE owner_id = "
                );
                count_builder.push_bind(&owner_id_str);
                
                if let Some(status) = status {
                    count_builder.push(" AND status = ");
                    count_builder.push_bind(status);
                }
                
                let total = count_builder
                    .build()
                    .fetch_one(pool)
                    .await?
                    .get::<i64, _>("count");
                
                Ok(PaginatedResult::new(sandboxes, total, page, page_size))
            }
        }
    }
    
    /// 列出租户的沙盒
    pub async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        page: u32,
        page_size: u32,
    ) -> DatabaseResult<PaginatedResult<DbSandbox>> {
        let offset = ((page - 1) * page_size) as i64;
        let limit = page_size as i64;
        
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let sandboxes = sqlx::query_as!(
                    DbSandbox,
                    r#"
                    SELECT * FROM sandboxes 
                    WHERE tenant_id = $1 
                    ORDER BY created_at DESC 
                    LIMIT $2 OFFSET $3
                    "#,
                    tenant_id,
                    limit,
                    offset
                )
                .fetch_all(pool)
                .await?;
                
                let total = sqlx::query!(
                    "SELECT COUNT(*) as count FROM sandboxes WHERE tenant_id = $1",
                    tenant_id
                )
                .fetch_one(pool)
                .await?
                .count
                .unwrap_or(0);
                
                Ok(PaginatedResult::new(sandboxes, total, page, page_size))
            }
            DatabasePool::Sqlite(pool) => {
                let tenant_id_str = tenant_id.to_string();
                let sandboxes = sqlx::query_as!(
                    DbSandbox,
                    r#"
                    SELECT * FROM sandboxes 
                    WHERE tenant_id = ?1 
                    ORDER BY created_at DESC 
                    LIMIT ?2 OFFSET ?3
                    "#,
                    tenant_id_str,
                    limit,
                    offset
                )
                .fetch_all(pool)
                .await?;
                
                let total = sqlx::query!(
                    "SELECT COUNT(*) as count FROM sandboxes WHERE tenant_id = ?1",
                    tenant_id_str
                )
                .fetch_one(pool)
                .await?
                .count;
                
                Ok(PaginatedResult::new(sandboxes, total, page, page_size))
            }
        }
    }
    
    /// 清理过期的沙盒
    pub async fn cleanup_expired(&self) -> DatabaseResult<u64> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query!(
                    "DELETE FROM sandboxes WHERE expires_at < NOW() AND status != 'stopped'",
                )
                .execute(pool)
                .await?;
                
                Ok(result.rows_affected())
            }
            DatabasePool::Sqlite(pool) => {
                let result = sqlx::query!(
                    "DELETE FROM sandboxes WHERE expires_at < datetime('now') AND status != 'stopped'",
                )
                .execute(pool)
                .await?;
                
                Ok(result.rows_affected())
            }
        }
    }
}