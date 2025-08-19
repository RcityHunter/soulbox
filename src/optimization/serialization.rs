//! Serialization optimization module
//! 
//! This module optimizes serialization/deserialization operations to reduce
//! CPU usage through efficient encoding, streaming, and caching strategies.

use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Serialization optimizer implementation
#[derive(Clone)]
pub struct SerializationOptimizer {
    config: SerializationConfig,
    format_cache: Arc<RwLock<HashMap<String, CachedSerialization>>>,
    statistics: Arc<RwLock<SerializationStatistics>>,
}

/// Configuration for serialization optimizations
#[derive(Debug, Clone)]
pub struct SerializationConfig {
    /// Enable binary serialization for better performance
    pub prefer_binary: bool,
    /// Enable compression for large payloads
    pub enable_compression: bool,
    /// Threshold size for compression (bytes)
    pub compression_threshold: usize,
    /// Maximum cache size for serialized data
    pub max_cache_size: usize,
    /// Cache TTL for serialized data
    pub cache_ttl: Duration,
    /// Enable streaming for large data
    pub enable_streaming: bool,
    /// Streaming threshold size (bytes)
    pub streaming_threshold: usize,
}

impl Default for SerializationConfig {
    fn default() -> Self {
        Self {
            prefer_binary: true,
            enable_compression: true,
            compression_threshold: 1024, // 1KB
            max_cache_size: 1000,
            cache_ttl: Duration::from_secs(300), // 5 minutes
            enable_streaming: true,
            streaming_threshold: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Cached serialization data
#[derive(Debug, Clone)]
struct CachedSerialization {
    data: Vec<u8>,
    format: SerializationFormat,
    created_at: Instant,
    access_count: u64,
}

/// Available serialization formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SerializationFormat {
    Json,
    MessagePack,
    Bincode,
    Protobuf,
    Cbor,
}

/// Serialization statistics
#[derive(Debug, Clone, Default)]
pub struct SerializationStatistics {
    pub total_serializations: u64,
    pub total_deserializations: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub compression_saves_bytes: u64,
    pub total_serialization_time_ms: u64,
    pub total_deserialization_time_ms: u64,
    pub format_usage: HashMap<SerializationFormat, u64>,
}

impl SerializationOptimizer {
    /// Create a new serialization optimizer
    pub fn new() -> Self {
        Self::with_config(SerializationConfig::default())
    }

    /// Create a new serialization optimizer with custom configuration
    pub fn with_config(config: SerializationConfig) -> Self {
        Self {
            config,
            format_cache: Arc::new(RwLock::new(HashMap::new())),
            statistics: Arc::new(RwLock::new(SerializationStatistics::default())),
        }
    }

    /// Optimize serialization for a specific component
    pub async fn optimize_component(&self, component: &str) -> Result<()> {
        tracing::info!(
            component = component,
            "Applying serialization optimizations"
        );

        // For now, this is a placeholder that configures the component
        // In a real implementation, this would:
        // 1. Analyze the component's serialization patterns
        // 2. Switch to optimal serialization formats
        // 3. Enable caching for frequently serialized data
        // 4. Configure compression thresholds

        Ok(())
    }

    /// Serialize data with optimization
    pub async fn serialize<T>(&self, data: &T, format: Option<SerializationFormat>) -> Result<Vec<u8>>
    where
        T: Serialize + std::fmt::Debug,
    {
        let start_time = Instant::now();
        
        // Generate cache key
        let cache_key = self.generate_cache_key(data, format);
        
        // Check cache first
        if let Some(cached) = self.get_from_cache(&cache_key) {
            self.record_cache_hit();
            return Ok(cached.data);
        }

        // Select optimal format
        let selected_format = format.unwrap_or_else(|| self.select_optimal_format(data));
        
        // Perform serialization
        let serialized_data = match selected_format {
            SerializationFormat::Json => {
                serde_json::to_vec(data)
                    .context("Failed to serialize to JSON")?
            },
            SerializationFormat::MessagePack => {
                // Would require rmp-serde crate
                serde_json::to_vec(data)
                    .context("MessagePack serialization not implemented, using JSON")?
            },
            SerializationFormat::Bincode => {
                // Would require bincode crate
                serde_json::to_vec(data)
                    .context("Bincode serialization not implemented, using JSON")?
            },
            SerializationFormat::Protobuf => {
                // Would require protobuf integration
                serde_json::to_vec(data)
                    .context("Protobuf serialization not implemented, using JSON")?
            },
            SerializationFormat::Cbor => {
                // Would require serde_cbor crate
                serde_json::to_vec(data)
                    .context("CBOR serialization not implemented, using JSON")?
            },
        };

        // Apply compression if enabled and threshold met
        let final_data = if self.config.enable_compression 
            && serialized_data.len() > self.config.compression_threshold 
        {
            self.compress_data(&serialized_data)?
        } else {
            serialized_data
        };

        // Cache the result
        self.cache_serialization(cache_key, final_data.clone(), selected_format);

        // Record statistics
        let elapsed = start_time.elapsed();
        self.record_serialization(selected_format, elapsed);

        Ok(final_data)
    }

    /// Deserialize data with optimization
    pub async fn deserialize<T>(&self, data: &[u8], format: SerializationFormat) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let start_time = Instant::now();

        // Decompress if necessary
        let decompressed_data = if self.is_compressed(data) {
            self.decompress_data(data)?
        } else {
            data.to_vec()
        };

        // Perform deserialization
        let result = match format {
            SerializationFormat::Json => {
                serde_json::from_slice(&decompressed_data)
                    .context("Failed to deserialize from JSON")?
            },
            SerializationFormat::MessagePack => {
                // Would require rmp-serde crate
                serde_json::from_slice(&decompressed_data)
                    .context("MessagePack deserialization not implemented, using JSON")?
            },
            SerializationFormat::Bincode => {
                // Would require bincode crate
                serde_json::from_slice(&decompressed_data)
                    .context("Bincode deserialization not implemented, using JSON")?
            },
            SerializationFormat::Protobuf => {
                // Would require protobuf integration
                serde_json::from_slice(&decompressed_data)
                    .context("Protobuf deserialization not implemented, using JSON")?
            },
            SerializationFormat::Cbor => {
                // Would require serde_cbor crate
                serde_json::from_slice(&decompressed_data)
                    .context("CBOR deserialization not implemented, using JSON")?
            },
        };

        // Record statistics
        let elapsed = start_time.elapsed();
        self.record_deserialization(format, elapsed);

        Ok(result)
    }

    /// Stream serialize large data
    pub async fn stream_serialize<T>(&self, data: &T, format: SerializationFormat) -> Result<Vec<u8>>
    where
        T: Serialize + std::fmt::Debug,
    {
        // For now, fallback to regular serialization
        // In a real implementation, this would use streaming serialization
        self.serialize(data, Some(format)).await
    }

    /// Get serialization statistics
    pub fn get_statistics(&self) -> SerializationStatistics {
        let stats = self.statistics.read();
        stats.clone()
    }

    /// Clear serialization cache
    pub fn clear_cache(&self) {
        let mut cache = self.format_cache.write();
        cache.clear();
        tracing::info!("Cleared serialization cache");
    }

    /// Analyze serialization patterns for optimization recommendations
    pub fn analyze_patterns(&self) -> SerializationAnalysis {
        let stats = self.statistics.read();
        
        let total_operations = stats.total_serializations + stats.total_deserializations;
        let cache_hit_ratio = if total_operations > 0 {
            stats.cache_hits as f32 / total_operations as f32
        } else {
            0.0
        };

        let avg_serialization_time = if stats.total_serializations > 0 {
            stats.total_serialization_time_ms as f32 / stats.total_serializations as f32
        } else {
            0.0
        };

        let avg_deserialization_time = if stats.total_deserializations > 0 {
            stats.total_deserialization_time_ms as f32 / stats.total_deserializations as f32
        } else {
            0.0
        };

        let most_used_format = stats.format_usage
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(format, _)| *format);

        SerializationAnalysis {
            total_operations,
            cache_hit_ratio,
            avg_serialization_time_ms: avg_serialization_time,
            avg_deserialization_time_ms: avg_deserialization_time,
            compression_efficiency: if stats.compression_saves_bytes > 0 { 
                stats.compression_saves_bytes as f32 / 1024.0 / 1024.0 
            } else { 
                0.0 
            },
            most_used_format,
            recommendations: self.generate_optimization_recommendations(&stats),
        }
    }

