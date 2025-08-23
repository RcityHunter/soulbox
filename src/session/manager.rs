use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use tracing::{info, warn, error, debug};

use crate::error::{Result, SoulBoxError};
use crate::database::SurrealPool;
use super::{Session, SessionManager};

/// Database-backed session manager with persistence and recovery
pub struct DatabaseSessionManager {
    /// Database connection pool
    db: Arc<SurrealPool>,
    /// In-memory cache for active sessions
    cache: tokio::sync::RwLock<HashMap<Uuid, Session>>,
    /// Background cleanup task handle
    cleanup_task: tokio::sync::RwLock<Option<tokio::task::JoinHandle<()>>>,
}

impl DatabaseSessionManager {
    /// Create a new database-backed session manager
    pub async fn new(db: Arc<SurrealPool>) -> Result<Self> {
        let manager = Self {
            db,
            cache: tokio::sync::RwLock::new(HashMap::new()),
            cleanup_task: tokio::sync::RwLock::new(None),
        };
        
        // Load existing sessions from database
        manager.load_sessions_from_db().await?;
        
        // Start background cleanup task
        manager.start_cleanup_task().await;
        
        Ok(manager)
    }
    
    /// Load active sessions from database into cache
    async fn load_sessions_from_db(&self) -> Result<()> {
        info!("Loading active sessions from database");
        
        let query = r#"
            SELECT * FROM sessions 
            WHERE is_active = true 
            AND expires_at > $now
        "#;
        
        let db = self.db.get_session().await?;
        let mut response = db
            .query(query)
            .bind(("now", Utc::now()))
            .await
            .map_err(|e| SoulBoxError::DatabaseError(format!("Failed to query sessions: {}", e)))?;
        
        let sessions: Vec<Session> = response
            .take(0)
            .map_err(|e| SoulBoxError::DatabaseError(format!("Failed to parse sessions: {}", e)))?;
        
        let mut cache = self.cache.write().await;
        for session in sessions {
            cache.insert(session.id, session);
        }
        
        info!("Loaded {} active sessions from database", cache.len());
        Ok(())
    }
    
    /// Start background task for cleaning up expired sessions
    async fn start_cleanup_task(&self) {
        let db = self.db.clone();
        let cache = self.cache.clone();
        
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300)); // 5 minutes
            
            loop {
                interval.tick().await;
                
                if let Err(e) = Self::cleanup_expired_sessions_static(&db, &cache).await {
                    error!("Failed to cleanup expired sessions: {}", e);
                }
            }
        });
        
        *self.cleanup_task.write().await = Some(handle);
        info!("Started session cleanup background task");
    }
    
    /// Static method for cleanup to avoid borrowing issues
    async fn cleanup_expired_sessions_static(
        db: &Arc<SurrealPool>,
        cache: &tokio::sync::RwLock<HashMap<Uuid, Session>>,
    ) -> Result<()> {
        debug!("Running session cleanup");
        
        // Clean up from database
        let query = r#"
            UPDATE sessions 
            SET is_active = false 
            WHERE expires_at <= $now 
            AND is_active = true
            RETURN BEFORE
        "#;
        
        let db_session = db.get_session().await?;
        let mut response = db_session
            .query(query)
            .bind(("now", Utc::now()))
            .await
            .map_err(|e| SoulBoxError::DatabaseError(format!("Failed to cleanup sessions: {}", e)))?;
        
        let expired_sessions: Vec<Session> = response
            .take(0)
            .unwrap_or_default();
        
        // Remove from cache
        if !expired_sessions.is_empty() {
            let mut cache_guard = cache.write().await;
            for session in &expired_sessions {
                cache_guard.remove(&session.id);
            }
            info!("Cleaned up {} expired sessions", expired_sessions.len());
        }
        
        Ok(())
    }
    
    /// Persist session to database
    async fn persist_session(&self, session: &Session) -> Result<()> {
        let query = r#"
            UPSERT sessions 
            CONTENT $session
        "#;
        
        let db = self.db.get_session().await?;
        db.query(query)
            .bind(("session", session))
            .await
            .map_err(|e| SoulBoxError::DatabaseError(format!("Failed to persist session: {}", e)))?;
        
        debug!("Persisted session {} to database", session.id);
        Ok(())
    }
    
    /// Delete session from database
    async fn delete_from_db(&self, session_id: Uuid) -> Result<()> {
        let query = r#"
            DELETE sessions 
            WHERE id = $id
        "#;
        
        let db = self.db.get_session().await?;
        db.query(query)
            .bind(("id", session_id.to_string()))
            .await
            .map_err(|e| SoulBoxError::DatabaseError(format!("Failed to delete session: {}", e)))?;
        
        debug!("Deleted session {} from database", session_id);
        Ok(())
    }
    
    /// Shutdown the manager and cleanup resources
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down DatabaseSessionManager");
        
        // Stop cleanup task
        if let Some(handle) = self.cleanup_task.write().await.take() {
            handle.abort();
        }
        
        // Persist all cached sessions
        let cache = self.cache.read().await;
        for session in cache.values() {
            if let Err(e) = self.persist_session(session).await {
                error!("Failed to persist session {} during shutdown: {}", session.id, e);
            }
        }
        
        info!("DatabaseSessionManager shutdown complete");
        Ok(())
    }
}

