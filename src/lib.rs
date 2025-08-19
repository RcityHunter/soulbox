pub mod config;
pub mod error;
// pub mod grpc; // Temporarily disabled until tonic-build 0.14 API is fixed
pub mod websocket;
pub mod container;
pub mod firecracker;
pub mod filesystem;
pub mod server;
pub mod auth;
pub mod api;
pub mod audit;
pub mod sandbox;
pub mod database;
pub mod template;
pub mod cli;
pub mod session;
pub mod runtime;
pub mod monitoring;
pub mod network;
pub mod dependencies;
pub mod debug;
pub mod snapshot;
pub mod optimization;
pub mod recovery;
pub mod billing;
pub mod validation;

// Simple implementation - Linus style
pub mod simple;

pub use config::Config;
pub use error::{SoulBoxError, Result};
pub use auth::{JwtManager, api_key::ApiKeyManager, middleware::AuthMiddleware};
pub use api::auth::{AuthState, auth_routes};
pub use audit::{AuditService, AuditConfig, AuditMiddleware, AuditEventType};
pub use template::{TemplateManager, Template, TemplateError};

// New module exports
pub use session::{Session, SessionManager, InMemorySessionManager};
pub use runtime::{RuntimeType, RuntimeManager, RuntimeConfig, ExecutionContext};
pub use monitoring::{MonitoringConfig, MonitoringService, PerformanceMetrics, HealthCheck, HealthStatus};
pub use network::{NetworkManager, SandboxNetworkConfig, PortMappingManager, ProxyManager, NetworkError};
pub use dependencies::{DependencyManager, PackageManager, PackageSpec, DependencyManifest, InstallationResult};
pub use debug::{DebugManager, DebugConfig, DebugLanguage, DebugSession, BreakpointManager, inspector::VariableInspector};
pub use snapshot::{SnapshotManager, SnapshotConfig, SnapshotType, SnapshotMetadata, SnapshotStatus};
pub use optimization::{OptimizationManager, OptimizationConfig, PerformanceHotspot, OptimizationPriority};
pub use recovery::{RecoveryManager, RecoveryConfig, RecoveryStrategy, RecoveryResult, ComponentHealth, HealthStatus as RecoveryHealthStatus};
pub use billing::{BillingService, BillingConfig, MetricsCollector, UsageAggregator, CostCalculator, BillingStorage};

// Include generated protobuf code
// pub mod proto {
//     tonic::include_proto!("soulbox.v1");
// } // Temporarily disabled until tonic-build 0.14 API is fixed