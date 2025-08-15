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

    #[error("TOML deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

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

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Unsupported operation: {0}")]
    Unsupported(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Template error: {0}")]
    Template(String),

    #[error("Audit error: {0}")]
    Audit(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Timeout error: {0}")]
    Timeout(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("CLI error: {0}")]
    Cli(String),

    #[error("Provider error: {0}")]
    Provider(String),
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

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn invalid_state(msg: impl Into<String>) -> Self {
        Self::InvalidState(msg.into())
    }

    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::Unsupported(msg.into())
    }

    pub fn template(msg: impl Into<String>) -> Self {
        Self::Template(msg.into())
    }

    pub fn audit(msg: impl Into<String>) -> Self {
        Self::Audit(msg.into())
    }

    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }

    pub fn timeout(msg: impl Into<String>) -> Self {
        Self::Timeout(msg.into())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn api(msg: impl Into<String>) -> Self {
        Self::Api(msg.into())
    }

    pub fn cli(msg: impl Into<String>) -> Self {
        Self::Cli(msg.into())
    }

    pub fn provider(msg: impl Into<String>) -> Self {
        Self::Provider(msg.into())
    }

    /// Get error code for consistent error handling across APIs
    pub fn error_code(&self) -> &'static str {
        match self {
            SoulBoxError::Config(_) => "CONFIG_ERROR",
            SoulBoxError::Sandbox(_) => "SANDBOX_ERROR",
            SoulBoxError::Authentication(_) => "AUTH_ERROR",
            SoulBoxError::Authorization(_) => "AUTHZ_ERROR",
            SoulBoxError::Io(_) => "IO_ERROR",
            SoulBoxError::Json(_) => "JSON_ERROR",
            SoulBoxError::TomlDe(_) => "TOML_PARSE_ERROR",
            SoulBoxError::TomlSer(_) => "TOML_SERIALIZE_ERROR",
            SoulBoxError::WebSocket(_) => "WEBSOCKET_ERROR",
            SoulBoxError::Transport(_) => "TRANSPORT_ERROR",
            SoulBoxError::Status(_) => "GRPC_ERROR",
            SoulBoxError::Container(_) => "CONTAINER_ERROR",
            SoulBoxError::Filesystem(_) => "FILESYSTEM_ERROR",
            SoulBoxError::ResourceLimit(_) => "RESOURCE_LIMIT_ERROR",
            SoulBoxError::Security(_) => "SECURITY_ERROR",
            SoulBoxError::Internal(_) => "INTERNAL_ERROR",
            SoulBoxError::NotFound(_) => "NOT_FOUND",
            SoulBoxError::InvalidState(_) => "INVALID_STATE",
            SoulBoxError::Unsupported(_) => "UNSUPPORTED",
            SoulBoxError::Database(_) => "DATABASE_ERROR",
            SoulBoxError::Template(_) => "TEMPLATE_ERROR",
            SoulBoxError::Audit(_) => "AUDIT_ERROR",
            SoulBoxError::Network(_) => "NETWORK_ERROR",
            SoulBoxError::Timeout(_) => "TIMEOUT_ERROR",
            SoulBoxError::Validation(_) => "VALIDATION_ERROR",
            SoulBoxError::Api(_) => "API_ERROR",
            SoulBoxError::Cli(_) => "CLI_ERROR",
            SoulBoxError::Provider(_) => "PROVIDER_ERROR",
        }
    }

    /// Check if error is recoverable (can be retried)
    pub fn is_recoverable(&self) -> bool {
        match self {
            SoulBoxError::Timeout(_) => true,
            SoulBoxError::Network(_) => true,
            SoulBoxError::Transport(_) => true,
            SoulBoxError::WebSocket(_) => true,
            SoulBoxError::ResourceLimit(_) => false,
            SoulBoxError::Authentication(_) => false,
            SoulBoxError::Authorization(_) => false,
            SoulBoxError::Validation(_) => false,
            SoulBoxError::NotFound(_) => false,
            SoulBoxError::Unsupported(_) => false,
            _ => false,
        }
    }

    /// Get HTTP status code for web API responses
    pub fn http_status(&self) -> u16 {
        match self {
            SoulBoxError::Authentication(_) => 401,
            SoulBoxError::Authorization(_) => 403,
            SoulBoxError::NotFound(_) => 404,
            SoulBoxError::Validation(_) => 400,
            SoulBoxError::ResourceLimit(_) => 429,
            SoulBoxError::Timeout(_) => 408,
            SoulBoxError::Unsupported(_) => 501,
            _ => 500,
        }
    }
}