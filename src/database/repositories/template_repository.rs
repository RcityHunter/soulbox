use sqlx::Row;
use std::sync::Arc;
use tracing::{info, error};
use uuid::Uuid;

use crate::database::{Database, DatabaseError, DatabasePool, DatabaseResult, models::{DbTemplate, PaginatedResult}};

/// 模板仓库
pub struct TemplateRepository {
    db: Arc<Database>,
}

impl TemplateRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
    
    /// 创建模板
    pub async fn create(&self, template: &DbTemplate) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                sqlx::query!(
                    r#"
                    INSERT INTO templates (
                        id, name, slug, description, runtime_type, base_image,
                        default_command, environment_vars, resource_limits,
                        is_public, owner_id, created_at, updated_at
                    ) VALUES (
                        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
                    )
                    "#,
                    template.id,
                    template.name,
                    template.slug,
                    template.description,
                    template.runtime_type,
                    template.base_image,
                    template.default_command,
                    template.environment_vars,
                    template.resource_limits,
                    template.is_public,
                    template.owner_id,
                    template.created_at,
                    template.updated_at
                )
                .execute(pool)
                .await?;
            }
            DatabasePool::Sqlite(pool) => {
                let id = template.id.to_string();
                let owner_id = template.owner_id.map(|o| o.to_string());
                let created_at = template.created_at.to_rfc3339();
                let updated_at = template.updated_at.to_rfc3339();
                let environment_vars = template.environment_vars.as_ref().map(|e| e.to_string());
                let resource_limits = template.resource_limits.as_ref().map(|r| r.to_string());
                
                sqlx::query!(
                    r#"
                    INSERT INTO templates (
                        id, name, slug, description, runtime_type, base_image,
                        default_command, environment_vars, resource_limits,
                        is_public, owner_id, created_at, updated_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
                    )
                    "#,
                    id,
                    template.name,
                    template.slug,
                    template.description,
                    template.runtime_type,
                    template.base_image,
                    template.default_command,
                    environment_vars,
                    resource_limits,
                    template.is_public,
                    owner_id,
                    created_at,
                    updated_at
                )
                .execute(pool)
                .await?;
            }
        }
        
        info!("Created template: {} ({})", template.name, template.slug);
        Ok(())
    }
    
    /// 根据ID查找模板
    pub async fn find_by_id(&self, id: Uuid) -> DatabaseResult<Option<DbTemplate>> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let template = sqlx::query_as!(
                    DbTemplate,
                    "SELECT * FROM templates WHERE id = $1",
                    id
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(template)
            }
            DatabasePool::Sqlite(pool) => {
                let id_str = id.to_string();
                let template = sqlx::query_as!(
                    DbTemplate,
                    "SELECT * FROM templates WHERE id = ?1",
                    id_str
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(template)
            }
        }
    }
    
    /// 根据slug查找模板
    pub async fn find_by_slug(&self, slug: &str) -> DatabaseResult<Option<DbTemplate>> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let template = sqlx::query_as!(
                    DbTemplate,
                    "SELECT * FROM templates WHERE slug = $1",
                    slug
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(template)
            }
            DatabasePool::Sqlite(pool) => {
                let template = sqlx::query_as!(
                    DbTemplate,
                    "SELECT * FROM templates WHERE slug = ?1",
                    slug
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(template)
            }
        }
    }
    
    /// 更新模板
    pub async fn update(&self, template: &DbTemplate) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query!(
                    r#"
                    UPDATE templates 
                    SET name = $2, slug = $3, description = $4, runtime_type = $5,
                        base_image = $6, default_command = $7, environment_vars = $8,
                        resource_limits = $9, is_public = $10, updated_at = NOW()
                    WHERE id = $1
                    "#,
                    template.id,
                    template.name,
                    template.slug,
                    template.description,
                    template.runtime_type,
                    template.base_image,
                    template.default_command,
                    template.environment_vars,
                    template.resource_limits,
                    template.is_public
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
            DatabasePool::Sqlite(pool) => {
                let id = template.id.to_string();
                let environment_vars = template.environment_vars.as_ref().map(|e| e.to_string());
                let resource_limits = template.resource_limits.as_ref().map(|r| r.to_string());
                
                let result = sqlx::query!(
                    r#"
                    UPDATE templates 
                    SET name = ?2, slug = ?3, description = ?4, runtime_type = ?5,
                        base_image = ?6, default_command = ?7, environment_vars = ?8,
                        resource_limits = ?9, is_public = ?10, updated_at = datetime('now')
                    WHERE id = ?1
                    "#,
                    id,
                    template.name,
                    template.slug,
                    template.description,
                    template.runtime_type,
                    template.base_image,
                    template.default_command,
                    environment_vars,
                    resource_limits,
                    template.is_public
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
        }
        
        info!("Updated template: {}", template.slug);
        Ok(())
    }
    
    /// 删除模板
    pub async fn delete(&self, id: Uuid) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query!(
                    "DELETE FROM templates WHERE id = $1",
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
                    "DELETE FROM templates WHERE id = ?1",
                    id_str
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
        }
        
        info!("Deleted template: {}", id);
        Ok(())
    }
    
    /// 列出公共模板
    pub async fn list_public(
        &self,
        runtime_type: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> DatabaseResult<PaginatedResult<DbTemplate>> {
        let offset = ((page - 1) * page_size) as i64;
        let limit = page_size as i64;
        
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                if let Some(runtime_type) = runtime_type {
                    let templates = sqlx::query_as!(
                        DbTemplate,
                        r#"
                        SELECT * FROM templates 
                        WHERE is_public = true AND runtime_type = $1
                        ORDER BY created_at DESC 
                        LIMIT $2 OFFSET $3
                        "#,
                        runtime_type,
                        limit,
                        offset
                    )
                    .fetch_all(pool)
                    .await?;
                    
                    let total = sqlx::query!(
                        "SELECT COUNT(*) as count FROM templates WHERE is_public = true AND runtime_type = $1",
                        runtime_type
                    )
                    .fetch_one(pool)
                    .await?
                    .count
                    .unwrap_or(0);
                    
                    Ok(PaginatedResult::new(templates, total, page, page_size))
                } else {
                    let templates = sqlx::query_as!(
                        DbTemplate,
                        r#"
                        SELECT * FROM templates 
                        WHERE is_public = true
                        ORDER BY created_at DESC 
                        LIMIT $1 OFFSET $2
                        "#,
                        limit,
                        offset
                    )
                    .fetch_all(pool)
                    .await?;
                    
                    let total = sqlx::query!(
                        "SELECT COUNT(*) as count FROM templates WHERE is_public = true"
                    )
                    .fetch_one(pool)
                    .await?
                    .count
                    .unwrap_or(0);
                    
                    Ok(PaginatedResult::new(templates, total, page, page_size))
                }
            }
            DatabasePool::Sqlite(pool) => {
                if let Some(runtime_type) = runtime_type {
                    let templates = sqlx::query_as!(
                        DbTemplate,
                        r#"
                        SELECT * FROM templates 
                        WHERE is_public = 1 AND runtime_type = ?1
                        ORDER BY created_at DESC 
                        LIMIT ?2 OFFSET ?3
                        "#,
                        runtime_type,
                        limit,
                        offset
                    )
                    .fetch_all(pool)
                    .await?;
                    
                    let total = sqlx::query!(
                        "SELECT COUNT(*) as count FROM templates WHERE is_public = 1 AND runtime_type = ?1",
                        runtime_type
                    )
                    .fetch_one(pool)
                    .await?
                    .count;
                    
                    Ok(PaginatedResult::new(templates, total, page, page_size))
                } else {
                    let templates = sqlx::query_as!(
                        DbTemplate,
                        r#"
                        SELECT * FROM templates 
                        WHERE is_public = 1
                        ORDER BY created_at DESC 
                        LIMIT ?1 OFFSET ?2
                        "#,
                        limit,
                        offset
                    )
                    .fetch_all(pool)
                    .await?;
                    
                    let total = sqlx::query!(
                        "SELECT COUNT(*) as count FROM templates WHERE is_public = 1"
                    )
                    .fetch_one(pool)
                    .await?
                    .count;
                    
                    Ok(PaginatedResult::new(templates, total, page, page_size))
                }
            }
        }
    }
    
    /// 列出用户的模板
    pub async fn list_by_owner(
        &self,
        owner_id: Uuid,
        page: u32,
        page_size: u32,
    ) -> DatabaseResult<PaginatedResult<DbTemplate>> {
        let offset = ((page - 1) * page_size) as i64;
        let limit = page_size as i64;
        
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let templates = sqlx::query_as!(
                    DbTemplate,
                    r#"
                    SELECT * FROM templates 
                    WHERE owner_id = $1 
                    ORDER BY created_at DESC 
                    LIMIT $2 OFFSET $3
                    "#,
                    owner_id,
                    limit,
                    offset
                )
                .fetch_all(pool)
                .await?;
                
                let total = sqlx::query!(
                    "SELECT COUNT(*) as count FROM templates WHERE owner_id = $1",
                    owner_id
                )
                .fetch_one(pool)
                .await?
                .count
                .unwrap_or(0);
                
                Ok(PaginatedResult::new(templates, total, page, page_size))
            }
            DatabasePool::Sqlite(pool) => {
                let owner_id_str = owner_id.to_string();
                let templates = sqlx::query_as!(
                    DbTemplate,
                    r#"
                    SELECT * FROM templates 
                    WHERE owner_id = ?1 
                    ORDER BY created_at DESC 
                    LIMIT ?2 OFFSET ?3
                    "#,
                    owner_id_str,
                    limit,
                    offset
                )
                .fetch_all(pool)
                .await?;
                
                let total = sqlx::query!(
                    "SELECT COUNT(*) as count FROM templates WHERE owner_id = ?1",
                    owner_id_str
                )
                .fetch_one(pool)
                .await?
                .count;
                
                Ok(PaginatedResult::new(templates, total, page, page_size))
            }
        }
    }
}