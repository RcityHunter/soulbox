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

// Snapshot system
pub mod snapshot;

// Monitoring and health checks
pub mod monitoring;

// Additional critical modules (re-enabling gradually)
pub mod audit;
pub mod validation;
pub mod filesystem;
pub mod server;
pub mod sandbox; // Re-enabled for gRPC service compatibility
pub mod billing;
pub mod sandbox_manager;
pub mod zero_copy;
pub mod lockfree;

// Firecracker VM integration
pub mod firecracker;

// CLI support 
pub mod cli;

// Simple API (disabled due to missing dependencies)
// pub mod simple;

// Core exports
pub use config::Config;
pub use error::{SoulBoxError, Result};
pub use network::{NetworkManager, SandboxNetworkConfig, PortMappingManager, ProxyManager, NetworkError};
pub use validation::InputValidator;

// Container exports
pub use container::{ContainerManager, SandboxContainer, ResourceLimits, NetworkConfig, PortMapping, ContainerPool, PoolConfig, CodeExecutor, CodeExecutionResult};

// Sandbox manager export
pub use sandbox_manager::{SandboxManager, SandboxSummary};

// Auth exports
pub use auth::{JwtManager, Claims, ApiKeyManager, ApiKey, AuthMiddleware, User, Role, Permission};

// Database exports
pub use database::{SurrealConfig, SurrealPool, SurrealConnection, DatabaseBackend, DatabaseError, DatabaseResult};

// Generated protobuf code from tonic-build
pub mod soulbox {
    pub mod v1 {
        tonic::include_proto!("soulbox.v1");
    }
}

// gRPC exports
pub use grpc::FILE_DESCRIPTOR_SET;