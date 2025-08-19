use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub auth: AuthConfig,
    pub sandbox: SandboxConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
    pub max_connections: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
}

// Legacy database configuration conversion - disabled for SurrealDB migration
// impl From<DatabaseConfig> for crate::database::DatabaseConfig {
//     fn from(config: DatabaseConfig) -> Self {
//         let database_type = if config.url.starts_with("postgres://") || config.url.starts_with("postgresql://") {
//             crate::database::DatabaseType::Postgres
//         } else {
//             crate::database::DatabaseType::Sqlite
//         };
//         
//         crate::database::DatabaseConfig {
//             database_type,
//             url: config.url,
//             pool: crate::database::PoolConfig {
//                 max_connections: config.max_connections,
//                 min_connections: config.min_connections,
//                 ..Default::default()
//             },
//             run_migrations: true,
//             reset_on_startup: false,
//         }
//     }
// }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub jwt_expiry_hours: u64,
    pub api_key_prefix: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SandboxConfig {
    pub max_sandboxes: usize,
    pub default_timeout_seconds: u64,
    pub max_memory_mb: u64,
    pub max_cpu_cores: f32,
    pub runtime: RuntimeConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RuntimeConfig {
    pub runtime_type: String, // "docker" or "firecracker"
    pub firecracker_kernel_path: Option<String>,
    pub firecracker_rootfs_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                workers: 4,
                max_connections: 1000,
            },
            database: DatabaseConfig {
                url: "postgresql://soulbox:password@localhost:5432/soulbox".to_string(),
                max_connections: 20,
                min_connections: 5,
            },
            redis: RedisConfig {
                url: "redis://localhost:6379".to_string(),
                pool_size: 10,
            },
            auth: AuthConfig {
                jwt_secret: "your-secret-key".to_string(),
                jwt_expiry_hours: 24,
                api_key_prefix: "sb_".to_string(),
            },
            sandbox: SandboxConfig {
                max_sandboxes: 100,
                default_timeout_seconds: 300,
                max_memory_mb: 1024,
                max_cpu_cores: 2.0,
                runtime: RuntimeConfig {
                    runtime_type: "docker".to_string(), // Default to Docker
                    firecracker_kernel_path: Some("/opt/firecracker/vmlinux".to_string()),
                    firecracker_rootfs_path: Some("/opt/firecracker/rootfs".to_string()),
                },
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
            },
        }
    }
}

impl Config {
    /// Load configuration from environment variables with comprehensive validation
    pub async fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok(); // Load .env file if present

        let mut config = Self::default();
        let mut validation_errors = Vec::new();

        // Server configuration with validation
        if let Ok(host) = std::env::var("SERVER_HOST") {
            if Self::validate_host(&host) {
                config.server.host = host;
            } else {
                validation_errors.push("SERVER_HOST contains invalid characters".to_string());
            }
        }
        
        if let Ok(port_str) = std::env::var("SERVER_PORT") {
            match port_str.parse::<u16>() {
                Ok(port) if port > 0 && port != 22 && port != 80 && port != 443 => {
                    config.server.port = port;
                }
                Ok(port) => {
                    validation_errors.push(format!("SERVER_PORT {} is reserved or invalid", port));
                }
                Err(_) => {
                    validation_errors.push(format!("SERVER_PORT '{}' is not a valid port number", port_str));
                }
            }
        }
        
        if let Ok(workers_str) = std::env::var("SERVER_WORKERS") {
            match workers_str.parse::<usize>() {
                Ok(workers) if workers > 0 && workers <= 256 => {
                    config.server.workers = workers;
                }
                Ok(workers) => {
                    validation_errors.push(format!("SERVER_WORKERS {} is out of valid range (1-256)", workers));
                }
                Err(_) => {
                    validation_errors.push(format!("SERVER_WORKERS '{}' is not a valid number", workers_str));
                }
            }
        }

        // Database configuration with validation
        if let Ok(url) = std::env::var("DATABASE_URL") {
            if Self::validate_database_url(&url) {
                config.database.url = url;
            } else {
                validation_errors.push("DATABASE_URL format is invalid".to_string());
            }
        }
        
        if let Ok(max_conn_str) = std::env::var("DATABASE_MAX_CONNECTIONS") {
            match max_conn_str.parse::<u32>() {
                Ok(max_conn) if max_conn > 0 && max_conn <= 1000 => {
                    config.database.max_connections = max_conn;
                }
                Ok(max_conn) => {
                    validation_errors.push(format!("DATABASE_MAX_CONNECTIONS {} is out of valid range (1-1000)", max_conn));
                }
                Err(_) => {
                    validation_errors.push(format!("DATABASE_MAX_CONNECTIONS '{}' is not a valid number", max_conn_str));
                }
            }
        }

        // Redis configuration with validation
        if let Ok(url) = std::env::var("REDIS_URL") {
            if Self::validate_redis_url(&url) {
                config.redis.url = url;
            } else {
                validation_errors.push("REDIS_URL format is invalid".to_string());
            }
        }

