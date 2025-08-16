//! Snapshot and checkpoint management system
//! 
//! This module provides comprehensive snapshot functionality for containers,
//! including container state, filesystem, and memory preservation with fast
//! restoration capabilities.

pub mod checkpoint;
pub mod storage;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::container::manager::ContainerManager;
use crate::filesystem::manager::FilesystemManager;

/// Represents different types of snapshots that can be created
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SnapshotType {
    /// Full system snapshot including container state, filesystem, and memory
    Full,
    /// Container state only (configuration, environment, process state)
    ContainerOnly,
    /// Filesystem state only
    FilesystemOnly,
    /// Memory dump only
    MemoryOnly,
    /// Incremental snapshot based on previous snapshot
    Incremental { base_snapshot_id: Uuid },
}

/// Metadata for a snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub snapshot_type: SnapshotType,
    pub container_id: String,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub compression_ratio: Option<f32>,
    pub tags: HashMap<String, String>,
    pub checksum: String,
}

/// Current status of a snapshot operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SnapshotStatus {
    Creating,
    Completed,
    Failed(String),
    Restoring,
    Restored,
    Deleted,
}

/// Configuration for snapshot creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    pub name: String,
    pub description: Option<String>,
    pub snapshot_type: SnapshotType,
    pub compress: bool,
    pub include_memory: bool,
    pub include_filesystem: bool,
    pub tags: HashMap<String, String>,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            name: format!("snapshot_{}", Uuid::new_v4()),
            description: None,
            snapshot_type: SnapshotType::Full,
            compress: true,
            include_memory: true,
            include_filesystem: true,
            tags: HashMap::new(),
        }
    }
}

/// Main snapshot manager responsible for coordinating snapshot operations
pub struct SnapshotManager {
    storage: storage::SnapshotStorage,
    checkpoint_manager: checkpoint::CheckpointManager,
    active_operations: RwLock<HashMap<Uuid, SnapshotStatus>>,
    container_manager: ContainerManager,
    filesystem_manager: FilesystemManager,
}

impl SnapshotManager {
    /// Create a new snapshot manager
    pub fn new(
        storage_path: PathBuf,
        container_manager: ContainerManager,
        filesystem_manager: FilesystemManager,
    ) -> Result<Self> {
        let storage = storage::SnapshotStorage::new(storage_path)
            .context("Failed to initialize snapshot storage")?;
        
        let checkpoint_manager = checkpoint::CheckpointManager::new()?;

        Ok(Self {
            storage,
            checkpoint_manager,
            active_operations: RwLock::new(HashMap::new()),
            container_manager,
            filesystem_manager,
        })
    }

    /// Create a new snapshot of the specified container
    pub async fn create_snapshot(
        &self,
        container_id: &str,
        config: SnapshotConfig,
    ) -> Result<Uuid> {
        let snapshot_id = Uuid::new_v4();
        
        // Mark operation as in progress
        {
            let mut operations = self.active_operations.write().await;
            operations.insert(snapshot_id, SnapshotStatus::Creating);
        }

        // Execute snapshot creation
        let result = self.execute_snapshot_creation(snapshot_id, container_id, config).await;

        // Update operation status
        {
            let mut operations = self.active_operations.write().await;
            match &result {
                Ok(_) => {
                    operations.insert(snapshot_id, SnapshotStatus::Completed);
                },
                Err(e) => {
                    operations.insert(snapshot_id, SnapshotStatus::Failed(e.to_string()));
                }
            }
        }

        result.map(|_| snapshot_id)
    }

    /// Execute the actual snapshot creation process
    async fn execute_snapshot_creation(
        &self,
        snapshot_id: Uuid,
        container_id: &str,
        config: SnapshotConfig,
    ) -> Result<()> {
        tracing::info!(
            snapshot_id = %snapshot_id,
            container_id = container_id,
            snapshot_type = ?config.snapshot_type,
            "Creating snapshot"
        );

        // Create checkpoint based on snapshot type
        let checkpoint_data = match config.snapshot_type {
            SnapshotType::Full => {
                self.checkpoint_manager.create_full_checkpoint(
                    container_id,
                    config.include_memory,
                    config.include_filesystem,
                ).await?
            },
            SnapshotType::ContainerOnly => {
                self.checkpoint_manager.create_container_checkpoint(container_id).await?
            },
            SnapshotType::FilesystemOnly => {
                self.checkpoint_manager.create_filesystem_checkpoint(container_id).await?
            },
            SnapshotType::MemoryOnly => {
                self.checkpoint_manager.create_memory_checkpoint(container_id).await?
            },
            SnapshotType::Incremental { base_snapshot_id } => {
                self.checkpoint_manager.create_incremental_checkpoint(
                    container_id,
                    base_snapshot_id,
                ).await?
            },
        };

        // Create metadata
        let metadata = SnapshotMetadata {
            id: snapshot_id,
            name: config.name,
            description: config.description,
            snapshot_type: config.snapshot_type,
            container_id: container_id.to_string(),
            created_at: Utc::now(),
            size_bytes: checkpoint_data.len() as u64,
            compression_ratio: if config.compress { Some(0.7) } else { None },
            tags: config.tags,
            checksum: self.calculate_checksum(&checkpoint_data),
        };

        // Store snapshot
        self.storage.store_snapshot(metadata, checkpoint_data, config.compress).await
            .context("Failed to store snapshot")?;

        tracing::info!(
            snapshot_id = %snapshot_id,
            size_bytes = metadata.size_bytes,
            "Snapshot created successfully"
        );

        Ok(())
    }

