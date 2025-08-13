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

pub use config::Config;
pub use error::{SoulBoxError, Result};
pub use auth::{JwtManager, api_key::ApiKeyManager, middleware::AuthMiddleware};
pub use api::auth::{AuthState, auth_routes};
pub use audit::{AuditService, AuditConfig, AuditMiddleware, AuditEventType};

// Include generated protobuf code
pub mod proto {
    tonic::include_proto!("soulbox.v1");
}