pub mod models;
pub mod manager;
pub mod storage;
pub mod file_storage;
pub mod dockerfile;

pub use models::{Template, TemplateVersion, TemplateMetadata, TemplateValidationResult, TemplateFile, TemplateFileUpload};
pub use manager::TemplateManager;
pub use storage::TemplateStorage;
pub use file_storage::TemplateFileStorage;
pub use dockerfile::{DockerfileParser, DockerImageBuilder};

use crate::error::{Result as SoulBoxResult, SoulBoxError};

/// Template system errors
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("Template not found: {0}")]
    NotFound(String),
    
    #[error("Template validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Conflict error: {0}")]
    ConflictError(String),
    
    #[error("Template version conflict: {0}")]
    VersionConflict(String),
    
    #[error("Template file error: {0}")]
    FileError(String),
    
    #[error("Template storage error: {0}")]
    StorageError(String),
    
    #[error("Security violation: {0}")]
    SecurityViolation(String),
    
    #[error("Template already exists with slug: {0}")]
    SlugConflict(String),
    
    #[error("Insufficient permissions: {0}")]
    PermissionDenied(String),
    
    #[error("Template is verified and cannot be modified")]
    VerifiedTemplateImmutable,
    
    #[error("Database operation failed: {0}")]
    DatabaseError(String),
}

impl From<TemplateError> for SoulBoxError {
    fn from(err: TemplateError) -> Self {
        SoulBoxError::Internal(err.to_string())
    }
}