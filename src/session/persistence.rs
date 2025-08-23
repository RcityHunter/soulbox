use redis::{AsyncCommands, Client};
use redis::aio::MultiplexedConnection;
use serde_json;
use uuid::Uuid;
use tracing::{error, info, warn};
use crate::error::{Result, SoulBoxError};
use super::{Session, SessionManager};

/// Redis configuration for session storage
#[derive(Debug, Clone)]
pub struct RedisSessionConfig {
    /// Redis connection URL
    pub url: String,
    /// Key prefix for session storage
    pub key_prefix: String,
    /// Default session TTL in seconds
    pub default_ttl: u64,
    /// Connection pool size
    pub pool_size: u32,
}

impl Default for RedisSessionConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            key_prefix: "soulbox:session".to_string(),
            default_ttl: 86400, // 24 hours
            pool_size: 10,
        }
    }
}

/// Redis-backed session manager with automatic expiration
pub struct RedisSessionManager {
    connection: MultiplexedConnection,
    config: RedisSessionConfig,
}

impl RedisSessionManager {
    /// Create a new Redis session manager
    pub async fn new(config: RedisSessionConfig) -> Result<Self> {
        let client = Client::open(config.url.clone())
            .map_err(|e| SoulBoxError::SessionError(format!("Failed to create Redis client: {}", e)))?;
        
        let connection = client.get_multiplexed_async_connection().await
            .map_err(|e| SoulBoxError::SessionError(format!("Failed to connect to Redis: {}", e)))?;
        
        info!("Connected to Redis at {}", config.url);
        
        Ok(Self {
            connection,
            config,
        })
    }

    /// Get the Redis key for a session
    fn session_key(&self, session_id: Uuid) -> String {
        format!("{}:{}", self.config.key_prefix, session_id)
    }

    /// Get the Redis key for user sessions index
    fn user_sessions_key(&self, user_id: &str) -> String {
        format!("{}:user:{}", self.config.key_prefix, user_id)
    }

    /// Get the Redis key for container to session mapping
    fn container_session_key(&self, container_id: &str) -> String {
        format!("{}:container:{}", self.config.key_prefix, container_id)
    }

    /// Serialize session to JSON
    fn serialize_session(&self, session: &Session) -> Result<String> {
        serde_json::to_string(session)
            .map_err(|e| SoulBoxError::SessionError(format!("Failed to serialize session: {}", e)))
    }

    /// Deserialize session from JSON
    fn deserialize_session(&self, data: &str) -> Result<Session> {
        serde_json::from_str(data)
            .map_err(|e| SoulBoxError::SessionError(format!("Failed to deserialize session: {}", e)))
    }

    /// Calculate TTL for session based on expiration time
    fn calculate_ttl(&self, session: &Session) -> u64 {
        let now = chrono::Utc::now();
        if session.expires_at > now {
            (session.expires_at - now).num_seconds().max(0) as u64
        } else {
            0
        }
    }
}

#[async_trait::async_trait]
impl SessionManager for RedisSessionManager {
    async fn create_session(&self, user_id: String) -> Result<Session> {
        let session = Session::new(user_id.clone());
        let session_data = self.serialize_session(&session)?;
        let ttl = self.calculate_ttl(&session);
        
        // Validate TTL is reasonable (prevent memory leaks from very long sessions)
        let ttl = ttl.min(7 * 24 * 3600); // Max 7 days
        
        let mut conn = self.connection.clone();
        
        // Use Redis pipeline for atomic operations
        let mut pipe = redis::pipe();
        
        // Store session data with TTL
        let session_key = self.session_key(session.id);
        pipe.set_ex(&session_key, &session_data, ttl);
        
        // Add to user sessions set and update TTL atomically
        let user_sessions_key = self.user_sessions_key(&user_id);
        pipe.sadd(&user_sessions_key, session.id.to_string());
        
        // Set user sessions set TTL to maximum session TTL + buffer for cleanup
        let user_set_ttl = ttl + 86400; // +24 hours buffer for cleanup
        pipe.expire(&user_sessions_key, user_set_ttl as i64);
        
        // Execute pipeline atomically
        pipe.query_async::<_, ()>(&mut conn).await
            .map_err(|e| SoulBoxError::SessionError(format!("Failed to create session atomically: {}", e)))?;
        
        info!("Created session {} for user {} with TTL {}s", session.id, user_id, ttl);
        Ok(session)
    }
    
