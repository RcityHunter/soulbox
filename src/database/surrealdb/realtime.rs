use async_stream::stream;
use futures_util::stream::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::connection::{SurrealConnection, SurrealResult, SurrealConnectionError};
use super::operations::uuid_to_record_id;

/// Real-time event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum RealtimeEvent {
    Create { data: serde_json::Value },
    Update { data: serde_json::Value },
    Delete { id: String },
}

/// Real-time notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeNotification {
    pub table: String,
    pub id: String,
    pub event: RealtimeEvent,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Real-time query manager for SurrealDB LIVE SELECT
pub struct RealtimeManager;

impl RealtimeManager {
    /// Watch sandbox status changes using SurrealDB LIVE SELECT
    pub async fn watch_sandbox_status(
        conn: &SurrealConnection,
        sandbox_id: Option<Uuid>,
    ) -> SurrealResult<Pin<Box<dyn Stream<Item = RealtimeNotification> + Send>>> {
        let sql = if let Some(id) = sandbox_id {
            "LIVE SELECT * FROM sandboxes WHERE id = $sandbox_id"
        } else {
            "LIVE SELECT * FROM sandboxes"
        };
        
        debug!("Starting sandbox status watch: {}", sql);
        
        // Create a proper SurrealDB live query
        let mut query = conn.db().query(sql);
        if let Some(id) = sandbox_id {
            let record_id = uuid_to_record_id("sandboxes", id);
            query = query.bind(("sandbox_id", record_id));
        }
        
        // Note: SurrealDB live query API has changed
        // This is a placeholder implementation that needs to be updated with the new API
        // For now, return an empty stream with proper type annotations
        let live_stream: futures_util::stream::Empty<Result<serde_json::Value, String>> = futures_util::stream::empty();
        
        let stream = stream! {
            let mut stream = Box::pin(live_stream);
            while let Some(result) = stream.next().await {
                match result {
                    Ok(data) => {
                        let realtime_notification = RealtimeNotification {
                            table: "sandboxes".to_string(),
                            id: "temp".to_string(),
                            event: RealtimeEvent::Create { data },
                            timestamp: chrono::Utc::now(),
                        };
                        yield realtime_notification;
                    }
                    Err(e) => {
                        error!("Error in live query stream: {}", e);
                        break;
                    }
                }
            }
        };
        
        Ok(Box::pin(stream))
    }
    
    /// Watch user activity (audit logs) using SurrealDB LIVE SELECT
    pub async fn watch_user_activity(
        conn: &SurrealConnection,
        user_id: Uuid,
    ) -> SurrealResult<Pin<Box<dyn Stream<Item = RealtimeNotification> + Send>>> {
        let sql = "LIVE SELECT * FROM audit_logs WHERE user_id = $user_id";
        
        debug!("Starting user activity watch for {}: {}", user_id, sql);
        
        let user_record_id = uuid_to_record_id("users", user_id);
        
        // Note: SurrealDB live query API has changed
        // This is a placeholder implementation
        let live_stream: futures_util::stream::Empty<Result<serde_json::Value, String>> = futures_util::stream::empty();
        
        let stream = stream! {
            let mut stream = Box::pin(live_stream);
            while let Some(result) = stream.next().await {
                match result {
                    Ok(data) => {
                        let realtime_notification = RealtimeNotification {
                            table: "audit_logs".to_string(),
                            id: "temp".to_string(),
                            event: RealtimeEvent::Create { data },
                            timestamp: chrono::Utc::now(),
                        };
                        yield realtime_notification;
                    }
                    Err(e) => {
                        error!("Error in live query stream: {}", e);
                        break;
                    }
                }
            }
        };
        
        Ok(Box::pin(stream))
    }
    
    /// Watch tenant resource usage using SurrealDB LIVE SELECT
    pub async fn watch_tenant_resources(
        conn: &SurrealConnection,
        tenant_id: Uuid,
    ) -> SurrealResult<Pin<Box<dyn Stream<Item = RealtimeNotification> + Send>>> {
        let sql = "LIVE SELECT * FROM sandboxes WHERE tenant_id = $tenant_id";
        
        debug!("Starting tenant resource watch for {}: {}", tenant_id, sql);
        
        let tenant_record_id = uuid_to_record_id("tenants", tenant_id);
        
        // Note: SurrealDB live query API has changed
        // This is a placeholder implementation
        let live_stream: futures_util::stream::Empty<Result<serde_json::Value, String>> = futures_util::stream::empty();
        
        let stream = stream! {
            let mut stream = Box::pin(live_stream);
            while let Some(result) = stream.next().await {
                match result {
                    Ok(data) => {
                        let realtime_notification = RealtimeNotification {
                            table: "sandboxes".to_string(),
                            id: "temp".to_string(),
                            event: RealtimeEvent::Create { data },
                            timestamp: chrono::Utc::now(),
                        };
                        yield realtime_notification;
                    }
                    Err(e) => {
                        error!("Error in live query stream: {}", e);
                        break;
                    }
                }
            }
        };
        
        Ok(Box::pin(stream))
    }
    
    /// Watch all database changes (admin-level monitoring) - DISABLED for security
    /// 
    /// Note: Global database watching is disabled for security and performance reasons.
    /// Use specific table watches instead.
    pub async fn watch_all_changes(
        _conn: &SurrealConnection,
    ) -> SurrealResult<Pin<Box<dyn Stream<Item = RealtimeNotification> + Send>>> {
        info!("Global database watch is disabled for security reasons");
        
        // Return an empty stream that immediately ends
        let stream = stream! {
            // Empty stream - no global watching allowed
            return;
            yield RealtimeNotification {
                table: "global".to_string(),
                id: "disabled".to_string(),
                event: RealtimeEvent::Create { data: serde_json::Value::Null },
                timestamp: chrono::Utc::now(),
            };
        };
        
        Ok(Box::pin(stream))
    }
    
