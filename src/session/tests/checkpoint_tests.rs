//! Tests for session checkpoint and recovery functionality

#[cfg(test)]
mod checkpoint_tests {
    use super::super::{Session, SessionManager, InMemorySessionManager};
    use super::super::recovery::{SessionRecoveryService, CheckpointManager, SessionCheckpoint};
    use crate::container::manager::ContainerManager;
    use crate::config::Config;
    use uuid::Uuid;
    use chrono::{Utc, Duration};
    use tempfile::TempDir;
    use std::collections::HashMap;

    async fn create_test_recovery_service() -> (TempDir, SessionRecoveryService) {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Box::new(InMemorySessionManager::new());
        let container_manager = Box::new(
            ContainerManager::new(Config::default()).await.unwrap()
        );
        let recovery_service = SessionRecoveryService::new(session_manager, container_manager);
        (temp_dir, recovery_service)
    }

    fn create_test_checkpoint_manager() -> (TempDir, CheckpointManager) {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_manager = CheckpointManager::new(
            temp_dir.path().to_string_lossy().to_string()
        );
        (temp_dir, checkpoint_manager)
    }

    #[tokio::test]
    async fn test_session_health_assessment() {
        let (_temp_dir, recovery_service) = create_test_recovery_service().await;
        
        // Test healthy session
        let session = Session::new("user123".to_string());
        let context = recovery_service.assess_session_health(&session).await.unwrap();
        
        assert_eq!(context.session.id, session.id);
        assert!(!context.container_exists);
        assert!(!context.container_running);
        assert!(context.recovery_actions.len() > 0);
    }

    #[tokio::test]
    async fn test_expired_session_assessment() {
        let (_temp_dir, recovery_service) = create_test_recovery_service().await;
        
        // Create expired session
        let mut session = Session::new("user123".to_string());
        session.expires_at = Utc::now() - Duration::hours(1);
        
        let context = recovery_service.assess_session_health(&session).await.unwrap();
        
        assert!(context.recovery_actions.iter().any(|action| 
            matches!(action, super::super::recovery::RecoveryAction::MarkAsFailed)
        ));
    }

    #[tokio::test]
    async fn test_checkpoint_creation() {
        let (_temp_dir, checkpoint_manager) = create_test_checkpoint_manager();
        
        let session_id = Uuid::new_v4();
        let checkpoint = checkpoint_manager.create_checkpoint(session_id).await.unwrap();
        
        assert_eq!(checkpoint.session_id, session_id);
        assert!(checkpoint.filesystem_snapshot.is_some());
        assert!(checkpoint.memory_dump.is_some());
        assert!(checkpoint.process_state.len() > 0);
        
        // Verify checkpoint was saved to disk
        let checkpoint_files = checkpoint_manager.list_checkpoints(session_id).await.unwrap();
        assert_eq!(checkpoint_files.len(), 1);
        assert_eq!(checkpoint_files[0].session_id, session_id);
    }