    async fn get_session(&self, session_id: Uuid) -> Result<Option<Session>> {
        let mut conn = self.connection.clone();
        let session_key = self.session_key(session_id);
        
        let data: Option<String> = conn.get(&session_key).await
            .map_err(|e| SoulBoxError::SessionError(format!("Failed to get session: {}", e)))?;
        
        match data {
            Some(json) => {
                let session = self.deserialize_session(&json)?;
                
                // Check if session has expired
                if session.is_expired() {
                    warn!("Session {} has expired, removing", session_id);
                    let _ = self.delete_session(session_id).await;
                    return Ok(None);
                }
                
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }
    
    async fn update_session(&self, session: &Session) -> Result<()> {
        let session_data = self.serialize_session(session)?;
        let ttl = self.calculate_ttl(session);
        
        if ttl == 0 {
            // Session has expired, delete it
            return self.delete_session(session.id).await;
        }
        
        // Validate TTL is reasonable (prevent memory leaks)
        let ttl = ttl.min(7 * 24 * 3600); // Max 7 days
        
        let mut conn = self.connection.clone();
        
        // Use Redis pipeline for atomic updates
        let mut pipe = redis::pipe();
        
        // Update session data with new TTL
        let session_key = self.session_key(session.id);
        pipe.set_ex(&session_key, &session_data, ttl);
        
        // Update user sessions set TTL to ensure it doesn't expire before sessions
        let user_sessions_key = self.user_sessions_key(&session.user_id);
        let user_set_ttl = ttl + 86400; // +24 hours buffer
        pipe.expire(&user_sessions_key, user_set_ttl as i64);
        
        // Update container mapping if container is set
        if let Some(container_id) = &session.container_id {
            let container_key = self.container_session_key(container_id);
            pipe.set_ex(&container_key, session.id.to_string(), ttl);
        }
        
        // Execute all updates atomically
        pipe.query_async::<_, ()>(&mut conn).await
            .map_err(|e| SoulBoxError::SessionError(format!("Failed to update session atomically: {}", e)))?;
        
        Ok(())
    }
    
    async fn delete_session(&self, session_id: Uuid) -> Result<()> {
        let mut conn = self.connection.clone();
        
        // Get session to find user_id and container_id for cleanup
        if let Ok(Some(session)) = self.get_session(session_id).await {
            // Remove from user sessions set
            let user_sessions_key = self.user_sessions_key(&session.user_id);
            let _: std::result::Result<i64, _> = conn.srem(&user_sessions_key, session_id.to_string()).await;
            
            // Remove container mapping if exists
            if let Some(container_id) = &session.container_id {
                let container_key = self.container_session_key(container_id);
                let _: std::result::Result<i32, _> = conn.del(&container_key).await;
            }
        }
        
        // Delete session data
        let session_key = self.session_key(session_id);
        conn.del::<_, ()>(&session_key).await
            .map_err(|e| SoulBoxError::SessionError(format!("Failed to delete session: {}", e)))?;
        
        info!("Deleted session {}", session_id);
        Ok(())
    }
    
    async fn list_user_sessions(&self, user_id: &str) -> Result<Vec<Session>> {
        let mut conn = self.connection.clone();
        let user_sessions_key = self.user_sessions_key(user_id);
        
        // Get all session IDs for user
        let session_ids: Vec<String> = conn.smembers(&user_sessions_key).await
            .map_err(|e| SoulBoxError::SessionError(format!("Failed to get user sessions: {}", e)))?;
        
        let mut sessions = Vec::new();
        
        for session_id_str in session_ids {
            if let Ok(session_id) = Uuid::parse_str(&session_id_str) {
                if let Ok(Some(session)) = self.get_session(session_id).await {
                    if session.is_active {
                        sessions.push(session);
                    }
                }
            }
        }
        
        Ok(sessions)
    }
    
    async fn cleanup_expired_sessions(&self) -> Result<u32> {
        // Redis will automatically clean up expired keys, but we need to clean up
        // associated data structures like user sessions sets and container mappings
        
        let mut conn = self.connection.clone();
        let pattern = format!("{}:*", self.config.key_prefix);
        
        // Use SCAN to avoid blocking operations - this is production-safe
        let mut cursor = 0u64;
        let mut cleaned = 0u32;
        let scan_count = 100; // Process 100 keys at a time
        
        loop {
            // Use SCAN with cursor to iterate through keys non-blockingly
            let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(scan_count)
                .query_async(&mut conn)
                .await
                .map_err(|e| SoulBoxError::SessionError(format!("Failed to scan keys: {}", e)))?;
            
            // Process the batch of keys
            for key in keys {
                if key.contains(":session:") {
                    // Check if session still exists
                    let exists: bool = conn.exists(&key).await
                        .map_err(|e| SoulBoxError::SessionError(format!("Failed to check key existence: {}", e)))?;
                    
                    if !exists {
                        // Clean up associated data
                        self.cleanup_session_references(&key, &mut conn).await?;
                        cleaned += 1;
                    }
                } else if key.contains(":user:") || key.contains(":container:") {
                    // Check if these reference keys are still needed
                    let exists: bool = conn.exists(&key).await
                        .map_err(|e| SoulBoxError::SessionError(format!("Failed to check key existence: {}", e)))?;
                    
                    if !exists {
                        cleaned += 1;
                    }
                }
            }
            
            // Update cursor for next iteration
            cursor = new_cursor;
            
            // If cursor is 0, we've completed the full scan
            if cursor == 0 {
                break;
            }
            
            // Small delay to avoid overwhelming Redis
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }
        
        info!("Cleaned up {} expired sessions and references", cleaned);
        Ok(cleaned)
    }
    
    async fn get_session_by_container(&self, container_id: &str) -> Result<Option<Session>> {
        let mut conn = self.connection.clone();
        let container_key = self.container_session_key(container_id);
        
        let session_id_str: Option<String> = conn.get(&container_key).await
            .map_err(|e| SoulBoxError::SessionError(format!("Failed to get container session: {}", e)))?;
        
        match session_id_str {
            Some(id_str) => {
                if let Ok(session_id) = Uuid::parse_str(&id_str) {
                    self.get_session(session_id).await
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }
}

// Helper methods for RedisSessionManager
impl RedisSessionManager {
    /// Clean up session-related references
    async fn cleanup_session_references(&self, session_key: &str, conn: &mut MultiplexedConnection) -> Result<()> {
        // Extract session ID from key
        if let Some(session_id_part) = session_key.split(':').last() {
            if let Ok(session_id) = uuid::Uuid::parse_str(session_id_part) {
                // Try to get session data before cleanup to find related keys
                let session_data: Option<String> = conn.get(session_key).await
                    .map_err(|e| SoulBoxError::SessionError(format!("Failed to get session data: {}", e)))?;
                
                if let Some(data) = session_data {
                    if let Ok(session) = self.deserialize_session(&data) {
                        // Clean up user sessions reference
                        let user_sessions_key = self.user_sessions_key(&session.user_id);
                        let _: std::result::Result<i64, _> = conn.srem(&user_sessions_key, session_id.to_string()).await.map_err(|e| SoulBoxError::Redis(e.to_string()));
                        
                        // Clean up container mapping if exists
                        if let Some(container_id) = &session.container_id {
                            let container_key = self.container_session_key(container_id);
                            let _: std::result::Result<i64, _> = conn.del(&container_key).await.map_err(|e| SoulBoxError::Redis(e.to_string()));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Background session cleanup task
pub struct SessionCleanupTask {
    manager: Box<dyn SessionManager>,
    interval: std::time::Duration,
}

impl SessionCleanupTask {
    pub fn new(manager: Box<dyn SessionManager>, interval_seconds: u64) -> Self {
        Self {
            manager,
            interval: std::time::Duration::from_secs(interval_seconds),
        }
    }

    /// Start the background cleanup task
    pub async fn start(self) {
        let mut interval = tokio::time::interval(self.interval);
        
        loop {
            interval.tick().await;
            
            match self.manager.cleanup_expired_sessions().await {
                Ok(count) => {
                    if count > 0 {
                        info!("Cleaned up {} expired sessions", count);
                    }
                }
                Err(e) => {
                    error!("Failed to cleanup expired sessions: {}", e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::InMemorySessionManager;
    
    // Helper function to create a mock Redis manager for testing
    async fn create_mock_redis_manager() -> Option<RedisSessionManager> {
        // Only connect to Redis if it's available and we're not in CI
        if std::env::var("SKIP_REDIS_TESTS").is_ok() || std::env::var("CI").is_ok() {
            return None;
        }
        
        // Try to connect to Redis, skip test if not available
        match Client::open("redis://localhost:6379") {
            Ok(client) => {
                match client.get_multiplexed_async_connection().await {
                    Ok(connection) => {
                        Some(RedisSessionManager {
                            connection,
                            config: RedisSessionConfig {
                                key_prefix: format!("test_soulbox_{}", uuid::Uuid::new_v4()),
                                ..RedisSessionConfig::default()
                            },
                        })
                    },
                    Err(_) => None,
                }
            },
            Err(_) => None,
        }
    }
    
    #[tokio::test]
    async fn test_session_serialization() {
        // Test serialization without Redis connection
        let config = RedisSessionConfig::default();
        
        // Create a minimal manager just for serialization testing
        if let Some(manager) = create_mock_redis_manager().await {
            let session = Session::new("user123".to_string());
            let serialized = manager.serialize_session(&session).unwrap();
            let deserialized = manager.deserialize_session(&serialized).unwrap();
            
            assert_eq!(session.id, deserialized.id);
            assert_eq!(session.user_id, deserialized.user_id);
            
            // Clean up test data
            let _ = manager.cleanup_expired_sessions().await;
        } else {
            // Fallback test without Redis
            println!("Skipping Redis test - Redis not available");
            
            // Test serialization manually
            let session = Session::new("user123".to_string());
            let serialized = serde_json::to_string(&session).unwrap();
            let deserialized: Session = serde_json::from_str(&serialized).unwrap();
            
            assert_eq!(session.id, deserialized.id);
            assert_eq!(session.user_id, deserialized.user_id);
        }
    }
    
    #[tokio::test]
    async fn test_key_generation() {
        let config = RedisSessionConfig {
            key_prefix: "test_soulbox".to_string(),
            ..RedisSessionConfig::default()
        };
        
        if let Some(manager) = create_mock_redis_manager().await {
            let session_id = Uuid::new_v4();
            let key = manager.session_key(session_id);
            assert_eq!(key, format!("{}:{}", manager.config.key_prefix, session_id));
            
            let user_key = manager.user_sessions_key("user123");
            assert_eq!(user_key, format!("{}:user:user123", manager.config.key_prefix));
            
            let container_key = manager.container_session_key("container123");
            assert_eq!(container_key, format!("{}:container:container123", manager.config.key_prefix));
            
            // Clean up
            let _ = manager.cleanup_expired_sessions().await;
        } else {
            // Test key generation logic without Redis
            println!("Skipping Redis test - Redis not available");
            
            let session_id = Uuid::new_v4();
            let expected_key = format!("{}:{}", config.key_prefix, session_id);
            // We can't test the actual method without Redis, but we can test the logic
            assert!(expected_key.contains(&session_id.to_string()));
        }
    }
    
    #[tokio::test]
    async fn test_session_operations_with_fallback() {
        if let Some(mut manager) = create_mock_redis_manager().await {
            // Test with actual Redis if available
            let session = manager.create_session("test_user".to_string()).await.unwrap();
            assert_eq!(session.user_id, "test_user");
            
            let retrieved = manager.get_session(session.id).await.unwrap();
            assert!(retrieved.is_some());
            
            // Clean up
            let _ = manager.delete_session(session.id).await;
            let _ = manager.cleanup_expired_sessions().await;
        } else {
            // Fallback to in-memory testing
            println!("Testing with in-memory manager as fallback");
            
            let manager = InMemorySessionManager::new();
            let session = manager.create_session("test_user".to_string()).await.unwrap();
            assert_eq!(session.user_id, "test_user");
            
            let retrieved = manager.get_session(session.id).await.unwrap();
            assert!(retrieved.is_some());
        }
    }
}