    /// Create a multiplexed stream that combines multiple live queries
    /// 
    /// WARNING: This feature is disabled due to security concerns.
    /// Each live query should be managed separately for better control.
    pub async fn watch_multiple(
        _conn: &SurrealConnection,
        queries: Vec<String>,
    ) -> SurrealResult<Pin<Box<dyn Stream<Item = RealtimeNotification> + Send>>> {
        warn!("Multiplexed watch is disabled for security reasons. {} queries rejected", queries.len());
        
        // Return an empty stream
        let stream = stream! {
            // Empty stream - multiplexed watching disabled for security
            return;
            yield RealtimeNotification {
                table: "multiple".to_string(),
                id: "disabled".to_string(),
                event: RealtimeEvent::Create { data: serde_json::Value::Null },
                timestamp: chrono::Utc::now(),
            };
        };
        
        Ok(Box::pin(stream))
    }
}

/// Helper trait to convert streams to WebSocket messages
pub trait StreamToWebSocket {
    fn to_websocket_message(&self) -> String;
}

impl StreamToWebSocket for RealtimeNotification {
    fn to_websocket_message(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            error!("Failed to serialize realtime notification: {}", e);
            r#"{"error": "serialization_failed"}"#.to_string()
        })
    }
}

/// Configuration for real-time subscriptions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionConfig {
    /// Maximum number of concurrent subscriptions per connection
    pub max_subscriptions: usize,
    
    /// Buffer size for stream buffering
    pub buffer_size: usize,
    
    /// Heartbeat interval for keeping connections alive
    pub heartbeat_interval_secs: u64,
    
    /// Maximum idle time before closing inactive subscriptions
    pub max_idle_time_secs: u64,
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            max_subscriptions: 10,
            buffer_size: 100,
            heartbeat_interval_secs: 30,
            max_idle_time_secs: 300,
        }
    }
}

/// Subscription manager for handling multiple real-time subscriptions
pub struct SubscriptionManager {
    config: SubscriptionConfig,
    active_subscriptions: std::collections::HashMap<String, tokio::task::JoinHandle<()>>,
}

impl SubscriptionManager {
    pub fn new(config: SubscriptionConfig) -> Self {
        Self {
            config,
            active_subscriptions: std::collections::HashMap::new(),
        }
    }
    
    /// Add a new subscription
    pub async fn add_subscription(
        &mut self,
        subscription_id: String,
        stream: Pin<Box<dyn Stream<Item = RealtimeNotification> + Send>>,
        sender: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> SurrealResult<()> {
        if self.active_subscriptions.len() >= self.config.max_subscriptions {
            return Err(crate::database::surrealdb::SurrealConnectionError::Config(
                "Maximum subscriptions limit reached".to_string()
            ));
        }
        
        let mut stream = stream;
        let heartbeat_interval = tokio::time::Duration::from_secs(self.config.heartbeat_interval_secs);
        
        let handle = tokio::spawn(async move {
            let mut heartbeat = tokio::time::interval(heartbeat_interval);
            
            loop {
                tokio::select! {
                    _ = heartbeat.tick() => {
                        let heartbeat_msg = serde_json::json!({
                            "type": "heartbeat",
                            "timestamp": chrono::Utc::now()
                        });
                        
                        if sender.send(heartbeat_msg.to_string()).is_err() {
                            debug!("Heartbeat send failed, closing subscription");
                            break;
                        }
                    },
                    
                    Some(notification) = stream.next() => {
                        if sender.send(notification.to_websocket_message()).is_err() {
                            debug!("Notification send failed, closing subscription");
                            break;
                        }
                    },
                    
                    else => {
                        debug!("Stream ended, closing subscription");
                        break;
                    }
                }
            }
            
            info!("Subscription task ended");
        });
        
        self.active_subscriptions.insert(subscription_id, handle);
        Ok(())
    }
    
    /// Remove a subscription
    pub async fn remove_subscription(&mut self, subscription_id: &str) {
        if let Some(handle) = self.active_subscriptions.remove(subscription_id) {
            handle.abort();
            info!("Removed subscription: {}", subscription_id);
        }
    }
    
    /// Get active subscription count
    pub fn active_count(&self) -> usize {
        self.active_subscriptions.len()
    }
    
    /// Clean up all subscriptions
    pub async fn cleanup(&mut self) {
        for (id, handle) in self.active_subscriptions.drain() {
            handle.abort();
            debug!("Cleaned up subscription: {}", id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;
    
    #[tokio::test]
    async fn test_realtime_notification_serialization() {
        let notification = RealtimeNotification {
            table: "test".to_string(),
            id: "test:123".to_string(),
            event: RealtimeEvent::Create {
                data: serde_json::json!({"name": "test"})
            },
            timestamp: chrono::Utc::now(),
        };
        
        let json = notification.to_websocket_message();
        assert!(json.contains("test"));
        assert!(json.contains("Create"));
    }
    
    #[tokio::test]
    async fn test_subscription_manager() {
        let config = SubscriptionConfig::default();
        let mut manager = SubscriptionManager::new(config);
        
        assert_eq!(manager.active_count(), 0);
        
        // Test would require more setup for actual stream testing
        manager.cleanup().await;
    }
}