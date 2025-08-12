use tempfile::TempDir;

use crate::error::Result;
use super::SandboxFileSystem;

#[derive(Debug)]
pub struct FileSystemManager {
    // Base directory for all sandbox filesystems
    base_dir: TempDir,
}

impl FileSystemManager {
    pub async fn new() -> Result<Self> {
        let base_dir = tempfile::tempdir()?;
        Ok(Self { base_dir })
    }

    pub async fn create_sandbox_filesystem(&self, sandbox_id: &str) -> Result<SandboxFileSystem> {
        let sandbox_root = self.base_dir.path().join(sandbox_id);
        SandboxFileSystem::new(sandbox_root, None, None).await
    }

    pub async fn create_sandbox_filesystem_with_limits(
        &self,
        sandbox_id: &str,
        total_limit_bytes: u64,
        file_limit_bytes: u64,
    ) -> Result<SandboxFileSystem> {
        let sandbox_root = self.base_dir.path().join(sandbox_id);
        SandboxFileSystem::new(sandbox_root, Some(total_limit_bytes), Some(file_limit_bytes)).await
    }
}