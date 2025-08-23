use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use crate::error::Result;

pub mod persistence;
pub mod recovery;

// Tests module is defined inline below

/// Session state for tracking active sandboxes and their metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier
    pub id: Uuid,
    /// User ID associated with this session
    pub user_id: String,
    /// Container ID if a container is associated
    pub container_id: Option<String>,
    /// Current runtime type (python, nodejs, etc.)
    pub runtime: Option<String>,
    /// Session creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp
    pub last_activity: DateTime<Utc>,
    /// Session expiration timestamp
    pub expires_at: DateTime<Utc>,
    /// Session metadata and variables
    pub metadata: HashMap<String, String>,
    /// Current working directory in the session
    pub working_directory: String,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Whether session is active
    pub is_active: bool,
}

impl Session {
    /// Create a new session with default settings
    pub fn new(user_id: String) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::hours(24); // 24 hour default TTL
        
        Self {
            id: Uuid::new_v4(),
            user_id,
            container_id: None,
            runtime: None,
            created_at: now,
            last_activity: now,
            expires_at,
            metadata: HashMap::new(),
            working_directory: "/workspace".to_string(),
            environment: HashMap::new(),
            is_active: true,
        }
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = Utc::now();
    }

    /// Check if session has expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Extend session expiration by given hours
    pub fn extend(&mut self, hours: i64) {
        self.expires_at = self.expires_at + chrono::Duration::hours(hours);
    }

    /// Set container association
    pub fn set_container(&mut self, container_id: String, runtime: String) {
        self.container_id = Some(container_id);
        self.runtime = Some(runtime);
        self.touch();
    }

    /// Remove container association
    pub fn clear_container(&mut self) {
        self.container_id = None;
        self.runtime = None;
        self.touch();
    }

    /// Add metadata entry
    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
        self.touch();
    }

    /// Set environment variable
    pub fn set_env(&mut self, key: String, value: String) {
        self.environment.insert(key, value);
        self.touch();
    }

    /// Mark session as inactive
    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.touch();
    }
}

/// Session manager trait for different storage backends
#[async_trait::async_trait]
pub trait SessionManager: Send + Sync {
    /// Create a new session
    async fn create_session(&self, user_id: String) -> Result<Session>;
    
    /// Get session by ID
    async fn get_session(&self, session_id: Uuid) -> Result<Option<Session>>;
    
    /// Update existing session
    async fn update_session(&self, session: &Session) -> Result<()>;
    
    /// Delete session
    async fn delete_session(&self, session_id: Uuid) -> Result<()>;
    
    /// List active sessions for a user
    async fn list_user_sessions(&self, user_id: &str) -> Result<Vec<Session>>;
    
    /// Clean up expired sessions
    async fn cleanup_expired_sessions(&self) -> Result<u32>;
    
    /// Get session by container ID
    async fn get_session_by_container(&self, container_id: &str) -> Result<Option<Session>>;
}

/// In-memory session manager for testing
pub struct InMemorySessionManager {
    sessions: tokio::sync::RwLock<HashMap<Uuid, Session>>,
}

impl InMemorySessionManager {
    pub fn new() -> Self {
        Self {
            sessions: tokio::sync::RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl SessionManager for InMemorySessionManager {
    async fn create_session(&self, user_id: String) -> Result<Session> {
        let session = Session::new(user_id);
        let session_id = session.id;
        
        self.sessions.write().await.insert(session_id, session.clone());
        
        Ok(session)
    }
    
    async fn get_session(&self, session_id: Uuid) -> Result<Option<Session>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(&session_id).cloned())
    }
    
    async fn update_session(&self, session: &Session) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id, session.clone());
        Ok(())
    }
    
    async fn delete_session(&self, session_id: Uuid) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(&session_id);
        Ok(())
    }
    
    async fn list_user_sessions(&self, user_id: &str) -> Result<Vec<Session>> {
        let sessions = self.sessions.read().await;
        let user_sessions: Vec<Session> = sessions
            .values()
            .filter(|s| s.user_id == user_id && s.is_active)
            .cloned()
            .collect();
        
        Ok(user_sessions)
    }
    
    async fn cleanup_expired_sessions(&self) -> Result<u32> {
        let mut sessions = self.sessions.write().await;
        let expired_keys: Vec<Uuid> = sessions
            .iter()
            .filter(|(_, session)| session.is_expired())
            .map(|(id, _)| *id)
            .collect();
        
        let count = expired_keys.len() as u32;
        for key in expired_keys {
            sessions.remove(&key);
        }
        
        Ok(count)
    }
    
    async fn get_session_by_container(&self, container_id: &str) -> Result<Option<Session>> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .values()
            .find(|s| s.container_id.as_ref() == Some(&container_id.to_string()))
            .cloned();
        
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_session_creation() {
        let manager = InMemorySessionManager::new();
        let session = manager.create_session("user123".to_string()).await.unwrap();
        
        assert_eq!(session.user_id, "user123");
        assert!(session.is_active);
        assert!(!session.is_expired());
    }
    
    #[tokio::test]
    async fn test_session_retrieval() {
        let manager = InMemorySessionManager::new();
        let session = manager.create_session("user123".to_string()).await.unwrap();
        
        let retrieved = manager.get_session(session.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().user_id, "user123");
    }
    
    #[tokio::test]
    async fn test_session_update() {
        let manager = InMemorySessionManager::new();
        let mut session = manager.create_session("user123".to_string()).await.unwrap();
        
        session.set_container("container123".to_string(), "python".to_string());
        manager.update_session(&session).await.unwrap();
        
        let retrieved = manager.get_session(session.id).await.unwrap().unwrap();
        assert_eq!(retrieved.container_id, Some("container123".to_string()));
        assert_eq!(retrieved.runtime, Some("python".to_string()));
    }
    
    #[tokio::test]
    async fn test_session_expiration() {
        let mut session = Session::new("user123".to_string());
        
        // Set expiration to past
        session.expires_at = Utc::now() - chrono::Duration::hours(1);
        assert!(session.is_expired());
        
        // Extend session
        session.extend(2);
        assert!(!session.is_expired());
    }
}