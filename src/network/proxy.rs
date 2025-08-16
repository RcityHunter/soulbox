//! HTTP/HTTPS proxy support for sandbox network traffic
//! 
//! This module provides:
//! - HTTP/HTTPS proxy configuration and management
//! - Traffic filtering and monitoring
//! - Content inspection and blocking
//! - Request/response logging
//! - SSL/TLS termination and inspection

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tracing::{debug, warn, error, info};

/// Proxy configuration errors
#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("Proxy server start failed: {0}")]
    ServerStartFailed(String),
    
    #[error("Proxy configuration invalid: {0}")]
    InvalidConfiguration(String),
    
    #[error("SSL/TLS configuration error: {0}")]
    TlsConfiguration(String),
    
    #[error("Certificate error: {0}")]
    Certificate(String),
    
    #[error("Proxy not found: {0}")]
    ProxyNotFound(String),
    
    #[error("Filter rule error: {0}")]
    FilterRule(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Authentication failed: {0}")]
    Authentication(String),
}

/// HTTP/HTTPS proxy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Proxy listening address
    pub listen_addr: SocketAddr,
    
    /// Enable HTTPS support
    pub enable_https: bool,
    
    /// SSL/TLS certificate configuration
    pub tls_config: Option<TlsConfig>,
    
    /// Request filtering rules
    pub filter_rules: Vec<FilterRule>,
    
    /// Allowed domains (whitelist)
    pub allowed_domains: HashSet<String>,
    
    /// Blocked domains (blacklist)
    pub blocked_domains: HashSet<String>,
    
    /// Content type filtering
    pub allowed_content_types: HashSet<String>,
    
    /// Maximum request size in bytes
    pub max_request_size: usize,
    
    /// Maximum response size in bytes
    pub max_response_size: usize,
    
    /// Enable request/response logging
    pub enable_logging: bool,
    
    /// Enable content inspection
    pub enable_content_inspection: bool,
    
    /// Authentication configuration
    pub auth_config: Option<ProxyAuthConfig>,
    
    /// Timeout settings
    pub timeout_config: TimeoutConfig,
    
    /// Rate limiting configuration
    pub rate_limit_config: Option<RateLimitConfig>,
}

/// SSL/TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Certificate file path
    pub cert_path: String,
    
    /// Private key file path
    pub key_path: String,
    
    /// CA certificate path for client verification
    pub ca_cert_path: Option<String>,
    
    /// Require client certificate
    pub require_client_cert: bool,
    
    /// Enable SNI (Server Name Indication)
    pub enable_sni: bool,
    
    /// Supported TLS versions
    pub tls_versions: Vec<String>,
}

/// Request filtering rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterRule {
    /// Rule name/description
    pub name: String,
    
    /// Rule type
    pub rule_type: FilterRuleType,
    
    /// Pattern to match against
    pub pattern: String,
    
    /// Action to take when rule matches
    pub action: FilterAction,
    
    /// Whether rule is enabled
    pub enabled: bool,
    
    /// Priority (higher numbers processed first)
    pub priority: u32,
}

/// Types of filter rules
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FilterRuleType {
    /// Match against URL
    Url,
    /// Match against HTTP method
    Method,
    /// Match against request headers
    Header,
    /// Match against request body content
    Body,
    /// Match against content type
    ContentType,
    /// Match against user agent
    UserAgent,
    /// Custom regex pattern
    Regex,
}

/// Actions to take when filter rule matches
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FilterAction {
    /// Allow the request
    Allow,
    /// Block the request
    Block,
    /// Log the request (but allow)
    Log,
    /// Modify the request
    Modify,
    /// Rate limit the request
    RateLimit,
}

/// Proxy authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyAuthConfig {
    /// Authentication type
    pub auth_type: ProxyAuthType,
    
    /// Username/password pairs for basic auth
    pub credentials: HashMap<String, String>,
    
    /// JWT configuration for bearer token auth
    pub jwt_config: Option<JwtConfig>,
    
    /// API key configuration
    pub api_key_config: Option<ApiKeyConfig>,
}

/// Authentication types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProxyAuthType {
    None,
    Basic,
    Bearer,
    ApiKey,
    Custom,
}

