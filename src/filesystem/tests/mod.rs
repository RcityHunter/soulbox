//! Tests for filesystem advanced features

#[cfg(test)]
mod advanced_features_tests {
    use super::super::sandbox_fs::SandboxFileSystem;
    use crate::filesystem::FilePermissions;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use tokio::fs;

    async fn create_test_sandbox() -> (TempDir, SandboxFileSystem) {
        let temp_dir = TempDir::new().unwrap();
        let sandbox = SandboxFileSystem::new(
            temp_dir.path().to_path_buf(),
            Some(1024 * 1024), // 1MB limit
            Some(1024), // 1KB file limit
        ).await.unwrap();
        (temp_dir, sandbox)
    }

    #[tokio::test]
    async fn test_read_symlink_target() {
        let (_temp_dir, sandbox) = create_test_sandbox().await;

        // Create a test file and symlink
        sandbox.write_file("test_file.txt", b"Hello, World!").await.unwrap();
        sandbox.create_symlink("test_link.txt", "test_file.txt").await.unwrap();

        // List directory and check symlink target is read
        let listing = sandbox.list_directory("").await.unwrap();
        
        let symlink_entry = listing.entries.iter()
            .find(|entry| entry.name == "test_link.txt")
            .expect("Symlink should be found");

        assert!(symlink_entry.is_symlink);
        assert!(symlink_entry.symlink_target.is_some());
        
        let target = symlink_entry.symlink_target.as_ref().unwrap();
        assert!(target.contains("test_file.txt"));
    }

    #[tokio::test]
    async fn test_set_permissions() {
        let (_temp_dir, sandbox) = create_test_sandbox().await;

        // Create a test file
        sandbox.write_file("test_file.txt", b"Hello, World!").await.unwrap();

        // Get current permissions
        let original_perms = sandbox.get_permissions("test_file.txt").await.unwrap();

        // Create new permissions
        let new_perms = FilePermissions {
            readable: true,
            writable: false,
            executable: false,
            owner_id: original_perms.owner_id,
            group_id: original_perms.group_id,
            mode: 0o444, // Read-only
        };

        // Set permissions
        sandbox.set_permissions("test_file.txt", new_perms.clone()).await.unwrap();

        // Verify permissions were set
        let updated_perms = sandbox.get_permissions("test_file.txt").await.unwrap();
        
        #[cfg(unix)]
        {
            assert_eq!(updated_perms.mode & 0o777, 0o444);
        }
        
        #[cfg(windows)]
        {
            // On Windows, we can only check the readonly flag
            assert!(!updated_perms.writable);
        }
    }

    #[tokio::test]
    async fn test_filesystem_snapshot_and_restore() {
        let (_temp_dir, mut sandbox) = create_test_sandbox().await;

        // Create initial file structure
        sandbox.write_file("file1.txt", b"Content 1").await.unwrap();
        sandbox.write_file("file2.txt", b"Content 2").await.unwrap();
        sandbox.create_directory("subdir", false).await.unwrap();
        sandbox.write_file("subdir/file3.txt", b"Content 3").await.unwrap();

        // Create snapshot
        let snapshot_id = sandbox.create_snapshot("test_snapshot").await.unwrap();
        assert!(snapshot_id.contains("test_snapshot"));

        // Modify files after snapshot
        sandbox.write_file("file1.txt", b"Modified Content 1").await.unwrap();
        sandbox.delete_file("file2.txt").await.unwrap();
        sandbox.write_file("new_file.txt", b"New Content").await.unwrap();

        // Verify changes
        let content = sandbox.read_file("file1.txt").await.unwrap();
        assert_eq!(content, b"Modified Content 1");
        
        assert!(!sandbox.exists("file2.txt").await.unwrap());
        assert!(sandbox.exists("new_file.txt").await.unwrap());

        // Restore from snapshot
        sandbox.restore_from_snapshot(&snapshot_id).await.unwrap();

        // Verify restoration
        let content = sandbox.read_file("file1.txt").await.unwrap();
        assert_eq!(content, b"Content 1");
        
        assert!(sandbox.exists("file2.txt").await.unwrap());
        assert!(!sandbox.exists("new_file.txt").await.unwrap());
        
        let subdir_content = sandbox.read_file("subdir/file3.txt").await.unwrap();
        assert_eq!(subdir_content, b"Content 3");
    }

    #[tokio::test]
    async fn test_snapshot_with_symlinks() {
        let (_temp_dir, mut sandbox) = create_test_sandbox().await;

        // Create file and symlink
        sandbox.write_file("original.txt", b"Original content").await.unwrap();
        sandbox.create_symlink("link.txt", "original.txt").await.unwrap();

        // Create snapshot
        let snapshot_id = sandbox.create_snapshot("symlink_test").await.unwrap();

        // Modify original file
        sandbox.write_file("original.txt", b"Modified content").await.unwrap();

        // Restore and verify symlink still works
        sandbox.restore_from_snapshot(&snapshot_id).await.unwrap();
        
        let content = sandbox.read_file("original.txt").await.unwrap();
        assert_eq!(content, b"Original content");
        
        // Verify symlink exists and points to the right target
        let metadata = sandbox.get_file_metadata("link.txt").await.unwrap();
        assert!(metadata.is_symlink);
        assert!(metadata.symlink_target.is_some());
    }

    #[tokio::test]
    async fn test_multiple_snapshots() {
        let (_temp_dir, mut sandbox) = create_test_sandbox().await;

        // Initial state
        sandbox.write_file("file.txt", b"Version 1").await.unwrap();
        let snapshot1 = sandbox.create_snapshot("v1").await.unwrap();

        // Second state
        sandbox.write_file("file.txt", b"Version 2").await.unwrap();
        let snapshot2 = sandbox.create_snapshot("v2").await.unwrap();

        // Third state
        sandbox.write_file("file.txt", b"Version 3").await.unwrap();

        // Restore to first snapshot
        sandbox.restore_from_snapshot(&snapshot1).await.unwrap();
        let content = sandbox.read_file("file.txt").await.unwrap();
        assert_eq!(content, b"Version 1");

        // Restore to second snapshot
        sandbox.restore_from_snapshot(&snapshot2).await.unwrap();
        let content = sandbox.read_file("file.txt").await.unwrap();
        assert_eq!(content, b"Version 2");
    }

    #[tokio::test]
    async fn test_snapshot_nonexistent_restore() {
        let (_temp_dir, sandbox) = create_test_sandbox().await;

        // Try to restore from non-existent snapshot
        let result = sandbox.restore_from_snapshot("nonexistent").await;
        assert!(result.is_err());
        
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Snapshot not found"));
    }
}