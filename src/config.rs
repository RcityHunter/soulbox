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
                    runtime_type: "firecracker".to_string(), // Default to Firecracker
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
    /// Load configuration from environment variables
    pub async fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok(); // Load .env file if present

        let mut config = Self::default();

        // Server configuration
        if let Ok(host) = std::env::var("SERVER_HOST") {
            config.server.host = host;
        }
        if let Ok(port) = std::env::var("SERVER_PORT") {
            config.server.port = port.parse().unwrap_or(8080);
        }

        // Database configuration
        if let Ok(url) = std::env::var("DATABASE_URL") {
            config.database.url = url;
        }

        // Redis configuration
        if let Ok(url) = std::env::var("REDIS_URL") {
            config.redis.url = url;
        }

        // Auth configuration
        if let Ok(secret) = std::env::var("JWT_SECRET") {
            config.auth.jwt_secret = secret;
        }

        Ok(config)
    }

    /// Load configuration from a TOML file (soulbox.toml)
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = tokio::fs::read_to_string(path).await?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.auth.jwt_secret == "your-secret-key" {
            anyhow::bail!("JWT secret must be changed from default value");
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