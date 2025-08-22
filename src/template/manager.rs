use crate::template::{
    models::{Template, CreateTemplateRequest, UpdateTemplateRequest},
    TemplateError,
};
use crate::database::{TemplateRepository, DatabaseResult};
use crate::error::Result as SoulBoxResult;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn, debug, error};
use uuid::Uuid;

/// Template manager for CRUD operations and default template management
pub struct TemplateManager {
    repository: Arc<TemplateRepository>,
    default_templates: HashMap<String, Template>,
}

impl TemplateManager {
    /// Create a new template manager with repository
    pub fn new(repository: Arc<TemplateRepository>) -> Self {
        let mut manager = Self {
            repository,
            default_templates: HashMap::new(),
        };
        
        // Initialize default templates
        manager.load_default_templates();
        manager
    }

    /// Create a template manager without database (temporary fallback)
    /// This method should not be used in production - it's only for compatibility
    pub fn new_without_database() -> Self {
        warn!("Creating template manager without database - functionality will be limited");
        
        // Create a minimal manager that only works with in-memory default templates
        let mut manager = Self {
            // Use a dummy repository that will return errors for all operations
            repository: Arc::new(TemplateRepository::new_stub()),
            default_templates: HashMap::new(),
        };
        
        // Load default templates which will still work
        manager.load_default_templates();
        manager
    }

    /// Load default templates into memory
    fn load_default_templates(&mut self) {
        debug!("Loading default templates");
        
        // Python template
        let python_template = Template::default_python();
        self.default_templates.insert("python".to_string(), python_template.clone());
        self.default_templates.insert("python-3-11".to_string(), python_template);
        
        // Node.js template
        let node_template = Template::default_node();
        self.default_templates.insert("node".to_string(), node_template.clone());
        self.default_templates.insert("nodejs".to_string(), node_template.clone());
        self.default_templates.insert("node-18".to_string(), node_template);
        
        // Rust template
        let rust_template = Template::default_rust();
        self.default_templates.insert("rust".to_string(), rust_template.clone());
        self.default_templates.insert("rust-latest".to_string(), rust_template);
        
        info!("Loaded {} default templates", self.default_templates.len());
    }

    /// Create a new template
    pub async fn create_template(&self, request: CreateTemplateRequest, owner_id: Uuid) -> SoulBoxResult<Template> {
        debug!("Creating template: {} for owner: {}", request.name, owner_id);
        
        // Validate request
        if request.name.trim().is_empty() {
            return Err(TemplateError::ValidationError("Template name cannot be empty".to_string()).into());
        }
        
        if request.slug.trim().is_empty() {
            return Err(TemplateError::ValidationError("Template slug cannot be empty".to_string()).into());
        }
        
        // Check if slug already exists
        if let Ok(Some(_)) = self.repository.find_by_slug(&request.slug).await {
            return Err(TemplateError::ConflictError(format!("Template with slug '{}' already exists", request.slug)).into());
        }
        
        // Create template
        let template = self.repository.create_from_request(&request, owner_id, None).await
            .map_err(|e| TemplateError::DatabaseError(e.to_string()))?;
        
        info!("Created template: {} ({})", template.metadata.name, template.metadata.id);
        Ok(template)
    }

    /// Get template by ID
    pub async fn get_template(&self, id: Uuid) -> SoulBoxResult<Option<Template>> {
        debug!("Getting template by ID: {}", id);
        
        self.repository.find_by_id(id).await
            .map_err(|e| TemplateError::DatabaseError(e.to_string()).into())
    }

    /// Update template
    pub async fn update_template(&self, id: Uuid, request: UpdateTemplateRequest) -> SoulBoxResult<Template> {
        debug!("Updating template: {}", id);
        
        let template = self.repository.update(id, &request).await
            .map_err(|e| match e {
                crate::database::DatabaseError::NotFound => TemplateError::NotFound(format!("Template with ID {} not found", id)),
                _ => TemplateError::DatabaseError(e.to_string()),
            })?;
        
        info!("Updated template: {} ({})", template.metadata.name, template.metadata.id);
        Ok(template)
    }

    /// Delete template
    pub async fn delete_template(&self, id: Uuid) -> SoulBoxResult<()> {
        debug!("Deleting template: {}", id);
        
        self.repository.delete(id).await
            .map_err(|e| match e {
                crate::database::DatabaseError::NotFound => TemplateError::NotFound(format!("Template with ID {} not found", id)),
                _ => TemplateError::DatabaseError(e.to_string()),
            })?;
        
        info!("Deleted template: {}", id);
        Ok(())
    }

