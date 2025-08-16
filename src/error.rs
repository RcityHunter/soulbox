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

    // Container-specific errors with detailed context
    #[error("Container creation failed: {message}")]
    ContainerCreationFailed { message: String, container_id: Option<String> },
    
    #[error("Container start failed: {message}")]
    ContainerStartFailed { message: String, container_id: String },
    
    #[error("Container stop failed: {message}")]
    ContainerStopFailed { message: String, container_id: String },
    
    #[error("Container not found: {container_id}")]
    ContainerNotFound { container_id: String },
    
    #[error("Container pool exhausted for runtime: {runtime}")]
    ContainerPoolExhausted { runtime: String, max_containers: u32 },
    
    // Generic container error fallback
    #[error("Container error: {0}")]
    Container(#[from] bollard::errors::Error),

    // Filesystem errors with detailed context
    #[error("File not found: {path}")]
    FileNotFound { path: String },
    
    #[error("Permission denied accessing: {path}")]
    FilePermissionDenied { path: String },
    
    #[error("Disk space insufficient: {required_bytes} bytes needed, {available_bytes} available")]
    DiskSpaceInsufficient { required_bytes: u64, available_bytes: u64 },
    
    #[error("File operation failed: {operation} on {path} - {reason}")]
    FileOperationFailed { operation: String, path: String, reason: String },

    // Resource limit errors with specific details
    #[error("Memory limit exceeded: {used_bytes} bytes used, limit is {limit_bytes} bytes")]
    MemoryLimitExceeded { used_bytes: u64, limit_bytes: u64 },
    
    #[error("CPU limit exceeded: {usage_percent}% usage, limit is {limit_percent}%")]
    CpuLimitExceeded { usage_percent: f64, limit_percent: f64 },
    
    #[error("Network bandwidth limit exceeded: {usage_bps} bps, limit is {limit_bps} bps")]
    NetworkBandwidthExceeded { usage_bps: u64, limit_bps: u64 },
    
    #[error("Process limit exceeded: {process_count} processes, limit is {limit_count}")]
    ProcessLimitExceeded { process_count: u32, limit_count: u32 },

    // Security errors with threat classification
    #[error("Authentication failed: {reason}")]
    AuthenticationFailed { reason: String, user_id: Option<String> },
    
    #[error("Authorization denied: {action} on {resource}")]
    AuthorizationDenied { action: String, resource: String, user_id: Option<String> },
    
    #[error("Malicious code detected: {threat_type} in {location}")]
    MaliciousCodeDetected { threat_type: String, location: String },
    
    #[error("Rate limit exceeded: {requests} requests in {window_seconds}s, limit is {limit}")]
    RateLimitExceeded { requests: u32, window_seconds: u32, limit: u32 },
    
    #[error("Input validation failed: {field} - {reason}")]
    InputValidationFailed { field: String, reason: String },

    // Network errors with detailed context
    #[error("Port allocation failed: port {port} already in use")]
    PortAllocationFailed { port: u16, sandbox_id: Option<String> },
    
    #[error("Network connection failed: {endpoint} - {reason}")]
    NetworkConnectionFailed { endpoint: String, reason: String },
    
    #[error("DNS resolution failed: {hostname}")]
    DnsResolutionFailed { hostname: String },
    
    #[error("Proxy configuration invalid: {proxy_type} - {reason}")]
    ProxyConfigurationInvalid { proxy_type: String, reason: String },

    // Database errors with operation context
    #[error("Database connection failed: {database_type} at {endpoint}")]
    DatabaseConnectionFailed { database_type: String, endpoint: String },
    
    #[error("Database query failed: {query_type} - {reason}")]
    DatabaseQueryFailed { query_type: String, reason: String },
    
    #[error("Database transaction failed: {operation} - {reason}")]
    DatabaseTransactionFailed { operation: String, reason: String },
    
    #[error("Database migration failed: version {version} - {reason}")]
    DatabaseMigrationFailed { version: String, reason: String },

    // Session errors with detailed context
    #[error("Session not found: {session_id}")]
    SessionNotFound { session_id: String },
    
    #[error("Session expired: {session_id}, expired at {expired_at}")]
    SessionExpired { session_id: String, expired_at: String },
    
    #[error("Session creation failed: {reason}")]
    SessionCreationFailed { reason: String, user_id: Option<String> },

    // Timeout errors with operation context
    #[error("Operation timeout: {operation} took longer than {timeout_seconds}s")]
    OperationTimeout { operation: String, timeout_seconds: u64 },
    
    #[error("Deadline exceeded: {operation} deadline was {deadline}")]
    DeadlineExceeded { operation: String, deadline: String },

    // Configuration errors
    #[error("Configuration error: {parameter} - {reason}")]
    Configuration { parameter: String, reason: String },
    
    #[error("Environment variable missing: {variable}")]
    EnvironmentVariableMissing { variable: String },
    
    #[error("Configuration validation failed: {section} - {errors}")]
    ConfigurationValidationFailed { section: String, errors: Vec<String> },

    // Legacy broad error types (to be gradually replaced)
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Unsupported operation: {0}")]
    Unsupported(String),

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