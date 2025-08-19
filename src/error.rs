use thiserror::Error;
use std::collections::HashMap;
use uuid::Uuid;
use tracing::{error, warn};
use chrono::{DateTime, Utc};

pub type Result<T> = std::result::Result<T, SoulBoxError>;

/// Simplified security context for error tracking
#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub user_id: Option<Uuid>,
    pub ip_address: Option<std::net::IpAddr>,
    pub timestamp: DateTime<Utc>,
    pub severity: SecuritySeverity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SecuritySeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for SecurityContext {
    fn default() -> Self {
        Self {
            user_id: None,
            ip_address: None,
            timestamp: Utc::now(),
            severity: SecuritySeverity::Low,
        }
    }
}

/// Simplified error metadata for essential debugging information
#[derive(Debug, Clone)]
pub struct ErrorMetadata {
    pub error_id: Uuid,
    pub retry_after: Option<chrono::Duration>,
}

#[derive(Error, Debug)]
pub enum SoulBoxError {
    /// Security-related errors with simplified tracking
    #[error("Security violation: {message}")]
    SecurityViolation { 
        message: String, 
        context: SecurityContext,
    },
    
    #[error("Input validation failed: {field} - {reason}")]
    ValidationError { 
        field: String, 
        reason: String,
        provided_value: Option<String>,
        security_context: Option<SecurityContext>,
    },
    
    #[error("Rate limit exceeded: {resource} - {limit} requests per {window}")]
    RateLimitExceeded {
        resource: String,
        limit: u64,
        window: String,
        retry_after: chrono::Duration,
        user_id: Option<Uuid>,
    },
    
    #[error("Resource exhaustion: {resource} - {current_usage}/{limit}")]
    ResourceExhausted {
        resource: String,
        current_usage: u64,
        limit: u64,
        retry_after: Option<chrono::Duration>,
    },
    #[error("Configuration error: {message}")]
    Configuration { 
        message: String,
        parameter: String,
        reason: String,
    },

    #[error("Sandbox error: {message}")]
    Sandbox { 
        message: String,
        sandbox_id: Option<String>,
        user_id: Option<Uuid>,
        operation: Option<String>,
    },

    #[error("Authentication failed: {reason}")]
    Authentication { 
        reason: String,
        user_id: Option<Uuid>,
        ip_address: Option<std::net::IpAddr>,
        attempt_count: Option<u32>,
        security_context: SecurityContext,
    },

    #[error("Authorization denied: {resource} requires {permission}")]
    Authorization { 
        resource: String,
        permission: String,
        user_id: Uuid,
        role: Option<String>,
        security_context: SecurityContext,
    },
    
    #[error("Feature not implemented: {feature}")]
    NotImplemented { 
        feature: String,
        planned_version: Option<String>,
    },

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

    // Container-specific errors with enhanced context
    #[error("Container creation failed: {message}")]
    ContainerCreationFailed { 
        message: String, 
        container_id: Option<String>,
        image: Option<String>,
        user_id: Option<Uuid>,
        retry_after: Option<chrono::Duration>,
    },
    
    #[error("Container start failed: {message}")]
    ContainerStartFailed { 
        message: String, 
        container_id: String,
        sandbox_id: Option<String>,
        user_id: Option<Uuid>,
        timeout: Option<chrono::Duration>,
    },
    
    #[error("Container stop failed: {message}")]
    ContainerStopFailed { message: String, container_id: String },
    
    #[error("Container not found: {container_id}")]
    ContainerNotFound { container_id: String },
    
    #[error("Container pool exhausted for runtime: {runtime}")]
    ContainerPoolExhausted { runtime: String, max_containers: u32 },
    
    #[error("Pool error: {0}")]
    PoolError(String),
    
    #[error("Monitoring error: {0}")]
    MonitoringError(String),
    
    #[error("Runtime error: {0}")]
    RuntimeError(String),
    
    #[error("Session error: {0}")]
    SessionError(String),
    
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
    
    #[error("Malicious code detected: {threat_type} in {location}")]
    MaliciousCodeDetected { threat_type: String, location: String },
    
    
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
    
    #[error("Environment variable missing: {variable}")]
    EnvironmentVariableMissing { variable: String },
    
    #[error("Configuration validation failed: {section} - {}", errors.join(", "))]
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

    #[error("Filesystem error: {0}")]
    Filesystem(String),

