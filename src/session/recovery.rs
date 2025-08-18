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
#[derive(Debug, Clone)]
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
            last_checkpoint: None, // TODO: Implement checkpointing
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
#[derive(Debug, Clone)]
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
        // TODO: Implement actual checkpointing
        // This would involve:
        // 1. Creating filesystem snapshots
        // 2. Capturing process memory state
        // 3. Saving container state
        
        Ok(SessionCheckpoint {
            session_id,
            timestamp: Utc::now(),
            filesystem_snapshot: None,
            memory_dump: None,
            process_state: HashMap::new(),
        })
    }

    /// Restore a session from a checkpoint
    pub async fn restore_checkpoint(&self, checkpoint: &SessionCheckpoint) -> Result<()> {
        // TODO: Implement checkpoint restoration
        // This would involve:
        // 1. Restoring filesystem from snapshot
        // 2. Restoring process memory state
        // 3. Restarting processes in correct state
        
        info!("Restoring checkpoint for session {}", checkpoint.session_id);
        Ok(())
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
        let container_manager = Box::new(MockContainerManager::new());
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
        let container_manager = Box::new(MockContainerManager::new());
        let recovery_service = SessionRecoveryService::new(session_manager, container_manager);
        
        let mut session = Session::new("user123".to_string());
        // Set session to expired
        session.expires_at = Utc::now() - chrono::Duration::hours(1);
        
        let context = recovery_service.assess_session_health(&session).await.unwrap();
        assert!(context.recovery_actions.contains(&RecoveryAction::MarkAsFailed));
    }
}