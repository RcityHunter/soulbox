use crate::template::{TemplateError, models::{TemplateFile, TemplateFileUpload}};
use crate::error::Result as SoulBoxResult;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use tokio::fs;
use tracing::{info, warn, error};
use uuid::Uuid;
use sha2::{Sha256, Digest};

/// Template file storage service
/// Handles actual file content storage and retrieval separately from template metadata
pub struct TemplateFileStorage {
    storage_path: PathBuf,
}

impl TemplateFileStorage {
    pub fn new(storage_path: impl AsRef<Path>) -> Self {
        let storage_path = storage_path.as_ref().to_path_buf();
        Self { storage_path }
    }

    /// Initialize storage directory
    pub async fn initialize(&self) -> SoulBoxResult<()> {
        if !self.storage_path.exists() {
            fs::create_dir_all(&self.storage_path).await
                .map_err(|e| TemplateError::StorageError(format!("Failed to create file storage directory: {}", e)))?;
            info!("Created template file storage directory: {:?}", self.storage_path);
        }
        Ok(())
    }

    /// Store files for a template
    pub async fn store_template_files(
        &self,
        template_id: Uuid,
        file_uploads: Vec<TemplateFileUpload>,
    ) -> SoulBoxResult<Vec<TemplateFile>> {
        let template_dir = self.storage_path.join(template_id.to_string());
        
        // Create template directory
        fs::create_dir_all(&template_dir).await
            .map_err(|e| TemplateError::FileError(format!("Failed to create template directory: {}", e)))?;

        let mut stored_files = Vec::new();
        let mut total_size = 0u64;
        const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1MB per file
        const MAX_TOTAL_SIZE: u64 = 10 * 1024 * 1024; // 10MB total

        for file_upload in file_uploads {
            // Security validation: Check path
            if self.is_unsafe_path(&file_upload.path)? {
                return Err(TemplateError::SecurityViolation(
                    format!("Invalid file path detected: '{}'", file_upload.path)
                ).into());
            }

            // For now, treat content as plain text (base64 support can be added later)
            let content = file_upload.content.as_bytes().to_vec();

            // Security validation: Check file size
            let content_size = content.len() as u64;
            if content_size > MAX_FILE_SIZE {
                return Err(TemplateError::SecurityViolation(
                    format!("File '{}' exceeds maximum size of 1MB", file_upload.path)
                ).into());
            }

            total_size += content_size;
            if total_size > MAX_TOTAL_SIZE {
                return Err(TemplateError::SecurityViolation(
                    "Total file size exceeds maximum limit of 10MB".to_string()
                ).into());
            }

            // Calculate checksum
            let mut hasher = Sha256::new();
            hasher.update(&content);
            let checksum = format!("{:x}", hasher.finalize());

            // Store file
            let file_path = template_dir.join(&file_upload.path);
            
            // Create parent directories if needed
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent).await
                    .map_err(|e| TemplateError::FileError(format!("Failed to create parent directory: {}", e)))?;
            }

            fs::write(&file_path, &content).await
                .map_err(|e| TemplateError::FileError(format!("Failed to write file {}: {}", file_upload.path, e)))?;

            // Create file reference
            let template_file = TemplateFile {
                path: file_upload.path.clone(),
                size: content_size,
                mime_type: file_upload.mime_type.or_else(|| self.detect_mime_type(&file_upload.path)),
                checksum: Some(checksum),
                created_at: chrono::Utc::now(),
            };

            stored_files.push(template_file);
            info!("Stored file: {} ({} bytes)", file_upload.path, content_size);
        }

        Ok(stored_files)
    }

    /// Retrieve file content for a template
    pub async fn get_file_content(
        &self,
        template_id: Uuid,
        file_path: &str,
    ) -> SoulBoxResult<Vec<u8>> {
        // Security validation
        if self.is_unsafe_path(file_path)? {
            return Err(TemplateError::SecurityViolation(
                format!("Invalid file path: '{}'", file_path)
            ).into());
        }

        let template_dir = self.storage_path.join(template_id.to_string());
        let full_path = template_dir.join(file_path);

        // Ensure path is within template directory
        if !self.is_path_within_directory(&full_path, &template_dir)? {
            return Err(TemplateError::SecurityViolation(
                format!("File path '{}' attempts to escape template directory", file_path)
            ).into());
        }

        let content = fs::read(&full_path).await
            .map_err(|e| TemplateError::FileError(format!("Failed to read file {}: {}", file_path, e)))?;

        Ok(content)
    }

    /// Delete all files for a template
    pub async fn delete_template_files(&self, template_id: Uuid) -> SoulBoxResult<()> {
        let template_dir = self.storage_path.join(template_id.to_string());
        
        if template_dir.exists() {
            fs::remove_dir_all(&template_dir).await
                .map_err(|e| TemplateError::FileError(format!("Failed to delete template files: {}", e)))?;
            info!("Deleted files for template: {}", template_id);
        }

        Ok(())
    }

    /// Get all file contents as a HashMap (for backward compatibility)
    pub async fn get_all_file_contents(
        &self,
        template_id: Uuid,
        files: &[TemplateFile],
    ) -> SoulBoxResult<HashMap<String, String>> {
        let mut file_contents = HashMap::new();

        for file in files {
            match self.get_file_content(template_id, &file.path).await {
                Ok(content) => {
                    // Convert to string (assuming text files for now)
                    match String::from_utf8(content) {
                        Ok(text) => {
                            file_contents.insert(file.path.clone(), text);
                        }
                        Err(_) => {
                            warn!("File '{}' contains binary content, skipping", file.path);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to read file '{}': {}", file.path, e);
                }
            }
        }

        Ok(file_contents)
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

        // Check for suspicious characters
        let suspicious_chars = ['<', '>', '|', '"', '*', '?'];
        if path.chars().any(|c| suspicious_chars.contains(&c)) {
            return Ok(true);
        }

        Ok(false)
    }

    /// Verify that a path is within a specific directory
    fn is_path_within_directory(&self, path: &PathBuf, base_dir: &PathBuf) -> SoulBoxResult<bool> {
        let normalized_path = self.normalize_path(path);
        let normalized_base = self.normalize_path(base_dir);
        Ok(normalized_path.starts_with(&normalized_base))
    }

    /// Normalize a path by resolving ".." components
    fn normalize_path(&self, path: &PathBuf) -> PathBuf {
        let mut components = Vec::new();
        
        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    components.pop();
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

    /// Detect MIME type based on file extension
    fn detect_mime_type(&self, path: &str) -> Option<String> {
        let extension = Path::new(path)
            .extension()?
            .to_str()?
            .to_lowercase();

        let mime_type = match extension.as_str() {
            "js" => "application/javascript",
            "ts" => "application/typescript",
            "py" => "text/x-python",
            "rs" => "text/x-rust",
            "go" => "text/x-go",
            "java" => "text/x-java-source",
            "json" => "application/json",
            "toml" => "text/x-toml",
            "yaml" | "yml" => "text/yaml",
            "md" => "text/markdown",
            "txt" => "text/plain",
            "html" => "text/html",
            "css" => "text/css",
            "xml" => "application/xml",
            "sh" => "application/x-sh",
            _ => "text/plain",
        };

        Some(mime_type.to_string())
    }
}