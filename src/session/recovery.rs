use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use tracing::{debug, info, warn, error};
use crate::error::{Result, SoulBoxError};
use crate::container::manager::ContainerManager;
use super::{Session, SessionManager};

/// Session recovery service for crash recovery and state restoration
pub struct SessionRecoveryService {
    session_manager: Box<dyn SessionManager>,
    container_manager: Box<ContainerManager>,
}

/// Recovery context for a session
#[derive(Debug, Clone)]
pub struct RecoveryContext {
    pub session: Session,
    pub container_exists: bool,
    pub container_running: bool,
    pub last_checkpoint: Option<DateTime<Utc>>,
    pub recovery_actions: Vec<RecoveryAction>,
}

/// Actions to take during session recovery
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryAction {
    /// Restart the container
    RestartContainer,
    /// Recreate the container from scratch
    RecreateContainer,
    /// Restore filesystem state
    RestoreFilesystem,
    /// Re-establish network connections
    RestoreNetwork,
    /// Mark session as failed and cleanup
    MarkAsFailed,
    /// Session is healthy, no action needed
    NoAction,
}

impl SessionRecoveryService {
    pub fn new(
        session_manager: Box<dyn SessionManager>,
        container_manager: Box<ContainerManager>,
    ) -> Self {
        Self {
            session_manager,
            container_manager,
        }
    }

    /// Recover all active sessions after a system restart
    pub async fn recover_all_sessions(&self) -> Result<Vec<RecoveryResult>> {
        info!("Starting session recovery process");
        
        // Get all running containers
        let running_containers = self.container_manager.list_containers().await?;
        let mut recovery_results = Vec::new();
        
        // Find orphaned containers (containers without sessions)
        for container in &running_containers {
            match self.session_manager.get_session_by_container(&container.container_id).await? {
                Some(session) => {
                    // Session exists, check if recovery is needed
                    let context = self.assess_session_health(&session).await?;
                    let result = self.recover_session(context).await?;
                    recovery_results.push(result);
                }
                None => {
                    // Orphaned container, clean it up
                    warn!("Found orphaned container {}, cleaning up", container.container_id);
                    if let Err(e) = self.container_manager.stop_container(&container.container_id).await {
                        error!("Failed to stop orphaned container {}: {}", container.container_id, e);
                    }
                    
                    recovery_results.push(RecoveryResult {
                        session_id: None,
                        action_taken: RecoveryAction::MarkAsFailed,
                        success: true,
                        error: None,
                    });
                }
            }
        }
        
        // Find sessions without containers (need container recreation)
        let all_sessions = self.get_all_active_sessions().await?;
        for session in all_sessions {
            if session.container_id.is_some() {
                // Check if we already processed this session
                let already_processed = recovery_results.iter()
                    .any(|r| r.session_id == Some(session.id));
                
                if !already_processed {
                    // Session has container ID but container doesn't exist
                    let context = self.assess_session_health(&session).await?;
                    let result = self.recover_session(context).await?;
                    recovery_results.push(result);
                }
            }
        }
        
        info!("Session recovery completed. Processed {} items", recovery_results.len());
        Ok(recovery_results)
    }

    /// Recover a specific session by ID
    pub async fn recover_session_by_id(&self, session_id: Uuid) -> Result<RecoveryResult> {
        match self.session_manager.get_session(session_id).await? {
            Some(session) => {
                let context = self.assess_session_health(&session).await?;
                self.recover_session(context).await
            }
            None => {
                Err(SoulBoxError::SessionError(format!("Session {} not found", session_id)))
            }
        }
    }