/// JWT configuration for proxy authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// JWT secret key
    pub secret: String,
    
    /// Token expiration time in seconds
    pub expiration_secs: u64,
    
    /// Issuer
    pub issuer: String,
    
    /// Audience
    pub audience: String,
}

/// API key configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    /// Valid API keys
    pub keys: HashSet<String>,
    
    /// Header name for API key
    pub header_name: String,
    
    /// Query parameter name for API key
    pub query_param_name: Option<String>,
}

/// Timeout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    /// Connection timeout in seconds
    pub connect_timeout_secs: u64,
    
    /// Request timeout in seconds
    pub request_timeout_secs: u64,
    
    /// Response timeout in seconds
    pub response_timeout_secs: u64,
    
    /// Keep-alive timeout in seconds
    pub keepalive_timeout_secs: u64,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Maximum requests per minute
    pub requests_per_minute: u32,
    
    /// Maximum requests per hour
    pub requests_per_hour: u32,
    
    /// Maximum concurrent connections
    pub max_concurrent_connections: u32,
    
    /// Rate limit by IP address
    pub limit_by_ip: bool,
    
    /// Rate limit by user
    pub limit_by_user: bool,
}

/// Proxy request/response log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyLogEntry {
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    
    /// Client IP address
    pub client_ip: String,
    
    /// HTTP method
    pub method: String,
    
    /// Request URL
    pub url: String,
    
    /// Request headers
    pub request_headers: HashMap<String, String>,
    
    /// Response status code
    pub status_code: u16,
    
    /// Response headers
    pub response_headers: HashMap<String, String>,
    
    /// Request size in bytes
    pub request_size: usize,
    
    /// Response size in bytes
    pub response_size: usize,
    
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
    
    /// Whether request was blocked
    pub blocked: bool,
    
    /// Reason for blocking (if applicable)
    pub block_reason: Option<String>,
}

/// Proxy statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyStats {
    /// Total requests processed
    pub total_requests: u64,
    
    /// Requests blocked
    pub blocked_requests: u64,
    
    /// Total bytes transferred
    pub bytes_transferred: u64,
    
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    
    /// Active connections
    pub active_connections: u32,
    
    /// Errors encountered
    pub error_count: u64,
    
    /// Last reset time
    pub last_reset: chrono::DateTime<chrono::Utc>,
}

/// Proxy manager for handling multiple proxy instances
pub struct ProxyManager {
    /// Active proxy configurations by sandbox ID
    proxies: Arc<Mutex<HashMap<String, ProxyConfig>>>,
    
    /// Proxy statistics by sandbox ID
    stats: Arc<Mutex<HashMap<String, ProxyStats>>>,
    
    /// Request logs by sandbox ID
    logs: Arc<Mutex<HashMap<String, Vec<ProxyLogEntry>>>>,
    
    /// Maximum log entries per sandbox
    max_log_entries: usize,
}