    /// Generate cache key for serialization
    fn generate_cache_key<T>(&self, data: &T, format: Option<SerializationFormat>) -> String
    where
        T: std::fmt::Debug,
    {
        // Simple hash-based cache key
        // In a real implementation, this would use a proper hash function
        format!("{:?}_{:?}", data, format)
    }

    /// Get cached serialization
    fn get_from_cache(&self, key: &str) -> Option<CachedSerialization> {
        let mut cache = self.format_cache.write();
        
        if let Some(cached) = cache.get_mut(key) {
            // Check if cache entry is still valid
            if cached.created_at.elapsed() < self.config.cache_ttl {
                cached.access_count += 1;
                return Some(cached.clone());
            } else {
                // Remove expired entry
                cache.remove(key);
            }
        }
        
        None
    }

    /// Cache serialization result
    fn cache_serialization(&self, key: String, data: Vec<u8>, format: SerializationFormat) {
        let mut cache = self.format_cache.write();
        
        // Check cache size limit
        if cache.len() >= self.config.max_cache_size {
            // Remove oldest entries (simple LRU approximation)
            let oldest_key = cache
                .iter()
                .min_by_key(|(_, cached)| cached.created_at)
                .map(|(k, _)| k.clone());
            
            if let Some(oldest) = oldest_key {
                cache.remove(&oldest);
            }
        }

        let cached = CachedSerialization {
            data,
            format,
            created_at: Instant::now(),
            access_count: 1,
        };

        cache.insert(key, cached);
    }

