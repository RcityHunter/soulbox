use std::path::PathBuf;
use std::collections::HashMap;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{Result, SoulBoxError};
use super::{FileWatcher, FilePermissions, DirectoryListing, FileMetadata, DiskUsage};

#[derive(Debug, Clone)]
pub struct SandboxFileSystem {
    root_path: PathBuf,
    total_limit_bytes: Option<u64>,
    file_limit_bytes: Option<u64>,
    snapshots: HashMap<String, PathBuf>,
}

impl SandboxFileSystem {
    pub async fn new(
        root_path: PathBuf,
        total_limit_bytes: Option<u64>,
        file_limit_bytes: Option<u64>,
    ) -> Result<Self> {
        // Create the sandbox root directory
        fs::create_dir_all(&root_path).await?;
        
        // Create standard directories
        let workspace = root_path.join("workspace");
        let tmp = root_path.join("tmp");
        let home = root_path.join("home");
        
        fs::create_dir_all(&workspace).await?;
        fs::create_dir_all(&tmp).await?;
        fs::create_dir_all(&home).await?;

        Ok(Self {
            root_path,
            total_limit_bytes,
            file_limit_bytes,
            snapshots: HashMap::new(),
        })
    }

    pub fn get_root_path(&self) -> &PathBuf {
        &self.root_path
    }

    pub async fn exists(&self, path: &str) -> Result<bool> {
        let full_path = self.resolve_path(path)?;
        Ok(full_path.exists())
    }

    pub async fn is_writable(&self, path: &str) -> Result<bool> {
        let full_path = self.resolve_path(path)?;
        if !full_path.exists() {
            return Ok(false);
        }
        
        let metadata = fs::metadata(&full_path).await?;
        Ok(!metadata.permissions().readonly())
    }

    pub async fn write_file(&self, path: &str, content: &[u8]) -> Result<()> {
        // Check file size limit
        if let Some(limit) = self.file_limit_bytes {
            if content.len() as u64 > limit {
                return Err(SoulBoxError::resource_limit(
                    format!("File size {} exceeds limit {}", content.len(), limit)
                ));
            }
        }

        let full_path = self.resolve_path(path)?;
        
        // Check total filesystem usage
        if let Some(total_limit) = self.total_limit_bytes {
            let current_usage = self.get_disk_usage().await?.used_bytes;
            if current_usage + content.len() as u64 > total_limit {
                return Err(SoulBoxError::resource_limit(
                    format!("Total filesystem usage would exceed limit {total_limit}")
                ));
            }
        }

        // Create parent directories if needed
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = fs::File::create(&full_path).await?;
        file.write_all(content).await?;
        file.sync_all().await?;

        Ok(())
    }

    pub async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let full_path = self.resolve_path(path)?;
        
        if !full_path.exists() {
            return Err(SoulBoxError::filesystem(
                format!("File not found: {path}")
            ));
        }

