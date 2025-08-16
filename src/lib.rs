pub mod config;
pub mod error;
pub mod grpc;
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

// Include generated protobuf code
pub mod proto {
    tonic::include_proto!("soulbox.v1");
}