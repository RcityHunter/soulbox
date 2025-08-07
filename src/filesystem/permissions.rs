use std::time::SystemTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePermissions {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub owner_id: u32,
    pub group_id: u32,
    pub mode: u32,
}

#[derive(Debug, Clone)]
pub struct DirectoryListing {
    pub entries: Vec<FileMetadata>,
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
    pub permissions: FilePermissions,
    pub created_at: SystemTime,
    pub modified_at: SystemTime,
}

impl FilePermissions {
    pub fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let mode = metadata.mode();
            Self {
                readable: mode & 0o444 != 0,
                writable: mode & 0o222 != 0,
                executable: mode & 0o111 != 0,
                owner_id: metadata.uid(),
                group_id: metadata.gid(),
                mode,
            }
        }

        #[cfg(windows)]
        {
            Self {
                readable: true, // Windows doesn't have the same permission model
                writable: !metadata.permissions().readonly(),
                executable: false, // Can't easily determine on Windows
                owner_id: 0,
                group_id: 0,
                mode: if metadata.permissions().readonly() { 0o444 } else { 0o644 },
            }
        }
    }
}