        // Auth configuration with validation - JWT_SECRET is REQUIRED
        match std::env::var("JWT_SECRET") {
            Ok(secret) => {
                if Self::validate_jwt_secret_strength(&secret) {
                    config.auth.jwt_secret = secret;
                } else {
                    validation_errors.push("JWT_SECRET is too weak (minimum 64 characters required for production)".to_string());
                }
            }
            Err(_) => {
                validation_errors.push("JWT_SECRET environment variable is REQUIRED for security".to_string());
            }
        }
        
        if let Ok(expiry_str) = std::env::var("JWT_EXPIRY_HOURS") {
            match expiry_str.parse::<u64>() {
                Ok(expiry) if expiry > 0 && expiry <= 8760 => { // Max 1 year
                    config.auth.jwt_expiry_hours = expiry;
                }
                Ok(expiry) => {
                    validation_errors.push(format!("JWT_EXPIRY_HOURS {} is out of valid range (1-8760)", expiry));
                }
                Err(_) => {
                    validation_errors.push(format!("JWT_EXPIRY_HOURS '{}' is not a valid number", expiry_str));
                }
            }
        }

        // Sandbox configuration with validation
        if let Ok(max_sandboxes_str) = std::env::var("SANDBOX_MAX_COUNT") {
            match max_sandboxes_str.parse::<usize>() {
                Ok(max_sandboxes) if max_sandboxes > 0 && max_sandboxes <= 10000 => {
                    config.sandbox.max_sandboxes = max_sandboxes;
                }
                Ok(max_sandboxes) => {
                    validation_errors.push(format!("SANDBOX_MAX_COUNT {} is out of valid range (1-10000)", max_sandboxes));
                }
                Err(_) => {
                    validation_errors.push(format!("SANDBOX_MAX_COUNT '{}' is not a valid number", max_sandboxes_str));
                }
            }
        }
        
        if let Ok(timeout_str) = std::env::var("SANDBOX_TIMEOUT_SECONDS") {
            match timeout_str.parse::<u64>() {
                Ok(timeout) if timeout > 0 && timeout <= 3600 => { // Max 1 hour
                    config.sandbox.default_timeout_seconds = timeout;
                }
                Ok(timeout) => {
                    validation_errors.push(format!("SANDBOX_TIMEOUT_SECONDS {} is out of valid range (1-3600)", timeout));
                }
                Err(_) => {
                    validation_errors.push(format!("SANDBOX_TIMEOUT_SECONDS '{}' is not a valid number", timeout_str));
                }
            }
        }

        // Runtime configuration
        if let Ok(runtime_type) = std::env::var("SANDBOX_RUNTIME_TYPE") {
            if ["docker", "firecracker"].contains(&runtime_type.as_str()) {
                config.sandbox.runtime.runtime_type = runtime_type;
            } else {
                validation_errors.push(format!("SANDBOX_RUNTIME_TYPE '{}' is not supported (use 'docker' or 'firecracker')", runtime_type));
            }
        }

        // Check for validation errors
        if !validation_errors.is_empty() {
            return Err(anyhow::anyhow!(
                "Configuration validation failed:\n{}",
                validation_errors.join("\n")
            ));
        }

        // Perform final validation
        config.validate()?;

        Ok(config)
    }
    
    fn validate_host(host: &str) -> bool {
        // Allow localhost, IP addresses, and valid hostnames
        host == "localhost" || 
        host == "0.0.0.0" || 
        host.parse::<std::net::IpAddr>().is_ok() ||
        (host.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-') && !host.is_empty())
    }
    
    fn validate_database_url(url: &str) -> bool {
        url.starts_with("postgresql://") || 
        url.starts_with("postgres://") || 
        url.starts_with("sqlite://") ||
        url.ends_with(".db") ||
        url == ":memory:"
    }
    
    fn validate_redis_url(url: &str) -> bool {
        url.starts_with("redis://") || url.starts_with("rediss://")
    }
    
    fn validate_jwt_secret_strength(secret: &str) -> bool {
        // 生产环境要求更严格的密钥强度
        if secret.len() < 64 {
            return false;
        }
        
        // 检查不安全的默认值
        let unsafe_secrets = [
            "your-secret-key",
            "your-super-secret-jwt-key-change-this-in-production",
            "default-secret",
            "secret",
            "jwt-secret",
        ];
        
        !unsafe_secrets.contains(&secret)
    }

    /// Load configuration from a TOML file (soulbox.toml)
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = tokio::fs::read_to_string(path).await?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // 检查多种不安全的默认值
        let unsafe_secrets = [
            "your-secret-key",
            "your-super-secret-jwt-key-change-this-in-production",
            "default-secret",
            "secret",
            "jwt-secret",
        ];
        
        if unsafe_secrets.contains(&self.auth.jwt_secret.as_str()) {
            anyhow::bail!(
                "SECURITY ERROR: JWT secret must be changed from default value. \
                Please set a secure JWT_SECRET environment variable or update the configuration file."
            );
        }
        
        // 强制密钥长度和强度要求
        if self.auth.jwt_secret.len() < 64 {
            anyhow::bail!(
                "SECURITY ERROR: JWT secret must be at least 64 characters long for production use. \
                Current length: {}", 
                self.auth.jwt_secret.len()
            );
        }
        
        if self.server.port == 0 {
            anyhow::bail!("Server port cannot be 0");
        }

        if self.sandbox.max_memory_mb < 128 {
            anyhow::bail!("Minimum memory limit is 128MB");
        }

        Ok(())
    }
}