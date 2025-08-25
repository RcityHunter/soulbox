pub mod manager;
pub mod runtime;
pub mod sandbox;
pub mod resource_limits;
pub mod network;
pub mod pool;
pub mod code_executor;
pub mod startup_optimizer;
pub mod error_recovery;

pub use manager::ContainerManager;
#[cfg(test)]
pub use manager::MockContainerManager;
pub use sandbox::SandboxContainer;
pub use resource_limits::ResourceLimits;
pub use network::{NetworkConfig, PortMapping};
pub use pool::{ContainerPool, PoolConfig, PoolContainer, PoolStats};
pub use code_executor::{CodeExecutor, CodeExecutionResult};
pub use startup_optimizer::{
    StartupOptimizer, StartupOptimizerConfig, StartupMetrics,
    WarmupStrategy, ImagePrePullConfig,
};
pub use error_recovery::{
    ErrorRecoverySystem, ErrorRecoveryConfig, ErrorRecoveryMetrics,
    FallbackStrategy, RecoveryStrategy, ErrorRecoveryContainerHealth,
    CircuitBreakerConfig, HealthCheckConfig,
};

#[cfg(test)]
mod tests;