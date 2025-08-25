//! Template repository management for storing and retrieving templates
//! 
//! This module provides a high-level interface for managing templates,
//! including versioning, caching, and repository operations.

use crate::template::{
    Template, TemplateVersion, TemplateError, TemplateMetadata,
    TemplateStorage, TemplateFileStorage, TemplateManager,
};
use crate::error::Result as SoulBoxResult;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, debug, error};
use uuid::Uuid;

/// Template repository configuration
#[derive(Debug, Clone)]
pub struct TemplateRepositoryConfig {
    /// Base path for template storage
    pub storage_path: PathBuf,
    /// Maximum cache size (number of templates)
    pub max_cache_size: usize,
    /// Enable automatic version management
    pub auto_versioning: bool,
    /// Enable template validation on save
    pub validate_on_save: bool,
}

impl Default for TemplateRepositoryConfig {
    fn default() -> Self {
        Self {
            storage_path: PathBuf::from("/var/lib/soulbox/templates"),
            max_cache_size: 100,
            auto_versioning: true,
            validate_on_save: true,
        }
    }
}

/// Template cache entry
#[derive(Debug, Clone)]
struct CacheEntry {
    template: Template,
    last_accessed: std::time::Instant,
    access_count: u64,
}

/// Template repository for managing template storage and retrieval
pub struct TemplateRepository {
    config: TemplateRepositoryConfig,
    storage: Arc<TemplateStorage>,
    file_storage: Arc<TemplateFileStorage>,
    cache: Arc<RwLock<HashMap<Uuid, CacheEntry>>>,
    version_index: Arc<RwLock<HashMap<Uuid, Vec<TemplateVersion>>>>,
}

