use crate::template::{
    models::{Template, CreateTemplateRequest, UpdateTemplateRequest},
    TemplateError,
};
use crate::database::TemplateRepository;
use crate::database::models::DbTemplate;
use crate::database::DatabaseError;
use crate::error::Result as SoulBoxResult;
use std::sync::Arc;
use std::collections::HashMap;
use tracing::{info, warn};
use uuid::Uuid;

/// Template manager for CRUD operations and default template management
pub struct TemplateManager {
    repository: Arc<TemplateRepository>,
}

impl TemplateManager {
    /// Create a new template manager with database repository
    pub fn new(repository: Arc<TemplateRepository>) -> Self {
        Self { repository }
    }

    /// Convert Template to DbTemplate
    fn to_db_template(&self, template: &Template) -> DbTemplate {
        DbTemplate {
            id: template.metadata.id,
            name: template.metadata.name.clone(),
            slug: template.metadata.slug.clone(),
            description: template.metadata.description.clone(),
            runtime_type: template.metadata.runtime_type.to_string(),
            base_image: template.base_image.clone(),
            default_command: template.default_command.clone(),
            environment_vars: if template.environment_vars.is_empty() {
                None
            } else {
                Some(serde_json::to_value(&template.environment_vars).unwrap_or(serde_json::Value::Null))
            },
            resource_limits: Some(serde_json::to_value(&template.resource_limits).unwrap_or(serde_json::Value::Null)),
            is_public: template.metadata.is_public,
            owner_id: template.owner_id,
            created_at: template.metadata.created_at,
            updated_at: template.metadata.updated_at,
        }
    }

    /// Convert DbTemplate to Template (without files)
    fn from_db_template(&self, db_template: &DbTemplate) -> SoulBoxResult<Template> {
        use std::str::FromStr;
        
        let runtime_type = crate::template::models::RuntimeType::from_str(&db_template.runtime_type)
            .map_err(|e| TemplateError::ValidationFailed(format!("Invalid runtime type: {}", e)))?;

        let environment_vars: HashMap<String, String> = db_template.environment_vars
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let resource_limits: crate::template::models::ResourceLimits = db_template.resource_limits
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let metadata = crate::template::models::TemplateMetadata {
            id: db_template.id,
            name: db_template.name.clone(),
            slug: db_template.slug.clone(),
            description: db_template.description.clone(),
            runtime_type,
            version: "1.0.0".to_string(), // Default version
            author: None,
            tags: vec![], // TODO: Add tags to DbTemplate
            is_public: db_template.is_public,
            is_verified: false, // TODO: Add verification to DbTemplate
            created_at: db_template.created_at,
            updated_at: db_template.updated_at,
        };

        let version = crate::template::models::TemplateVersion {
            version: "1.0.0".to_string(),
            changelog: Some("Database version".to_string()),
            created_at: db_template.created_at,
            is_stable: true,
            dependencies: vec![],
        };

        Ok(Template {
            metadata,
            base_image: db_template.base_image.clone(),
            default_command: db_template.default_command.clone(),
            environment_vars,
            resource_limits,
            files: vec![], // Files handled separately
            setup_commands: vec![], // TODO: Add setup_commands to DbTemplate
            versions: vec![version],
            owner_id: db_template.owner_id,
            tenant_id: None, // TODO: Add tenant_id to DbTemplate
        })
    }

    /// Initialize the template manager
    pub async fn initialize(&self) -> SoulBoxResult<()> {
        info!("Template manager initialized");
        Ok(())
    }

    /// Ensure default templates exist (lazy initialization)
    pub async fn ensure_default_templates_lazy(&self) -> SoulBoxResult<()> {
        self.ensure_default_templates().await
    }

