//! Real-time streaming functionality for WebSocket
//! 
//! Provides efficient real-time data streaming including:
//! - Container logs streaming
//! - Metrics streaming
//! - File change notifications
//! - Execution status updates

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tokio::time::Duration;
use tokio_stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tracing::{info, debug};
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::error::{Result, SoulBoxError};

/// Real-time stream types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum StreamType {
    Logs,
    Metrics,
    FileChanges,
    ExecutionStatus,
    TerminalOutput,
    SystemEvents,
}

/// Stream configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub stream_type: StreamType,
    pub buffer_size: usize,
    pub throttle_ms: Option<u64>,
    pub filters: Vec<StreamFilter>,
    pub max_subscribers: usize,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            stream_type: StreamType::Logs,
            buffer_size: 1000,
            throttle_ms: Some(100),
            filters: Vec::new(),
            max_subscribers: 100,
        }
    }
}

/// Stream filter for selective data streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamFilter {
    pub field: String,
    pub operator: FilterOperator,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Equals,
    Contains,
    StartsWith,
    EndsWith,
    Regex,
    GreaterThan,
    LessThan,
}

/// Real-time stream message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMessage {
    pub id: Uuid,
    pub stream_type: StreamType,
    pub timestamp: DateTime<Utc>,
    pub sandbox_id: Option<String>,
    pub data: StreamData,
}

