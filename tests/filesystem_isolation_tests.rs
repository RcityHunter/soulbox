use std::path::PathBuf;
use std::collections::HashMap;
use tempfile::TempDir;

// Import from our crate - these will initially fail to compile
use soulbox::container::{ContainerManager, SandboxContainer, ResourceLimits, NetworkConfig};
use soulbox::filesystem::{
    FileSystemManager, SandboxFileSystem, FilePermissions, DirectoryListing,
    FileWatcher, FileEvent, FileEventType
};
use soulbox::error::Result;

#[tokio::test]
async fn test_sandbox_filesystem_creation() {
    // Test that each sandbox gets its own isolated file system
    let fs_manager = FileSystemManager::new().await.unwrap();
    
    let sandbox1_fs = fs_manager.create_sandbox_filesystem("sandbox-1").await.unwrap();
    let sandbox2_fs = fs_manager.create_sandbox_filesystem("sandbox-2").await.unwrap();
    
    // Verify each sandbox has its own root directory
    assert_ne!(sandbox1_fs.get_root_path(), sandbox2_fs.get_root_path());
    
    // Verify basic directories exist
    assert!(sandbox1_fs.exists("/workspace").await.unwrap());
    assert!(sandbox1_fs.exists("/tmp").await.unwrap());
    assert!(sandbox1_fs.exists("/home").await.unwrap());
    
    // Verify directories are writable
    assert!(sandbox1_fs.is_writable("/workspace").await.unwrap());
    assert!(sandbox1_fs.is_writable("/tmp").await.unwrap());
}

#[tokio::test]
async fn test_file_operations_isolation() {
    // Test that file operations in one sandbox don't affect another
    let fs_manager = FileSystemManager::new().await.unwrap();
    
    let sandbox1_fs = fs_manager.create_sandbox_filesystem("fs-isolation-1").await.unwrap();
    let sandbox2_fs = fs_manager.create_sandbox_filesystem("fs-isolation-2").await.unwrap();
    
    // Write file in sandbox1
    let file_content = "This is sandbox 1 data";
    sandbox1_fs.write_file("/workspace/test.txt", file_content.as_bytes()).await.unwrap();
    
    // Verify file exists in sandbox1
    assert!(sandbox1_fs.exists("/workspace/test.txt").await.unwrap());
    let read_content = sandbox1_fs.read_file("/workspace/test.txt").await.unwrap();
    assert_eq!(String::from_utf8(read_content).unwrap(), file_content);
    
    // Verify file doesn't exist in sandbox2
    assert!(!sandbox2_fs.exists("/workspace/test.txt").await.unwrap());
    
    // Write different file in sandbox2
    let file2_content = "This is sandbox 2 data";
    sandbox2_fs.write_file("/workspace/test.txt", file2_content.as_bytes()).await.unwrap();
    
    // Verify both files exist independently
    let content1 = sandbox1_fs.read_file("/workspace/test.txt").await.unwrap();
    let content2 = sandbox2_fs.read_file("/workspace/test.txt").await.unwrap();
    
    assert_eq!(String::from_utf8(content1).unwrap(), file_content);
    assert_eq!(String::from_utf8(content2).unwrap(), file2_content);
}

#[tokio::test]
async fn test_file_permissions_enforcement() {
    // Test that file permissions are properly enforced
    let fs_manager = FileSystemManager::new().await.unwrap();
    let sandbox_fs = fs_manager.create_sandbox_filesystem("permissions-test").await.unwrap();
    
    // Create file with specific permissions
    let content = b"test file content";
    sandbox_fs.write_file("/workspace/test.txt", content).await.unwrap();
    
    // Set read-only permissions
    sandbox_fs.set_permissions("/workspace/test.txt", FilePermissions {
        readable: true,
        writable: false,
        executable: false,
        owner_id: 1000,
        group_id: 1000,
        mode: 0o444, // Read-only for all
    }).await.unwrap();
    
    // Verify file is readable
    let read_content = sandbox_fs.read_file("/workspace/test.txt").await.unwrap();
    assert_eq!(read_content, content);
    
    // Verify file is not writable
    let write_result = sandbox_fs.write_file("/workspace/test.txt", b"new content").await;
    assert!(write_result.is_err());
    
    // Test executable permissions
    sandbox_fs.write_file("/workspace/script.sh", b"#!/bin/bash\necho 'hello'").await.unwrap();
    sandbox_fs.set_permissions("/workspace/script.sh", FilePermissions {
        readable: true,
        writable: true,
        executable: true,
        owner_id: 1000,
        group_id: 1000,
        mode: 0o755,
    }).await.unwrap();
    
    let perms = sandbox_fs.get_permissions("/workspace/script.sh").await.unwrap();
    assert!(perms.executable);
}

