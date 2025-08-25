//! E2B API Compatibility Layer
//! 
//! This module provides compatibility with E2B's API, allowing SoulBox to be
//! used as a drop-in replacement for E2B sandbox services.

pub mod api_simple;
pub mod models;
pub mod client;
pub mod adapter;

pub use api_simple::E2BApiService;
pub use models::{E2BSandbox, E2BProcess, E2BFilesystem};
pub use client::E2BClient;
pub use adapter::E2BAdapter;

use crate::error::{Result, SoulBoxError};

/// E2B compatibility configuration
#[derive(Debug, Clone)]
pub struct E2BConfig {
    /// Enable E2B API compatibility mode
    pub enabled: bool,
    /// API version to emulate (e.g., "v1", "v2")
    pub api_version: String,
    /// Port for E2B API service
    pub api_port: u16,
    /// Enable legacy endpoints
    pub legacy_support: bool,
}

impl Default for E2BConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_version: "v1".to_string(),
            api_port: 49982, // E2B's default port
            legacy_support: false,
        }
    }
}

/// E2B compatibility errors
#[derive(Debug, thiserror::Error)]
pub enum E2BError {
    #[error("E2B API version {0} not supported")]
    UnsupportedVersion(String),
    
    #[error("E2B feature {0} not implemented")]
    NotImplemented(String),
    
    #[error("E2B client error: {0}")]
    ClientError(String),
    
    #[error("E2B adapter error: {0}")]
    AdapterError(String),
}

impl From<E2BError> for SoulBoxError {
    fn from(err: E2BError) -> Self {
        SoulBoxError::Internal(err.to_string())
    }
}