    /// Create a new template
    pub async fn create_template(
        &self,
        request: CreateTemplateRequest,
        owner_id: Option<Uuid>,
        tenant_id: Option<Uuid>,
    ) -> SoulBoxResult<Template> {
        // Check if template with slug already exists
        match self.repository.find_by_slug(&request.slug).await {
            Ok(Some(_)) => {
                return Err(TemplateError::ValidationFailed(
                    format!("Template with slug '{}' already exists", request.slug)
                ).into());
            }
            Ok(None) => {}, // OK to proceed
            Err(e) => {
                return Err(TemplateError::StorageError(format!("Database error: {}", e)).into());
            }
        }

        // Create template from request
        let mut template = Template::new(
            request.name,
            request.slug,
            request.runtime_type,
            request.base_image,
        );

        // Apply optional fields
        if let Some(description) = request.description {
            template.metadata.description = Some(description);
        }

        if let Some(command) = request.default_command {
            template.default_command = Some(command);
        }

        if let Some(env_vars) = request.environment_vars {
            template.environment_vars = env_vars;
        }

        if let Some(limits) = request.resource_limits {
            template.resource_limits = limits;
        }

        // Note: Files are no longer stored in the template itself
        // They should be handled by a separate file storage service

        if let Some(commands) = request.setup_commands {
            template.setup_commands = commands;
        }

        if let Some(tags) = request.tags {
            template.metadata.tags = tags;
        }

        if let Some(is_public) = request.is_public {
            template.metadata.is_public = is_public;
        }

        template.owner_id = owner_id;
        template.tenant_id = tenant_id;

        // Convert to DB model and store
        let db_template = self.to_db_template(&template);
        self.repository.create(&db_template).await
            .map_err(|e| TemplateError::StorageError(format!("Failed to create template: {}", e)))?;

        info!("Created template: {} ({})", template.metadata.name, template.metadata.id);
        Ok(template)
    }