impl ProxyManager {
    /// Create a new proxy manager
    pub fn new() -> Self {
        Self {
            proxies: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(HashMap::new())),
            logs: Arc::new(Mutex::new(HashMap::new())),
            max_log_entries: 1000,
        }
    }
    
    /// Configure proxy for a sandbox
    pub async fn configure_proxy(
        &mut self,
        sandbox_id: &str,
        config: ProxyConfig,
    ) -> Result<(), ProxyError> {
        // Validate configuration
        self.validate_proxy_config(&config)?;
        
        // Store configuration
        {
            let mut proxies = self.proxies.lock().unwrap();
            proxies.insert(sandbox_id.to_string(), config.clone());
        }
        
        // Initialize statistics
        {
            let mut stats = self.stats.lock().unwrap();
            stats.insert(sandbox_id.to_string(), ProxyStats::default());
        }
        
        // Initialize logs
        {
            let mut logs = self.logs.lock().unwrap();
            logs.insert(sandbox_id.to_string(), Vec::new());
        }
        
        info!("Configured proxy for sandbox {} on {}", sandbox_id, config.listen_addr);
        Ok(())
    }
    
    /// Remove proxy configuration for a sandbox
    pub async fn remove_proxy(&mut self, sandbox_id: &str) -> Result<(), ProxyError> {
        // Remove configuration
        let removed = {
            let mut proxies = self.proxies.lock().unwrap();
            proxies.remove(sandbox_id)
        };
        
        if removed.is_some() {
            // Clean up statistics and logs
            {
                let mut stats = self.stats.lock().unwrap();
                stats.remove(sandbox_id);
            }
            {
                let mut logs = self.logs.lock().unwrap();
                logs.remove(sandbox_id);
            }
            
            info!("Removed proxy configuration for sandbox {}", sandbox_id);
            Ok(())
        } else {
            Err(ProxyError::ProxyNotFound(sandbox_id.to_string()))
        }
    }
    
    /// Get proxy configuration for a sandbox
    pub fn get_proxy_config(&self, sandbox_id: &str) -> Option<ProxyConfig> {
        self.proxies.lock().unwrap().get(sandbox_id).cloned()
    }
    
    /// Get proxy statistics for a sandbox
    pub fn get_proxy_stats(&self, sandbox_id: &str) -> Option<ProxyStats> {
        self.stats.lock().unwrap().get(sandbox_id).cloned()
    }
    
    /// Get proxy logs for a sandbox
    pub fn get_proxy_logs(&self, sandbox_id: &str, limit: Option<usize>) -> Vec<ProxyLogEntry> {
        let logs = self.logs.lock().unwrap();
        if let Some(sandbox_logs) = logs.get(sandbox_id) {
            let limit = limit.unwrap_or(sandbox_logs.len());
            sandbox_logs.iter().rev().take(limit).cloned().collect()
        } else {
            Vec::new()
        }
    }
    
    /// Log a proxy request
    pub fn log_request(&mut self, sandbox_id: &str, log_entry: ProxyLogEntry) {
        let mut logs = self.logs.lock().unwrap();
        if let Some(sandbox_logs) = logs.get_mut(sandbox_id) {
            sandbox_logs.push(log_entry);
            
            // Limit log size
            if sandbox_logs.len() > self.max_log_entries {
                sandbox_logs.remove(0);
            }
        }
    }
    
    /// Update proxy statistics
    pub fn update_stats(&mut self, sandbox_id: &str, request_blocked: bool, response_time_ms: u64, bytes_transferred: usize) {
        let mut stats = self.stats.lock().unwrap();
        if let Some(proxy_stats) = stats.get_mut(sandbox_id) {
            proxy_stats.total_requests += 1;
            if request_blocked {
                proxy_stats.blocked_requests += 1;
            }
            proxy_stats.bytes_transferred += bytes_transferred as u64;
            
            // Update average response time using exponential moving average
            if proxy_stats.avg_response_time_ms == 0.0 {
                proxy_stats.avg_response_time_ms = response_time_ms as f64;
            } else {
                proxy_stats.avg_response_time_ms = 
                    0.7 * proxy_stats.avg_response_time_ms + 0.3 * response_time_ms as f64;
            }
        }
    }
    
    /// Validate proxy configuration
    fn validate_proxy_config(&self, config: &ProxyConfig) -> Result<(), ProxyError> {
        // Validate listen address
        if config.listen_addr.port() == 0 {
            return Err(ProxyError::InvalidConfiguration(
                "Invalid listen port".to_string(),
            ));
        }
        
        // Validate TLS configuration if HTTPS is enabled
        if config.enable_https {
            if let Some(tls_config) = &config.tls_config {
                if tls_config.cert_path.is_empty() || tls_config.key_path.is_empty() {
                    return Err(ProxyError::TlsConfiguration(
                        "Certificate and key paths are required for HTTPS".to_string(),
                    ));
                }
            } else {
                return Err(ProxyError::TlsConfiguration(
                    "TLS configuration required for HTTPS".to_string(),
                ));
            }
        }
        
        // Validate filter rules
        for rule in &config.filter_rules {
            if rule.pattern.is_empty() {
                return Err(ProxyError::FilterRule(
                    format!("Empty pattern in filter rule '{}'", rule.name),
                ));
            }
        }
        
        // Validate size limits
        if config.max_request_size == 0 || config.max_response_size == 0 {
            return Err(ProxyError::InvalidConfiguration(
                "Request and response size limits must be greater than 0".to_string(),
            ));
        }
        
        Ok(())
    }
    
    /// Check if a request should be allowed based on filter rules
    pub fn check_request_allowed(
        &self,
        sandbox_id: &str,
        url: &str,
        method: &str,
        headers: &HashMap<String, String>,
    ) -> (bool, Option<String>) {
        let proxies = self.proxies.lock().unwrap();
        if let Some(config) = proxies.get(sandbox_id) {
            // Enhanced URL validation and security checks
            match self.validate_url_security(url) {
                Ok(parsed_url) => {
                    if let Some(domain) = parsed_url.host_str() {
                        // Normalize domain (remove potential bypass attempts)
                        let normalized_domain = self.normalize_domain(domain);
                        
                        // Check for IP address bypasses
                        if self.is_ip_address(&normalized_domain) {
                            // Only allow if IP is explicitly whitelisted
                            if !config.allowed_domains.contains(&normalized_domain) {
                                return (false, Some(format!("Direct IP access not allowed: {}", normalized_domain)));
                            }
                        }
                        
                        // Check blacklist first (case-insensitive and normalized)
                        if self.domain_matches_any(&normalized_domain, &config.blocked_domains) {
                            return (false, Some(format!("Domain {} is blocked", normalized_domain)));
                        }
                        
                        // Check whitelist if it's not empty
                        if !config.allowed_domains.is_empty() && !self.domain_matches_any(&normalized_domain, &config.allowed_domains) {
                            return (false, Some(format!("Domain {} is not in whitelist", normalized_domain)));
                        }
                        
                        // Additional security checks
                        if let Err(reason) = self.validate_request_security(&parsed_url, method, headers) {
                            return (false, Some(reason));
                        }
                    } else {
                        return (false, Some("Invalid URL: no host specified".to_string()));
                    }
                },
                Err(reason) => {
                    return (false, Some(reason));
                }
            }
            
            // Check filter rules (sorted by priority)
            let mut sorted_rules = config.filter_rules.clone();
            sorted_rules.sort_by(|a, b| b.priority.cmp(&a.priority));
            
            for rule in &sorted_rules {
                if !rule.enabled {
                    continue;
                }
                
                let matches = match rule.rule_type {
                    FilterRuleType::Url => url.contains(&rule.pattern),
                    FilterRuleType::Method => method.eq_ignore_ascii_case(&rule.pattern),
                    FilterRuleType::Header => {
                        headers.iter().any(|(key, value)| {
                            key.contains(&rule.pattern) || value.contains(&rule.pattern)
                        })
                    },
                    FilterRuleType::UserAgent => {
                        headers.get("user-agent")
                            .map(|ua| ua.contains(&rule.pattern))
                            .unwrap_or(false)
                    },
                    FilterRuleType::Regex => {
                        // For now, simple contains check. Can be extended with actual regex
                        url.contains(&rule.pattern)
                    },
                    _ => false, // Other types need request body which we don't have here
                };
                
                if matches {
                    match rule.action {
                        FilterAction::Block => {
                            return (false, Some(format!("Blocked by rule: {}", rule.name)));
                        },
                        FilterAction::Allow => {
                            return (true, None);
                        },
                        _ => continue, // Other actions don't affect allowing/blocking here
                    }
                }
            }
        }
        
        // Default: allow
        (true, None)
    }
    
    /// List all active proxies
    pub fn list_active_proxies(&self) -> Vec<String> {
        self.proxies.lock().unwrap().keys().cloned().collect()
    }
    
    /// Reset statistics for a sandbox
    pub fn reset_stats(&mut self, sandbox_id: &str) {
        let mut stats = self.stats.lock().unwrap();
        if let Some(proxy_stats) = stats.get_mut(sandbox_id) {
            *proxy_stats = ProxyStats::default();
            proxy_stats.last_reset = chrono::Utc::now();
        }
    }
    
    /// Clear logs for a sandbox
    pub fn clear_logs(&mut self, sandbox_id: &str) {
        let mut logs = self.logs.lock().unwrap();
        if let Some(sandbox_logs) = logs.get_mut(sandbox_id) {
            sandbox_logs.clear();
        }
    }
    
    /// Enhanced URL security validation to prevent bypasses
    fn validate_url_security(&self, url: &str) -> Result<url::Url, String> {
        // Check for dangerous URL patterns
        if url.contains("@") && !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("Potentially malicious URL with @ character".to_string());
        }
        
        // Check for URL encoding bypasses
        if url.contains("%") {
            let decoded = urlencoding::decode(url).map_err(|_| "Invalid URL encoding")?;
            if decoded.contains("@") || decoded.contains("//") {
                return Err("URL encoding bypass attempt detected".to_string());
            }
        }
        
        // Parse URL with strict validation
        let parsed_url = url::Url::parse(url)
            .map_err(|e| format!("Invalid URL format: {}", e))?;
        
        // Validate scheme
        match parsed_url.scheme() {
            "http" | "https" => {},
            _ => return Err(format!("Unsupported URL scheme: {}", parsed_url.scheme())),
        }
        
        // Check for suspicious URL patterns
        if parsed_url.host_str().map_or(false, |h| h.is_empty()) {
            return Err("Empty host in URL".to_string());
        }
        
        Ok(parsed_url)
    }
    
    /// Normalize domain to prevent bypass attempts
    fn normalize_domain(&self, domain: &str) -> String {
        domain
            .to_lowercase()
            .trim_start_matches("www.")
            .trim_end_matches(".")
            .to_string()
    }
    
    /// Check if domain is an IP address
    fn is_ip_address(&self, domain: &str) -> bool {
        use std::net::IpAddr;
        domain.parse::<IpAddr>().is_ok()
    }
    
    /// Check if domain matches any in the given set (with wildcard support)
    fn domain_matches_any(&self, domain: &str, domain_set: &HashSet<String>) -> bool {
        // Direct match
        if domain_set.contains(domain) {
            return true;
        }
        
        // Check for wildcard matches (*.example.com)
        for pattern in domain_set {
            if pattern.starts_with("*.") {
                let pattern_domain = &pattern[2..];
                if domain.ends_with(pattern_domain) {
                    // Ensure it's a proper subdomain match
                    if domain == pattern_domain || domain.ends_with(&format!(".{}", pattern_domain)) {
                        return true;
                    }
                }
            }
        }
        
        false
    }
    
    /// Additional request security validation
    fn validate_request_security(
        &self, 
        parsed_url: &url::Url, 
        method: &str, 
        headers: &HashMap<String, String>
    ) -> Result<(), String> {
        // Check for dangerous ports
        if let Some(port) = parsed_url.port() {
            match port {
                22 | 23 | 25 | 53 | 110 | 143 | 993 | 995 => {
                    return Err(format!("Access to port {} is not allowed", port));
                },
                _ => {}
            }
        }
        
        // Check for potentially dangerous HTTP methods
        match method.to_uppercase().as_str() {
            "GET" | "POST" | "PUT" | "DELETE" | "HEAD" | "OPTIONS" => {},
            _ => return Err(format!("HTTP method {} is not allowed", method)),
        }
        
        // Check for suspicious headers
        for (header_name, header_value) in headers {
            let header_name_lower = header_name.to_lowercase();
            
            // Block headers that might be used for attacks
            if header_name_lower.starts_with("x-forwarded") ||
               header_name_lower.starts_with("x-real-ip") ||
               header_name_lower == "x-cluster-client-ip" {
                return Err(format!("Header {} is not allowed", header_name));
            }
            
            // Check for header injection attempts
            if header_value.contains('\n') || header_value.contains('\r') {
                return Err("Header injection attempt detected".to_string());
            }
        }
        
        Ok(())
    }
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:8888".parse().unwrap(),
            enable_https: false,
            tls_config: None,
            filter_rules: Vec::new(),
            allowed_domains: HashSet::new(),
            blocked_domains: HashSet::new(),
            allowed_content_types: {
                let mut types = HashSet::new();
                types.insert("text/html".to_string());
                types.insert("text/plain".to_string());
                types.insert("application/json".to_string());
                types.insert("application/xml".to_string());
                types.insert("application/javascript".to_string());
                types.insert("text/css".to_string());
                types
            },
            max_request_size: 10 * 1024 * 1024,  // 10MB
            max_response_size: 100 * 1024 * 1024, // 100MB
            enable_logging: true,
            enable_content_inspection: false,
            auth_config: None,
            timeout_config: TimeoutConfig::default(),
            rate_limit_config: None,
        }
    }
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect_timeout_secs: 10,
            request_timeout_secs: 30,
            response_timeout_secs: 30,
            keepalive_timeout_secs: 60,
        }
    }
}