/// Stream data variants
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamData {
    Log {
        level: String,
        source: String,
        message: String,
        metadata: HashMap<String, serde_json::Value>,
    },
    Metric {
        name: String,
        value: f64,
        unit: String,
        tags: HashMap<String, String>,
    },
    FileChange {
        path: String,
        event_type: FileEventType,
        size: Option<u64>,
        checksum: Option<String>,
    },
    ExecutionStatus {
        execution_id: String,
        status: String,
        progress: Option<f32>,
        output: Option<String>,
        error: Option<String>,
    },
    Terminal {
        session_id: String,
        output: String,
        is_stderr: bool,
    },
    SystemEvent {
        event_type: String,
        description: String,
        severity: String,
        context: HashMap<String, serde_json::Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEventType {
    Created,
    Modified,
    Deleted,
    Renamed,
    Accessed,
}

/// Real-time stream manager
pub struct StreamManager {
    streams: Arc<RwLock<HashMap<Uuid, ActiveStream>>>,
    subscribers: Arc<RwLock<HashMap<Uuid, Vec<StreamSubscriber>>>>,
    message_buffer: Arc<RwLock<HashMap<Uuid, Vec<StreamMessage>>>>,
    config: Arc<RwLock<HashMap<StreamType, StreamConfig>>>,
}

struct ActiveStream {
    id: Uuid,
    stream_type: StreamType,
    sandbox_id: Option<String>,
    created_at: DateTime<Utc>,
    sender: broadcast::Sender<StreamMessage>,
    config: StreamConfig,
}

struct StreamSubscriber {
    id: Uuid,
    connection_id: String,
    stream_id: Uuid,
    receiver: broadcast::Receiver<StreamMessage>,
    filters: Vec<StreamFilter>,
}

impl StreamManager {
    pub fn new() -> Self {
        Self {
            streams: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            message_buffer: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(RwLock::new(Self::default_configs())),
        }
    }

    fn default_configs() -> HashMap<StreamType, StreamConfig> {
        let mut configs = HashMap::new();
        
        configs.insert(StreamType::Logs, StreamConfig {
            stream_type: StreamType::Logs,
            buffer_size: 5000,
            throttle_ms: Some(50),
            filters: Vec::new(),
            max_subscribers: 200,
        });
        
        configs.insert(StreamType::Metrics, StreamConfig {
            stream_type: StreamType::Metrics,
            buffer_size: 1000,
            throttle_ms: Some(1000),
            filters: Vec::new(),
            max_subscribers: 50,
        });
        
        configs.insert(StreamType::FileChanges, StreamConfig {
            stream_type: StreamType::FileChanges,
            buffer_size: 500,
            throttle_ms: Some(200),
            filters: Vec::new(),
            max_subscribers: 100,
        });
        
        configs
    }

    /// Create a new stream
    pub async fn create_stream(
        &self,
        stream_type: StreamType,
        sandbox_id: Option<String>,
        custom_config: Option<StreamConfig>,
    ) -> Result<Uuid> {
        let stream_id = Uuid::new_v4();
        
        let config = if let Some(cfg) = custom_config {
            cfg
        } else {
            self.config.read().await
                .get(&stream_type)
                .cloned()
                .unwrap_or_default()
        };
        
        let (sender, _) = broadcast::channel(config.buffer_size);
        
        let stream = ActiveStream {
            id: stream_id,
            stream_type: stream_type.clone(),
            sandbox_id,
            created_at: Utc::now(),
            sender,
            config,
        };
        
        self.streams.write().await.insert(stream_id, stream);
        self.message_buffer.write().await.insert(stream_id, Vec::new());
        
        info!("Created stream {} of type {:?}", stream_id, stream_type);
        Ok(stream_id)
    }

    /// Subscribe to a stream
    pub async fn subscribe(
        &self,
        stream_id: Uuid,
        connection_id: String,
        filters: Vec<StreamFilter>,
    ) -> Result<broadcast::Receiver<StreamMessage>> {
        let streams = self.streams.read().await;
        let stream = streams.get(&stream_id)
            .ok_or_else(|| SoulBoxError::not_found(format!("Stream {} not found", stream_id)))?;
        
        let receiver = stream.sender.subscribe();
        let subscriber_id = Uuid::new_v4();
        
        let subscriber = StreamSubscriber {
            id: subscriber_id,
            connection_id,
            stream_id,
            receiver: stream.sender.subscribe(),
            filters,
        };
        
        self.subscribers.write().await
            .entry(stream_id)
            .or_insert_with(Vec::new)
            .push(subscriber);
        
        debug!("Subscribed to stream {} from connection {}", stream_id, connection_id);
        Ok(receiver)
    }

    /// Publish message to stream
    pub async fn publish(&self, stream_id: Uuid, data: StreamData) -> Result<()> {
        let streams = self.streams.read().await;
        let stream = streams.get(&stream_id)
            .ok_or_else(|| SoulBoxError::not_found(format!("Stream {} not found", stream_id)))?;
        
        let message = StreamMessage {
            id: Uuid::new_v4(),
            stream_type: stream.stream_type.clone(),
            timestamp: Utc::now(),
            sandbox_id: stream.sandbox_id.clone(),
            data,
        };
        
        // Add to buffer
        let mut buffer = self.message_buffer.write().await;
        if let Some(messages) = buffer.get_mut(&stream_id) {
            messages.push(message.clone());
            
            // Trim buffer if needed
            if messages.len() > stream.config.buffer_size {
                messages.drain(0..messages.len() - stream.config.buffer_size);
            }
        }
        
        // Broadcast to subscribers
        let _ = stream.sender.send(message);
        
        Ok(())
    }

    /// Get buffered messages
    pub async fn get_buffered_messages(
        &self,
        stream_id: Uuid,
        limit: Option<usize>,
    ) -> Result<Vec<StreamMessage>> {
        let buffer = self.message_buffer.read().await;
        let messages = buffer.get(&stream_id)
            .ok_or_else(|| SoulBoxError::not_found(format!("Stream {} not found", stream_id)))?;
        
        let result = if let Some(limit) = limit {
            messages.iter()
                .rev()
                .take(limit)
                .rev()
                .cloned()
                .collect()
        } else {
            messages.clone()
        };
        
        Ok(result)
    }

    /// Apply filters to message
    fn apply_filters(message: &StreamMessage, filters: &[StreamFilter]) -> bool {
        if filters.is_empty() {
            return true;
        }
        
        for filter in filters {
            let matches = match &message.data {
                StreamData::Log { level, source, message: msg, .. } => {
                    match filter.field.as_str() {
                        "level" => Self::apply_filter_operator(&filter.operator, level, &filter.value),
                        "source" => Self::apply_filter_operator(&filter.operator, source, &filter.value),
                        "message" => Self::apply_filter_operator(&filter.operator, msg, &filter.value),
                        _ => false,
                    }
                }
                StreamData::Metric { name, value, .. } => {
                    match filter.field.as_str() {
                        "name" => Self::apply_filter_operator(&filter.operator, name, &filter.value),
                        "value" => {
                            if let (Ok(v), FilterOperator::GreaterThan | FilterOperator::LessThan) = 
                                (filter.value.parse::<f64>(), &filter.operator) {
                                match filter.operator {
                                    FilterOperator::GreaterThan => *value > v,
                                    FilterOperator::LessThan => *value < v,
                                    _ => false,
                                }
                            } else {
                                false
                            }
                        }
                        _ => false,
                    }
                }
                _ => true,
            };
            
            if !matches {
                return false;
            }
        }
        
        true
    }

    fn apply_filter_operator(operator: &FilterOperator, field_value: &str, filter_value: &str) -> bool {
        match operator {
            FilterOperator::Equals => field_value == filter_value,
            FilterOperator::Contains => field_value.contains(filter_value),
            FilterOperator::StartsWith => field_value.starts_with(filter_value),
            FilterOperator::EndsWith => field_value.ends_with(filter_value),
            FilterOperator::Regex => {
                regex::Regex::new(filter_value)
                    .map(|re| re.is_match(field_value))
                    .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Create a throttled stream
    pub async fn create_throttled_stream(
        &self,
        stream_id: Uuid,
        throttle_ms: u64,
    ) -> impl Stream<Item = StreamMessage> {
        let receiver = self.subscribe(stream_id, "throttled".to_string(), Vec::new())
            .await
            .unwrap();
        
        let interval_duration = Duration::from_millis(throttle_ms);
        
        // Use tokio_stream's throttle for simpler implementation
        tokio_stream::wrappers::BroadcastStream::new(receiver)
            .filter_map(|result| result.ok())
            .throttle(interval_duration)
    }

    /// Clean up inactive streams
    pub async fn cleanup_inactive_streams(&self, max_age: Duration) {
        let now = Utc::now();
        let mut streams = self.streams.write().await;
        let mut to_remove = Vec::new();
        
        for (id, stream) in streams.iter() {
            let age = now.signed_duration_since(stream.created_at);
            if age.num_seconds() > max_age.as_secs() as i64 {
                let subscribers = self.subscribers.read().await;
                if !subscribers.contains_key(id) || subscribers.get(id).map(|s| s.is_empty()).unwrap_or(true) {
                    to_remove.push(*id);
                }
            }
        }
        
        for id in to_remove {
            streams.remove(&id);
            self.message_buffer.write().await.remove(&id);
            self.subscribers.write().await.remove(&id);
            info!("Cleaned up inactive stream {}", id);
        }
    }

    /// Get stream statistics
    pub async fn get_statistics(&self) -> StreamStatistics {
        let streams = self.streams.read().await;
        let subscribers = self.subscribers.read().await;
        let buffer = self.message_buffer.read().await;
        
        let mut stats_by_type = HashMap::new();
        for stream in streams.values() {
            let entry = stats_by_type
                .entry(stream.stream_type.clone())
                .or_insert(TypeStatistics::default());
            entry.stream_count += 1;
            entry.subscriber_count += subscribers.get(&stream.id)
                .map(|s| s.len())
                .unwrap_or(0);
            entry.buffered_messages += buffer.get(&stream.id)
                .map(|b| b.len())
                .unwrap_or(0);
        }
        
        StreamStatistics {
            total_streams: streams.len(),
            total_subscribers: subscribers.values().map(|s| s.len()).sum(),
            total_buffered_messages: buffer.values().map(|b| b.len()).sum(),
            stats_by_type,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamStatistics {
    pub total_streams: usize,
    pub total_subscribers: usize,
    pub total_buffered_messages: usize,
    pub stats_by_type: HashMap<StreamType, TypeStatistics>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypeStatistics {
    pub stream_count: usize,
    pub subscriber_count: usize,
    pub buffered_messages: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stream_creation() {
        let manager = StreamManager::new();
        
        let stream_id = manager.create_stream(
            StreamType::Logs,
            Some("sandbox-123".to_string()),
            None,
        ).await.unwrap();
        
        assert!(!stream_id.is_nil());
    }

    #[tokio::test]
    async fn test_publish_and_subscribe() {
        let manager = StreamManager::new();
        
        let stream_id = manager.create_stream(StreamType::Logs, None, None).await.unwrap();
        let mut receiver = manager.subscribe(stream_id, "test".to_string(), Vec::new()).await.unwrap();
        
        let data = StreamData::Log {
            level: "info".to_string(),
            source: "test".to_string(),
            message: "Test message".to_string(),
            metadata: HashMap::new(),
        };
        
        manager.publish(stream_id, data).await.unwrap();
        
        let msg = receiver.recv().await.unwrap();
        assert_eq!(msg.stream_type, StreamType::Logs);
    }

    #[tokio::test]
    async fn test_filters() {
        let filter = StreamFilter {
            field: "level".to_string(),
            operator: FilterOperator::Equals,
            value: "error".to_string(),
        };
        
        let message = StreamMessage {
            id: Uuid::new_v4(),
            stream_type: StreamType::Logs,
            timestamp: Utc::now(),
            sandbox_id: None,
            data: StreamData::Log {
                level: "error".to_string(),
                source: "test".to_string(),
                message: "Error message".to_string(),
                metadata: HashMap::new(),
            },
        };
        
        assert!(StreamManager::apply_filters(&message, &[filter]));
    }
}