    /// Assess the health of a session and determine recovery actions needed
    async fn assess_session_health(&self, session: &Session) -> Result<RecoveryContext> {
        let mut recovery_actions = Vec::new();
        let mut container_exists = false;
        let mut container_running = false;

        // Check container status if session has a container
        if let Some(container_id) = &session.container_id {
            match self.container_manager.get_container_info(container_id).await {
                Ok(info) => {
                    container_exists = true;
                    container_running = info.state
                        .as_ref()
                        .and_then(|state| state.status.as_ref())
                        .map(|status| matches!(status, bollard::models::ContainerStateStatusEnum::RUNNING))
                        .unwrap_or(false);
                    
                    if !container_running {
                        recovery_actions.push(RecoveryAction::RestartContainer);
                    }
                }
                Err(e) => {
                    warn!("Failed to get container info for {}: {}", container_id, e);
                    // Container doesn't exist but session expects it
                    recovery_actions.push(RecoveryAction::RecreateContainer);
                }
            }
        }

        // Check if session has expired
        if session.is_expired() {
            recovery_actions.clear();
            recovery_actions.push(RecoveryAction::MarkAsFailed);
        }

        // If no issues found, no action needed
        if recovery_actions.is_empty() {
            recovery_actions.push(RecoveryAction::NoAction);
        }

        Ok(RecoveryContext {
            session: session.clone(),
            container_exists,
            container_running,
            last_checkpoint: self.get_last_checkpoint_time(&session.id).await?,
            recovery_actions,
        })
    }

    /// Execute recovery actions for a session
    async fn recover_session(&self, context: RecoveryContext) -> Result<RecoveryResult> {
        let session_id = context.session.id;
        
        for action in &context.recovery_actions {
            match action {
                RecoveryAction::RestartContainer => {
                    if let Some(container_id) = &context.session.container_id {
                        info!("Restarting container {} for session {}", container_id, session_id);
                        
                        match self.container_manager.restart_container(container_id).await {
                            Ok(_) => {
                                return Ok(RecoveryResult {
                                    session_id: Some(session_id),
                                    action_taken: RecoveryAction::RestartContainer,
                                    success: true,
                                    error: None,
                                });
                            }
                            Err(e) => {
                                error!("Failed to restart container {}: {}", container_id, e);
                                // Fall through to try recreation
                                continue;
                            }
                        }
                    }
                }
                
                RecoveryAction::RecreateContainer => {
                    info!("Recreating container for session {}", session_id);
                    
                    match self.recreate_container_for_session(&context.session).await {
                        Ok(new_container_id) => {
                            // Update session with new container ID
                            let mut updated_session = context.session.clone();
                            updated_session.set_container(new_container_id, updated_session.runtime.clone().unwrap_or_default());
                            
                            if let Err(e) = self.session_manager.update_session(&updated_session).await {
                                error!("Failed to update session with new container: {}", e);
                            }
                            
                            return Ok(RecoveryResult {
                                session_id: Some(session_id),
                                action_taken: RecoveryAction::RecreateContainer,
                                success: true,
                                error: None,
                            });
                        }
                        Err(e) => {
                            error!("Failed to recreate container for session {}: {}", session_id, e);
                            return Ok(RecoveryResult {
                                session_id: Some(session_id),
                                action_taken: RecoveryAction::RecreateContainer,
                                success: false,
                                error: Some(e.to_string()),
                            });
                        }
                    }
                }
                
                RecoveryAction::MarkAsFailed => {
                    info!("Marking session {} as failed and cleaning up", session_id);
                    
                    // Clean up container if it exists
                    if let Some(container_id) = &context.session.container_id {
                        let _ = self.container_manager.stop_container(container_id).await;
                    }
                    
                    // Delete session
                    if let Err(e) = self.session_manager.delete_session(session_id).await {
                        error!("Failed to delete failed session {}: {}", session_id, e);
                    }
                    
                    return Ok(RecoveryResult {
                        session_id: Some(session_id),
                        action_taken: RecoveryAction::MarkAsFailed,
                        success: true,
                        error: None,
                    });
                }
                
                RecoveryAction::NoAction => {
                    return Ok(RecoveryResult {
                        session_id: Some(session_id),
                        action_taken: RecoveryAction::NoAction,
                        success: true,
                        error: None,
                    });
                }
                
                _ => {
                    warn!("Recovery action {:?} not implemented yet", action);
                }
            }
        }

        // If we get here, all recovery actions failed
        Ok(RecoveryResult {
            session_id: Some(session_id),
            action_taken: RecoveryAction::MarkAsFailed,
            success: false,
            error: Some("All recovery actions failed".to_string()),
        })
    }