    #[tokio::test]
    async fn test_checkpoint_restoration() {
        let (_temp_dir, checkpoint_manager) = create_test_checkpoint_manager();
        
        let session_id = Uuid::new_v4();
        
        // Create checkpoint
        let checkpoint = checkpoint_manager.create_checkpoint(session_id).await.unwrap();
        
        // Restore checkpoint
        let result = checkpoint_manager.restore_checkpoint(&checkpoint).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_checkpoints() {
        let (_temp_dir, checkpoint_manager) = create_test_checkpoint_manager();
        
        let session_id = Uuid::new_v4();
        
        // Create multiple checkpoints
        let checkpoint1 = checkpoint_manager.create_checkpoint(session_id).await.unwrap();
        
        // Wait a bit to ensure different timestamps
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        let checkpoint2 = checkpoint_manager.create_checkpoint(session_id).await.unwrap();
        
        // List checkpoints
        let checkpoints = checkpoint_manager.list_checkpoints(session_id).await.unwrap();
        assert_eq!(checkpoints.len(), 2);
        
        // Verify they're sorted by timestamp (newest first)
        assert!(checkpoints[0].timestamp >= checkpoints[1].timestamp);
    }

    #[tokio::test]
    async fn test_checkpoint_load() {
        let (_temp_dir, checkpoint_manager) = create_test_checkpoint_manager();
        
        let session_id = Uuid::new_v4();
        
        // Create checkpoint
        let original_checkpoint = checkpoint_manager.create_checkpoint(session_id).await.unwrap();
        
        // Load checkpoint
        let loaded_checkpoint = checkpoint_manager.load_checkpoint(
            session_id, 
            original_checkpoint.timestamp.timestamp()
        ).await.unwrap();
        
        assert_eq!(loaded_checkpoint.session_id, original_checkpoint.session_id);
        assert_eq!(loaded_checkpoint.timestamp, original_checkpoint.timestamp);
        assert_eq!(loaded_checkpoint.filesystem_snapshot, original_checkpoint.filesystem_snapshot);
    }

    #[tokio::test]
    async fn test_checkpoint_with_custom_process_state() {
        let (_temp_dir, checkpoint_manager) = create_test_checkpoint_manager();
        
        let session_id = Uuid::new_v4();
        
        // Set some environment variables before checkpointing
        std::env::set_var("SOULBOX_TEST_VAR", "test_value");
        std::env::set_var("SOULBOX_SESSION_ID", session_id.to_string());
        
        let checkpoint = checkpoint_manager.create_checkpoint(session_id).await.unwrap();
        
        // Verify environment variables were captured
        assert!(checkpoint.process_state.contains_key("env_SOULBOX_TEST_VAR"));
        assert!(checkpoint.process_state.contains_key("env_SOULBOX_SESSION_ID"));
        
        let captured_session_id = checkpoint.process_state.get("env_SOULBOX_SESSION_ID").unwrap();
        assert_eq!(captured_session_id, &session_id.to_string());
        
        // Clean up
        std::env::remove_var("SOULBOX_TEST_VAR");
        std::env::remove_var("SOULBOX_SESSION_ID");
    }

    #[tokio::test]
    async fn test_checkpoint_memory_state() {
        let (_temp_dir, checkpoint_manager) = create_test_checkpoint_manager();
        
        let session_id = Uuid::new_v4();
        let checkpoint = checkpoint_manager.create_checkpoint(session_id).await.unwrap();
        
        // Verify memory dump contains expected information
        assert!(checkpoint.memory_dump.is_some());
        
        let memory_data = checkpoint.memory_dump.unwrap();
        let memory_str = String::from_utf8_lossy(&memory_data);
        
        assert!(memory_str.contains(&session_id.to_string()));
        assert!(memory_str.contains("timestamp"));
        assert!(memory_str.contains("captured_at"));
    }

    #[tokio::test]
    async fn test_session_recovery_no_action_needed() {
        let (_temp_dir, recovery_service) = create_test_recovery_service().await;
        
        // Create a healthy, active session
        let session = Session::new("user123".to_string());
        let context = recovery_service.assess_session_health(&session).await.unwrap();
        
        // Healthy session should have NoAction as the primary action
        let result = recovery_service.recover_session(context).await.unwrap();
        assert!(result.success);
        assert_eq!(result.action_taken, super::super::recovery::RecoveryAction::NoAction);
    }

    #[tokio::test]
    async fn test_last_checkpoint_time_detection() {
        let (_temp_dir, recovery_service) = create_test_recovery_service().await;
        let (_checkpoint_temp_dir, checkpoint_manager) = create_test_checkpoint_manager();
        
        let session_id = Uuid::new_v4();
        
        // Initially no checkpoints
        let last_checkpoint = recovery_service.get_last_checkpoint_time(&session_id).await.unwrap();
        assert!(last_checkpoint.is_none());
        
        // Create a checkpoint
        let _checkpoint = checkpoint_manager.create_checkpoint(session_id).await.unwrap();
        
        // Note: In a real test, we'd need to coordinate the storage paths
        // For now, this test verifies the method doesn't crash
    }

    #[tokio::test]
    async fn test_checkpoint_serialization() {
        let session_id = Uuid::new_v4();
        let timestamp = Utc::now();
        let mut process_state = HashMap::new();
        process_state.insert("test_key".to_string(), "test_value".to_string());
        
        let checkpoint = SessionCheckpoint {
            session_id,
            timestamp,
            filesystem_snapshot: Some("snapshot_123".to_string()),
            memory_dump: Some(b"test_memory_data".to_vec()),
            process_state,
        };
        
        // Test serialization
        let json = serde_json::to_string(&checkpoint).unwrap();
        assert!(json.contains(&session_id.to_string()));
        assert!(json.contains("snapshot_123"));
        assert!(json.contains("test_key"));
        
        // Test deserialization
        let deserialized: SessionCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.session_id, checkpoint.session_id);
        assert_eq!(deserialized.timestamp, checkpoint.timestamp);
        assert_eq!(deserialized.filesystem_snapshot, checkpoint.filesystem_snapshot);
        assert_eq!(deserialized.memory_dump, checkpoint.memory_dump);
        assert_eq!(deserialized.process_state, checkpoint.process_state);
    }
}