#[tokio::test]
async fn test_directory_operations() {
    // Test directory creation, listing, and deletion
    let fs_manager = FileSystemManager::new().await.unwrap();
    let sandbox_fs = fs_manager.create_sandbox_filesystem("dir-test").await.unwrap();
    
    // Create nested directories
    sandbox_fs.create_directory("/workspace/projects/my-app/src", true).await.unwrap();
    
    // Verify directories exist
    assert!(sandbox_fs.exists("/workspace/projects").await.unwrap());
    assert!(sandbox_fs.exists("/workspace/projects/my-app").await.unwrap());
    assert!(sandbox_fs.exists("/workspace/projects/my-app/src").await.unwrap());
    
    // Create some files in directories
    sandbox_fs.write_file("/workspace/projects/my-app/package.json", br#"{"name": "my-app"}"#).await.unwrap();
    sandbox_fs.write_file("/workspace/projects/my-app/src/index.js", b"console.log('hello');").await.unwrap();
    sandbox_fs.write_file("/workspace/projects/my-app/src/utils.js", b"export const add = (a, b) => a + b;").await.unwrap();
    
    // List directory contents
    let listing = sandbox_fs.list_directory("/workspace/projects/my-app").await.unwrap();
    assert_eq!(listing.entries.len(), 2); // package.json + src/
    
    let src_listing = sandbox_fs.list_directory("/workspace/projects/my-app/src").await.unwrap();
    assert_eq!(src_listing.entries.len(), 2); // index.js + utils.js
    
    // Verify file metadata
    let package_json_entry = listing.entries.iter()
        .find(|e| e.name == "package.json")
        .unwrap();
    assert!(!package_json_entry.is_directory);
    assert!(package_json_entry.size > 0);
    
    let src_entry = listing.entries.iter()
        .find(|e| e.name == "src")
        .unwrap();
    assert!(src_entry.is_directory);
}

#[tokio::test]
async fn test_file_size_limits() {
    // Test that file size limits are enforced
    let fs_manager = FileSystemManager::new().await.unwrap();
    let sandbox_fs = fs_manager.create_sandbox_filesystem_with_limits(
        "size-limit-test",
        1024 * 1024, // 1MB total limit
        512 * 1024,   // 512KB per file limit
    ).await.unwrap();
    
    // Create file within limit
    let small_content = vec![b'A'; 256 * 1024]; // 256KB
    sandbox_fs.write_file("/workspace/small.txt", &small_content).await.unwrap();
    
    // Try to create file exceeding per-file limit
    let large_content = vec![b'B'; 1024 * 1024]; // 1MB
    let result = sandbox_fs.write_file("/workspace/large.txt", &large_content).await;
    assert!(result.is_err());
    
    // Try to exceed total filesystem limit
    let medium_content = vec![b'C'; 400 * 1024]; // 400KB
    sandbox_fs.write_file("/workspace/medium1.txt", &medium_content).await.unwrap();
    
    // This should fail because 256KB + 400KB + 400KB > 1MB limit
    let result = sandbox_fs.write_file("/workspace/medium2.txt", &medium_content).await;
    assert!(result.is_err());
    
    // Verify current usage
    let usage = sandbox_fs.get_disk_usage().await.unwrap();
    assert!(usage.used_bytes <= 1024 * 1024);
    assert!(usage.used_bytes >= 656 * 1024); // 256KB + 400KB
}

#[tokio::test]
async fn test_file_watching_and_events() {
    // Test file system event monitoring
    let fs_manager = FileSystemManager::new().await.unwrap();
    let sandbox_fs = fs_manager.create_sandbox_filesystem("watcher-test").await.unwrap();
    
    // Create file watcher
    let mut watcher = sandbox_fs.create_file_watcher("/workspace", true).await.unwrap();
    
    // Create file and check for create event
    sandbox_fs.write_file("/workspace/watched.txt", b"initial content").await.unwrap();
    
    let event = watcher.next_event().await.unwrap();
    assert_eq!(event.event_type, FileEventType::Created);
    assert_eq!(event.path, "/workspace/watched.txt");
    
    // Modify file and check for modify event
    sandbox_fs.write_file("/workspace/watched.txt", b"modified content").await.unwrap();
    
    let event = watcher.next_event().await.unwrap();
    assert_eq!(event.event_type, FileEventType::Modified);
    assert_eq!(event.path, "/workspace/watched.txt");
    
    // Delete file and check for delete event
    sandbox_fs.delete_file("/workspace/watched.txt").await.unwrap();
    
    let event = watcher.next_event().await.unwrap();
    assert_eq!(event.event_type, FileEventType::Deleted);
    assert_eq!(event.path, "/workspace/watched.txt");
}

#[tokio::test]
async fn test_symbolic_links_and_security() {
    // Test that symbolic links are handled securely and can't escape sandbox
    let fs_manager = FileSystemManager::new().await.unwrap();
    let sandbox_fs = fs_manager.create_sandbox_filesystem("symlink-test").await.unwrap();
    
    // Create legitimate symlink within sandbox
    sandbox_fs.write_file("/workspace/target.txt", b"target content").await.unwrap();
    sandbox_fs.create_symlink("/workspace/link.txt", "/workspace/target.txt").await.unwrap();
    
    // Verify symlink works
    let content = sandbox_fs.read_file("/workspace/link.txt").await.unwrap();
    assert_eq!(content, b"target content");
    
    // Try to create symlink that escapes sandbox (should be prevented)
    let result = sandbox_fs.create_symlink("/workspace/escape.txt", "/etc/passwd").await;
    assert!(result.is_err());
    
    // Try to create symlink with relative path that could escape
    let result = sandbox_fs.create_symlink("/workspace/relative_escape.txt", "../../../etc/passwd").await;
    assert!(result.is_err());
    
    // Verify existing symlinks are validated
    let metadata = sandbox_fs.get_file_metadata("/workspace/link.txt").await.unwrap();
    assert!(metadata.is_symlink);
    assert_eq!(metadata.symlink_target.unwrap(), "/workspace/target.txt");
}

#[tokio::test]
async fn test_filesystem_snapshots_and_restoration() {
    // Test filesystem snapshot and restore functionality
    let fs_manager = FileSystemManager::new().await.unwrap();
    let mut sandbox_fs = fs_manager.create_sandbox_filesystem("snapshot-test").await.unwrap();
    
    // Create initial state
    sandbox_fs.write_file("/workspace/file1.txt", b"original content 1").await.unwrap();
    sandbox_fs.write_file("/workspace/file2.txt", b"original content 2").await.unwrap();
    sandbox_fs.create_directory("/workspace/subdir", false).await.unwrap();
    sandbox_fs.write_file("/workspace/subdir/file3.txt", b"original content 3").await.unwrap();
    
    // Create snapshot
    let snapshot_id = sandbox_fs.create_snapshot("initial_state").await.unwrap();
    
    // Make changes
    sandbox_fs.write_file("/workspace/file1.txt", b"modified content 1").await.unwrap();
    sandbox_fs.delete_file("/workspace/file2.txt").await.unwrap();
    sandbox_fs.write_file("/workspace/new_file.txt", b"new content").await.unwrap();
    
    // Verify changes
    let content1 = sandbox_fs.read_file("/workspace/file1.txt").await.unwrap();
    assert_eq!(content1, b"modified content 1");
    assert!(!sandbox_fs.exists("/workspace/file2.txt").await.unwrap());
    assert!(sandbox_fs.exists("/workspace/new_file.txt").await.unwrap());
    
    // Restore from snapshot
    sandbox_fs.restore_from_snapshot(&snapshot_id).await.unwrap();
    
    // Verify restoration
    let content1_restored = sandbox_fs.read_file("/workspace/file1.txt").await.unwrap();
    assert_eq!(content1_restored, b"original content 1");
    assert!(sandbox_fs.exists("/workspace/file2.txt").await.unwrap());
    assert!(!sandbox_fs.exists("/workspace/new_file.txt").await.unwrap());
    
    let content2_restored = sandbox_fs.read_file("/workspace/file2.txt").await.unwrap();
    assert_eq!(content2_restored, b"original content 2");
}

#[tokio::test]
async fn test_concurrent_filesystem_access() {
    // Test that concurrent filesystem operations are safe
    let fs_manager = FileSystemManager::new().await.unwrap();
    let sandbox_fs = fs_manager.create_sandbox_filesystem("concurrent-test").await.unwrap();
    
    let file_count = 50;
    let mut tasks = Vec::new();
    
    // Create many files concurrently
    for i in 0..file_count {
        let fs_clone = sandbox_fs.clone();
        let task = tokio::spawn(async move {
            let filename = format!("/workspace/concurrent_file_{}.txt", i);
            let content = format!("Content for file {}", i);
            
            fs_clone.write_file(&filename, content.as_bytes()).await.unwrap();
            
            // Verify file was written correctly
            let read_content = fs_clone.read_file(&filename).await.unwrap();
            assert_eq!(String::from_utf8(read_content).unwrap(), content);
            
            filename
        });
        tasks.push(task);
    }
    
    // Wait for all tasks to complete
    let results = futures::future::try_join_all(tasks).await.unwrap();
    assert_eq!(results.len(), file_count);
    
    // Verify all files exist and have correct content
    let listing = sandbox_fs.list_directory("/workspace").await.unwrap();
    assert_eq!(listing.entries.len(), file_count);
    
    for i in 0..file_count {
        let filename = format!("/workspace/concurrent_file_{}.txt", i);
        let expected_content = format!("Content for file {}", i);
        
        let content = sandbox_fs.read_file(&filename).await.unwrap();
        assert_eq!(String::from_utf8(content).unwrap(), expected_content);
    }
}

#[tokio::test]
async fn test_filesystem_cleanup_on_sandbox_deletion() {
    // Test that filesystem resources are properly cleaned up
    let fs_manager = FileSystemManager::new().await.unwrap();
    
    let sandbox_root_path = {
        let sandbox_fs = fs_manager.create_sandbox_filesystem("cleanup-test").await.unwrap();
        
        // Create some files
        sandbox_fs.write_file("/workspace/test1.txt", b"content1").await.unwrap();
        sandbox_fs.write_file("/workspace/test2.txt", b"content2").await.unwrap();
        sandbox_fs.create_directory("/workspace/subdir", false).await.unwrap();
        
        sandbox_fs.get_root_path().clone()
        
        // sandbox_fs goes out of scope here
    };
    
    // Give some time for cleanup
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Verify the sandbox directory has been cleaned up
    assert!(!std::path::Path::new(&sandbox_root_path).exists());
}

// Test helper types and functions
#[derive(Debug, Clone)]
pub struct MockFileMetadata {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
    pub permissions: FilePermissions,
    pub created_at: std::time::SystemTime,
    pub modified_at: std::time::SystemTime,
}

#[derive(Debug, Clone)]
pub struct MockDiskUsage {
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub file_count: u64,
    pub directory_count: u64,
}

pub async fn setup_test_filesystem() -> TempDir {
    // Helper function to set up temporary filesystem for testing
    tempfile::tempdir().unwrap()
}