    #[error("Redis error: {0}")]
    Redis(String),

    // Additional error variants needed by the codebase
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Timeout occurred: {0}")]
    Timeout(String),

    #[error("Resource limit reached: {0}")]
    ResourceLimit(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Template error: {0}")]
    Template(String),

    #[error("Security error: {0}")]
    Security(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Audit error: {0}")]
    Audit(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Prometheus error: {0}")]
    Prometheus(String),
}

impl SoulBoxError {
    /// Create a configuration error with detailed context
    pub fn configuration(parameter: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Configuration {
            message: format!("Configuration parameter '{}' invalid: {}", parameter.into(), reason.into()),
            parameter: parameter.into(),
            reason: reason.into(),
        }
    }
    
    /// Create a security violation error with simplified context
    pub fn security_violation(
        message: impl Into<String>, 
        severity: SecuritySeverity,
        user_id: Option<Uuid>,
        ip_address: Option<std::net::IpAddr>,
    ) -> Self {
        let context = SecurityContext {
            user_id,
            ip_address,
            severity,
            timestamp: Utc::now(),
        };
        
        let error_id = Uuid::new_v4();
        error!("Security violation detected: {} (ID: {})", message.into(), error_id);
        
        Self::SecurityViolation {
            message: message.into(),
            context,
        }
    }
    
    /// Create an authentication error with security tracking
    pub fn authentication(
        reason: impl Into<String>,
        user_id: Option<Uuid>,
        ip_address: Option<std::net::IpAddr>,
    ) -> Self {
        let context = SecurityContext {
            user_id,
            ip_address,
            severity: SecuritySeverity::High,
            timestamp: Utc::now(),
        };
        
        warn!("Authentication failed: {} (User: {:?}, IP: {:?})", 
              reason.into(), user_id, ip_address);
        
        Self::Authentication {
            reason: reason.into(),
            user_id,
            ip_address,
            attempt_count: None,
            security_context: context,
        }
    }
    
    /// Create an authorization error with full context
    pub fn authorization(
        resource: impl Into<String>,
        permission: impl Into<String>,
        user_id: Uuid,
        role: Option<String>,
    ) -> Self {
        let context = SecurityContext {
            user_id: Some(user_id),
            ip_address: None,
            severity: SecuritySeverity::Medium,
            timestamp: Utc::now(),
        };
        
        warn!("Authorization denied: user {} (role: {:?}) attempted {} on {}", 
              user_id, role, permission.into(), resource.into());
        
        Self::Authorization {
            resource: resource.into(),
            permission: permission.into(),
            user_id,
            role,
            security_context: context,
        }
    }
    
    /// Create a validation error with security context
    pub fn validation(
        field: impl Into<String>,
        reason: impl Into<String>,
        provided_value: Option<String>,
    ) -> Self {
        let context = SecurityContext {
            user_id: None,
            ip_address: None,
            severity: SecuritySeverity::Low,
            timestamp: Utc::now(),
        };
        
        Self::ValidationError {
            field: field.into(),
            reason: reason.into(),
            provided_value,
            security_context: Some(context),
        }
    }
    
    /// Create a rate limit error
    pub fn rate_limit(
        resource: impl Into<String>,
        limit: u64,
        window: impl Into<String>,
        retry_after: chrono::Duration,
        user_id: Option<Uuid>,
    ) -> Self {
        warn!("Rate limit exceeded for resource '{}' by user {:?}", resource.into(), user_id);
        
        Self::RateLimitExceeded {
            resource: resource.into(),
            limit,
            window: window.into(),
            retry_after,
            user_id,
        }
    }
    
    /// Create a resource exhaustion error - simplified
    pub fn resource_exhausted(
        resource: impl Into<String>,
        current_usage: u64,
        limit: u64,
    ) -> Self {
        let resource_str = resource.into();
        error!("Resource exhaustion: {} ({}/{})", resource_str, current_usage, limit);
        
        Self::ResourceExhausted {
            resource: resource_str,
            current_usage,
            limit,
            retry_after: Some(chrono::Duration::minutes(5)),
        }
    }
    
    /// Create a container creation error - simplified
    pub fn container_creation_failed(
        message: impl Into<String>,
        container_id: Option<String>,
        image: Option<String>,
        user_id: Option<Uuid>,
    ) -> Self {
        let error_id = Uuid::new_v4();
        let message_str = message.into();
        error!("Container creation failed: {} (ID: {})", message_str, error_id);
        
        Self::ContainerCreationFailed {
            message: message_str,
            container_id,
            image,
            user_id,
            retry_after: Some(chrono::Duration::seconds(30)),
        }
    }
    
    /// Create a sandbox error with context
    pub fn sandbox_error(
        message: impl Into<String>,
        sandbox_id: Option<String>,
        user_id: Option<Uuid>,
        operation: Option<String>,
    ) -> Self {
        Self::Sandbox {
            message: message.into(),
            sandbox_id,
            user_id,
            operation,
        }
    }
    
    /// Create a simple internal error (for backward compatibility)
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Sandbox {
            message: message.into(),
            sandbox_id: None,
            user_id: None,
            operation: None,
        }
    }
    
    /// Get error severity for monitoring and alerting
    pub fn severity(&self) -> SecuritySeverity {
        match self {
            Self::SecurityViolation { context, .. } => context.severity.clone(),
            Self::Authentication { .. } => SecuritySeverity::High,
            Self::Authorization { .. } => SecuritySeverity::Medium,
            Self::ValidationError { .. } => SecuritySeverity::Low,
            Self::RateLimitExceeded { .. } => SecuritySeverity::Medium,
            Self::ResourceExhausted { .. } => SecuritySeverity::High,
            Self::ContainerCreationFailed { .. } => SecuritySeverity::Medium,
            _ => SecuritySeverity::Low,
        }
    }
    
    /// Check if error should trigger security alerts
    pub fn requires_security_alert(&self) -> bool {
        matches!(self.severity(), SecuritySeverity::High | SecuritySeverity::Critical)
    }
    
    /// Get error context for monitoring - simplified
    pub fn get_context(&self) -> HashMap<String, String> {
        let mut context = HashMap::new();
        
        match self {
            Self::SecurityViolation { context: sec_ctx, .. } => {
                if let Some(uid) = sec_ctx.user_id {
                    context.insert("user_id".to_string(), uid.to_string());
                }
                if let Some(ip) = sec_ctx.ip_address {
                    context.insert("ip_address".to_string(), ip.to_string());
                }
            }
            Self::Authentication { user_id, ip_address, .. } => {
                if let Some(uid) = user_id {
                    context.insert("user_id".to_string(), uid.to_string());
                }
                if let Some(ip) = ip_address {
                    context.insert("ip_address".to_string(), ip.to_string());
                }
            }
            Self::Authorization { user_id, resource, permission, .. } => {
                context.insert("user_id".to_string(), user_id.to_string());
                context.insert("resource".to_string(), resource.clone());
                context.insert("permission".to_string(), permission.clone());
            }
            Self::ContainerCreationFailed { container_id, image, user_id, .. } => {
                if let Some(ref id) = container_id {
                    context.insert("container_id".to_string(), id.clone());
                }
                if let Some(ref img) = image {
                    context.insert("image".to_string(), img.clone());
                }
                if let Some(uid) = user_id {
                    context.insert("user_id".to_string(), uid.to_string());
                }
            }
            Self::ResourceExhausted { resource, current_usage, limit, .. } => {
                context.insert("resource".to_string(), resource.clone());
                context.insert("current_usage".to_string(), current_usage.to_string());
                context.insert("limit".to_string(), limit.to_string());
            }
            _ => {}
        }
        
        context.insert("error_type".to_string(), self.error_code().to_string());
        context.insert("severity".to_string(), format!("{:?}", self.severity()));
        context.insert("timestamp".to_string(), Utc::now().to_rfc3339());
        
        context
    }
    
    /// Get suggested retry delay - simplified
    pub fn retry_after(&self) -> Option<chrono::Duration> {
        match self {
            Self::RateLimitExceeded { retry_after, .. } => Some(*retry_after),
            Self::ResourceExhausted { retry_after, .. } => *retry_after,
            Self::ContainerCreationFailed { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
    
    // Legacy compatibility methods
    pub fn config(msg: impl Into<String>) -> Self {
        Self::configuration("unknown", msg)
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

// Additional From implementations for third-party library errors
impl From<prometheus::Error> for SoulBoxError {
    fn from(err: prometheus::Error) -> Self {
        SoulBoxError::Prometheus(err.to_string())
    }
}