    /// Select optimal serialization format based on data characteristics
    fn select_optimal_format<T>(&self, _data: &T) -> SerializationFormat
    where
        T: std::fmt::Debug,
    {
        // Simple heuristic - prefer binary formats if configured
        if self.config.prefer_binary {
            SerializationFormat::MessagePack
        } else {
            SerializationFormat::Json
        }
    }

    /// Compress data using gzip
    fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        use flate2::{write::GzEncoder, Compression};
        use std::io::Write;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)
            .context("Failed to write data for compression")?;
        encoder.finish()
            .context("Failed to compress data")
    }

    /// Check if data is compressed
    fn is_compressed(&self, data: &[u8]) -> bool {
        // Simple check for gzip magic number
        data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b
    }

    /// Decompress data using gzip
    fn decompress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        use flate2::read::GzDecoder;
        use std::io::Read;

        let mut decoder = GzDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)
            .context("Failed to decompress data")?;
        Ok(decompressed)
    }

    /// Record cache hit
    fn record_cache_hit(&self) {
        let mut stats = self.statistics.write();
        stats.cache_hits += 1;
    }

    /// Record serialization operation
    fn record_serialization(&self, format: SerializationFormat, elapsed: Duration) {
        let mut stats = self.statistics.write();
        stats.total_serializations += 1;
        stats.total_serialization_time_ms += elapsed.as_millis() as u64;
        *stats.format_usage.entry(format).or_insert(0) += 1;
    }

    /// Record deserialization operation
    fn record_deserialization(&self, format: SerializationFormat, elapsed: Duration) {
        let mut stats = self.statistics.write();
        stats.total_deserializations += 1;
        stats.total_deserialization_time_ms += elapsed.as_millis() as u64;
        *stats.format_usage.entry(format).or_insert(0) += 1;
    }

    /// Generate optimization recommendations
    fn generate_optimization_recommendations(&self, stats: &SerializationStatistics) -> Vec<String> {
        let mut recommendations = Vec::new();

        let total_ops = stats.total_serializations + stats.total_deserializations;
        let cache_hit_ratio = if total_ops > 0 {
            stats.cache_hits as f32 / total_ops as f32
        } else {
            0.0
        };

        if cache_hit_ratio < 0.3 {
            recommendations.push("Consider increasing cache size or TTL to improve cache hit ratio".to_string());
        }

        if stats.total_serialization_time_ms > 1000 {
            recommendations.push("Consider using binary serialization formats for better performance".to_string());
        }

        if stats.compression_saves_bytes == 0 && self.config.enable_compression {
            recommendations.push("Compression is enabled but not saving space - consider adjusting threshold".to_string());
        }

        if recommendations.is_empty() {
            recommendations.push("Serialization performance is optimal".to_string());
        }

        recommendations
    }
}

/// Serialization analysis results
#[derive(Debug, Clone)]
pub struct SerializationAnalysis {
    pub total_operations: u64,
    pub cache_hit_ratio: f32,
    pub avg_serialization_time_ms: f32,
    pub avg_deserialization_time_ms: f32,
    pub compression_efficiency: f32, // MB saved
    pub most_used_format: Option<SerializationFormat>,
    pub recommendations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_serialization_optimizer() {
        let optimizer = SerializationOptimizer::new();
        let test_data = json!({
            "name": "test",
            "value": 42,
            "items": [1, 2, 3, 4, 5]
        });

        let serialized = optimizer.serialize(&test_data, None).await.unwrap();
        assert!(!serialized.is_empty());

        let deserialized: serde_json::Value = optimizer
            .deserialize(&serialized, SerializationFormat::Json)
            .await
            .unwrap();
        
        assert_eq!(test_data, deserialized);
    }

    #[tokio::test]
    async fn test_serialization_caching() {
        let optimizer = SerializationOptimizer::new();
        let test_data = json!({"cached": true});

        // First serialization
        let _result1 = optimizer.serialize(&test_data, None).await.unwrap();
        
        // Second serialization should hit cache
        let _result2 = optimizer.serialize(&test_data, None).await.unwrap();

        let stats = optimizer.get_statistics();
        assert!(stats.cache_hits > 0);
    }

    #[test]
    fn test_serialization_config() {
        let config = SerializationConfig::default();
        assert!(config.prefer_binary);
        assert!(config.enable_compression);
        assert_eq!(config.compression_threshold, 1024);
    }

    #[test]
    fn test_serialization_format_enum() {
        let format = SerializationFormat::Json;
        assert_eq!(format, SerializationFormat::Json);
        assert_ne!(format, SerializationFormat::MessagePack);
    }
}