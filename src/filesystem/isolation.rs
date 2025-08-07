use std::collections::HashMap;
use crate::error::{Result, SoulBoxError};

#[derive(Debug, Clone)]
pub struct DiskUsage {
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

impl Default for DiskUsage {
    fn default() -> Self {
        Self {
            used_bytes: 0,
            available_bytes: u64::MAX,
            total_bytes: u64::MAX,
            file_count: 0,
            directory_count: 0,
        }
    }
}

#[derive(Debug)]
pub struct FileSystemIsolation {
    sandbox_filesystems: HashMap<String, super::SandboxFileSystem>,
}

impl FileSystemIsolation {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            sandbox_filesystems: HashMap::new(),
        })
    }

    pub async fn create_isolated_filesystem(
        &mut self,
        sandbox_id: &str,
        total_limit_bytes: Option<u64>,
        file_limit_bytes: Option<u64>,
    ) -> Result<()> {
        let temp_dir = tempfile::tempdir()
            .map_err(|e| SoulBoxError::filesystem(format!("Failed to create temp directory: {}", e)))?;
        
        let sandbox_fs = super::SandboxFileSystem::new(
            temp_dir.path().to_path_buf(),
            total_limit_bytes,
            file_limit_bytes,
        ).await?;

        self.sandbox_filesystems.insert(sandbox_id.to_string(), sandbox_fs);
        std::mem::forget(temp_dir); // Keep temp directory alive
        Ok(())
    }

    pub fn get_filesystem(&self, sandbox_id: &str) -> Result<&super::SandboxFileSystem> {
        self.sandbox_filesystems
            .get(sandbox_id)
            .ok_or_else(|| SoulBoxError::filesystem(format!("Sandbox filesystem not found: {}", sandbox_id)))
    }

    pub fn get_filesystem_mut(&mut self, sandbox_id: &str) -> Result<&mut super::SandboxFileSystem> {
        self.sandbox_filesystems
            .get_mut(sandbox_id)
            .ok_or_else(|| SoulBoxError::filesystem(format!("Sandbox filesystem not found: {}", sandbox_id)))
    }

    pub async fn remove_isolated_filesystem(&mut self, sandbox_id: &str) -> Result<()> {
        self.sandbox_filesystems.remove(sandbox_id)
            .ok_or_else(|| SoulBoxError::filesystem(format!("Sandbox filesystem not found: {}", sandbox_id)))?;
        Ok(())
    }

    pub fn list_isolated_filesystems(&self) -> Vec<String> {
        self.sandbox_filesystems.keys().cloned().collect()
    }
}