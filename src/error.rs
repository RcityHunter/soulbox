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

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

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
}