    /// Get template by ID
    pub async fn get_template(&self, id: Uuid) -> SoulBoxResult<Option<Template>> {
        match self.repository.find_by_id(id).await {
            Ok(Some(db_template)) => Ok(Some(self.from_db_template(&db_template)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(TemplateError::StorageError(format!("Database error: {}", e)).into()),
        }
    }

    /// Get template by slug
    pub async fn get_template_by_slug(&self, slug: &str) -> SoulBoxResult<Option<Template>> {
        match self.repository.find_by_slug(slug).await {
            Ok(Some(db_template)) => Ok(Some(self.from_db_template(&db_template)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(TemplateError::StorageError(format!("Database error: {}", e)).into()),
        }
    }

    /// Update existing template
    pub async fn update_template(
        &self,
        id: Uuid,
        request: UpdateTemplateRequest,
        user_id: Uuid,
    ) -> SoulBoxResult<Template> {
        // Get existing template
        let db_template = self.repository.find_by_id(id).await
            .map_err(|e| TemplateError::StorageError(format!("Database error: {}", e)))?
            .ok_or_else(|| TemplateError::NotFound(format!("Template with ID {} not found", id)))?;

        let mut template = self.from_db_template(&db_template)?;

        // Check ownership or admin permissions
        if let Some(owner_id) = template.owner_id {
            if owner_id != user_id {
                return Err(TemplateError::ValidationFailed(
                    "Only template owner can update the template".to_string()
                ).into());
            }
        }

        // Apply updates
        if let Some(name) = request.name {
            template.metadata.name = name;
        }

        if let Some(description) = request.description {
            template.metadata.description = Some(description);
        }

        if let Some(base_image) = request.base_image {
            template.base_image = base_image;
        }

        if let Some(command) = request.default_command {
            template.default_command = Some(command);
        }

        if let Some(env_vars) = request.environment_vars {
            template.environment_vars = env_vars;
        }

        if let Some(limits) = request.resource_limits {
            template.resource_limits = limits;
        }

        // Note: Files are handled separately, not stored in template

        if let Some(commands) = request.setup_commands {
            template.setup_commands = commands;
        }

        if let Some(tags) = request.tags {
            template.metadata.tags = tags;
        }

        if let Some(is_public) = request.is_public {
            template.metadata.is_public = is_public;
        }

        // Update timestamp
        template.metadata.updated_at = chrono::Utc::now();

        // Convert to DB model and update
        let updated_db_template = self.to_db_template(&template);
        self.repository.update(&updated_db_template).await
            .map_err(|e| TemplateError::StorageError(format!("Failed to update template: {}", e)))?;

        info!("Updated template: {} ({})", template.metadata.name, template.metadata.id);
        Ok(template)
    }

    /// Delete template
    pub async fn delete_template(&self, id: Uuid, user_id: Uuid) -> SoulBoxResult<()> {
        // Get existing template to check ownership
        let db_template = self.repository.find_by_id(id).await
            .map_err(|e| TemplateError::StorageError(format!("Database error: {}", e)))?
            .ok_or_else(|| TemplateError::NotFound(format!("Template with ID {} not found", id)))?;

        let template = self.from_db_template(&db_template)?;

        // Check ownership or admin permissions
        if let Some(owner_id) = template.owner_id {
            if owner_id != user_id {
                return Err(TemplateError::ValidationFailed(
                    "Only template owner can delete the template".to_string()
                ).into());
            }
        }

        // Prevent deletion of verified public templates
        if template.metadata.is_public && template.metadata.is_verified {
            return Err(TemplateError::ValidationFailed(
                "Cannot delete verified public templates".to_string()
            ).into());
        }

        self.repository.delete(id).await
            .map_err(|e| TemplateError::StorageError(format!("Failed to delete template: {}", e)))?;
        
        info!("Deleted template: {}", id);
        Ok(())
    }

    /// List public templates
    pub async fn list_public_templates(
        &self,
        runtime_type: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> SoulBoxResult<Vec<Template>> {
        // Validate page parameters
        if page == 0 || page_size == 0 || page_size > 100 {
            return Err(TemplateError::ValidationFailed(
                "Invalid page parameters".to_string()
            ).into());
        }

        let paginated_result = self.repository.list_public(runtime_type, page, page_size).await
            .map_err(|e| TemplateError::StorageError(format!("Database error: {}", e)))?;

        // Convert DB templates to domain templates
        let mut templates = Vec::new();
        for db_template in paginated_result.items {
            match self.from_db_template(&db_template) {
                Ok(template) => templates.push(template),
                Err(e) => {
                    warn!("Failed to convert DB template {}: {}", db_template.id, e);
                    continue;
                }
            }
        }

        Ok(templates)
    }

    /// List user's templates
    pub async fn list_user_templates(
        &self,
        user_id: Uuid,
        page: u32,
        page_size: u32,
    ) -> SoulBoxResult<Vec<Template>> {
        // Validate page parameters
        if page == 0 || page_size == 0 || page_size > 100 {
            return Err(TemplateError::ValidationFailed(
                "Invalid page parameters".to_string()
            ).into());
        }

        let paginated_result = self.repository.list_by_owner(user_id, page, page_size).await
            .map_err(|e| TemplateError::StorageError(format!("Database error: {}", e)))?;

        // Convert DB templates to domain templates
        let mut templates = Vec::new();
        for db_template in paginated_result.items {
            match self.from_db_template(&db_template) {
                Ok(template) => templates.push(template),
                Err(e) => {
                    warn!("Failed to convert DB template {}: {}", db_template.id, e);
                    continue;
                }
            }
        }

        Ok(templates)
    }

    /// Validate template
    pub async fn validate_template(&self, template: &Template) -> SoulBoxResult<()> {
        let result = template.validate();
        if !result.is_valid {
            return Err(TemplateError::ValidationFailed(result.errors.join(", ")).into());
        }

        // Log warnings if any
        for warning in result.warnings {
            warn!("Template validation warning: {}", warning);
        }

        Ok(())
    }

    /// Clone template (create a copy with new ID)
    pub async fn clone_template(
        &self,
        source_id: Uuid,
        new_name: String,
        new_slug: String,
        owner_id: Uuid,
        tenant_id: Option<Uuid>,
    ) -> SoulBoxResult<Template> {
        // Get source template
        let db_template = self.repository.find_by_id(source_id).await
            .map_err(|e| TemplateError::StorageError(format!("Database error: {}", e)))?
            .ok_or_else(|| TemplateError::NotFound(format!("Template with ID {} not found", source_id)))?;

        let source_template = self.from_db_template(&db_template)?;

        // Check if new slug is available
        match self.repository.find_by_slug(&new_slug).await {
            Ok(Some(_)) => {
                return Err(TemplateError::ValidationFailed(
                    format!("Template with slug '{}' already exists", new_slug)
                ).into());
            }
            Ok(None) => {}, // OK to proceed
            Err(e) => {
                return Err(TemplateError::StorageError(format!("Database error: {}", e)).into());
            }
        }

        // Create new template based on source
        let mut new_template = source_template.clone();
        new_template.metadata.id = Uuid::new_v4();
        new_template.metadata.name = new_name;
        new_template.metadata.slug = new_slug;
        new_template.metadata.is_public = false; // Cloned templates are private by default
        new_template.metadata.is_verified = false;
        new_template.metadata.created_at = chrono::Utc::now();
        new_template.metadata.updated_at = chrono::Utc::now();
        new_template.owner_id = Some(owner_id);
        new_template.tenant_id = tenant_id;

        // Convert to DB model and store
        let new_db_template = self.to_db_template(&new_template);
        self.repository.create(&new_db_template).await
            .map_err(|e| TemplateError::StorageError(format!("Failed to clone template: {}", e)))?;

        info!("Cloned template {} to {} ({})", source_id, new_template.metadata.name, new_template.metadata.id);
        Ok(new_template)
    }

    /// Ensure default templates exist with improved error handling and tracking
    async fn ensure_default_templates(&self) -> SoulBoxResult<()> {
        let default_templates = vec![
            ("python", Template::default_python()),
            ("node", Template::default_node()),
            ("rust", Template::default_rust()),
        ];

        let mut created_count = 0;
        let mut error_count = 0;

        for (template_type, template) in default_templates {
            // Check if template already exists by slug
            match self.repository.find_by_slug(&template.metadata.slug).await {
                Ok(Some(_)) => {
                    // Template exists, skip
                    continue;
                }
                Ok(None) => {
                    // Template doesn't exist, create it
                    let db_template = self.to_db_template(&template);
                    match self.repository.create(&db_template).await {
                        Ok(_) => {
                            info!("Created default {} template: {}", template_type, template.metadata.name);
                            created_count += 1;
                        }
                        Err(e) => {
                            error_count += 1;
                            warn!("Failed to create default {} template {}: {}", template_type, template.metadata.name, e);
                            
                            // For critical templates, we might want to retry or take additional action
                            if template_type == "python" || template_type == "node" {
                                warn!("Critical default template failed to create: {}", template_type);
                            }
                        }
                    }
                }
                Err(e) => {
                    error_count += 1;
                    warn!("Error checking for default {} template {}: {}", template_type, template.metadata.name, e);
                }
            }
        }

        if created_count > 0 {
            info!("Successfully created {} default templates", created_count);
        }
        
        if error_count > 0 {
            warn!("Encountered {} errors while ensuring default templates", error_count);
            // Still return Ok since this shouldn't prevent the system from starting
        }

        Ok(())
    }

    /// Get template for container creation (with lazy default template initialization)
    pub async fn get_template_for_container(&self, template_spec: &str) -> SoulBoxResult<Template> {
        // Try to parse as UUID first
        if let Ok(uuid) = Uuid::parse_str(template_spec) {
            if let Some(template) = self.get_template(uuid).await? {
                return Ok(template);
            }
        }

        // Try to find by slug
        if let Some(template) = self.get_template_by_slug(template_spec).await? {
            return Ok(template);
        }

        // If not found, try to find a default template by runtime type
        let templates = self.list_public_templates(Some(template_spec), 1, 10).await?;
        if let Some(template) = templates.into_iter().find(|t| t.metadata.is_verified) {
            return Ok(template);
        }

        // If still not found and it looks like a runtime type, ensure default templates exist and try again
        if matches!(template_spec, "python" | "node" | "rust" | "go" | "java") {
            info!("Template '{}' not found, ensuring default templates exist", template_spec);
            
            if let Err(e) = self.ensure_default_templates().await {
                warn!("Failed to ensure default templates: {}", e);
            }

            // Try once more after ensuring defaults
            let templates = self.list_public_templates(Some(template_spec), 1, 10).await?;
            if let Some(template) = templates.into_iter().find(|t| t.metadata.is_verified) {
                return Ok(template);
            }
        }

        Err(TemplateError::NotFound(format!("Template not found: {}", template_spec)).into())
    }
}