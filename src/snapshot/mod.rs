// Snapshot system for container and filesystem state management
use std::collections::HashMap;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use tracing::info;

use crate::error::{Result, SoulBoxError};
use crate::container::manager::ContainerManager;
use crate::filesystem::SandboxFileSystem;

/// Snapshot types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnapshotType {
    /// Full snapshot - captures entire state
    Full,
    /// Incremental snapshot - captures only changes since last snapshot
    Incremental { base_snapshot_id: String },
}

/// Container state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSnapshot {
    pub container_id: String,
    pub image: String,
    pub environment: HashMap<String, String>,
    pub resource_limits: ContainerResourceLimits,
    pub network_config: ContainerNetworkConfig,
    pub running_processes: Vec<ProcessInfo>,
    pub memory_usage: u64,
    pub cpu_usage: f64,
}

/// Process information for container snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub command: String,
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub started_at: DateTime<Utc>,
}

/// Resource limits snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerResourceLimits {
    pub memory_mb: u64,
    pub cpu_cores: f64,
    pub disk_mb: u64,
}

/// Network configuration snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerNetworkConfig {
    pub enable_internet: bool,
    pub port_mappings: Vec<PortMapping>,
    pub allowed_domains: Vec<String>,
}

/// Port mapping configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: String,
}

/// Complete snapshot of a sandbox session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxSnapshot {
    pub id: String,
    pub session_id: Uuid,
    pub snapshot_type: SnapshotType,
    pub created_at: DateTime<Utc>,
    pub container_snapshot: Option<ContainerSnapshot>,
    pub filesystem_snapshot_id: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Snapshot manager for creating and managing snapshots
pub struct SnapshotManager {
    storage_path: PathBuf,
    container_manager: Box<ContainerManager>,
    snapshots: HashMap<String, SandboxSnapshot>,
}

impl SnapshotManager {
    pub fn new(storage_path: PathBuf, container_manager: Box<ContainerManager>) -> Self {
        std::fs::create_dir_all(&storage_path).ok();
        
        Self {
            storage_path,
            container_manager,
            snapshots: HashMap::new(),
        }
    }

    /// Create a full snapshot of a sandbox
    pub async fn create_full_snapshot(
        &mut self,
        session_id: Uuid,
        sandbox_fs: &mut SandboxFileSystem,
    ) -> Result<String> {
        info!("Creating full snapshot for session {}", session_id);
        
        let snapshot_id = format!("snap_{}_{}", session_id, Utc::now().timestamp());
        
        // Create filesystem snapshot
        let fs_snapshot_id = sandbox_fs.create_snapshot(&snapshot_id).await?;
        
        // Get container state
        let container_snapshot = self.capture_container_state(&session_id.to_string()).await?;
        
        let snapshot = SandboxSnapshot {
            id: snapshot_id.clone(),
            session_id,
            snapshot_type: SnapshotType::Full,
            created_at: Utc::now(),
            container_snapshot: Some(container_snapshot),
            filesystem_snapshot_id: Some(fs_snapshot_id),
            metadata: HashMap::new(),
        };
        
        // Save snapshot metadata
        self.save_snapshot(&snapshot).await?;
        self.snapshots.insert(snapshot_id.clone(), snapshot);
        
        info!("Full snapshot {} created successfully", snapshot_id);
        Ok(snapshot_id)
    }

    /// Create an incremental snapshot
    pub async fn create_incremental_snapshot(
        &mut self,
        session_id: Uuid,
        base_snapshot_id: &str,
        sandbox_fs: &mut SandboxFileSystem,
    ) -> Result<String> {
        info!("Creating incremental snapshot for session {} based on {}", session_id, base_snapshot_id);
        
        // Verify base snapshot exists
        if !self.snapshots.contains_key(base_snapshot_id) {
            return Err(SoulBoxError::not_found(format!("Base snapshot not found: {}", base_snapshot_id)));
        }
        
        let snapshot_id = format!("snap_incr_{}_{}", session_id, Utc::now().timestamp());
        
        // Create incremental filesystem snapshot
        let fs_snapshot_id = sandbox_fs.create_snapshot(&snapshot_id).await?;
        
        // Capture only changed container state
        let container_snapshot = self.capture_container_state(&session_id.to_string()).await?;
        
        let snapshot = SandboxSnapshot {
            id: snapshot_id.clone(),
            session_id,
            snapshot_type: SnapshotType::Incremental { 
                base_snapshot_id: base_snapshot_id.to_string() 
            },
            created_at: Utc::now(),
            container_snapshot: Some(container_snapshot),
            filesystem_snapshot_id: Some(fs_snapshot_id),
            metadata: HashMap::new(),
        };
        
        // Save snapshot metadata
        self.save_snapshot(&snapshot).await?;
        self.snapshots.insert(snapshot_id.clone(), snapshot);
        
        info!("Incremental snapshot {} created successfully", snapshot_id);
        Ok(snapshot_id)
    }

