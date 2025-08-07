use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu: CpuLimits,
    pub memory: MemoryLimits,
    pub disk: DiskLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuLimits {
    pub cores: f64,
    pub shares: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLimits {
    pub limit_mb: u64,
    pub swap_limit_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskLimits {
    pub limit_mb: u64,
    pub iops_limit: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu: CpuLimits {
                cores: 1.0,
                shares: Some(1024),
            },
            memory: MemoryLimits {
                limit_mb: 512,
                swap_limit_mb: Some(1024),
            },
            disk: DiskLimits {
                limit_mb: 2048,
                iops_limit: Some(1000),
            },
        }
    }
}