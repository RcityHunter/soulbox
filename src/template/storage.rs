use crate::template::{Template, TemplateError};
use crate::error::Result as SoulBoxResult;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

/// Template file storage manager (file system based)
pub struct TemplateStorage {
    storage_path: PathBuf,
    // Cache mapping slug -> template_id for fast lookups
    slug_cache: Arc<RwLock<HashMap<String, Uuid>>>,
}

impl TemplateStorage {
    pub fn new(storage_path: impl AsRef<Path>) -> Self {
        let storage_path = storage_path.as_ref().to_path_buf();
        Self { 
            storage_path,
            slug_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize storage directory and populate slug cache
    pub async fn initialize(&self) -> SoulBoxResult<()> {
        if !self.storage_path.exists() {
            fs::create_dir_all(&self.storage_path).await
                .map_err(|e| TemplateError::StorageError(format!("Failed to create storage directory: {}", e)))?;
            info!("Created template storage directory: {:?}", self.storage_path);
        }
        
        // Populate the slug cache
        self.rebuild_slug_cache().await?;
        
        Ok(())
    }

    /// Rebuild the slug cache by scanning all templates
    async fn rebuild_slug_cache(&self) -> SoulBoxResult<()> {
        let mut cache = self.slug_cache.write().await;
        cache.clear();
        
        let mut entries = fs::read_dir(&self.storage_path).await
            .map_err(|e| TemplateError::StorageError(format!("Failed to read storage directory: {}", e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| TemplateError::StorageError(format!("Failed to read directory entry: {}", e)))? {
            
            if entry.file_type().await
                .map_err(|e| TemplateError::StorageError(format!("Failed to get file type: {}", e)))?
                .is_dir() 
            {
                let metadata_path = entry.path().join("metadata.json");
                if metadata_path.exists() {
                    match fs::read_to_string(metadata_path).await {
                        Ok(metadata_content) => {
                            match serde_json::from_str::<Template>(&metadata_content) {
                                Ok(template) => {
                                    cache.insert(template.metadata.slug.clone(), template.metadata.id);
                                }
                                Err(e) => warn!("Failed to parse template metadata in {:?}: {}", entry.path(), e),
                            }
                        }
                        Err(e) => warn!("Failed to read template metadata in {:?}: {}", entry.path(), e),
                    }
                }
            }
        }
        
        info!("Rebuilt slug cache with {} entries", cache.len());
        Ok(())
    }

    /// Store template in file system
    pub async fn store_template(&self, template: &Template) -> SoulBoxResult<()> {
        // Validate template first
        let validation = template.validate();
        if !validation.is_valid {
            return Err(TemplateError::ValidationFailed(validation.errors.join(", ")).into());
        }

        // Store template files on disk
        self.store_template_files(template).await?;

        // Update the slug cache
        {
            let mut cache = self.slug_cache.write().await;
            cache.insert(template.metadata.slug.clone(), template.metadata.id);
        }

        info!("Stored template: {} ({})", template.metadata.name, template.metadata.id);
        Ok(())
    }

    /// Retrieve template from storage
    pub async fn get_template(&self, id: Uuid) -> SoulBoxResult<Option<Template>> {
        let template_dir = self.storage_path.join(id.to_string());
        let metadata_path = template_dir.join("metadata.json");
        
        if !metadata_path.exists() {
            return Ok(None);
        }

        // Load template from JSON file
        let metadata_content = fs::read_to_string(metadata_path).await
            .map_err(|e| TemplateError::FileError(format!("Failed to read metadata: {}", e)))?;
        
        let template: Template = serde_json::from_str(&metadata_content)
            .map_err(|e| TemplateError::FileError(format!("Failed to parse metadata: {}", e)))?;

        Ok(Some(template))
    }

    /// Retrieve template by slug using cache for performance
    pub async fn get_template_by_slug(&self, slug: &str) -> SoulBoxResult<Option<Template>> {
        // First, check the cache for the template ID
        let template_id = {
            let cache = self.slug_cache.read().await;
            cache.get(slug).copied()
        };

        // If found in cache, get the template by ID
        if let Some(id) = template_id {
            return self.get_template(id).await;
        }

        // If not in cache, it might be a new template or the cache is stale
        // Fall back to scanning (this should be rare after cache is properly maintained)
        warn!("Template slug '{}' not found in cache, falling back to directory scan", slug);
        
        let mut entries = fs::read_dir(&self.storage_path).await
            .map_err(|e| TemplateError::StorageError(format!("Failed to read storage directory: {}", e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| TemplateError::StorageError(format!("Failed to read directory entry: {}", e)))? {
            
            if entry.file_type().await
                .map_err(|e| TemplateError::StorageError(format!("Failed to get file type: {}", e)))?
                .is_dir() 
            {
                let metadata_path = entry.path().join("metadata.json");
                if metadata_path.exists() {
                    if let Ok(metadata_content) = fs::read_to_string(metadata_path).await {
                        if let Ok(template) = serde_json::from_str::<Template>(&metadata_content) {
                            if template.metadata.slug == slug {
                                // Update cache with found template
                                {
                                    let mut cache = self.slug_cache.write().await;
                                    cache.insert(slug.to_string(), template.metadata.id);
                                }
                                return Ok(Some(template));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Update existing template
    pub async fn update_template(&self, template: &Template) -> SoulBoxResult<()> {
        // Validate template
        let validation = template.validate();
        if !validation.is_valid {
            return Err(TemplateError::ValidationFailed(validation.errors.join(", ")).into());
        }

        // Check if template exists
        let existing_template = self.get_template(template.metadata.id).await?;
        let existing_template = existing_template.ok_or_else(|| {
            TemplateError::NotFound(format!("Template {} not found", template.metadata.id))
        })?;

        // Update template files
        self.store_template_files(template).await?;

        // Update cache if slug changed
        {
            let mut cache = self.slug_cache.write().await;
            // Remove old slug if it changed
            if existing_template.metadata.slug != template.metadata.slug {
                cache.remove(&existing_template.metadata.slug);
            }
            // Insert/update new slug
            cache.insert(template.metadata.slug.clone(), template.metadata.id);
        }

        info!("Updated template: {} ({})", template.metadata.name, template.metadata.id);
        Ok(())
    }

    /// Delete template
    pub async fn delete_template(&self, id: Uuid) -> SoulBoxResult<()> {
        let template_dir = self.storage_path.join(id.to_string());
        
        if !template_dir.exists() {
            return Err(TemplateError::NotFound(format!("Template {} not found", id)).into());
        }

        // Get the template to find its slug for cache removal
        let template = self.get_template(id).await?;
        
        // Delete template directory
        fs::remove_dir_all(template_dir).await
            .map_err(|e| TemplateError::FileError(format!("Failed to delete template files: {}", e)))?;

        // Remove from cache
        if let Some(template) = template {
            let mut cache = self.slug_cache.write().await;
            cache.remove(&template.metadata.slug);
        }

        info!("Deleted template: {}", id);
        Ok(())
    }

    /// List all templates
    pub async fn list_templates(&self) -> SoulBoxResult<Vec<Template>> {
        let mut templates = Vec::new();
        
        let mut entries = fs::read_dir(&self.storage_path).await
            .map_err(|e| TemplateError::StorageError(format!("Failed to read storage directory: {}", e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| TemplateError::StorageError(format!("Failed to read directory entry: {}", e)))? {
            
            if entry.file_type().await
                .map_err(|e| TemplateError::StorageError(format!("Failed to get file type: {}", e)))?
                .is_dir() 
            {
                let metadata_path = entry.path().join("metadata.json");
                if metadata_path.exists() {
                    match fs::read_to_string(metadata_path).await {
                        Ok(metadata_content) => {
                            match serde_json::from_str::<Template>(&metadata_content) {
                                Ok(template) => templates.push(template),
                                Err(e) => warn!("Failed to parse template metadata in {:?}: {}", entry.path(), e),
                            }
                        }
                        Err(e) => warn!("Failed to read template metadata in {:?}: {}", entry.path(), e),
                    }
                }
            }
        }

        Ok(templates)
    }

    /// List public templates with proper pagination
    pub async fn list_public_templates(
        &self,
        runtime_type: Option<&str>,
        page: u32,
        page_size: u32,
    ) -> SoulBoxResult<Vec<Template>> {
        let all_templates = self.list_templates().await?;
        
        let filtered_templates: Vec<Template> = all_templates
            .into_iter()
            .filter(|t| t.metadata.is_public)
            .filter(|t| {
                if let Some(rt) = runtime_type {
                    t.metadata.runtime_type.to_string() == rt
                } else {
                    true
                }
            })
            .collect();

        // Apply pagination
        let total_items = filtered_templates.len();
        let start_index = ((page - 1) * page_size) as usize;
        let end_index = (start_index + page_size as usize).min(total_items);

        if start_index >= total_items {
            Ok(Vec::new())
        } else {
            Ok(filtered_templates[start_index..end_index].to_vec())
        }
    }

    /// List templates by owner with proper pagination
    pub async fn list_user_templates(
        &self,
        owner_id: Uuid,
        page: u32,
        page_size: u32,
    ) -> SoulBoxResult<Vec<Template>> {
        let all_templates = self.list_templates().await?;
        
        let user_templates: Vec<Template> = all_templates
            .into_iter()
            .filter(|t| t.owner_id == Some(owner_id))
            .collect();

        // Apply pagination
        let total_items = user_templates.len();
        let start_index = ((page - 1) * page_size) as usize;
        let end_index = (start_index + page_size as usize).min(total_items);

        if start_index >= total_items {
            Ok(Vec::new())
        } else {
            Ok(user_templates[start_index..end_index].to_vec())
        }
    }

    /// Store template files to disk with security validations
    async fn store_template_files(&self, template: &Template) -> SoulBoxResult<()> {
        let template_dir = self.storage_path.join(template.metadata.id.to_string());
        
        // Create template directory
        fs::create_dir_all(&template_dir).await
            .map_err(|e| TemplateError::FileError(format!("Failed to create template directory: {}", e)))?;

        // Store metadata
        let metadata_path = template_dir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(template)
            .map_err(|e| TemplateError::FileError(format!("Failed to serialize template: {}", e)))?;
        
        fs::write(metadata_path, metadata_json).await
            .map_err(|e| TemplateError::FileError(format!("Failed to write metadata: {}", e)))?;

        // Store individual files with security checks
        let files_dir = template_dir.join("files");
        if !template.files.is_empty() {
            fs::create_dir_all(&files_dir).await
                .map_err(|e| TemplateError::FileError(format!("Failed to create files directory: {}", e)))?;

            // Calculate total file size for validation
            let mut total_size: u64 = 0;
            const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1MB per file
            const MAX_TOTAL_SIZE: u64 = 10 * 1024 * 1024; // 10MB total

            for (file_path, content) in &template.files {
                // Security validation: Check file size
                let content_size = content.len() as u64;
                if content_size > MAX_FILE_SIZE {
                    return Err(TemplateError::SecurityViolation(
                        format!("File '{}' exceeds maximum size of 1MB", file_path)
                    ).into());
                }

                total_size += content_size;
                if total_size > MAX_TOTAL_SIZE {
                    return Err(TemplateError::SecurityViolation(
                        "Total file size exceeds maximum limit of 10MB".to_string()
                    ).into());
                }

                // Security validation: Prevent path traversal
                if self.is_unsafe_path(file_path)? {
                    return Err(TemplateError::SecurityViolation(
                        format!("Invalid file path detected: '{}'", file_path)
                    ).into());
                }

                let full_path = files_dir.join(file_path);
                
                // Double-check that the resolved path is still within files_dir
                if !self.is_path_within_directory(&full_path, &files_dir)? {
                    return Err(TemplateError::SecurityViolation(
                        format!("File path '{}' attempts to escape template directory", file_path)
                    ).into());
                }
                
                // Create parent directories if needed
                if let Some(parent) = full_path.parent() {
                    fs::create_dir_all(parent).await
                        .map_err(|e| TemplateError::FileError(format!("Failed to create parent directory: {}", e)))?;
                }

                fs::write(full_path, content).await
                    .map_err(|e| TemplateError::FileError(format!("Failed to write file {}: {}", file_path, e)))?;
            }
        }

        Ok(())
    }

    /// Check if a path is unsafe (contains path traversal attempts or absolute paths)
    fn is_unsafe_path(&self, path: &str) -> SoulBoxResult<bool> {
        // Check for common path traversal patterns
        if path.contains("../") || path.contains("..\\") {
            return Ok(true);
        }

        // Check for absolute paths
        if path.starts_with('/') || path.starts_with('\\') {
            return Ok(true);
        }

        // Check for drive letters on Windows
        if path.len() >= 2 && path.chars().nth(1) == Some(':') {
            return Ok(true);
        }

        // Check for null bytes
        if path.contains('\0') {
            return Ok(true);
        }

        // Check for suspicious characters that might be used for attacks
        let suspicious_chars = ['<', '>', '|', '"', '*', '?'];
        if path.chars().any(|c| suspicious_chars.contains(&c)) {
            return Ok(true);
        }

        Ok(false)
    }

    /// Verify that a path is within a specific directory
    fn is_path_within_directory(&self, path: &PathBuf, base_dir: &PathBuf) -> SoulBoxResult<bool> {
        // For security, we need to resolve the path without requiring it to exist
        // We'll manually resolve ".." components by normalizing the path
        let normalized_path = self.normalize_path(path);
        let normalized_base = self.normalize_path(base_dir);

        // Check if the normalized path starts with the base directory
        Ok(normalized_path.starts_with(&normalized_base))
    }

    /// Normalize a path by resolving ".." components without requiring the path to exist
    fn normalize_path(&self, path: &PathBuf) -> PathBuf {
        let mut components = Vec::new();
        
        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    components.pop(); // Remove the last component for ".."
                }
                std::path::Component::CurDir => {
                    // Skip "." components
                }
                _ => {
                    components.push(component);
                }
            }
        }
        
        components.iter().collect()
    }
}