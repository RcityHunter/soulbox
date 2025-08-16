use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, error, debug};
use uuid::Uuid;

use crate::database::surrealdb::{
    SurrealPool, SurrealOperations, 
    uuid_to_record_id, record_id_to_uuid, SurrealResult, SurrealConnectionError, PaginationResult
};
use crate::database::{DatabaseError, DatabaseResult, models::{DbTemplate, PaginatedResult}};

/// SurrealDB 模板模型
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SurrealTemplate {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub runtime_type: String,
    pub base_image: String,
    pub default_command: Option<String>,
    pub environment_vars: Option<serde_json::Value>,
    pub resource_limits: Option<serde_json::Value>,
    pub is_public: bool,
    pub owner_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// 模板仓库
pub struct TemplateRepository {
    pool: Arc<SurrealPool>,
}

impl TemplateRepository {
    pub fn new(pool: Arc<SurrealPool>) -> Self {
        Self { pool }
    }
    
    /// 创建模板
    pub async fn create(&self, template: &DbTemplate) -> DatabaseResult<()> {
        debug!("创建模板: {} ({})", template.name, template.id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let surreal_template = SurrealTemplate {
            id: uuid_to_record_id("templates", template.id),
            name: template.name.clone(),
            slug: template.slug.clone(),
            description: template.description.clone(),
            runtime_type: template.runtime_type.clone(),
            base_image: template.base_image.clone(),
            default_command: template.default_command.clone(),
            environment_vars: template.environment_vars.clone(),
            resource_limits: template.resource_limits.clone(),
            is_public: template.is_public,
            owner_id: template.owner_id.map(|id| uuid_to_record_id("users", id)),
            created_at: template.created_at,
            updated_at: template.updated_at,
        };
        
        ops.create("templates", &surreal_template).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        info!("Created template: {} ({})", template.name, template.id);
        Ok(())
    }
    
    /// 根据ID查找模板
    pub async fn find_by_id(&self, id: Uuid) -> DatabaseResult<Option<DbTemplate>> {
        debug!("根据ID查找模板: {}", id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        match ops.find_by_id::<SurrealTemplate>("templates", &id.to_string()).await {
            Ok(Some(surreal_template)) => {
                let db_template = self.surreal_to_db_template(surreal_template)?;
                Ok(Some(db_template))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(DatabaseError::Query(e.to_string())),
        }
    }
    
    /// 根据slug查找模板
    pub async fn find_by_slug(&self, slug: &str) -> DatabaseResult<Option<DbTemplate>> {
        debug!("根据slug查找模板: {}", slug);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let query = SurrealQueryBuilder::new("templates")
            .where_clause(format!("slug = '{}'", slug));
        
        match ops.find_where::<SurrealTemplate>(query).await {
            Ok(mut templates) => {
                if let Some(surreal_template) = templates.pop() {
                    let db_template = self.surreal_to_db_template(surreal_template)?;
                    Ok(Some(db_template))
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(DatabaseError::Query(e.to_string())),
        }
    }
    
    /// 列出公共模板
    pub async fn list_public(&self, page: u32, page_size: u32) -> DatabaseResult<PaginatedResult<DbTemplate>> {
        debug!("列出公共模板，页码: {}, 大小: {}", page, page_size);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let query = SurrealQueryBuilder::new("templates")
            .where_clause("is_public = true")
            .order_by("created_at DESC");
        
        let pagination_result = ops.find_paginated::<SurrealTemplate>(query, page as usize, page_size as usize).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        let mut db_templates = Vec::new();
        for surreal_template in pagination_result.items {
            db_templates.push(self.surreal_to_db_template(surreal_template)?);
        }
        
        Ok(PaginatedResult::new(
            db_templates,
            pagination_result.total,
            pagination_result.page as u32,
            pagination_result.page_size as u32,
        ))
    }
    
    /// 更新模板
    pub async fn update(&self, template: &DbTemplate) -> DatabaseResult<()> {
        debug!("更新模板: {} ({})", template.name, template.id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let update = UpdateBuilder::new("templates")
            .set(format!("name = '{}'", template.name))
            .set(format!("slug = '{}'", template.slug))
            .set(format!("runtime_type = '{}'", template.runtime_type))
            .set(format!("base_image = '{}'", template.base_image))
            .set(format!("is_public = {}", template.is_public))
            .set("updated_at = time::now()")
            .where_clause(format!("id = {}", uuid_to_record_id("templates", template.id)));
        
        let results: Vec<SurrealTemplate> = ops.update_where(update).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        if results.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        
        info!("Updated template: {} ({})", template.name, template.id);
        Ok(())
    }
    
    /// 删除模板
    pub async fn delete(&self, id: Uuid) -> DatabaseResult<()> {
        debug!("删除模板: {}", id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let deleted = ops.delete("templates", &id.to_string()).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        if !deleted {
            return Err(DatabaseError::NotFound);
        }
        
        info!("Deleted template: {}", id);
        Ok(())
    }
    
    /// 将 SurrealTemplate 转换为 DbTemplate
    fn surreal_to_db_template(&self, surreal_template: SurrealTemplate) -> DatabaseResult<DbTemplate> {
        let id_str = surreal_template.id.split(':').last()
            .ok_or_else(|| DatabaseError::Other("Invalid template record ID format".to_string()))?;
        
        let id = id_str.parse::<Uuid>()
            .map_err(|e| DatabaseError::Other(format!("Failed to parse template UUID: {}", e)))?;
        
        let owner_id = if let Some(owner_record_id) = &surreal_template.owner_id {
            let owner_id_str = owner_record_id.split(':').last()
                .ok_or_else(|| DatabaseError::Other("Invalid owner record ID format".to_string()))?;
            Some(owner_id_str.parse::<Uuid>()
                .map_err(|e| DatabaseError::Other(format!("Failed to parse owner UUID: {}", e)))?)
        } else {
            None
        };
        
        Ok(DbTemplate {
            id,
            name: surreal_template.name,
            slug: surreal_template.slug,
            description: surreal_template.description,
            runtime_type: surreal_template.runtime_type,
            base_image: surreal_template.base_image,
            default_command: surreal_template.default_command,
            environment_vars: surreal_template.environment_vars,
            resource_limits: surreal_template.resource_limits,
            is_public: surreal_template.is_public,
            owner_id,
            created_at: surreal_template.created_at,
            updated_at: surreal_template.updated_at,
        })
    }
}

// Convert DatabaseError from SurrealConnectionError
// Note: From<SurrealConnectionError> implementation is in database/mod.rs to avoid duplicates