        let mut file = fs::File::open(&full_path).await?;
        let mut content = Vec::new();
        file.read_to_end(&mut content).await?;
        Ok(content)
    }

    pub async fn delete_file(&self, path: &str) -> Result<()> {
        let full_path = self.resolve_path(path)?;
        
        if !full_path.exists() {
            return Err(SoulBoxError::filesystem(
                format!("File not found: {path}")
            ));
        }

        fs::remove_file(&full_path).await?;
        Ok(())
    }

    pub async fn create_directory(&self, path: &str, recursive: bool) -> Result<()> {
        let full_path = self.resolve_path(path)?;
        
        if recursive {
            fs::create_dir_all(&full_path).await?;
        } else {
            fs::create_dir(&full_path).await?;
        }
        
        Ok(())
    }

    pub async fn list_directory(&self, path: &str) -> Result<DirectoryListing> {
        let full_path = self.resolve_path(path)?;
        
        if !full_path.exists() {
            return Err(SoulBoxError::filesystem(
                format!("Directory not found: {path}")
            ));
        }

        let mut entries = Vec::new();
        let mut dir_stream = fs::read_dir(&full_path).await?;
        
        while let Some(entry) = dir_stream.next_entry().await? {
            let metadata = entry.metadata().await?;
            let file_name = entry.file_name().to_string_lossy().to_string();
            
            entries.push(FileMetadata {
                name: file_name,
                path: entry.path().to_string_lossy().to_string(),
                size: metadata.len(),
                is_directory: metadata.is_dir(),
                is_symlink: metadata.is_symlink(),
                symlink_target: None, // TODO: Read symlink target
                permissions: FilePermissions::from_metadata(&metadata),
                created_at: metadata.created().unwrap_or(std::time::UNIX_EPOCH),
                modified_at: metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
            });
        }

        Ok(DirectoryListing { entries })
    }

    pub async fn set_permissions(&self, path: &str, permissions: FilePermissions) -> Result<()> {
        let full_path = self.resolve_path(path)?;
        
        // TODO: Implement actual permission setting
        // For now, just verify the file exists
        if !full_path.exists() {
            return Err(SoulBoxError::filesystem(
                format!("File not found: {path}")
            ));
        }

        Ok(())
    }

    pub async fn get_permissions(&self, path: &str) -> Result<FilePermissions> {
        let full_path = self.resolve_path(path)?;
        
        if !full_path.exists() {
            return Err(SoulBoxError::filesystem(
                format!("File not found: {path}")
            ));
        }

        let metadata = fs::metadata(&full_path).await?;
        Ok(FilePermissions::from_metadata(&metadata))
    }

    pub async fn create_symlink(&self, link_path: &str, target_path: &str) -> Result<()> {
        // Security check: ensure target is within sandbox
        let target_full = self.resolve_path(target_path)?;
        if !target_full.starts_with(&self.root_path) {
            return Err(SoulBoxError::security(
                format!("Symlink target outside sandbox: {target_path}")
            ));
        }

        let link_full = self.resolve_path(link_path)?;
        
        #[cfg(unix)]
        tokio::fs::symlink(&target_full, &link_full).await?;
        
        #[cfg(windows)]
        {
            if target_full.is_dir() {
                tokio::fs::symlink_dir(&target_full, &link_full).await?;
            } else {
                tokio::fs::symlink_file(&target_full, &link_full).await?;
            }
        }

        Ok(())
    }

    pub async fn get_file_metadata(&self, path: &str) -> Result<FileMetadata> {
        let full_path = self.resolve_path(path)?;
        
        if !full_path.exists() {
            return Err(SoulBoxError::filesystem(
                format!("File not found: {path}")
            ));
        }

        let metadata = fs::metadata(&full_path).await?;
        let file_name = full_path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let symlink_target = if metadata.is_symlink() {
            Some(fs::read_link(&full_path).await?.to_string_lossy().to_string())
        } else {
            None
        };

        Ok(FileMetadata {
            name: file_name,
            path: path.to_string(),
            size: metadata.len(),
            is_directory: metadata.is_dir(),
            is_symlink: metadata.is_symlink(),
            symlink_target,
            permissions: FilePermissions::from_metadata(&metadata),
            created_at: metadata.created().unwrap_or(std::time::UNIX_EPOCH),
            modified_at: metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
        })
    }

    pub async fn get_disk_usage(&self) -> Result<DiskUsage> {
        let mut used_bytes = 0u64;
        let mut file_count = 0u64;
        let mut directory_count = 0u64;

        let mut stack = vec![self.root_path.clone()];
        
        while let Some(dir_path) = stack.pop() {
            let mut dir_stream = match fs::read_dir(&dir_path).await {
                Ok(stream) => stream,
                Err(_) => continue,
            };

            while let Some(entry) = dir_stream.next_entry().await? {
                let metadata = entry.metadata().await?;
                
                if metadata.is_dir() {
                    directory_count += 1;
                    stack.push(entry.path());
                } else {
                    file_count += 1;
                    used_bytes += metadata.len();
                }
            }
        }

        let total_bytes = self.total_limit_bytes.unwrap_or(u64::MAX);
        let available_bytes = total_bytes.saturating_sub(used_bytes);

        Ok(DiskUsage {
            used_bytes,
            available_bytes,
            total_bytes,
            file_count,
            directory_count,
        })
    }

    pub async fn create_file_watcher(&self, path: &str, recursive: bool) -> Result<FileWatcher> {
        let full_path = self.resolve_path(path)?;
        FileWatcher::new(full_path, recursive).await
    }

    pub async fn create_snapshot(&mut self, name: &str) -> Result<String> {
        let snapshot_id = format!("snapshot_{}_{}", name, chrono::Utc::now().timestamp());
        let snapshot_path = self.root_path.parent()
            .unwrap()
            .join(format!("{snapshot_id}_snapshot"));

        // TODO: Implement actual filesystem snapshot
        // For now, just create an empty directory as a placeholder
        fs::create_dir_all(&snapshot_path).await?;
        
        self.snapshots.insert(snapshot_id.clone(), snapshot_path);
        Ok(snapshot_id)
    }

    pub async fn restore_from_snapshot(&self, snapshot_id: &str) -> Result<()> {
        let _snapshot_path = self.snapshots.get(snapshot_id)
            .ok_or_else(|| SoulBoxError::filesystem(
                format!("Snapshot not found: {snapshot_id}")
            ))?;

        // TODO: Implement actual snapshot restoration
        // For now, just return success
        Ok(())
    }

    fn resolve_path(&self, path: &str) -> Result<PathBuf> {
        let path = path.strip_prefix('/').unwrap_or(path);
        let full_path = self.root_path.join(path);
        
        // Security check: ensure path is within sandbox
        // Use canonical root path for comparison
        let canonical_root = self.root_path.canonicalize().unwrap_or(self.root_path.clone());
        
        let resolved_path = if full_path.exists() {
            full_path.canonicalize().unwrap_or(full_path)
        } else {
            // For non-existent paths, normalize manually to prevent .. attacks
            let mut components = Vec::new();
            for component in full_path.components() {
                match component {
                    std::path::Component::Normal(name) => components.push(name),
                    std::path::Component::ParentDir => {
                        if !components.is_empty() {
                            components.pop();
                        }
                    }
                    std::path::Component::CurDir => {}
                    _ => {}
                }
            }
            canonical_root.join(components.iter().collect::<PathBuf>())
        };

        if !resolved_path.starts_with(&canonical_root) {
            return Err(SoulBoxError::security(
                format!("Path outside sandbox: {path}")
            ));
        }

        Ok(resolved_path)
    }
}