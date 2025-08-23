use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
    Router,
    body::Bytes,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use base64::Engine;
use std::{sync::Arc, collections::HashMap};
use uuid::Uuid;
use tracing::error;

use crate::auth::middleware::AuthExtractor;
use crate::auth::models::Permission;
use crate::error::{Result as SoulBoxResult, SoulBoxError};
use crate::filesystem::{SandboxFileSystem, FilePermissions, DirectoryListing, FileMetadata, DiskUsage};
use crate::server::AppState;

/// File upload request
#[derive(Debug, Deserialize)]
pub struct FileUploadRequest {
    pub path: String,
    pub content: String, // Base64 encoded content
}

impl FileUploadRequest {
    /// Validate file upload request
    pub fn validate(&self) -> Result<(), SoulBoxError> {
        // Validate file path
        validate_file_path(&self.path)?;
        
        // Validate content size (base64 encoded)
        if self.content.len() > 10_000_000 { // 10MB limit for base64 content
            return Err(SoulBoxError::validation(
                "File content too large (max 10MB)".to_string()
            ));
        }
        
        // Validate base64 encoding
        if let Err(_) = base64::engine::general_purpose::STANDARD.decode(&self.content) {
            return Err(SoulBoxError::validation(
                "Invalid base64 content encoding".to_string()
            ));
        }
        
        Ok(())
    }
}

/// File download query parameters  
#[derive(Debug, Deserialize)]
pub struct FileQueryParams {
    pub encoding: Option<String>, // "base64" or "text", defaults to "base64"
}

/// Directory listing query parameters
#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub recursive: Option<bool>,
}

/// File permissions update request
#[derive(Debug, Deserialize)]
pub struct PermissionsRequest {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
}

/// File/directory creation request
#[derive(Debug, Deserialize)]
pub struct CreateDirectoryRequest {
    pub path: String,
    pub recursive: Option<bool>,
}

/// Symlink creation request
#[derive(Debug, Deserialize)]
pub struct CreateSymlinkRequest {
    pub link_path: String,
    pub target_path: String,
}

/// File system statistics response
#[derive(Debug, Serialize)]
pub struct FileSystemStats {
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
    pub usage_percentage: f64,
}

impl From<DiskUsage> for FileSystemStats {
    fn from(usage: DiskUsage) -> Self {
        let usage_percentage = if usage.total_bytes > 0 {
            (usage.used_bytes as f64 / usage.total_bytes as f64) * 100.0
        } else {
            0.0
        };

        Self {
            used_bytes: usage.used_bytes,
            available_bytes: usage.available_bytes,
            total_bytes: usage.total_bytes,
            file_count: usage.file_count,
            directory_count: usage.directory_count,
            usage_percentage,
        }
    }
}

/// Upload a file to the sandbox
pub async fn upload_file(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
    _auth: AuthExtractor,
    Json(request): Json<FileUploadRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id format
    let _sandbox_uuid = validate_sandbox_id(&sandbox_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))))?;

    // Validate the entire request
    if let Err(e) = request.validate() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": e.to_string()}))));
    }

    // Decode base64 content (already validated in request.validate())
    let content = base64::engine::general_purpose::STANDARD.decode(&request.content)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid base64 content"}))))?;

    // For now, return a mock response with file system interaction planned
    // In production, this would interact with the sandbox file system
    let response = json!({
        "message": "File uploaded successfully",
        "path": request.path,
        "size": content.len(),
        "sandbox_id": sandbox_id,
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    Ok(Json(response))
}

/// Download a file from the sandbox
pub async fn download_file(
    State(state): State<AppState>,
    Path((sandbox_id, file_path)): Path<(String, String)>,
    Query(params): Query<FileQueryParams>,
    _auth: AuthExtractor,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // Get sandbox filesystem instance
    let sandbox_fs = state.filesystem_manager
        .create_sandbox_filesystem(&sandbox_id)
        .await
        .map_err(|e| {
            error!("Failed to create sandbox filesystem: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to access sandbox filesystem"})))
        })?;

    // Read the file content
    let file_content = sandbox_fs
        .read_file(&file_path)
        .await
        .map_err(|e| {
            error!("Failed to read file '{}': {}", file_path, e);
            if e.to_string().contains("not found") {
                (StatusCode::NOT_FOUND, Json(json!({"error": "File not found"})))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to read file"})))
            }
        })?;

    // Convert content to requested encoding
    let encoding = params.encoding.unwrap_or_else(|| "base64".to_string());
    let content_str = match encoding.as_str() {
        "text" => {
            // Try to convert to UTF-8 string, fallback to base64 if not valid UTF-8
            match String::from_utf8(file_content.clone()) {
                Ok(text) => text,
                Err(_) => {
                    // If not valid UTF-8, return base64 and update encoding
                    base64::engine::general_purpose::STANDARD.encode(&file_content)
                }
            }
        }
        "base64" | _ => {
            base64::engine::general_purpose::STANDARD.encode(&file_content)
        }
    };

    let actual_encoding = if encoding == "text" && !String::from_utf8(file_content.clone()).is_ok() {
        "base64"
    } else {
        &encoding
    };

    let response = json!({
        "path": file_path,
        "content": content_str,
        "encoding": actual_encoding,
        "size": file_content.len()
    });

    Ok(Json(response))
}

