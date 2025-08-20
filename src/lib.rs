// Core modules
pub mod config;
pub mod error;
pub mod network;

// Container management
pub mod container;

// Authentication and authorization
pub mod auth;

// gRPC services
pub mod grpc;

// Database operations
pub mod database;

// API endpoints
pub mod api;

// WebSocket support
pub mod websocket;

// Runtime support
pub mod runtime;

// Template management
pub mod template;

// Session management
pub mod session;

// Additional critical modules (re-enabling gradually)
pub mod audit;
pub mod validation;
pub mod filesystem;
pub mod server;
pub mod sandbox; // Re-enabled for gRPC service compatibility
pub mod billing;

// Firecracker VM integration
pub mod firecracker;

// CLI support (disabled due to missing dependencies)
// pub mod cli;

// Simple API (disabled due to missing dependencies)
// pub mod simple;

// Core exports
pub use config::Config;
pub use error::{SoulBoxError, Result};
pub use network::{NetworkManager, SandboxNetworkConfig, PortMappingManager, ProxyManager, NetworkError};

// Container exports
pub use container::{ContainerManager, SandboxContainer, ResourceLimits, NetworkConfig, PortMapping, ContainerPool, PoolConfig};

// Auth exports
pub use auth::{JwtManager, Claims, ApiKeyManager, ApiKey, AuthMiddleware, User, Role, Permission};

// Database exports
pub use database::{SurrealConfig, SurrealPool, SurrealConnection, DatabaseBackend, DatabaseError, DatabaseResult};

// Generated protobuf code (using mock implementation for now)
pub mod soulbox {
    pub mod v1 {
        pub use crate::soulbox_v1::*;
    }
}

// Include the mock implementation file as a module
#[path = "soulbox.v1.rs"]
mod soulbox_v1;

// gRPC exports
pub use grpc::FILE_DESCRIPTOR_SET;