    /// Restore a container from a snapshot
    pub async fn restore_snapshot(
        &self,
        snapshot_id: Uuid,
        target_container_id: Option<String>,
    ) -> Result<String> {
        // Mark operation as in progress
        {
            let mut operations = self.active_operations.write().await;
            operations.insert(snapshot_id, SnapshotStatus::Restoring);
        }

        let result = self.execute_snapshot_restoration(snapshot_id, target_container_id).await;

        // Update operation status
        {
            let mut operations = self.active_operations.write().await;
            match &result {
                Ok(_) => {
                    operations.insert(snapshot_id, SnapshotStatus::Restored);
                },
                Err(e) => {
                    operations.insert(snapshot_id, SnapshotStatus::Failed(e.to_string()));
                }
            }
        }

        result
    }

    /// Execute the actual snapshot restoration process
    async fn execute_snapshot_restoration(
        &self,
        snapshot_id: Uuid,
        target_container_id: Option<String>,
    ) -> Result<String> {
        tracing::info!(
            snapshot_id = %snapshot_id,
            target_container_id = ?target_container_id,
            "Restoring snapshot"
        );

        // Load snapshot data
        let (metadata, checkpoint_data) = self.storage.load_snapshot(snapshot_id).await
            .context("Failed to load snapshot")?;

        // Verify checksum
        let calculated_checksum = self.calculate_checksum(&checkpoint_data);
        if calculated_checksum != metadata.checksum {
            return Err(anyhow::anyhow!("Snapshot checksum verification failed"));
        }

        // Determine target container ID
        let container_id = target_container_id.unwrap_or_else(|| {
            format!("{}_restored_{}", metadata.container_id, Uuid::new_v4())
        });

        // Restore from checkpoint
        self.checkpoint_manager.restore_from_checkpoint(
            &container_id,
            checkpoint_data,
            metadata.snapshot_type,
        ).await.context("Failed to restore from checkpoint")?;

        tracing::info!(
            snapshot_id = %snapshot_id,
            container_id = container_id,
            "Snapshot restored successfully"
        );

        Ok(container_id)
    }

    /// List all available snapshots
    pub async fn list_snapshots(&self) -> Result<Vec<SnapshotMetadata>> {
        self.storage.list_snapshots().await
    }

    /// Get metadata for a specific snapshot
    pub async fn get_snapshot_metadata(&self, snapshot_id: Uuid) -> Result<SnapshotMetadata> {
        self.storage.get_snapshot_metadata(snapshot_id).await
    }

    /// Delete a snapshot
    pub async fn delete_snapshot(&self, snapshot_id: Uuid) -> Result<()> {
        self.storage.delete_snapshot(snapshot_id).await
            .context("Failed to delete snapshot")?;

        // Update operation status
        {
            let mut operations = self.active_operations.write().await;
            operations.insert(snapshot_id, SnapshotStatus::Deleted);
        }

        tracing::info!(snapshot_id = %snapshot_id, "Snapshot deleted");
        Ok(())
    }

    /// Get the status of an ongoing operation
    pub async fn get_operation_status(&self, snapshot_id: Uuid) -> Option<SnapshotStatus> {
        let operations = self.active_operations.read().await;
        operations.get(&snapshot_id).cloned()
    }

    /// Calculate checksum for data integrity verification
    fn calculate_checksum(&self, data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Clean up old snapshots based on retention policy
    pub async fn cleanup_old_snapshots(&self, max_age_days: u32, max_count: usize) -> Result<usize> {
        self.storage.cleanup_old_snapshots(max_age_days, max_count).await
    }

    /// Get storage statistics
    pub async fn get_storage_stats(&self) -> Result<storage::StorageStats> {
        self.storage.get_storage_stats().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_snapshot_config_default() {
        let config = SnapshotConfig::default();
        assert!(config.name.starts_with("snapshot_"));
        assert_eq!(config.snapshot_type, SnapshotType::Full);
        assert!(config.compress);
        assert!(config.include_memory);
        assert!(config.include_filesystem);
    }

    #[tokio::test]
    async fn test_snapshot_metadata_creation() {
        let metadata = SnapshotMetadata {
            id: Uuid::new_v4(),
            name: "test_snapshot".to_string(),
            description: Some("Test description".to_string()),
            snapshot_type: SnapshotType::Full,
            container_id: "test_container".to_string(),
            created_at: Utc::now(),
            size_bytes: 1024,
            compression_ratio: Some(0.7),
            tags: HashMap::new(),
            checksum: "abc123".to_string(),
        };

        assert_eq!(metadata.name, "test_snapshot");
        assert_eq!(metadata.container_id, "test_container");
        assert_eq!(metadata.size_bytes, 1024);
    }
}