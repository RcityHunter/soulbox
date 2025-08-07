use thiserror::Error;

pub type Result<T> = std::result::Result<T, SoulBoxError>;

#[derive(Error, Debug)]
pub enum SoulBoxError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Sandbox error: {0}")]
    Sandbox(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Authorization error: {0}")]
    Authorization(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("gRPC transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    #[error("gRPC status error: {0}")]
    Status(#[from] tonic::Status),

    #[error("Container error: {0}")]
    Container(#[from] bollard::errors::Error),

    #[error("Filesystem error: {0}")]
    Filesystem(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),

    #[error("Security violation: {0}")]
    Security(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl SoulBoxError {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub fn sandbox(msg: impl Into<String>) -> Self {
        Self::Sandbox(msg.into())
    }

    pub fn authentication(msg: impl Into<String>) -> Self {
        Self::Authentication(msg.into())
    }

    pub fn authorization(msg: impl Into<String>) -> Self {
        Self::Authorization(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    pub fn filesystem(msg: impl Into<String>) -> Self {
        Self::Filesystem(msg.into())
    }

    pub fn resource_limit(msg: impl Into<String>) -> Self {
        Self::ResourceLimit(msg.into())
    }

    pub fn security(msg: impl Into<String>) -> Self {
        Self::Security(msg.into())
    }
}