impl Default for ProxyStats {
    fn default() -> Self {
        Self {
            total_requests: 0,
            blocked_requests: 0,
            bytes_transferred: 0,
            avg_response_time_ms: 0.0,
            active_connections: 0,
            error_count: 0,
            last_reset: chrono::Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_proxy_manager_creation() {
        let manager = ProxyManager::new();
        assert_eq!(manager.list_active_proxies().len(), 0);
    }
    
    #[tokio::test]
    async fn test_proxy_configuration() {
        let mut manager = ProxyManager::new();
        let config = ProxyConfig::default();
        
        let result = manager.configure_proxy("test-sandbox", config).await;
        assert!(result.is_ok());
        
        let proxies = manager.list_active_proxies();
        assert_eq!(proxies.len(), 1);
        assert!(proxies.contains(&"test-sandbox".to_string()));
    }
    
    #[tokio::test]
    async fn test_proxy_removal() {
        let mut manager = ProxyManager::new();
        let config = ProxyConfig::default();
        
        // Configure proxy
        manager.configure_proxy("test-sandbox", config).await.unwrap();
        assert_eq!(manager.list_active_proxies().len(), 1);
        
        // Remove proxy
        let result = manager.remove_proxy("test-sandbox").await;
        assert!(result.is_ok());
        assert_eq!(manager.list_active_proxies().len(), 0);
    }
    
    #[test]
    fn test_request_filtering() {
        let mut manager = ProxyManager::new();
        let mut config = ProxyConfig::default();
        
        // Add a block rule for a specific domain
        config.blocked_domains.insert("evil.com".to_string());
        
        // Add a filter rule to block certain URLs
        config.filter_rules.push(FilterRule {
            name: "Block admin pages".to_string(),
            rule_type: FilterRuleType::Url,
            pattern: "/admin".to_string(),
            action: FilterAction::Block,
            enabled: true,
            priority: 100,
        });
        
        let proxies = manager.proxies.clone();
        {
            let mut proxies = proxies.lock().unwrap();
            proxies.insert("test-sandbox".to_string(), config);
        }
        
        // Test blocked domain
        let (allowed, reason) = manager.check_request_allowed(
            "test-sandbox",
            "https://evil.com/page",
            "GET",
            &HashMap::new(),
        );
        assert!(!allowed);
        assert!(reason.is_some());
        
        // Test blocked URL pattern
        let (allowed, reason) = manager.check_request_allowed(
            "test-sandbox",
            "https://example.com/admin/users",
            "GET",
            &HashMap::new(),
        );
        assert!(!allowed);
        assert!(reason.is_some());
        
        // Test allowed request
        let (allowed, reason) = manager.check_request_allowed(
            "test-sandbox",
            "https://example.com/public/page",
            "GET",
            &HashMap::new(),
        );
        assert!(allowed);
        assert!(reason.is_none());
    }
    
    #[test]
    fn test_stats_update() {
        let mut manager = ProxyManager::new();
        
        // Initialize stats
        {
            let mut stats = manager.stats.lock().unwrap();
            stats.insert("test-sandbox".to_string(), ProxyStats::default());
        }
        
        // Update stats
        manager.update_stats("test-sandbox", false, 150, 1024);
        
        let stats = manager.get_proxy_stats("test-sandbox").unwrap();
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.blocked_requests, 0);
        assert_eq!(stats.bytes_transferred, 1024);
        assert_eq!(stats.avg_response_time_ms, 150.0);
    }
}