#[async_trait::async_trait]
impl SessionManager for DatabaseSessionManager {
    async fn create_session(&self, user_id: String) -> Result<Session> {
        let session = Session::new(user_id);
        
        // Add to cache
        self.cache.write().await.insert(session.id, session.clone());
        
        // Persist to database
        self.persist_session(&session).await?;
        
        info!("Created new session {} for user {}", session.id, session.user_id);
        Ok(session)
    }
    
    async fn get_session(&self, session_id: Uuid) -> Result<Option<Session>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(session) = cache.get(&session_id) {
                if !session.is_expired() {
                    return Ok(Some(session.clone()));
                }
            }
        }
        
        // Try to load from database
        let query = r#"
            SELECT * FROM sessions 
            WHERE id = $id 
            AND is_active = true 
            AND expires_at > $now
            LIMIT 1
        "#;
        
        let db = self.db.get_session().await?;
        let mut response = db
            .query(query)
            .bind(("id", session_id.to_string()))
            .bind(("now", Utc::now()))
            .await
            .map_err(|e| SoulBoxError::DatabaseError(format!("Failed to query session: {}", e)))?;
        
        let sessions: Vec<Session> = response
            .take(0)
            .unwrap_or_default();
        
        if let Some(session) = sessions.first() {
            // Add to cache
            self.cache.write().await.insert(session.id, session.clone());
            Ok(Some(session.clone()))
        } else {
            Ok(None)
        }
    }
    
    async fn update_session(&self, session: &Session) -> Result<()> {
        // Update cache
        self.cache.write().await.insert(session.id, session.clone());
        
        // Persist to database
        self.persist_session(session).await?;
        
        debug!("Updated session {}", session.id);
        Ok(())
    }
    
    async fn delete_session(&self, session_id: Uuid) -> Result<()> {
        // Remove from cache
        self.cache.write().await.remove(&session_id);
        
        // Delete from database
        self.delete_from_db(session_id).await?;
        
        info!("Deleted session {}", session_id);
        Ok(())
    }
    
    async fn list_user_sessions(&self, user_id: &str) -> Result<Vec<Session>> {
        let query = r#"
            SELECT * FROM sessions 
            WHERE user_id = $user_id 
            AND is_active = true 
            AND expires_at > $now
            ORDER BY created_at DESC
        "#;
        
        let db = self.db.get_session().await?;
        let mut response = db
            .query(query)
            .bind(("user_id", user_id))
            .bind(("now", Utc::now()))
            .await
            .map_err(|e| SoulBoxError::DatabaseError(format!("Failed to query user sessions: {}", e)))?;
        
        let sessions: Vec<Session> = response
            .take(0)
            .unwrap_or_default();
        
        debug!("Found {} active sessions for user {}", sessions.len(), user_id);
        Ok(sessions)
    }
    
    async fn cleanup_expired_sessions(&self) -> Result<u32> {
        let query = r#"
            UPDATE sessions 
            SET is_active = false 
            WHERE expires_at <= $now 
            AND is_active = true
            RETURN BEFORE
        "#;
        
        let db = self.db.get_session().await?;
        let mut response = db
            .query(query)
            .bind(("now", Utc::now()))
            .await
            .map_err(|e| SoulBoxError::DatabaseError(format!("Failed to cleanup sessions: {}", e)))?;
        
        let expired_sessions: Vec<Session> = response
            .take(0)
            .unwrap_or_default();
        
        let count = expired_sessions.len() as u32;
        
        // Remove from cache
        if count > 0 {
            let mut cache = self.cache.write().await;
            for session in &expired_sessions {
                cache.remove(&session.id);
            }
        }
        
        info!("Cleaned up {} expired sessions", count);
        Ok(count)
    }
    
    async fn get_session_by_container(&self, container_id: &str) -> Result<Option<Session>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            for session in cache.values() {
                if session.container_id.as_ref() == Some(&container_id.to_string()) {
                    if !session.is_expired() {
                        return Ok(Some(session.clone()));
                    }
                }
            }
        }
        
        // Query database
        let query = r#"
            SELECT * FROM sessions 
            WHERE container_id = $container_id 
            AND is_active = true 
            AND expires_at > $now
            LIMIT 1
        "#;
        
        let db = self.db.get_session().await?;
        let mut response = db
            .query(query)
            .bind(("container_id", container_id))
            .bind(("now", Utc::now()))
            .await
            .map_err(|e| SoulBoxError::DatabaseError(format!("Failed to query session by container: {}", e)))?;
        
        let sessions: Vec<Session> = response
            .take(0)
            .unwrap_or_default();
        
        if let Some(session) = sessions.first() {
            // Add to cache
            self.cache.write().await.insert(session.id, session.clone());
            Ok(Some(session.clone()))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_database_session_manager() {
        // This would require a test database setup
        // For now, we'll skip the actual database tests
    }
}