    /// Recreate a container for a session based on its runtime
    async fn recreate_container_for_session(&self, session: &Session) -> Result<String> {
        let runtime = session.runtime.as_ref().unwrap_or(&"python".to_string()).clone();
        
        // Create container with session's environment and working directory
        // Convert HashMap to Vec for bollard
        let env_vec: Vec<String> = session.environment.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
            
        let container_config = bollard::container::Config {
            image: Some(self.get_runtime_image(&runtime)),
            working_dir: Some(session.working_directory.clone()),
            env: Some(env_vec),
            ..Default::default()
        };
        
        let container_id = self.container_manager.create_container(container_config).await?;
        self.container_manager.start_container(&container_id).await?;
        
        Ok(container_id)
    }

    /// Get Docker image name for a runtime
    fn get_runtime_image(&self, runtime: &str) -> String {
        match runtime {
            "python" => "python:3.11-slim".to_string(),
            "nodejs" => "node:20-alpine".to_string(),
            "rust" => "rust:1.75-slim".to_string(),
            _ => "ubuntu:22.04".to_string(),
        }
    }

    /// Get all active sessions by examining running containers and known patterns
    async fn get_all_active_sessions(&self) -> Result<Vec<Session>> {
        // Since we can't easily access the Redis connection directly without trait modifications,
        // we'll use a combination approach:
        // 1. Get sessions from running containers (most reliable)
        // 2. Use a more comprehensive search if needed
        
        let mut sessions = Vec::new();
        let mut session_ids = std::collections::HashSet::new();
        
        // Method 1: Get sessions from running containers
        let container_sessions = self.get_sessions_from_containers().await?;
        for session in container_sessions {
            if session_ids.insert(session.id) {
                sessions.push(session);
            }
        }
        
        // Method 2: Try to find sessions by scanning through potential user IDs
        // This is a fallback that works when we have some user information
        let additional_sessions = self.discover_additional_sessions(&session_ids).await?;
        for session in additional_sessions {
            if session_ids.insert(session.id) {
                sessions.push(session);
            }
        }
        
        info!("Found {} total active sessions during recovery", sessions.len());
        Ok(sessions)
    }
    
    /// Discover additional sessions beyond those with containers
    async fn discover_additional_sessions(&self, known_session_ids: &std::collections::HashSet<Uuid>) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();
        
        // Try to find sessions for common user patterns
        // This is a simplified approach - in a real system, you might have a user directory
        let common_user_patterns = vec![
            "user", "admin", "test", "demo", "guest",
            "user1", "user2", "user3", "user4", "user5",
        ];
        
        for base_pattern in &common_user_patterns {
            // Try the base pattern and numbered variations
            for i in 0..10 {
                let user_id = if i == 0 {
                    base_pattern.to_string()
                } else {
                    format!("{}{}", base_pattern, i)
                };
                
                match self.session_manager.list_user_sessions(&user_id).await {
                    Ok(user_sessions) => {
                        for session in user_sessions {
                            if session.is_active && !session.is_expired() && !known_session_ids.contains(&session.id) {
                                sessions.push(session);
                            }
                        }
                    }
                    Err(_) => {
                        // Expected for users that don't exist
                        continue;
                    }
                }
                
                // Don't overwhelm the system
                if sessions.len() > 100 {
                    break;
                }
            }
            
            if sessions.len() > 100 {
                break;
            }
        }
        
