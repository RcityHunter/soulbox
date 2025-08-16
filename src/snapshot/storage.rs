//! Snapshot storage backend
//! 
//! This module handles the storage and retrieval of snapshot data,
//! including compression, integrity verification, and retention policies.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

use super::SnapshotMetadata;

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_snapshots: usize,
    pub total_size_bytes: u64,
    pub compressed_size_bytes: u64,
    pub average_compression_ratio: f32,
    pub oldest_snapshot: Option<DateTime<Utc>>,
    pub newest_snapshot: Option<DateTime<Utc>>,
    pub storage_efficiency: f32,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub base_path: PathBuf,
    pub max_size_gb: Option<u64>,
    pub compression_level: u32,
    pub checksum_verification: bool,
    pub auto_cleanup_enabled: bool,
    pub retention_days: u32,
    pub max_snapshots: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            base_path: PathBuf::from("./snapshots"),
            max_size_gb: Some(10), // 10GB default limit
            compression_level: 6,   // Balanced compression
            checksum_verification: true,
            auto_cleanup_enabled: true,
            retention_days: 30,
            max_snapshots: 100,
        }
    }
}

/// Snapshot storage implementation
pub struct SnapshotStorage {
    config: StorageConfig,
    metadata_cache: parking_lot::RwLock<HashMap<Uuid, SnapshotMetadata>>,
}

impl SnapshotStorage {
    /// Create a new snapshot storage instance
    pub fn new(base_path: PathBuf) -> Result<Self> {
        let config = StorageConfig {
            base_path: base_path.clone(),
            ..Default::default()
        };

        // Create necessary directories
        std::fs::create_dir_all(&base_path)
            .context("Failed to create snapshot storage directory")?;
        
        std::fs::create_dir_all(base_path.join("data"))
            .context("Failed to create snapshot data directory")?;
        
        std::fs::create_dir_all(base_path.join("metadata"))
            .context("Failed to create snapshot metadata directory")?;

        let storage = Self {
            config,
            metadata_cache: parking_lot::RwLock::new(HashMap::new()),
        };

        Ok(storage)
    }