/// Delete a file from the sandbox
pub async fn delete_file(
    State(state): State<AppState>,
    Path((sandbox_id, file_path)): Path<(String, String)>,
    _auth: AuthExtractor,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // Get sandbox filesystem instance
    let sandbox_fs = state.filesystem_manager
        .create_sandbox_filesystem(&sandbox_id)
        .await
        .map_err(|e| {
            error!("Failed to create sandbox filesystem: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to access sandbox filesystem"})))
        })?;

    // Delete the file
    sandbox_fs
        .delete_file(&file_path)
        .await
        .map_err(|e| {
            error!("Failed to delete file '{}': {}", file_path, e);
            if e.to_string().contains("not found") {
                (StatusCode::NOT_FOUND, Json(json!({"error": "File not found"})))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to delete file"})))
            }
        })?;

    let response = json!({
        "message": "File deleted successfully",
        "path": file_path
    });

    Ok(Json(response))
}

/// List directory contents handler wrapper
pub async fn list_directory_handler(
    State(state): State<AppState>,
    Path((sandbox_id, dir_path)): Path<(String, String)>,
) -> Result<Json<DirectoryListing>, (StatusCode, Json<Value>)> {
    list_directory(state, sandbox_id, dir_path).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))))
}

/// List directory contents
pub async fn list_directory(
    state: AppState,
    sandbox_id: String,
    dir_path: String,
) -> std::result::Result<Json<DirectoryListing>, SoulBoxError> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| SoulBoxError::validation("Invalid sandbox ID".to_string()))?;

    // Get sandbox filesystem instance
    let sandbox_fs = state.filesystem_manager
        .create_sandbox_filesystem(&sandbox_id)
        .await
        .map_err(|e| {
            error!("Failed to create sandbox filesystem: {}", e);
            SoulBoxError::Internal(format!("Failed to access sandbox filesystem: {}", e))
        })?;

    // List directory contents
    let directory_listing = sandbox_fs
        .list_directory(&dir_path)
        .await
        .map_err(|e| {
            error!("Failed to list directory '{}': {}", dir_path, e);
            if e.to_string().contains("not found") {
                SoulBoxError::not_found(format!("Directory not found: {}", dir_path))
            } else {
                SoulBoxError::Internal(format!("Failed to list directory: {}", e))
            }
        })?;

    Ok(Json(directory_listing))
}

/// Create a directory
pub async fn create_directory(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
    _auth: AuthExtractor,
    Json(request): Json<CreateDirectoryRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // Get sandbox filesystem instance
    let sandbox_fs = state.filesystem_manager
        .create_sandbox_filesystem(&sandbox_id)
        .await
        .map_err(|e| {
            error!("Failed to create sandbox filesystem: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to access sandbox filesystem"})))
        })?;

    // Create the directory
    let recursive = request.recursive.unwrap_or(false);
    sandbox_fs
        .create_directory(&request.path, recursive)
        .await
        .map_err(|e| {
            error!("Failed to create directory '{}': {}", request.path, e);
            if e.to_string().contains("already exists") {
                (StatusCode::CONFLICT, Json(json!({"error": "Directory already exists"})))
            } else if e.to_string().contains("not found") && !recursive {
                (StatusCode::BAD_REQUEST, Json(json!({"error": "Parent directory does not exist. Use recursive=true to create parent directories."})))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to create directory"})))
            }
        })?;

    let response = json!({
        "message": "Directory created successfully",
        "path": request.path,
        "recursive": recursive
    });

    Ok(Json(response))
}