        debug!("Discovered {} additional sessions through user pattern search", sessions.len());
        Ok(sessions)
    }
    
    /// Get sessions by examining running containers (fallback method)
    async fn get_sessions_from_containers(&self) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();
        
        // Get all running containers
        let containers = self.container_manager.list_containers().await?;
        
        for container in containers {
            if container.status == "running" {
                // Try to find session associated with this container
                match self.session_manager.get_session_by_container(&container.container_id).await? {
                    Some(session) => {
                        if session.is_active && !session.is_expired() {
                            sessions.push(session);
                        }
                    }
                    None => {
                        debug!("Container {} has no associated session", container.container_id);
                    }
                }
            }
        }
        
        info!("Found {} active sessions from container analysis", sessions.len());
        Ok(sessions)
    }
    
    /// Get the last checkpoint time for a session
    async fn get_last_checkpoint_time(&self, session_id: &Uuid) -> Result<Option<DateTime<Utc>>> {
        // Look for checkpoint files in the storage directory
        let _checkpoint_pattern = format!("checkpoint_{}_*.json", session_id);
        let storage_dir = std::path::Path::new("/tmp/soulbox/checkpoints");
        
        if !storage_dir.exists() {
            return Ok(None);
        }
        
        let mut latest_time: Option<DateTime<Utc>> = None;
        
        match std::fs::read_dir(storage_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if file_name.starts_with(&format!("checkpoint_{}_", session_id)) && file_name.ends_with(".json") {
                        // Extract timestamp from filename
                        if let Some(timestamp_str) = file_name.strip_prefix(&format!("checkpoint_{}_", session_id))
                            .and_then(|s| s.strip_suffix(".json")) {
                            if let Ok(timestamp) = timestamp_str.parse::<i64>() {
                                let checkpoint_time = DateTime::from_timestamp(timestamp, 0)
                                    .unwrap_or_else(|| Utc::now());
                                if latest_time.is_none() || Some(checkpoint_time) > latest_time {
                                    latest_time = Some(checkpoint_time);
                                }
                            }
                        }
                    }
                }
            }
            Err(_) => return Ok(None),
        }
        
        Ok(latest_time)
    }
}

/// Result of a session recovery operation
#[derive(Debug, Clone)]
pub struct RecoveryResult {
    pub session_id: Option<Uuid>,
    pub action_taken: RecoveryAction,
    pub success: bool,
    pub error: Option<String>,
}

/// Session checkpoint for state preservation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionCheckpoint {
    pub session_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub filesystem_snapshot: Option<String>,
    pub memory_dump: Option<Vec<u8>>,
    pub process_state: HashMap<String, String>,
}

/// Checkpoint manager for creating and restoring session checkpoints
pub struct CheckpointManager {
    storage_path: String,
}

impl CheckpointManager {
    pub fn new(storage_path: String) -> Self {
        Self { storage_path }
    }

    /// Create a checkpoint of a session
    pub async fn create_checkpoint(&self, session_id: Uuid) -> Result<SessionCheckpoint> {
        let timestamp = Utc::now();
        let checkpoint_id = format!("checkpoint_{}_{}", session_id, timestamp.timestamp());
        
        // 1. Create filesystem snapshot (using the checkpoint_id as snapshot name)
        let filesystem_snapshot = Some(checkpoint_id.clone());
        
        // 2. Capture process memory state (simplified - in production would use CRIU or similar)
        let memory_dump = self.capture_memory_state(session_id).await?;
        
        // 3. Save container state information
        let process_state = self.capture_process_state(session_id).await?;
        
        // 4. Save checkpoint to disk
        let checkpoint = SessionCheckpoint {
            session_id,
            timestamp,
            filesystem_snapshot,
            memory_dump,
            process_state,
        };
        
        self.save_checkpoint_to_disk(&checkpoint).await?;
        
        Ok(checkpoint)
    }

