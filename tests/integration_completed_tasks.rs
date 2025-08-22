use soulbox::filesystem::FileSystemManager;
use soulbox::soulbox::v1::*;
use std::sync::Arc;

#[tokio::test]
async fn test_filesystem_manager_integration() {
    // Test that FileSystemManager can be created and used
    let fs_manager = FileSystemManager::new().await.expect("Should create filesystem manager");
    
    // Test creating a sandbox filesystem
    let sandbox_id = "test-sandbox-123";
    let sandbox_fs = fs_manager
        .create_sandbox_filesystem(sandbox_id)
        .await
        .expect("Should create sandbox filesystem");
    
    // Test basic filesystem operations
    let test_content = b"Hello, SoulBox!";
    let test_path = "test-file.txt";
    
    // Write a file
    sandbox_fs
        .write_file(test_path, test_content)
        .await
        .expect("Should write file");
    
    // Read the file back
    let read_content = sandbox_fs
        .read_file(test_path)
        .await
        .expect("Should read file");
    
    assert_eq!(read_content, test_content);
    
    // Get file metadata
    let metadata = sandbox_fs
        .get_file_metadata(test_path)
        .await
        .expect("Should get metadata");
    
    assert_eq!(metadata.size, test_content.len() as u64);
    assert!(!metadata.is_directory);
    
    // Create a directory
    let test_dir = "test-directory";
    sandbox_fs
        .create_directory(test_dir, false)
        .await
        .expect("Should create directory");
    
    // List directory contents
    let listing = sandbox_fs
        .list_directory("/")
        .await
        .expect("Should list directory");
    
    // Should contain our test file and directory
    assert!(listing.entries.len() >= 2);
    
    // Delete the file
    sandbox_fs
        .delete_file(test_path)
        .await
        .expect("Should delete file");
    
    // Verify file is deleted (should return error)
    assert!(sandbox_fs.read_file(test_path).await.is_err());
}

#[tokio::test]
async fn test_protobuf_types_are_usable() {
    // Test that protobuf types from Task 1 are properly generated and usable
    let create_request = CreateSandboxRequest {
        template_id: "nodejs".to_string(),
        config: Some(SandboxConfig {
            resource_limits: Some(ResourceLimits {
                memory_mb: 512,
                cpu_cores: 1.0,
                disk_mb: 1024,
                max_processes: 50,
            }),
            network_config: Some(NetworkConfig {
                exposed_ports: vec![3000, 8080],
                enable_port_forwarding: true,
                subnet: "10.0.0.0/24".to_string(),
            }),
            allowed_domains: vec!["example.com".to_string(), "api.github.com".to_string()],
            enable_internet: true,
        }),
        environment_variables: std::collections::HashMap::new(),
        timeout: Some(300),
    };
    
    // Verify fields are accessible and have expected values
    assert_eq!(create_request.template_id, "nodejs");
    assert!(create_request.config.is_some());
    
    let config = create_request.config.unwrap();
    assert!(config.resource_limits.is_some());
    assert!(config.network_config.is_some());
    assert!(config.enable_internet);
    assert_eq!(config.allowed_domains.len(), 2);
    
    let resource_limits = config.resource_limits.unwrap();
    assert_eq!(resource_limits.memory_mb, 512);
    assert_eq!(resource_limits.cpu_cores, 1.0);
    
    let network_config = config.network_config.unwrap();
    assert_eq!(network_config.exposed_ports, vec![3000, 8080]);
    assert!(network_config.enable_port_forwarding);
}

#[test]
fn test_file_descriptor_set_available() {
    // Test that Task 1.2 properly provides file descriptor set
    use soulbox::FILE_DESCRIPTOR_SET;
    
    // The descriptor set should not be empty after proper protobuf generation
    assert!(!FILE_DESCRIPTOR_SET.is_empty(), "File descriptor set should contain protobuf descriptors");
    
    // Basic sanity check that it looks like a protobuf descriptor
    assert!(FILE_DESCRIPTOR_SET.len() > 100, "File descriptor set should be substantial in size");
}