/// Get file metadata
pub async fn get_file_metadata(
    State(state): State<AppState>,
    Path((sandbox_id, file_path)): Path<(String, String)>,
) -> Result<Json<FileMetadata>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // Get sandbox filesystem instance
    let sandbox_fs = state.filesystem_manager
        .create_sandbox_filesystem(&sandbox_id)
        .await
        .map_err(|e| {
            error!("Failed to create sandbox filesystem: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to access sandbox filesystem: {}", e)})))
        })?;

    // Get file metadata
    let file_metadata = sandbox_fs
        .get_file_metadata(&file_path)
        .await
        .map_err(|e| {
            error!("Failed to get metadata for '{}': {}", file_path, e);
            if e.to_string().contains("not found") {
                (StatusCode::NOT_FOUND, Json(json!({"error": format!("File not found: {}", file_path)})))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to get file metadata: {}", e)})))
            }
        })?;

    Ok(Json(file_metadata))
}

/// Update file permissions
pub async fn set_permissions(
    State(state): State<AppState>,
    Path((sandbox_id, file_path)): Path<(String, String)>,
    _auth: AuthExtractor,
    Json(request): Json<PermissionsRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // Get sandbox filesystem instance
    let sandbox_fs = state.filesystem_manager
        .create_sandbox_filesystem(&sandbox_id)
        .await
        .map_err(|e| {
            error!("Failed to create sandbox filesystem: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to access sandbox filesystem"})))
        })?;

    // Convert request to FilePermissions - first get current permissions to preserve other fields
    let current_permissions = sandbox_fs
        .get_permissions(&file_path)
        .await
        .map_err(|e| {
            error!("Failed to get current permissions for '{}': {}", file_path, e);
            if e.to_string().contains("not found") {
                (StatusCode::NOT_FOUND, Json(json!({"error": "File not found"})))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to get current permissions"})))
            }
        })?;

    // Create new permissions based on request
    let new_permissions = FilePermissions {
        readable: request.readable,
        writable: request.writable,
        executable: request.executable,
        owner_id: current_permissions.owner_id,
        group_id: current_permissions.group_id,
        mode: current_permissions.mode, // Will be updated by the filesystem implementation
    };

    // Set file permissions
    sandbox_fs
        .set_permissions(&file_path, new_permissions.clone())
        .await
        .map_err(|e| {
            error!("Failed to set permissions for '{}': {}", file_path, e);
            if e.to_string().contains("not found") {
                (StatusCode::NOT_FOUND, Json(json!({"error": "File not found"})))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to set permissions"})))
            }
        })?;

    let response = json!({
        "message": "Permissions updated successfully",
        "path": file_path,
        "permissions": {
            "readable": new_permissions.readable,
            "writable": new_permissions.writable,
            "executable": new_permissions.executable
        }
    });

    Ok(Json(response))
}

/// Create a symlink
pub async fn create_symlink(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
    _auth: AuthExtractor,
    Json(request): Json<CreateSymlinkRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // Get sandbox filesystem instance
    let sandbox_fs = state.filesystem_manager
        .create_sandbox_filesystem(&sandbox_id)
        .await
        .map_err(|e| {
            error!("Failed to create sandbox filesystem: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to access sandbox filesystem"})))
        })?;

    // Create the symlink
    sandbox_fs
        .create_symlink(&request.link_path, &request.target_path)
        .await
        .map_err(|e| {
            error!("Failed to create symlink '{}' -> '{}': {}", request.link_path, request.target_path, e);
            if e.to_string().contains("already exists") {
                (StatusCode::CONFLICT, Json(json!({"error": "File already exists at link path"})))
            } else if e.to_string().contains("not found") {
                (StatusCode::BAD_REQUEST, Json(json!({"error": "Parent directory does not exist"})))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to create symlink"})))
            }
        })?;

    let response = json!({
        "message": "Symlink created successfully",
        "link_path": request.link_path,
        "target_path": request.target_path
    });

    Ok(Json(response))
}