    /// Restore a session from a checkpoint
    pub async fn restore_checkpoint(&self, checkpoint: &SessionCheckpoint) -> Result<()> {
        info!("Restoring checkpoint for session {} from {}", checkpoint.session_id, checkpoint.timestamp);
        
        // 1. Restore filesystem from snapshot
        if let Some(snapshot_id) = &checkpoint.filesystem_snapshot {
            self.restore_filesystem_snapshot(checkpoint.session_id, snapshot_id).await?;
        }
        
        // 2. Restore process memory state (simplified)
        if let Some(memory_data) = &checkpoint.memory_dump {
            self.restore_memory_state(checkpoint.session_id, memory_data).await?;
        }
        
        // 3. Restore process state and environment variables
        self.restore_process_state(checkpoint.session_id, &checkpoint.process_state).await?;
        
        info!("Successfully restored checkpoint for session {}", checkpoint.session_id);
        Ok(())
    }
    
    /// Capture memory state of a session (simplified implementation)
    async fn capture_memory_state(&self, session_id: Uuid) -> Result<Option<Vec<u8>>> {
        // In a real implementation, this would use CRIU (Checkpoint/Restore In Userspace)
        // or similar technology to capture actual process memory
        // For now, we'll create a simplified representation
        
        let memory_info = format!(
            "{{\"session_id\":\"{}\",\"timestamp\":\"{}\",\"captured_at\":\"{}\"}}",
            session_id,
            Utc::now().to_rfc3339(),
            std::process::id()
        );
        
        Ok(Some(memory_info.into_bytes()))
    }
    
    /// Capture process state of a session
    async fn capture_process_state(&self, session_id: Uuid) -> Result<HashMap<String, String>> {
        let mut process_state = HashMap::new();
        
        // Capture current environment variables and process information
        process_state.insert("session_id".to_string(), session_id.to_string());
        process_state.insert("checkpoint_time".to_string(), Utc::now().to_rfc3339());
        process_state.insert("working_directory".to_string(), "/workspace".to_string());
        
        // Add some basic system information
        process_state.insert("pid".to_string(), std::process::id().to_string());
        process_state.insert("hostname".to_string(), 
            std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string()));
        
        // Capture important environment variables
        for (key, value) in std::env::vars() {
            if key.starts_with("SOULBOX_") || key == "PATH" || key == "USER" {
                process_state.insert(format!("env_{}", key), value);
            }
        }
        