    /// Restore from a snapshot
    pub fn restore_snapshot<'a>(
        &'a self,
        snapshot_id: &'a str,
        sandbox_fs: &'a SandboxFileSystem,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            info!("Restoring from snapshot {}", snapshot_id);
            
            let snapshot = self.snapshots.get(snapshot_id)
                .ok_or_else(|| SoulBoxError::not_found(format!("Snapshot not found: {}", snapshot_id)))?;
            
            // Handle incremental snapshots by applying them in order
            match &snapshot.snapshot_type {
                SnapshotType::Full => {
                    self.restore_full_snapshot(snapshot, sandbox_fs).await?;
                }
                SnapshotType::Incremental { base_snapshot_id } => {
                    // First restore the base snapshot
                    self.restore_snapshot(base_snapshot_id, sandbox_fs).await?;
                    // Then apply incremental changes
                    self.apply_incremental_snapshot(snapshot, sandbox_fs).await?;
                }
            }
            
            info!("Successfully restored from snapshot {}", snapshot_id);
            Ok(())
        })
    }

    /// Restore a full snapshot
    async fn restore_full_snapshot(
        &self,
        snapshot: &SandboxSnapshot,
        sandbox_fs: &SandboxFileSystem,
    ) -> Result<()> {
        // Restore filesystem
        if let Some(fs_snapshot_id) = &snapshot.filesystem_snapshot_id {
            sandbox_fs.restore_from_snapshot(fs_snapshot_id).await?;
        }
        
        // Restore container state
        if let Some(container_snapshot) = &snapshot.container_snapshot {
            self.restore_container_state(&snapshot.session_id.to_string(), container_snapshot).await?;
        }
        
        Ok(())
    }

    /// Apply incremental snapshot changes
    async fn apply_incremental_snapshot(
        &self,
        snapshot: &SandboxSnapshot,
        sandbox_fs: &SandboxFileSystem,
    ) -> Result<()> {
        // Apply filesystem changes
        if let Some(fs_snapshot_id) = &snapshot.filesystem_snapshot_id {
            // In a real implementation, this would apply only the delta
            sandbox_fs.restore_from_snapshot(fs_snapshot_id).await?;
        }
        
        // Apply container state changes
        if let Some(container_snapshot) = &snapshot.container_snapshot {
            self.restore_container_state(&snapshot.session_id.to_string(), container_snapshot).await?;
        }
        
        Ok(())
    }

    /// Capture current container state
    async fn capture_container_state(&self, container_id: &str) -> Result<ContainerSnapshot> {
        let _container = self.container_manager.get_container(container_id).await
            .ok_or_else(|| SoulBoxError::not_found(format!("Container not found: {}", container_id)))?;
        
        // Get container stats - simplified for MVP
        let cpu_usage = 10.0; // Placeholder
        let memory_usage = 100; // Placeholder
        
        // Get running processes - simplified for MVP
        let processes = vec![]; // Would get actual processes in production
        
        Ok(ContainerSnapshot {
            container_id: container_id.to_string(),
            image: "python:3.11-slim".to_string(), // Would get from container info
            environment: HashMap::new(), // Would get from container config
            resource_limits: ContainerResourceLimits {
                memory_mb: 512, // Get from actual container config
                cpu_cores: 1.0,
                disk_mb: 2048,
            },
            network_config: ContainerNetworkConfig {
                enable_internet: true,
                port_mappings: vec![],
                allowed_domains: vec![],
            },
            running_processes: processes,
            memory_usage,
            cpu_usage,
        })
    }

    /// Restore container to a specific state
    async fn restore_container_state(
        &self,
        container_id: &str,
        snapshot: &ContainerSnapshot,
    ) -> Result<()> {
        // Stop existing container if running
        if let Some(container) = self.container_manager.get_container(container_id).await {
            container.stop().await?;
        }
        
        // Recreate container with snapshot configuration
        let resource_limits = crate::container::ResourceLimits {
            memory: crate::container::resource_limits::MemoryLimits {
                limit_mb: snapshot.resource_limits.memory_mb,
                swap_limit_mb: Some(snapshot.resource_limits.memory_mb * 2),
                swap_mb: Some(snapshot.resource_limits.memory_mb / 2),
            },
            cpu: crate::container::resource_limits::CpuLimits {
                cores: snapshot.resource_limits.cpu_cores,
                shares: Some(1024),
                cpu_percent: Some(80.0),
            },
            disk: crate::container::resource_limits::DiskLimits {
                limit_mb: snapshot.resource_limits.disk_mb,
                iops_limit: Some(1000),
            },
            network: crate::container::resource_limits::NetworkLimits {
                upload_bps: Some(1024 * 1024 * 10),
                download_bps: Some(1024 * 1024 * 100),
                max_connections: Some(100),
            },
        };
        
        let network_config = crate::container::NetworkConfig {
            enable_internet: snapshot.network_config.enable_internet,
            port_mappings: vec![],
            allowed_domains: snapshot.network_config.allowed_domains.clone(),
            dns_servers: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
        };
        
        // Create new container with snapshot state
        let container = self.container_manager.create_sandbox_container(
            container_id,
            &snapshot.image,
            resource_limits,
            network_config,
            snapshot.environment.clone(),
        ).await?;
        
        // Start the container
        container.start().await?;
        
        Ok(())
    }

    /// Save snapshot metadata to disk
    async fn save_snapshot(&self, snapshot: &SandboxSnapshot) -> Result<()> {
        let snapshot_file = self.storage_path.join(format!("{}.json", snapshot.id));
        let json = serde_json::to_string_pretty(snapshot)?;
        tokio::fs::write(snapshot_file, json).await?;
        Ok(())
    }

    /// Load snapshot metadata from disk
    pub async fn load_snapshot(&mut self, snapshot_id: &str) -> Result<SandboxSnapshot> {
        if let Some(snapshot) = self.snapshots.get(snapshot_id) {
            return Ok(snapshot.clone());
        }
        
        let snapshot_file = self.storage_path.join(format!("{}.json", snapshot_id));
        let json = tokio::fs::read_to_string(snapshot_file).await?;
        let snapshot: SandboxSnapshot = serde_json::from_str(&json)?;
        
        self.snapshots.insert(snapshot_id.to_string(), snapshot.clone());
        Ok(snapshot)
    }

    /// List all available snapshots for a session
    pub async fn list_snapshots(&self, session_id: Option<Uuid>) -> Vec<SandboxSnapshot> {
        self.snapshots.values()
            .filter(|s| session_id.is_none() || Some(s.session_id) == session_id)
            .cloned()
            .collect()
    }

    /// Delete a snapshot
    pub async fn delete_snapshot(&mut self, snapshot_id: &str) -> Result<()> {
        self.snapshots.remove(snapshot_id);
        let snapshot_file = self.storage_path.join(format!("{}.json", snapshot_id));
        tokio::fs::remove_file(snapshot_file).await.ok();
        Ok(())
    }

    /// Clean up old snapshots based on retention policy
    pub async fn cleanup_old_snapshots(&mut self, retention_days: i64) -> Result<()> {
        let cutoff_time = Utc::now() - chrono::Duration::days(retention_days);
        
        let old_snapshots: Vec<String> = self.snapshots.iter()
            .filter(|(_, s)| s.created_at < cutoff_time)
            .map(|(id, _)| id.clone())
            .collect();
        
        for snapshot_id in old_snapshots {
            self.delete_snapshot(&snapshot_id).await?;
        }
        
        Ok(())
    }
}