    /// Create a new snapshot storage instance with custom configuration
    pub fn with_config(config: StorageConfig) -> Result<Self> {
        // Create necessary directories
        std::fs::create_dir_all(&config.base_path)
            .context("Failed to create snapshot storage directory")?;
        
        std::fs::create_dir_all(config.base_path.join("data"))
            .context("Failed to create snapshot data directory")?;
        
        std::fs::create_dir_all(config.base_path.join("metadata"))
            .context("Failed to create snapshot metadata directory")?;

        let storage = Self {
            config,
            metadata_cache: parking_lot::RwLock::new(HashMap::new()),
        };

        // Load existing metadata into cache
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(storage.load_metadata_cache())
        })?;

        Ok(storage)
    }

    /// Store a snapshot with its metadata and data
    pub async fn store_snapshot(
        &self,
        metadata: SnapshotMetadata,
        data: Vec<u8>,
        compress: bool,
    ) -> Result<()> {
        let snapshot_id = metadata.id;
        
        tracing::info!(
            snapshot_id = %snapshot_id,
            size_bytes = data.len(),
            compress = compress,
            "Storing snapshot"
        );

        // Check storage limits before storing
        self.check_storage_limits(&data).await?;

        // Prepare data (compress if requested)
        let (final_data, actual_compression_ratio) = if compress {
            let compressed = self.compress_data(&data)?;
            let ratio = compressed.len() as f32 / data.len() as f32;
            (compressed, Some(ratio))
        } else {
            (data, None)
        };

        // Update metadata with actual compression ratio
        let mut final_metadata = metadata;
        final_metadata.compression_ratio = actual_compression_ratio;
        final_metadata.size_bytes = final_data.len() as u64;

        // Store data file
        let data_path = self.get_data_path(snapshot_id);
        fs::write(&data_path, &final_data).await
            .context("Failed to write snapshot data")?;

        // Store metadata file
        let metadata_path = self.get_metadata_path(snapshot_id);
        let metadata_json = serde_json::to_string_pretty(&final_metadata)
            .context("Failed to serialize metadata")?;
        fs::write(&metadata_path, metadata_json).await
            .context("Failed to write snapshot metadata")?;

        // Update cache
        {
            let mut cache = self.metadata_cache.write();
            cache.insert(snapshot_id, final_metadata);
        }

        // Perform auto-cleanup if enabled
        if self.config.auto_cleanup_enabled {
            self.auto_cleanup().await?;
        }

        tracing::info!(
            snapshot_id = %snapshot_id,
            final_size_bytes = final_data.len(),
            compression_ratio = ?actual_compression_ratio,
            "Snapshot stored successfully"
        );

        Ok(())
    }

    /// Load a snapshot's metadata and data
    pub async fn load_snapshot(&self, snapshot_id: Uuid) -> Result<(SnapshotMetadata, Vec<u8>)> {
        tracing::info!(snapshot_id = %snapshot_id, "Loading snapshot");

        // Load metadata
        let metadata = self.get_snapshot_metadata(snapshot_id).await?;

        // Load data
        let data_path = self.get_data_path(snapshot_id);
        let compressed_data = fs::read(&data_path).await
            .context("Failed to read snapshot data")?;

        // Decompress if necessary
        let data = if metadata.compression_ratio.is_some() {
            self.decompress_data(&compressed_data)?
        } else {
            compressed_data
        };

        tracing::info!(
            snapshot_id = %snapshot_id,
            size_bytes = data.len(),
            "Snapshot loaded successfully"
        );

        Ok((metadata, data))
    }

    /// Get metadata for a specific snapshot
    pub async fn get_snapshot_metadata(&self, snapshot_id: Uuid) -> Result<SnapshotMetadata> {
        // Try cache first
        {
            let cache = self.metadata_cache.read();
            if let Some(metadata) = cache.get(&snapshot_id) {
                return Ok(metadata.clone());
            }
        }

        // Load from disk
        let metadata_path = self.get_metadata_path(snapshot_id);
        let metadata_json = fs::read_to_string(&metadata_path).await
            .context("Failed to read snapshot metadata")?;
        
        let metadata: SnapshotMetadata = serde_json::from_str(&metadata_json)
            .context("Failed to parse snapshot metadata")?;

        // Update cache
        {
            let mut cache = self.metadata_cache.write();
            cache.insert(snapshot_id, metadata.clone());
        }

        Ok(metadata)
    }

    /// List all available snapshots
    pub async fn list_snapshots(&self) -> Result<Vec<SnapshotMetadata>> {
        let metadata_dir = self.config.base_path.join("metadata");
        let mut snapshots = Vec::new();

        let mut entries = fs::read_dir(&metadata_dir).await
            .context("Failed to read metadata directory")?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(snapshot_id) = Uuid::parse_str(filename) {
                        match self.get_snapshot_metadata(snapshot_id).await {
                            Ok(metadata) => snapshots.push(metadata),
                            Err(e) => {
                                tracing::warn!(
                                    snapshot_id = %snapshot_id,
                                    error = %e,
                                    "Failed to load snapshot metadata"
                                );
                            }
                        }
                    }
                }
            }
        }

        // Sort by creation time (newest first)
        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(snapshots)
    }

    /// Delete a snapshot
    pub async fn delete_snapshot(&self, snapshot_id: Uuid) -> Result<()> {
        tracing::info!(snapshot_id = %snapshot_id, "Deleting snapshot");

        let data_path = self.get_data_path(snapshot_id);
        let metadata_path = self.get_metadata_path(snapshot_id);

        // Remove files
        if data_path.exists() {
            fs::remove_file(&data_path).await
                .context("Failed to delete snapshot data")?;
        }

        if metadata_path.exists() {
            fs::remove_file(&metadata_path).await
                .context("Failed to delete snapshot metadata")?;
        }

        // Remove from cache
        {
            let mut cache = self.metadata_cache.write();
            cache.remove(&snapshot_id);
        }

        tracing::info!(snapshot_id = %snapshot_id, "Snapshot deleted successfully");
        Ok(())
    }

    /// Clean up old snapshots based on retention policy
    pub async fn cleanup_old_snapshots(&self, max_age_days: u32, max_count: usize) -> Result<usize> {
        let snapshots = self.list_snapshots().await?;
        let cutoff_time = Utc::now() - Duration::days(max_age_days as i64);
        
        let mut deleted_count = 0;
        let mut snapshots_to_delete = Vec::new();

        // Mark snapshots for deletion based on age
        for snapshot in &snapshots {
            if snapshot.created_at < cutoff_time {
                snapshots_to_delete.push(snapshot.id);
            }
        }

        // Mark excess snapshots for deletion based on count
        if snapshots.len() > max_count {
            let excess_count = snapshots.len() - max_count;
            for snapshot in snapshots.iter().skip(max_count).take(excess_count) {
                if !snapshots_to_delete.contains(&snapshot.id) {
                    snapshots_to_delete.push(snapshot.id);
                }
            }
        }

        // Delete marked snapshots
        for snapshot_id in snapshots_to_delete {
            match self.delete_snapshot(snapshot_id).await {
                Ok(_) => {
                    deleted_count += 1;
                    tracing::info!(snapshot_id = %snapshot_id, "Cleaned up old snapshot");
                },
                Err(e) => {
                    tracing::error!(
                        snapshot_id = %snapshot_id,
                        error = %e,
                        "Failed to delete old snapshot during cleanup"
                    );
                }
            }
        }

        tracing::info!(deleted_count = deleted_count, "Cleanup completed");
        Ok(deleted_count)
    }

    /// Get storage statistics
    pub async fn get_storage_stats(&self) -> Result<StorageStats> {
        let snapshots = self.list_snapshots().await?;
        
        if snapshots.is_empty() {
            return Ok(StorageStats {
                total_snapshots: 0,
                total_size_bytes: 0,
                compressed_size_bytes: 0,
                average_compression_ratio: 0.0,
                oldest_snapshot: None,
                newest_snapshot: None,
                storage_efficiency: 0.0,
            });
        }

        let total_snapshots = snapshots.len();
        let total_size_bytes: u64 = snapshots.iter().map(|s| s.size_bytes).sum();
        
        let mut compressed_count = 0;
        let mut compression_ratio_sum = 0.0;
        
        for snapshot in &snapshots {
            if let Some(ratio) = snapshot.compression_ratio {
                compressed_count += 1;
                compression_ratio_sum += ratio;
            }
        }

        let average_compression_ratio = if compressed_count > 0 {
            compression_ratio_sum / compressed_count as f32
        } else {
            1.0
        };

        let compressed_size_bytes = (total_size_bytes as f32 * average_compression_ratio) as u64;

        let oldest_snapshot = snapshots.iter().map(|s| s.created_at).min();
        let newest_snapshot = snapshots.iter().map(|s| s.created_at).max();

        let storage_efficiency = if total_size_bytes > 0 {
            1.0 - (compressed_size_bytes as f32 / total_size_bytes as f32)
        } else {
            0.0
        };

        Ok(StorageStats {
            total_snapshots,
            total_size_bytes,
            compressed_size_bytes,
            average_compression_ratio,
            oldest_snapshot,
            newest_snapshot,
            storage_efficiency,
        })
    }

    /// Load metadata cache from disk
    async fn load_metadata_cache(&self) -> Result<()> {
        let snapshots = self.list_snapshots().await?;
        
        let mut cache = self.metadata_cache.write();
        for snapshot in snapshots {
            cache.insert(snapshot.id, snapshot);
        }

        tracing::info!(cached_snapshots = cache.len(), "Metadata cache loaded");
        Ok(())
    }

    /// Compress data using gzip
    fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::new(self.config.compression_level));
        encoder.write_all(data).context("Failed to compress data")?;
        encoder.finish().context("Failed to finalize compression")
    }

    /// Decompress data using gzip
    fn decompress_data(&self, compressed_data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = GzDecoder::new(compressed_data);
        let mut data = Vec::new();
        decoder.read_to_end(&mut data).context("Failed to decompress data")?;
        Ok(data)
    }

    /// Get the file path for snapshot data
    fn get_data_path(&self, snapshot_id: Uuid) -> PathBuf {
        self.config.base_path.join("data").join(format!("{}.dat", snapshot_id))
    }

    /// Get the file path for snapshot metadata
    fn get_metadata_path(&self, snapshot_id: Uuid) -> PathBuf {
        self.config.base_path.join("metadata").join(format!("{}.json", snapshot_id))
    }

    /// Check storage limits before storing new data
    async fn check_storage_limits(&self, new_data: &[u8]) -> Result<()> {
        if let Some(max_size_gb) = self.config.max_size_gb {
            let stats = self.get_storage_stats().await?;
            let max_size_bytes = max_size_gb * 1024 * 1024 * 1024;
            let projected_size = stats.total_size_bytes + new_data.len() as u64;

            if projected_size > max_size_bytes {
                return Err(anyhow::anyhow!(
                    "Storage limit exceeded: {} bytes would exceed limit of {} bytes",
                    projected_size,
                    max_size_bytes
                ));
            }
        }

        Ok(())
    }

    /// Perform automatic cleanup based on configuration
    async fn auto_cleanup(&self) -> Result<()> {
        if self.config.auto_cleanup_enabled {
            let deleted = self.cleanup_old_snapshots(
                self.config.retention_days,
                self.config.max_snapshots,
            ).await?;

            if deleted > 0 {
                tracing::info!(deleted_snapshots = deleted, "Auto-cleanup completed");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_storage_creation() {
        let temp_dir = tempdir().unwrap();
        let storage = SnapshotStorage::new(temp_dir.path().to_path_buf()).unwrap();
        
        assert_eq!(storage.config.base_path, temp_dir.path());
        assert!(temp_dir.path().join("data").exists());
        assert!(temp_dir.path().join("metadata").exists());
    }

    #[tokio::test]
    async fn test_compression() {
        let temp_dir = tempdir().unwrap();
        let storage = SnapshotStorage::new(temp_dir.path().to_path_buf()).unwrap();
        
        let test_data = b"Hello, World! This is test data for compression.".repeat(100);
        let compressed = storage.compress_data(&test_data).unwrap();
        let decompressed = storage.decompress_data(&compressed).unwrap();
        
        assert_eq!(test_data, decompressed);
        assert!(compressed.len() < test_data.len());
    }

    #[tokio::test]
    async fn test_storage_stats_empty() {
        let temp_dir = tempdir().unwrap();
        let storage = SnapshotStorage::new(temp_dir.path().to_path_buf()).unwrap();
        
        let stats = storage.get_storage_stats().await.unwrap();
        assert_eq!(stats.total_snapshots, 0);
        assert_eq!(stats.total_size_bytes, 0);
    }

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.base_path, PathBuf::from("./snapshots"));
        assert_eq!(config.max_size_gb, Some(10));
        assert_eq!(config.compression_level, 6);
        assert!(config.checksum_verification);
        assert!(config.auto_cleanup_enabled);
        assert_eq!(config.retention_days, 30);
        assert_eq!(config.max_snapshots, 100);
    }
}