impl TemplateRepository {
    /// Create a new template repository
    pub async fn new(config: TemplateRepositoryConfig) -> SoulBoxResult<Self> {
        let storage = Arc::new(TemplateStorage::new(&config.storage_path));
        let file_storage = Arc::new(TemplateFileStorage::new(
            config.storage_path.join("files")
        ));
        
        // Initialize storage
        storage.initialize().await?;
        file_storage.initialize().await?;
        
        Ok(Self {
            config,
            storage,
            file_storage,
            cache: Arc::new(RwLock::new(HashMap::new())),
            version_index: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    /// Store a template in the repository
    pub async fn store(&self, template: &Template) -> SoulBoxResult<()> {
        debug!("Storing template: {} ({})", template.metadata.name, template.metadata.id);
        
        // Validate if required
        if self.config.validate_on_save {
            let validation = template.validate();
            if !validation.is_valid {
                return Err(TemplateError::ValidationFailed(
                    validation.errors.join(", ")
                ).into());
            }
        }
        
        // Store template
        self.storage.store_template(template).await?;
        
        // Update cache
        self.add_to_cache(template.clone()).await;
        
        // Update version index
        self.update_version_index(template).await;
        
        info!("Stored template: {} v{}", template.metadata.name, template.metadata.version);
        Ok(())
    }
    
    /// Retrieve a template by ID
    pub async fn get(&self, id: Uuid) -> SoulBoxResult<Option<Template>> {
        debug!("Getting template: {}", id);
        
        // Check cache first
        if let Some(template) = self.get_from_cache(id).await {
            debug!("Template {} found in cache", id);
            return Ok(Some(template));
        }
        
        // Load from storage
        if let Some(template) = self.storage.get_template(id).await? {
            debug!("Template {} loaded from storage", id);
            self.add_to_cache(template.clone()).await;
            Ok(Some(template))
        } else {
            Ok(None)
        }
    }
    
    /// Retrieve a template by slug
    pub async fn get_by_slug(&self, slug: &str) -> SoulBoxResult<Option<Template>> {
        debug!("Getting template by slug: {}", slug);
        self.storage.get_template_by_slug(slug).await
    }
    
    /// Update a template
    pub async fn update(&self, template: &Template) -> SoulBoxResult<()> {
        debug!("Updating template: {} ({})", template.metadata.name, template.metadata.id);
        
        // Validate if required
        if self.config.validate_on_save {
            let validation = template.validate();
            if !validation.is_valid {
                return Err(TemplateError::ValidationFailed(
                    validation.errors.join(", ")
                ).into());
            }
        }
        
        // Check if template exists
        let existing = self.storage.get_template(template.metadata.id).await?
            .ok_or_else(|| TemplateError::NotFound(
                format!("Template {} not found", template.metadata.id)
            ))?;
        
        // Handle versioning if enabled
        if self.config.auto_versioning && existing.metadata.version != template.metadata.version {
            self.handle_version_change(&existing, template).await?;
        }
        
        // Update template
        self.storage.update_template(template).await?;
        
        // Update cache
        self.add_to_cache(template.clone()).await;
        
        // Update version index
        self.update_version_index(template).await;
        
        info!("Updated template: {} v{}", template.metadata.name, template.metadata.version);
        Ok(())
    }
    
    /// Delete a template
    pub async fn delete(&self, id: Uuid) -> SoulBoxResult<()> {
        debug!("Deleting template: {}", id);
        
        // Remove from storage
        self.storage.delete_template(id).await?;
        
        // Remove from cache
        self.remove_from_cache(id).await;
        
        // Remove from version index
        {
            let mut index = self.version_index.write().await;
            index.remove(&id);
        }
        
        // Delete associated files
        self.file_storage.delete_template_files(id).await?;
        
        info!("Deleted template: {}", id);
        Ok(())
    }
    
    /// List all templates
    pub async fn list(&self) -> SoulBoxResult<Vec<Template>> {
        debug!("Listing all templates");
        self.storage.list_templates().await
    }
    
    /// List public templates with pagination
    pub async fn list_public(
        &self,
        runtime_type: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> SoulBoxResult<Vec<Template>> {
        debug!("Listing public templates - runtime: {:?}, page: {}, size: {}", 
               runtime_type, page, page_size);
        self.storage.list_public_templates(runtime_type, page, page_size).await
    }
    
    /// List user templates with pagination
    pub async fn list_user(
        &self,
        owner_id: Uuid,
        page: u32,
        page_size: u32,
    ) -> SoulBoxResult<Vec<Template>> {
        debug!("Listing user templates - owner: {}, page: {}, size: {}", 
               owner_id, page, page_size);
        self.storage.list_user_templates(owner_id, page, page_size).await
    }
    
    /// Get all versions of a template
    pub async fn get_versions(&self, template_id: Uuid) -> SoulBoxResult<Vec<TemplateVersion>> {
        let index = self.version_index.read().await;
        Ok(index.get(&template_id).cloned().unwrap_or_default())
    }
    
    /// Get a specific version of a template
    pub async fn get_version(
        &self,
        template_id: Uuid,
        version: &str,
    ) -> SoulBoxResult<Option<Template>> {
        debug!("Getting template {} version {}", template_id, version);
        
        // Get the current template
        if let Some(mut template) = self.get(template_id).await? {
            // Find the requested version
            if let Some(ver) = template.versions.iter().find(|v| v.version == version) {
                // Update template metadata to reflect the requested version
                template.metadata.version = ver.version.clone();
                return Ok(Some(template));
            }
        }
        
        Ok(None)
    }
    
    /// Clone a template
    pub async fn clone_template(
        &self,
        template_id: Uuid,
        new_name: String,
        new_slug: String,
        owner_id: Uuid,
    ) -> SoulBoxResult<Template> {
        debug!("Cloning template {} as {}", template_id, new_name);
        
        // Get the original template
        let original = self.get(template_id).await?
            .ok_or_else(|| TemplateError::NotFound(
                format!("Template {} not found", template_id)
            ))?;
        
        // Create a new template based on the original
        let mut cloned = original.clone();
        cloned.metadata.id = Uuid::new_v4();
        cloned.metadata.name = new_name;
        cloned.metadata.slug = new_slug;
        cloned.metadata.is_public = false;
        cloned.metadata.is_verified = false;
        cloned.metadata.created_at = chrono::Utc::now();
        cloned.metadata.updated_at = chrono::Utc::now();
        cloned.owner_id = Some(owner_id);
        
        // Reset versions for the cloned template
        cloned.versions = vec![TemplateVersion {
            version: "1.0.0".to_string(),
            changelog: Some(format!("Cloned from template {}", original.metadata.name)),
            created_at: chrono::Utc::now(),
            is_stable: true,
            dependencies: vec![],
        }];
        cloned.metadata.version = "1.0.0".to_string();
        
        // Store the cloned template
        self.store(&cloned).await?;
        
        info!("Cloned template {} as {}", template_id, cloned.metadata.id);
        Ok(cloned)
    }
    
    /// Search templates
    pub async fn search(
        &self,
        query: &str,
        runtime_type: Option<&str>,
        is_public: Option<bool>,
    ) -> SoulBoxResult<Vec<Template>> {
        debug!("Searching templates - query: {}, runtime: {:?}, public: {:?}", 
               query, runtime_type, is_public);
        
        let all_templates = self.list().await?;
        let query_lower = query.to_lowercase();
        
        let filtered: Vec<Template> = all_templates
            .into_iter()
            .filter(|t| {
                // Filter by query
                let matches_query = t.metadata.name.to_lowercase().contains(&query_lower)
                    || t.metadata.description.as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
                    || t.metadata.tags.iter()
                        .any(|tag| tag.to_lowercase().contains(&query_lower));
                
                // Filter by runtime type
                let matches_runtime = runtime_type.map(|rt| {
                    t.metadata.runtime_type.to_string() == rt
                }).unwrap_or(true);
                
                // Filter by public status
                let matches_public = is_public.map(|p| t.metadata.is_public == p)
                    .unwrap_or(true);
                
                matches_query && matches_runtime && matches_public
            })
            .collect();
        
        Ok(filtered)
    }
    
    /// Add template to cache
    async fn add_to_cache(&self, template: Template) {
        let mut cache = self.cache.write().await;
        
        // Check cache size limit
        if cache.len() >= self.config.max_cache_size {
            // Evict least recently used entry
            if let Some((&id, _)) = cache.iter()
                .min_by_key(|(_, entry)| entry.last_accessed) {
                cache.remove(&id);
                debug!("Evicted template {} from cache", id);
            }
        }
        
        cache.insert(template.metadata.id, CacheEntry {
            template,
            last_accessed: std::time::Instant::now(),
            access_count: 0,
        });
    }
    
    /// Get template from cache
    async fn get_from_cache(&self, id: Uuid) -> Option<Template> {
        let mut cache = self.cache.write().await;
        if let Some(entry) = cache.get_mut(&id) {
            entry.last_accessed = std::time::Instant::now();
            entry.access_count += 1;
            Some(entry.template.clone())
        } else {
            None
        }
    }
    
    /// Remove template from cache
    async fn remove_from_cache(&self, id: Uuid) {
        let mut cache = self.cache.write().await;
        cache.remove(&id);
    }
    
    /// Update version index
    async fn update_version_index(&self, template: &Template) {
        let mut index = self.version_index.write().await;
        index.insert(template.metadata.id, template.versions.clone());
    }
    
    /// Handle version change
    async fn handle_version_change(
        &self,
        existing: &Template,
        new_template: &Template,
    ) -> SoulBoxResult<()> {
        debug!("Handling version change for template {} from {} to {}", 
               existing.metadata.id, 
               existing.metadata.version,
               new_template.metadata.version);
        
        // Ensure the new version doesn't already exist
        if existing.versions.iter().any(|v| v.version == new_template.metadata.version) {
            return Err(TemplateError::VersionConflict(
                format!("Version {} already exists", new_template.metadata.version)
            ).into());
        }
        
        info!("Version change detected for template {}: {} -> {}", 
              existing.metadata.id,
              existing.metadata.version,
              new_template.metadata.version);
        
        Ok(())
    }
    
    /// Export template to archive
    pub async fn export_template(&self, template_id: Uuid) -> SoulBoxResult<Vec<u8>> {
        debug!("Exporting template: {}", template_id);
        
        let template = self.get(template_id).await?
            .ok_or_else(|| TemplateError::NotFound(
                format!("Template {} not found", template_id)
            ))?;
        
        // Serialize template to JSON
        let json = serde_json::to_vec_pretty(&template)
            .map_err(|e| TemplateError::StorageError(
                format!("Failed to serialize template: {}", e)
            ))?;
        
        Ok(json)
    }
    
    /// Import template from archive
    pub async fn import_template(&self, data: &[u8], owner_id: Uuid) -> SoulBoxResult<Template> {
        debug!("Importing template");
        
        // Deserialize template from JSON
        let mut template: Template = serde_json::from_slice(data)
            .map_err(|e| TemplateError::StorageError(
                format!("Failed to deserialize template: {}", e)
            ))?;
        
        // Generate new ID and set owner
        template.metadata.id = Uuid::new_v4();
        template.owner_id = Some(owner_id);
        template.metadata.is_verified = false;
        template.metadata.created_at = chrono::Utc::now();
        template.metadata.updated_at = chrono::Utc::now();
        
        // Ensure unique slug
        let mut slug = template.metadata.slug.clone();
        let mut counter = 1;
        while self.get_by_slug(&slug).await?.is_some() {
            slug = format!("{}-{}", template.metadata.slug, counter);
            counter += 1;
        }
        template.metadata.slug = slug;
        
        // Store the imported template
        self.store(&template).await?;
        
        info!("Imported template: {} ({})", template.metadata.name, template.metadata.id);
        Ok(template)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RuntimeType;
    use tempfile::TempDir;
    
    async fn setup_test_repository() -> (TemplateRepository, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = TemplateRepositoryConfig {
            storage_path: temp_dir.path().to_path_buf(),
            max_cache_size: 10,
            auto_versioning: true,
            validate_on_save: true,
        };
        
        let repo = TemplateRepository::new(config).await.unwrap();
        (repo, temp_dir)
    }
    
    #[tokio::test]
    async fn test_template_repository_crud() {
        let (repo, _temp_dir) = setup_test_repository().await;
        
        // Create a test template
        let template = Template::default_python();
        
        // Store template
        repo.store(&template).await.unwrap();
        
        // Retrieve template
        let retrieved = repo.get(template.metadata.id).await.unwrap().unwrap();
        assert_eq!(retrieved.metadata.name, template.metadata.name);
        
        // Update template
        let mut updated = retrieved.clone();
        updated.metadata.name = "Updated Python Template".to_string();
        repo.update(&updated).await.unwrap();
        
        // Verify update
        let updated_retrieved = repo.get(template.metadata.id).await.unwrap().unwrap();
        assert_eq!(updated_retrieved.metadata.name, "Updated Python Template");
        
        // Delete template
        repo.delete(template.metadata.id).await.unwrap();
        
        // Verify deletion
        let deleted = repo.get(template.metadata.id).await.unwrap();
        assert!(deleted.is_none());
    }
    
    #[tokio::test]
    async fn test_template_search() {
        let (repo, _temp_dir) = setup_test_repository().await;
        
        // Store multiple templates
        let python_template = Template::default_python();
        let node_template = Template::default_node();
        let rust_template = Template::default_rust();
        
        repo.store(&python_template).await.unwrap();
        repo.store(&node_template).await.unwrap();
        repo.store(&rust_template).await.unwrap();
        
        // Search for Python templates
        let results = repo.search("python", None, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].metadata.name, "Python 3.11");
        
        // Search for public templates
        let public_results = repo.search("", None, Some(true)).await.unwrap();
        assert_eq!(public_results.len(), 3); // All default templates are public
    }
}