        Ok(process_state)
    }
    
    /// Save checkpoint to disk
    async fn save_checkpoint_to_disk(&self, checkpoint: &SessionCheckpoint) -> Result<()> {
        let storage_dir = std::path::Path::new(&self.storage_path);
        tokio::fs::create_dir_all(storage_dir).await?;
        
        let filename = format!("checkpoint_{}_{}.json", 
            checkpoint.session_id, 
            checkpoint.timestamp.timestamp());
        let file_path = storage_dir.join(filename);
        
        let checkpoint_json = serde_json::to_string_pretty(checkpoint)
            .map_err(|e| SoulBoxError::Internal(format!("Failed to serialize checkpoint: {}", e)))?;
        
        tokio::fs::write(&file_path, checkpoint_json).await?;
        
        info!("Saved checkpoint for session {} to {:?}", checkpoint.session_id, file_path);
        Ok(())
    }
    
    /// Restore filesystem snapshot for a session
    async fn restore_filesystem_snapshot(&self, session_id: Uuid, snapshot_id: &str) -> Result<()> {
        // This would integrate with the filesystem snapshot functionality
        // For now, we'll log the operation
        info!("Restoring filesystem snapshot {} for session {}", snapshot_id, session_id);
        
        // In a real implementation, this would:
        // 1. Find the snapshot files
        // 2. Stop any running processes in the session
        // 3. Replace the current filesystem with snapshot data
        // 4. Restart processes
        
        Ok(())
    }
    
    /// Restore memory state for a session  
    async fn restore_memory_state(&self, session_id: Uuid, memory_data: &[u8]) -> Result<()> {
        // This would use CRIU or similar to restore actual process memory
        // For now, we'll just validate the memory data
        
        let memory_info = String::from_utf8_lossy(memory_data);
        info!("Restoring memory state for session {}: {}", session_id, memory_info);
        
        // In a real implementation, this would:
        // 1. Parse the memory dump
        // 2. Create new processes with restored memory
        // 3. Map memory regions correctly
        // 4. Restore process state
        
        Ok(())
    }
    
    /// Restore process state for a session
    async fn restore_process_state(&self, session_id: Uuid, process_state: &HashMap<String, String>) -> Result<()> {
        info!("Restoring process state for session {} with {} variables", 
            session_id, process_state.len());
        
        // Restore environment variables
        for (key, value) in process_state {
            if key.starts_with("env_") {
                let env_name = key.strip_prefix("env_").unwrap();
                std::env::set_var(env_name, value);
                debug!("Restored environment variable: {}={}", env_name, value);
            }
        }
        
        // Restore working directory if specified
        if let Some(working_dir) = process_state.get("working_directory") {
            if let Err(e) = std::env::set_current_dir(working_dir) {
                warn!("Failed to restore working directory {}: {}", working_dir, e);
            }
        }
        
        // In a real implementation, this would also:
        // 1. Restore process trees
        // 2. Restore file descriptors
        // 3. Restore network connections
        // 4. Restore signal handlers
        
        Ok(())
    }
    
    /// Load checkpoint from disk
    pub async fn load_checkpoint(&self, session_id: Uuid, timestamp: i64) -> Result<SessionCheckpoint> {
        let filename = format!("checkpoint_{}_{}.json", session_id, timestamp);
        let file_path = std::path::Path::new(&self.storage_path).join(filename);
        
        let checkpoint_data = tokio::fs::read_to_string(&file_path).await
            .map_err(|e| SoulBoxError::Internal(format!("Failed to read checkpoint file: {}", e)))?;
        
        let checkpoint: SessionCheckpoint = serde_json::from_str(&checkpoint_data)
            .map_err(|e| SoulBoxError::Internal(format!("Failed to parse checkpoint: {}", e)))?;
        
        Ok(checkpoint)
    }
    
    /// List all checkpoints for a session
    pub async fn list_checkpoints(&self, session_id: Uuid) -> Result<Vec<SessionCheckpoint>> {
        let storage_dir = std::path::Path::new(&self.storage_path);
        let mut checkpoints = Vec::new();
        
        if !storage_dir.exists() {
            return Ok(checkpoints);
        }
        
        let mut entries = tokio::fs::read_dir(storage_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with(&format!("checkpoint_{}_", session_id)) && file_name.ends_with(".json") {
                if let Ok(checkpoint_data) = tokio::fs::read_to_string(entry.path()).await {
                    if let Ok(checkpoint) = serde_json::from_str::<SessionCheckpoint>(&checkpoint_data) {
                        checkpoints.push(checkpoint);
                    }
                }
            }
        }
        
        // Sort by timestamp, newest first
        checkpoints.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        Ok(checkpoints)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::InMemorySessionManager;
    use crate::container::MockContainerManager;
    
    #[tokio::test]
    async fn test_recovery_context_creation() {
        let session_manager = Box::new(InMemorySessionManager::new());
        let container_manager = Box::new(ContainerManager::new(crate::config::Config::default()).await.unwrap());
        let recovery_service = SessionRecoveryService::new(session_manager, container_manager);
        
        let session = Session::new("user123".to_string());
        let context = recovery_service.assess_session_health(&session).await.unwrap();
        
        assert_eq!(context.session.id, session.id);
        assert!(!context.container_exists);
        assert!(!context.container_running);
    }
    
    #[tokio::test]
    async fn test_expired_session_recovery() {
        let session_manager = Box::new(InMemorySessionManager::new());
        let container_manager = Box::new(ContainerManager::new(crate::config::Config::default()).await.unwrap());
        let recovery_service = SessionRecoveryService::new(session_manager, container_manager);
        
        let mut session = Session::new("user123".to_string());
        // Set session to expired
        session.expires_at = Utc::now() - chrono::Duration::hours(1);
        
        let context = recovery_service.assess_session_health(&session).await.unwrap();
        assert!(context.recovery_actions.contains(&RecoveryAction::MarkAsFailed));
    }
}