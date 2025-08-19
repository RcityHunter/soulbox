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
    State(_state): State<AppState>,
    Path(sandbox_id): Path<String>,
    _auth: AuthExtractor,
    Json(request): Json<FileUploadRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // Decode base64 content
    let content = base64::engine::general_purpose::STANDARD.decode(&request.content)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid base64 content"}))))?;

    // TODO: Get actual SandboxFileSystem instance for the sandbox
    // For now, return a mock response
    let response = json!({
        "message": "File uploaded successfully",
        "path": request.path,
        "size": content.len()
    });

    Ok(Json(response))
}

/// Download a file from the sandbox
pub async fn download_file(
    State(_state): State<AppState>,
    Path((sandbox_id, file_path)): Path<(String, String)>,
    Query(params): Query<FileQueryParams>,
    _auth: AuthExtractor,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // TODO: Get actual SandboxFileSystem instance and read file
    // For now, return a mock response
    let encoding = params.encoding.unwrap_or_else(|| "base64".to_string());
    let mock_content = if encoding == "text" {
        "Hello, World!".to_string()
    } else {
        base64::engine::general_purpose::STANDARD.encode("Hello, World!")
    };

    let response = json!({
        "path": file_path,
        "content": mock_content,
        "encoding": encoding,
        "size": 13
    });

    Ok(Json(response))
}

/// Delete a file from the sandbox
pub async fn delete_file(
    State(_state): State<AppState>,
    Path((sandbox_id, file_path)): Path<(String, String)>,
    _auth: AuthExtractor,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // TODO: Get actual SandboxFileSystem instance and delete file
    
    let response = json!({
        "message": "File deleted successfully",
        "path": file_path
    });

    Ok(Json(response))
}

/// List directory contents
pub async fn list_directory(
    State(_state): State<AppState>,
    Path((sandbox_id, dir_path)): Path<(String, String)>,
    Query(_params): Query<ListParams>,
    _auth: AuthExtractor,
) -> Result<Json<DirectoryListing>, (StatusCode, Json<Value>)> {
    let _dir_path = dir_path;
    
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // TODO: Get actual SandboxFileSystem instance and list directory
    // For now, return a mock response
    let mock_listing = DirectoryListing {
        entries: vec![], // Empty directory
    };

    Ok(Json(mock_listing))
}

/// Create a directory
pub async fn create_directory(
    State(_state): State<AppState>,
    Path(sandbox_id): Path<String>,
    _auth: AuthExtractor,
    Json(request): Json<CreateDirectoryRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // TODO: Get actual SandboxFileSystem instance and create directory
    
    let response = json!({
        "message": "Directory created successfully",
        "path": request.path,
        "recursive": request.recursive.unwrap_or(false)
    });

    Ok(Json(response))
}

/// Get file metadata
pub async fn get_file_metadata(
    State(_state): State<AppState>,
    Path((sandbox_id, file_path)): Path<(String, String)>,
    _auth: AuthExtractor,
) -> Result<Json<FileMetadata>, (StatusCode, Json<Value>)> {
    
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // TODO: Get actual SandboxFileSystem instance and get metadata
    // For now, return a mock response
    let mock_metadata = FileMetadata {
        name: "mock_file.txt".to_string(),
        path: file_path.to_string(),
        size: 0,
        is_directory: false,
        is_symlink: false,
        symlink_target: None,
        permissions: FilePermissions {
            readable: true,
            writable: true,
            executable: false,
            owner_id: 1000,  // Default user ID
            group_id: 1000,  // Default group ID
            mode: 0o644,     // Default file mode (readable/writable by owner, readable by others)
        },
        created_at: std::time::UNIX_EPOCH,
        modified_at: std::time::UNIX_EPOCH,
    };

    Ok(Json(mock_metadata))
}

/// Update file permissions
pub async fn set_permissions(
    State(_state): State<AppState>,
    Path((sandbox_id, file_path)): Path<(String, String)>,
    _auth: AuthExtractor,
    Json(request): Json<PermissionsRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // TODO: Get actual SandboxFileSystem instance and set permissions
    
    let response = json!({
        "message": "Permissions updated successfully",
        "path": file_path,
        "permissions": {
            "readable": request.readable,
            "writable": request.writable,
            "executable": request.executable
        }
    });

    Ok(Json(response))
}

/// Create a symlink
pub async fn create_symlink(
    State(_state): State<AppState>,
    Path(sandbox_id): Path<String>,
    _auth: AuthExtractor,
    Json(request): Json<CreateSymlinkRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // TODO: Get actual SandboxFileSystem instance and create symlink
    
    let response = json!({
        "message": "Symlink created successfully",
        "link_path": request.link_path,
        "target_path": request.target_path
    });

    Ok(Json(response))
}

/// Get filesystem statistics
pub async fn get_filesystem_stats(
    State(_state): State<AppState>,
    Path(sandbox_id): Path<String>,
    _auth: AuthExtractor,
) -> Result<Json<FileSystemStats>, (StatusCode, Json<Value>)> {
    // Validate sandbox_id is a valid UUID
    let _sandbox_uuid = Uuid::parse_str(&sandbox_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid sandbox ID"}))))?;

    // TODO: Get actual SandboxFileSystem instance and get stats
    // For now, return mock stats
    let mock_stats = FileSystemStats {
        used_bytes: 1024,
        available_bytes: 1024 * 1024 * 100, // 100MB
        total_bytes: 1024 * 1024 * 100 + 1024,
        file_count: 5,
        directory_count: 3,
        usage_percentage: 0.001,
    };

    Ok(Json(mock_stats))
}

/// Create file system routes
pub fn file_routes() -> Router<AppState> {
    Router::new()
        // File operations
        .route("/sandboxes/:sandbox_id/files/upload", post(upload_file))
        .route("/sandboxes/:sandbox_id/files/*file_path", get(download_file))
        .route("/sandboxes/:sandbox_id/files/*file_path", delete(delete_file))
        
        // Directory operations
        .route("/sandboxes/:sandbox_id/directories", post(create_directory))
        .route("/sandboxes/:sandbox_id/directories/*dir_path", get(list_directory))
        
        // Metadata and permissions
        .route("/sandboxes/:sandbox_id/metadata/*file_path", get(get_file_metadata))
        .route("/sandboxes/:sandbox_id/permissions/*file_path", put(set_permissions))
        
        // Advanced operations
        .route("/sandboxes/:sandbox_id/symlinks", post(create_symlink))
        .route("/sandboxes/:sandbox_id/stats", get(get_filesystem_stats))
}