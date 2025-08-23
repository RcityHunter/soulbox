use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, debug};
use uuid::Uuid;

use crate::template::models::{
    Template, TemplateMetadata, ResourceLimits, TemplateFile, TemplateVersion,
    CreateTemplateRequest, UpdateTemplateRequest, TemplateFileUpload
};
use crate::database::surrealdb::{
    SurrealPool, SurrealOperations, uuid_to_record_id
};
use crate::database::{DatabaseError, DatabaseResult};

/// SurrealDB template model
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SurrealTemplate {
    pub id: String, // SurrealDB record ID
    pub metadata: TemplateMetadata,
    pub base_image: String,
    pub default_command: Option<String>,
    pub environment_vars: HashMap<String, String>,
    pub resource_limits: ResourceLimits,
    pub files: Vec<TemplateFile>,
    pub setup_commands: Vec<String>,
    pub versions: Vec<TemplateVersion>,
    pub owner_id: Option<String>,
    pub tenant_id: Option<String>,
}

/// Template repository for SurrealDB operations
pub struct TemplateRepository {
    pool: Arc<SurrealPool>,
}

impl TemplateRepository {
    pub fn new(pool: Arc<SurrealPool>) -> Self {
        Self { pool }
    }

    /// Create a stub repository that returns errors for all operations
    /// This is used as a fallback when no database is available
    pub fn new_stub() -> Self {
        // Create a stub pool - operations will fail but won't panic
        let config = crate::database::surrealdb::SurrealConfig::memory();
        let pool = Arc::new(
            futures::executor::block_on(crate::database::surrealdb::SurrealPool::new(config))
                .unwrap_or_else(|_| panic!("Failed to create stub SurrealDB pool"))
        );
        Self { pool }
    }

    /// Create a new template
    pub async fn create(&self, template: &Template) -> DatabaseResult<()> {
        debug!("Creating template: {}", template.metadata.name);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let surreal_template = self.template_to_surreal(template);
        
        ops.create("templates", &surreal_template).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        info!("Created template: {}", template.metadata.name);
        Ok(())
    }