    /// List templates with filters and pagination
    pub async fn list_templates(
        &self,
        runtime_type: Option<String>,
        is_public: Option<bool>,
        owner_id: Option<Uuid>,
        page: u32,
        page_size: u32,
    ) -> SoulBoxResult<(Vec<Template>, u64)> {
        debug!("Listing templates with filters - runtime_type: {:?}, is_public: {:?}, owner_id: {:?}, page: {}, page_size: {}", 
               runtime_type, is_public, owner_id, page, page_size);
        
        self.repository.list(runtime_type, is_public, owner_id, page, page_size).await
            .map_err(|e| TemplateError::DatabaseError(e.to_string()).into())
    }

    /// Get default templates for runtime
    pub fn get_default_templates(&self) -> HashMap<String, Template> {
        self.default_templates.clone()
    }

    /// Initialize default templates in database
    pub async fn initialize_default_templates(&self) -> SoulBoxResult<()> {
        info!("Initializing default templates in database");
        
        for (key, template) in &self.default_templates {
            debug!("Checking if default template '{}' exists", key);
            
            // Check if template already exists by slug
            if let Ok(Some(_)) = self.repository.find_by_slug(&template.metadata.slug).await {
                debug!("Default template '{}' already exists, skipping", key);
                continue;
            }
            
            // Create the default template
            match self.repository.create(template).await {
                Ok(_) => {
                    info!("Created default template: {}", template.metadata.name);
                }
                Err(e) => {
                    error!("Failed to create default template '{}': {}", key, e);
                    // Continue with other templates rather than failing completely
                }
            }
        }
        
        info!("Default templates initialization completed");
        Ok(())
    }

    /// List public templates
    pub async fn list_public_templates(
        &self,
        runtime_type: Option<&str>,
        page: usize,
        page_size: usize,
    ) -> SoulBoxResult<Vec<Template>> {
        debug!("Listing public templates - runtime_type: {:?}, page: {}, page_size: {}", runtime_type, page, page_size);
        
        self.repository.list_public(runtime_type, page, page_size).await
            .map_err(|e| TemplateError::DatabaseError(e.to_string()).into())
    }

    /// Get template by slug
    pub async fn get_template_by_slug(&self, slug: &str) -> SoulBoxResult<Option<Template>> {
        debug!("Getting template by slug: {}", slug);
        
        self.repository.find_by_slug(slug).await
            .map_err(|e| TemplateError::DatabaseError(e.to_string()).into())
    }

    /// List user templates
    pub async fn list_user_templates(
        &self,
        user_id: uuid::Uuid,
        page: usize,
        page_size: usize,
    ) -> SoulBoxResult<Vec<Template>> {
        debug!("Listing user templates for user: {}, page: {}, page_size: {}", user_id, page, page_size);
        
        self.repository.list_user(user_id, page, page_size).await
            .map_err(|e| TemplateError::DatabaseError(e.to_string()).into())
    }

    /// Clone template
    pub async fn clone_template(
        &self,
        template_id: uuid::Uuid,
        user_id: uuid::Uuid,
    ) -> SoulBoxResult<Template> {
        debug!("Cloning template: {} for user: {}", template_id, user_id);
        
        let cloned = self.repository.clone_template(template_id, user_id).await
            .map_err(|e| match e {
                crate::database::DatabaseError::NotFound => TemplateError::NotFound(format!("Template with ID {} not found", template_id)),
                _ => TemplateError::DatabaseError(e.to_string()),
            })?;
        
        info!("Cloned template: {} -> {} for user: {}", template_id, cloned.metadata.id, user_id);
        Ok(cloned)
    }

    /// Get template for runtime with fallback to default
    pub async fn get_template_for_runtime(&self, runtime_type: &str, template_id: Option<Uuid>) -> SoulBoxResult<Template> {
        debug!("Getting template for runtime: {}, template_id: {:?}", runtime_type, template_id);
        
        // If specific template ID is provided, use it
        if let Some(id) = template_id {
            if let Some(template) = self.get_template(id).await? {
                return Ok(template);
            } else {
                warn!("Requested template {} not found, falling back to default", id);
            }
        }
        
        // Try to get default template for runtime
        if let Some(default_template) = self.default_templates.get(runtime_type) {
            return Ok(default_template.clone());
        }
        
        // Fallback to the first available default template
        if let Some((_, template)) = self.default_templates.iter().next() {
            warn!("No default template for runtime '{}', using fallback", runtime_type);
            return Ok(template.clone());
        }
        
        Err(TemplateError::NotFound(format!("No template available for runtime '{}'", runtime_type)).into())
    }
}