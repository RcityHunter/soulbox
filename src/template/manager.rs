// Temporarily simplified template manager to fix compilation
// TODO: Restore full functionality after SurrealDB migration is complete

use crate::template::{
    models::{Template, CreateTemplateRequest, UpdateTemplateRequest},
    TemplateError,
};
use crate::error::Result as SoulBoxResult;
use std::collections::HashMap;
use tracing::{info, warn};
use uuid::Uuid;

/// Template manager for CRUD operations and default template management
pub struct TemplateManager {
    // Temporarily empty - will be restored after repository refactoring
}

impl TemplateManager {
    /// Create a template manager without database (temporary)
    pub fn new_without_database() -> Self {
        Self {}
    }

    /// Create a new template (stub implementation)
    pub async fn create_template(&self, _request: CreateTemplateRequest, _owner_id: Uuid) -> SoulBoxResult<Template> {
        Err(TemplateError::DatabaseError("Template functionality temporarily disabled during database migration".to_string()).into())
    }

    /// Get template by ID (stub implementation)
    pub async fn get_template(&self, _id: Uuid) -> SoulBoxResult<Option<Template>> {
        Ok(None)
    }

    /// Update template (stub implementation)
    pub async fn update_template(&self, _id: Uuid, _request: UpdateTemplateRequest) -> SoulBoxResult<Template> {
        Err(TemplateError::DatabaseError("Template functionality temporarily disabled during database migration".to_string()).into())
    }

    /// Delete template (stub implementation)
    pub async fn delete_template(&self, _id: Uuid) -> SoulBoxResult<()> {
        Err(TemplateError::DatabaseError("Template functionality temporarily disabled during database migration".to_string()).into())
    }

    /// List templates (stub implementation)
    pub async fn list_templates(
        &self,
        _runtime_type: Option<String>,
        _is_public: Option<bool>,
        _owner_id: Option<Uuid>,
        _page: u32,
        _page_size: u32,
    ) -> SoulBoxResult<(Vec<Template>, u64)> {
        Ok((Vec::new(), 0))
    }

    /// Get default templates for runtime (stub implementation)
    pub fn get_default_templates(&self) -> HashMap<String, Template> {
        HashMap::new()
    }

    /// Initialize default templates (stub implementation)
    pub async fn initialize_default_templates(&self) -> SoulBoxResult<()> {
        info!("Template initialization temporarily disabled during database migration");
        Ok(())
    }

    /// List public templates (stub implementation)
    pub async fn list_public_templates(
        &self,
        _runtime_type: Option<&str>,
        _page: usize,
        _page_size: usize,
    ) -> SoulBoxResult<Vec<Template>> {
        info!("Template public listing temporarily disabled during database migration");
        Ok(Vec::new())
    }

    /// Get template by slug (stub implementation)
    pub async fn get_template_by_slug(&self, _slug: &str) -> SoulBoxResult<Option<Template>> {
        info!("Template slug lookup temporarily disabled during database migration");
        Ok(None)
    }

    /// List user templates (stub implementation)
    pub async fn list_user_templates(
        &self,
        _user_id: uuid::Uuid,
        _page: usize,
        _page_size: usize,
    ) -> SoulBoxResult<Vec<Template>> {
        info!("Template user listing temporarily disabled during database migration");
        Ok(Vec::new())
    }

    /// Clone template (stub implementation)
    pub async fn clone_template(
        &self,
        _template_id: uuid::Uuid,
        _user_id: uuid::Uuid,
    ) -> SoulBoxResult<Template> {
        info!("Template cloning temporarily disabled during database migration");
        Err(crate::error::SoulBoxError::NotImplemented("Template cloning not yet implemented".into()))
    }
}