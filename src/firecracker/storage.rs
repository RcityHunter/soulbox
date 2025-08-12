use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;
use tracing::{info, error};
use crate::error::Result;

/// Storage manager for Firecracker VM disk images
pub struct StorageManager {
    images_path: PathBuf,
    snapshots_path: PathBuf,
}

impl StorageManager {
    pub fn new() -> Self {
        Self {
            images_path: PathBuf::from("/opt/firecracker/images"),
            snapshots_path: PathBuf::from("/opt/firecracker/snapshots"),
        }
    }

    /// Create a new ext4 filesystem image
    pub async fn create_rootfs(&self, size_mb: u64, output_path: &Path) -> Result<()> {
        info!("Creating rootfs image: {:?} ({}MB)", output_path, size_mb);

        // Create sparse file
        Command::new("dd")
            .args(&[
                "if=/dev/zero",
                &format!("of={}", output_path.display()),
                "bs=1M",
                "count=0",
                &format!("seek={}", size_mb),
            ])
            .status()?;

        // Create ext4 filesystem
        Command::new("mkfs.ext4")
            .args(&["-F", output_path.to_str().unwrap()])
            .status()?;

        Ok(())
    }

    /// Mount rootfs image for modification
    pub async fn mount_rootfs(&self, image_path: &Path, mount_point: &Path) -> Result<()> {
        fs::create_dir_all(mount_point).await?;

        Command::new("mount")
            .args(&[
                "-o", "loop",
                image_path.to_str().unwrap(),
                mount_point.to_str().unwrap(),
            ])
            .status()?;

        Ok(())
    }

    /// Unmount rootfs image
    pub async fn unmount_rootfs(&self, mount_point: &Path) -> Result<()> {
        Command::new("umount")
            .args(&[mount_point.to_str().unwrap()])
            .status()?;

        Ok(())
    }

    /// Create a copy-on-write overlay for rootfs
    pub async fn create_overlay(&self, base_image: &Path, overlay_path: &Path) -> Result<PathBuf> {
        let overlay_dir = overlay_path.parent().unwrap();
        fs::create_dir_all(&overlay_dir).await?;

        // Create qcow2 overlay using base image as backing file
        let overlay_file = overlay_path.with_extension("qcow2");
        
        Command::new("qemu-img")
            .args(&[
                "create",
                "-f", "qcow2",
                "-b", base_image.to_str().unwrap(),
                "-F", "raw",
                overlay_file.to_str().unwrap(),
            ])
            .status()?;

        Ok(overlay_file)
    }

    /// Inject files into rootfs
    pub async fn inject_files(
        &self,
        rootfs_path: &Path,
        files: Vec<(PathBuf, Vec<u8>)>,
    ) -> Result<()> {
        let mount_point = PathBuf::from("/tmp/firecracker_mount");
        
        // Mount rootfs
        self.mount_rootfs(rootfs_path, &mount_point).await?;

        // Copy files
        for (path, content) in files {
            let full_path = mount_point.join(&path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::write(&full_path, content).await?;
        }

        // Unmount
        self.unmount_rootfs(&mount_point).await?;

        Ok(())
    }

    /// Create a snapshot of a running VM
    pub async fn create_snapshot(
        &self,
        vm_id: &str,
        snapshot_name: &str,
    ) -> Result<(PathBuf, PathBuf)> {
        let snapshot_dir = self.snapshots_path.join(vm_id).join(snapshot_name);
        fs::create_dir_all(&snapshot_dir).await?;

        let mem_file = snapshot_dir.join("memory.snap");
        let state_file = snapshot_dir.join("state.snap");

        // In a real implementation, this would use Firecracker's snapshot API
        // For now, return the paths where snapshots would be saved
        
        Ok((mem_file, state_file))
    }

    /// Restore VM from snapshot
    pub async fn restore_snapshot(
        &self,
        mem_file: &Path,
        state_file: &Path,
    ) -> Result<()> {
        // This would use Firecracker's restore API
        // For now, just verify files exist
        
        if !mem_file.exists() || !state_file.exists() {
            return Err(crate::error::SoulBoxError::not_found(
                "Snapshot files not found"
            ));
        }

        Ok(())
    }
}

impl Default for StorageManager {
    fn default() -> Self {
        Self::new()
    }
}