    /// Find template by ID
    pub async fn find_by_id(&self, id: Uuid) -> DatabaseResult<Option<Template>> {
        debug!("Finding template by ID: {}", id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let record_id = uuid_to_record_id("templates", id);
        let sql = "SELECT * FROM $record_id";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("record_id", &record_id))
            .await
            .map_err(|e| DatabaseError::Query(format!("Query template failed: {}", e)))?;
        
        let templates: Vec<SurrealTemplate> = response
            .take::<Vec<SurrealTemplate>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("Parse query result failed: {}", e)))?;
        
        if let Some(surreal_template) = templates.into_iter().next() {
            let template = self.surreal_to_template(surreal_template)?;
            Ok(Some(template))
        } else {
            Ok(None)
        }
    }

    /// Find template by slug
    pub async fn find_by_slug(&self, slug: &str) -> DatabaseResult<Option<Template>> {
        debug!("Finding template by slug: {}", slug);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let sql = "SELECT * FROM templates WHERE metadata.slug = $slug";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("slug", slug))
            .await
            .map_err(|e| DatabaseError::Query(format!("Query template failed: {}", e)))?;
        
        let templates: Vec<SurrealTemplate> = response
            .take::<Vec<SurrealTemplate>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("Parse query result failed: {}", e)))?;
        
        if let Some(surreal_template) = templates.into_iter().next() {
            let template = self.surreal_to_template(surreal_template)?;
            Ok(Some(template))
        } else {
            Ok(None)
        }
    }

    /// Update template
    pub async fn update(&self, id: Uuid, update_request: &UpdateTemplateRequest) -> DatabaseResult<Template> {
        debug!("Updating template: {}", id);
        
        // First, get the existing template
        let existing = self.find_by_id(id).await?
            .ok_or(DatabaseError::NotFound)?;
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let record_id = uuid_to_record_id("templates", id);
        
        // Build update query dynamically based on provided fields
        let mut update_fields = Vec::new();
        let mut bindings = vec![("record_id", record_id.clone())];
        
        if let Some(name) = &update_request.name {
            update_fields.push("metadata.name = $name");
            bindings.push(("name", name.clone()));
        }
        
        if let Some(description) = &update_request.description {
            update_fields.push("metadata.description = $description");
            bindings.push(("description", description.clone()));
        }
        
        if let Some(base_image) = &update_request.base_image {
            update_fields.push("base_image = $base_image");
            bindings.push(("base_image", base_image.clone()));
        }
        
        if let Some(default_command) = &update_request.default_command {
            update_fields.push("default_command = $default_command");
            bindings.push(("default_command", default_command.clone()));
        }
        
        if let Some(is_public) = &update_request.is_public {
            update_fields.push("metadata.is_public = $is_public");
            bindings.push(("is_public", is_public.to_string()));
        }
        
        // Always update the updated_at timestamp
        update_fields.push("metadata.updated_at = time::now()");
        
        if update_fields.is_empty() {
            return Ok(existing); // No updates needed
        }
        
        let sql = format!("UPDATE $record_id SET {}", update_fields.join(", "));
        
        let mut query = conn.db().query(&sql);
        for (key, value) in bindings {
            query = query.bind((key, value));
        }
        
        let mut response = query.await
            .map_err(|e| DatabaseError::Query(format!("Update template failed: {}", e)))?;
        
        let result: Vec<serde_json::Value> = response.take::<Vec<serde_json::Value>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("Update failed: {}", e)))?;
        
        if result.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        
        // Return the updated template
        self.find_by_id(id).await?
            .ok_or(DatabaseError::NotFound)
    }

    /// Delete template
    pub async fn delete(&self, id: Uuid) -> DatabaseResult<()> {
        debug!("Deleting template: {}", id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let record_id = uuid_to_record_id("templates", id);
        let sql = "DELETE $record_id";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("record_id", &record_id))
            .await
            .map_err(|e| DatabaseError::Query(format!("Delete template failed: {}", e)))?;
        
        let result: Vec<serde_json::Value> = response.take::<Vec<serde_json::Value>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("Delete failed: {}", e)))?;
        
        if result.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        
        info!("Deleted template: {}", id);
        Ok(())
    }

    /// List public templates
    pub async fn list_public(&self, runtime_type: Option<&str>, page: usize, page_size: usize) -> DatabaseResult<Vec<Template>> {
        debug!("Listing public templates, runtime_type: {:?}, page: {}, page_size: {}", runtime_type, page, page_size);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let offset = (page.saturating_sub(1)) * page_size;
        
        let (sql, bindings) = if let Some(runtime) = runtime_type {
            (
                "SELECT * FROM templates WHERE metadata.is_public = true AND metadata.runtime_type = $runtime_type ORDER BY metadata.created_at DESC LIMIT $limit START $offset",
                vec![
                    ("runtime_type", runtime.to_string()),
                    ("limit", page_size.to_string()),
                    ("offset", offset.to_string()),
                ]
            )
        } else {
            (
                "SELECT * FROM templates WHERE metadata.is_public = true ORDER BY metadata.created_at DESC LIMIT $limit START $offset",
                vec![
                    ("limit", page_size.to_string()),
                    ("offset", offset.to_string()),
                ]
            )
        };
        
        let mut query = conn.db().query(sql);
        for (key, value) in bindings {
            query = query.bind((key, value));
        }
        
        let mut response = query.await
            .map_err(|e| DatabaseError::Query(format!("List public templates failed: {}", e)))?;
        
        let surreal_templates: Vec<SurrealTemplate> = response
            .take::<Vec<SurrealTemplate>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("Parse query result failed: {}", e)))?;
        
        let mut templates = Vec::new();
        for surreal_template in surreal_templates {
            templates.push(self.surreal_to_template(surreal_template)?);
        }
        
        Ok(templates)
    }

    /// List user templates
    pub async fn list_user(&self, user_id: Uuid, page: usize, page_size: usize) -> DatabaseResult<Vec<Template>> {
        debug!("Listing user templates for user: {}, page: {}, page_size: {}", user_id, page, page_size);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let owner_record_id = uuid_to_record_id("users", user_id);
        let offset = (page.saturating_sub(1)) * page_size;
        
        let sql = "SELECT * FROM templates WHERE owner_id = $owner_id ORDER BY metadata.created_at DESC LIMIT $limit START $offset";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("owner_id", &owner_record_id))
            .bind(("limit", page_size))
            .bind(("offset", offset))
            .await
            .map_err(|e| DatabaseError::Query(format!("List user templates failed: {}", e)))?;
        
        let surreal_templates: Vec<SurrealTemplate> = response
            .take::<Vec<SurrealTemplate>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("Parse query result failed: {}", e)))?;
        
        let mut templates = Vec::new();
        for surreal_template in surreal_templates {
            templates.push(self.surreal_to_template(surreal_template)?);
        }
        
        Ok(templates)
    }

    /// List templates with filters and pagination
    pub async fn list(&self, runtime_type: Option<String>, is_public: Option<bool>, owner_id: Option<Uuid>, page: u32, page_size: u32) -> DatabaseResult<(Vec<Template>, u64)> {
        debug!("Listing templates with filters - runtime_type: {:?}, is_public: {:?}, owner_id: {:?}, page: {}, page_size: {}", 
               runtime_type, is_public, owner_id, page, page_size);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let offset = ((page - 1) * page_size) as usize;
        
        // Build WHERE clause dynamically
        let mut where_conditions = Vec::new();
        let mut bindings = Vec::new();
        
        if let Some(runtime) = &runtime_type {
            where_conditions.push("metadata.runtime_type = $runtime_type");
            bindings.push(("runtime_type", runtime.clone()));
        }
        
        if let Some(public) = is_public {
            where_conditions.push("metadata.is_public = $is_public");
            bindings.push(("is_public", public.to_string()));
        }
        
        if let Some(owner) = owner_id {
            let owner_record_id = uuid_to_record_id("users", owner);
            where_conditions.push("owner_id = $owner_id");
            bindings.push(("owner_id", owner_record_id));
        }
        
        let where_clause = if where_conditions.is_empty() {
            "".to_string()
        } else {
            format!("WHERE {}", where_conditions.join(" AND "))
        };
        
        // Query for templates
        let sql = format!("SELECT * FROM templates {} ORDER BY metadata.created_at DESC LIMIT $limit START $offset", where_clause);
        
        let mut query = conn.db().query(&sql);
        for (key, value) in &bindings {
            query = query.bind((key, value.clone()));
        }
        query = query.bind(("limit", page_size as usize));
        query = query.bind(("offset", offset));
        
        let mut response = query.await
            .map_err(|e| DatabaseError::Query(format!("List templates failed: {}", e)))?;
        
        let surreal_templates: Vec<SurrealTemplate> = response
            .take::<Vec<SurrealTemplate>>(0usize)
            .map_err(|e| DatabaseError::Query(format!("Parse query result failed: {}", e)))?;
        
        // Query for total count
        let count_sql = format!("SELECT count() FROM templates {} GROUP ALL", where_clause);
        let mut count_query = conn.db().query(&count_sql);
        for (key, value) in bindings {
            count_query = count_query.bind((key, value));
        }
        
        let mut count_response = count_query.await
            .map_err(|e| DatabaseError::Query(format!("Count templates failed: {}", e)))?;
        
        let count_results: Vec<serde_json::Value> = count_response
            .take(0usize)
            .map_err(|e| DatabaseError::Query(format!("Parse count result failed: {}", e)))?;
        
        let total_count = if let Some(count_value) = count_results.first() {
            count_value.as_u64().unwrap_or(0)
        } else {
            0
        };
        
        let mut templates = Vec::new();
        for surreal_template in surreal_templates {
            templates.push(self.surreal_to_template(surreal_template)?);
        }
        
        Ok((templates, total_count))
    }

    /// Clone template
    pub async fn clone_template(&self, template_id: Uuid, user_id: Uuid) -> DatabaseResult<Template> {
        debug!("Cloning template: {} for user: {}", template_id, user_id);
        
        // Get the original template
        let original = self.find_by_id(template_id).await?
            .ok_or(DatabaseError::NotFound)?;
        
        // Create a new template based on the original
        let mut cloned = original.clone();
        cloned.metadata.id = Uuid::new_v4();
        cloned.metadata.name = format!("{} (Copy)", cloned.metadata.name);
        cloned.metadata.slug = format!("{}-copy-{}", cloned.metadata.slug, cloned.metadata.id.simple());
        cloned.metadata.is_public = false; // Cloned templates are private by default
        cloned.metadata.is_verified = false;
        cloned.metadata.created_at = Utc::now();
        cloned.metadata.updated_at = Utc::now();
        cloned.owner_id = Some(user_id);
        
        // Save the cloned template
        self.create(&cloned).await?;
        
        Ok(cloned)
    }

    /// Create template from request
    pub async fn create_from_request(&self, request: &CreateTemplateRequest, owner_id: Uuid, tenant_id: Option<Uuid>) -> DatabaseResult<Template> {
        debug!("Creating template from request: {}", request.name);
        
        let now = Utc::now();
        let template_id = Uuid::new_v4();
        
        let metadata = TemplateMetadata {
            id: template_id,
            name: request.name.clone(),
            slug: request.slug.clone(),
            description: request.description.clone(),
            runtime_type: request.runtime_type.clone(),
            version: "1.0.0".to_string(),
            author: None,
            tags: request.tags.clone().unwrap_or_default(),
            is_public: request.is_public.unwrap_or(false),
            is_verified: false,
            created_at: now,
            updated_at: now,
        };
        
        let version = TemplateVersion {
            version: "1.0.0".to_string(),
            changelog: Some("Initial version".to_string()),
            created_at: now,
            is_stable: true,
            dependencies: vec![],
        };
        
        let template = Template {
            metadata,
            base_image: request.base_image.clone(),
            default_command: request.default_command.clone(),
            environment_vars: request.environment_vars.clone().unwrap_or_default(),
            resource_limits: request.resource_limits.clone().unwrap_or_default(),
            files: self.convert_file_uploads(&request.files.clone().unwrap_or_default())?,
            setup_commands: request.setup_commands.clone().unwrap_or_default(),
            versions: vec![version],
            owner_id: Some(owner_id),
            tenant_id,
        };
        
        self.create(&template).await?;
        Ok(template)
    }

    /// Convert template file uploads to template files
    fn convert_file_uploads(&self, uploads: &[TemplateFileUpload]) -> DatabaseResult<Vec<TemplateFile>> {
        let mut files = Vec::new();
        let now = Utc::now();
        
        for upload in uploads {
            // In a real implementation, you would save the file content to storage
            // and calculate the checksum. For now, we'll just store the metadata.
            files.push(TemplateFile {
                path: upload.path.clone(),
                size: upload.content.len() as u64,
                mime_type: upload.mime_type.clone(),
                checksum: None, // TODO: Calculate SHA-256 checksum
                created_at: now,
            });
        }
        
        Ok(files)
    }

    /// Convert Template to SurrealTemplate
    fn template_to_surreal(&self, template: &Template) -> SurrealTemplate {
        SurrealTemplate {
            id: uuid_to_record_id("templates", template.metadata.id),
            metadata: template.metadata.clone(),
            base_image: template.base_image.clone(),
            default_command: template.default_command.clone(),
            environment_vars: template.environment_vars.clone(),
            resource_limits: template.resource_limits.clone(),
            files: template.files.clone(),
            setup_commands: template.setup_commands.clone(),
            versions: template.versions.clone(),
            owner_id: template.owner_id.map(|id| uuid_to_record_id("users", id)),
            tenant_id: template.tenant_id.map(|id| uuid_to_record_id("tenants", id)),
        }
    }

    /// Convert SurrealTemplate to Template
    fn surreal_to_template(&self, surreal_template: SurrealTemplate) -> DatabaseResult<Template> {
        let owner_id = if let Some(owner_record_id) = &surreal_template.owner_id {
            let owner_id_str = owner_record_id.split(':').last()
                .ok_or_else(|| DatabaseError::Other("Invalid owner record ID format".to_string()))?;
            Some(owner_id_str.parse::<Uuid>()
                .map_err(|e| DatabaseError::Other(format!("Failed to parse owner UUID: {}", e)))?)
        } else {
            None
        };
        
        let tenant_id = if let Some(tenant_record_id) = &surreal_template.tenant_id {
            let tenant_id_str = tenant_record_id.split(':').last()
                .ok_or_else(|| DatabaseError::Other("Invalid tenant record ID format".to_string()))?;
            Some(tenant_id_str.parse::<Uuid>()
                .map_err(|e| DatabaseError::Other(format!("Failed to parse tenant UUID: {}", e)))?)
        } else {
            None
        };
        
        Ok(Template {
            metadata: surreal_template.metadata,
            base_image: surreal_template.base_image,
            default_command: surreal_template.default_command,
            environment_vars: surreal_template.environment_vars,
            resource_limits: surreal_template.resource_limits,
            files: surreal_template.files,
            setup_commands: surreal_template.setup_commands,
            versions: surreal_template.versions,
            owner_id,
            tenant_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::surrealdb::SurrealConfig;
    use crate::template::models::RuntimeType;

    async fn setup_test_db() -> Arc<SurrealPool> {
        let config = SurrealConfig::memory();
        Arc::new(SurrealPool::new(config).await.expect("Failed to create test database"))
    }

    #[tokio::test]
    async fn test_template_repository_crud() {
        let pool = setup_test_db().await;
        let repo = TemplateRepository::new(pool);
        
        // Create a test template
        let template = Template::default_python();
        
        // Test create
        repo.create(&template).await.expect("Failed to create template");
        
        // Test find by ID
        let found = repo.find_by_id(template.metadata.id).await
            .expect("Failed to find template")
            .expect("Template not found");
        
        assert_eq!(found.metadata.name, template.metadata.name);
        assert_eq!(found.metadata.slug, template.metadata.slug);
        
        // Test find by slug
        let found_by_slug = repo.find_by_slug(&template.metadata.slug).await
            .expect("Failed to find template by slug")
            .expect("Template not found by slug");
        
        assert_eq!(found_by_slug.metadata.id, template.metadata.id);
        
        // Test update
        let update_request = UpdateTemplateRequest {
            name: Some("Updated Python Template".to_string()),
            description: Some("Updated description".to_string()),
            base_image: None,
            default_command: None,
            environment_vars: None,
            resource_limits: None,
            files: None,
            setup_commands: None,
            tags: None,
            is_public: None,
        };
        
        let updated = repo.update(template.metadata.id, &update_request).await
            .expect("Failed to update template");
        
        assert_eq!(updated.metadata.name, "Updated Python Template");
        
        // Test delete
        repo.delete(template.metadata.id).await
            .expect("Failed to delete template");
        
        // Verify deletion
        let deleted = repo.find_by_id(template.metadata.id).await
            .expect("Failed to query for deleted template");
        
        assert!(deleted.is_none());
    }
}