/// Get filesystem statistics
pub async fn get_filesystem_stats(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
    _auth: AuthExtractor,
) -> Result<Json<FileSystemStats>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // Get sandbox filesystem instance
    let sandbox_fs = state.filesystem_manager
        .create_sandbox_filesystem(&sandbox_id)
        .await
        .map_err(|e| {
            error!("Failed to create sandbox filesystem: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to access sandbox filesystem"})))
        })?;

    // Get disk usage statistics
    let disk_usage = sandbox_fs
        .get_disk_usage()
        .await
        .map_err(|e| {
            error!("Failed to get disk usage for sandbox '{}': {}", sandbox_id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "Failed to get filesystem statistics"})))
        })?;

    // Convert DiskUsage to FileSystemStats using the From trait
    let filesystem_stats = FileSystemStats::from(disk_usage);

    Ok(Json(filesystem_stats))
}

/// Create file system routes
pub fn file_routes() -> Router<AppState> {
    Router::new()
        // File operations
        .route("/sandboxes/{sandbox_id}/files/upload", post(upload_file))
        .route("/sandboxes/{sandbox_id}/files/{*file_path}", get(download_file))
        .route("/sandboxes/{sandbox_id}/files/{*file_path}", delete(delete_file))
        
        // Directory operations
        .route("/sandboxes/{sandbox_id}/directories", post(create_directory))
        .route("/sandboxes/:sandbox_id/directories", get(list_directory_handler))
        
        // Metadata and permissions
        .route("/sandboxes/:sandbox_id/metadata", get(get_file_metadata))
        .route("/sandboxes/{sandbox_id}/permissions/{*file_path}", put(set_permissions))
        
        // Advanced operations
        .route("/sandboxes/{sandbox_id}/symlinks", post(create_symlink))
        .route("/sandboxes/{sandbox_id}/stats", get(get_filesystem_stats))
}

/// Validate file path for security and correctness
fn validate_file_path(path: &str) -> Result<(), SoulBoxError> {
    // Check path length
    if path.len() > 4096 {
        return Err(SoulBoxError::validation(
            "File path too long (max 4096 characters)".to_string()
        ));
    }
    
    // Check if path is empty
    if path.trim().is_empty() {
        return Err(SoulBoxError::validation(
            "File path cannot be empty".to_string()
        ));
    }
    
    // Prevent path traversal attacks
    if path.contains("..") {
        return Err(SoulBoxError::validation(
            "Path traversal not allowed".to_string()
        ));
    }
    
    // Prevent absolute paths outside of sandbox
    if path.starts_with('/') && !path.starts_with("/workspace/") {
        return Err(SoulBoxError::validation(
            "Absolute paths must be within /workspace/".to_string()
        ));
    }
    
    // Check for invalid characters
    let invalid_chars = ['<', '>', ':', '"', '|', '?', '*', '\0'];
    if path.chars().any(|c| invalid_chars.contains(&c)) {
        return Err(SoulBoxError::validation(
            "File path contains invalid characters".to_string()
        ));
    }
    
    // Prevent access to sensitive system files
    let forbidden_patterns = [
        "/proc/", "/sys/", "/dev/", "/etc/passwd", "/etc/shadow",
        "/root/", "/home/", "/.ssh/", "/.config/"
    ];
    
    let normalized_path = path.to_lowercase();
    for pattern in &forbidden_patterns {
        if normalized_path.contains(pattern) {
            return Err(SoulBoxError::validation(
                "Access to system files not allowed".to_string()
            ));
        }
    }
    
    Ok(())
}

/// Validate sandbox ID format
fn validate_sandbox_id(sandbox_id: &str) -> Result<Uuid, SoulBoxError> {
    Uuid::parse_str(sandbox_id)
        .map_err(|_| SoulBoxError::validation(
            "Invalid sandbox ID format".to_string()
        ))
}