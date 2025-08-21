use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu: CpuLimits,
    pub memory: MemoryLimits,
    pub disk: DiskLimits,
    pub network: NetworkLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuLimits {
    pub cores: f64,
    pub shares: Option<u64>,
    pub cpu_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLimits {
    pub limit_mb: u64,
    pub swap_limit_mb: Option<u64>,
    pub swap_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskLimits {
    pub limit_mb: u64,
    pub iops_limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkLimits {
    /// Upload bandwidth limit in bytes per second (0 = unlimited)
    pub upload_bps: Option<u64>,
    /// Download bandwidth limit in bytes per second (0 = unlimited)
    pub download_bps: Option<u64>,
    /// Maximum number of concurrent connections
    pub max_connections: Option<u32>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu: CpuLimits {
                cores: 1.0,
                shares: Some(1024),
                cpu_percent: Some(80.0),
            },
            memory: MemoryLimits {
                limit_mb: 512,
                swap_limit_mb: Some(1024),
                swap_mb: Some(512),
            },
            disk: DiskLimits {
                limit_mb: 2048,
                iops_limit: Some(1000),
            },
            network: NetworkLimits {
                upload_bps: Some(1024 * 1024), // 1 MB/s
                download_bps: Some(10 * 1024 * 1024), // 10 MB/s
                max_connections: Some(100),
            },
        }
    }
}