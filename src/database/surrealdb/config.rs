use serde::{Deserialize, Serialize};
use std::time::Duration;

/// SurrealDB 连接协议
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SurrealProtocol {
    /// 内存数据库
    Memory,
    /// RocksDB 本地文件数据库
    RocksDb,
    /// WebSocket 连接
    WebSocket,
    /// HTTP 连接
    Http,
}

/// SurrealDB 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurrealConfig {
    /// 连接协议
    pub protocol: SurrealProtocol,
    
    /// 连接地址
    /// - Memory: "mem://"
    /// - RocksDB: "rocksdb://path/to/database"
    /// - WebSocket: "ws://host:port"
    /// - HTTP: "http://host:port"
    pub endpoint: String,
    
    /// 命名空间
    pub namespace: String,
    
    /// 数据库名称
    pub database: String,
    
    /// 用户名（用于远程连接）
    pub username: Option<String>,
    
    /// 密码（用于远程连接）
    pub password: Option<String>,
    
    /// 连接池配置
    pub pool: SurrealPoolConfig,
    
    /// 是否运行数据库初始化
    pub run_initialization: bool,
    
    /// 是否在开发模式下重置数据库
    pub reset_on_startup: bool,
    
    /// 连接重试配置
    pub retry: RetryConfig,
}

/// SurrealDB 连接池配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurrealPoolConfig {
    /// 最大连接数
    pub max_connections: usize,
    
    /// 最小连接数
    pub min_connections: usize,
    
    /// 连接超时时间（秒）
    pub connect_timeout_secs: u64,
    
    /// 空闲连接超时时间（秒）
    pub idle_timeout_secs: u64,
    
    /// 连接最大生命周期（秒）
    pub max_lifetime_secs: Option<u64>,
    
    /// 健康检查间隔（秒）
    pub health_check_interval_secs: u64,
}

/// 连接重试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_attempts: usize,
    
    /// 初始重试间隔（毫秒）
    pub initial_interval_ms: u64,
    
    /// 重试间隔倍数
    pub multiplier: f64,
    
    /// 最大重试间隔（毫秒）
    pub max_interval_ms: u64,
    
    /// 是否启用指数退避
    pub exponential_backoff: bool,
}

impl Default for SurrealConfig {
    fn default() -> Self {
        Self {
            protocol: SurrealProtocol::RocksDb,
            endpoint: "rocksdb://./data/soulbox".to_string(),
            namespace: "soulbox".to_string(),
            database: "main".to_string(),
            username: None,
            password: None,
            pool: SurrealPoolConfig::default(),
            run_initialization: true,
            reset_on_startup: false,
            retry: RetryConfig::default(),
        }
    }
}

impl Default for SurrealPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 2,
            connect_timeout_secs: 30,
            idle_timeout_secs: 300,
            max_lifetime_secs: Some(1800), // 30 minutes
            health_check_interval_secs: 60,
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_interval_ms: 1000,
            multiplier: 2.0,
            max_interval_ms: 30000,
            exponential_backoff: true,
        }
    }
}

impl SurrealConfig {
    /// 从环境变量创建配置
    pub fn from_env() -> anyhow::Result<Self> {
        let endpoint = std::env::var("SURREAL_ENDPOINT")
            .unwrap_or_else(|_| "rocksdb://./data/soulbox".to_string());
        
        let protocol = Self::detect_protocol(&endpoint)?;
        
        Ok(Self {
            protocol,
            endpoint,
            namespace: std::env::var("SURREAL_NAMESPACE")
                .unwrap_or_else(|_| "soulbox".to_string()),
            database: std::env::var("SURREAL_DATABASE")
                .unwrap_or_else(|_| "main".to_string()),
            username: std::env::var("SURREAL_USERNAME").ok(),
            password: std::env::var("SURREAL_PASSWORD").ok(),
            pool: SurrealPoolConfig {
                max_connections: std::env::var("SURREAL_MAX_CONNECTIONS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10),
                min_connections: std::env::var("SURREAL_MIN_CONNECTIONS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(2),
                connect_timeout_secs: std::env::var("SURREAL_CONNECT_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
                idle_timeout_secs: std::env::var("SURREAL_IDLE_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(300),
                max_lifetime_secs: std::env::var("SURREAL_MAX_LIFETIME")
                    .ok()
                    .and_then(|s| s.parse().ok()),
                health_check_interval_secs: std::env::var("SURREAL_HEALTH_CHECK_INTERVAL")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(60),
            },
            run_initialization: std::env::var("SURREAL_RUN_INIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),
            reset_on_startup: std::env::var("SURREAL_RESET_ON_STARTUP")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(false),
            retry: RetryConfig::default(),
        })
    }
    
    /// 检测协议类型
    fn detect_protocol(endpoint: &str) -> anyhow::Result<SurrealProtocol> {
        if endpoint.starts_with("mem://") {
            Ok(SurrealProtocol::Memory)
        } else if endpoint.starts_with("rocksdb://") {
            Ok(SurrealProtocol::RocksDb)
        } else if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
            Ok(SurrealProtocol::WebSocket)
        } else if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
            Ok(SurrealProtocol::Http)
        } else {
            anyhow::bail!("Unsupported SurrealDB endpoint: {}", endpoint);
        }
    }
    
    /// 获取连接超时时间
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.pool.connect_timeout_secs)
    }
    
    /// 获取空闲超时时间
    pub fn idle_timeout(&self) -> Duration {
        Duration::from_secs(self.pool.idle_timeout_secs)
    }
    
    /// 获取最大生命周期
    pub fn max_lifetime(&self) -> Option<Duration> {
        self.pool.max_lifetime_secs.map(Duration::from_secs)
    }
    
    /// 获取健康检查间隔
    pub fn health_check_interval(&self) -> Duration {
        Duration::from_secs(self.pool.health_check_interval_secs)
    }
    
    /// 获取初始重试间隔
    pub fn initial_retry_interval(&self) -> Duration {
        Duration::from_millis(self.retry.initial_interval_ms)
    }
    
    /// 获取最大重试间隔
    pub fn max_retry_interval(&self) -> Duration {
        Duration::from_millis(self.retry.max_interval_ms)
    }
    
    /// 是否需要认证
    pub fn requires_auth(&self) -> bool {
        matches!(self.protocol, SurrealProtocol::WebSocket | SurrealProtocol::Http)
            && (self.username.is_some() || self.password.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_surreal_config_default() {
        let config = SurrealConfig::default();
        assert_eq!(config.protocol, SurrealProtocol::RocksDb);
        assert_eq!(config.endpoint, "rocksdb://./data/soulbox");
        assert_eq!(config.namespace, "soulbox");
        assert_eq!(config.database, "main");
        assert_eq!(config.pool.max_connections, 10);
    }
    
    #[test]
    fn test_protocol_detection() {
        assert_eq!(
            SurrealConfig::detect_protocol("mem://").unwrap(),
            SurrealProtocol::Memory
        );
        assert_eq!(
            SurrealConfig::detect_protocol("rocksdb://./data").unwrap(),
            SurrealProtocol::RocksDb
        );
        assert_eq!(
            SurrealConfig::detect_protocol("ws://localhost:8000").unwrap(),
            SurrealProtocol::WebSocket
        );
        assert_eq!(
            SurrealConfig::detect_protocol("http://localhost:8000").unwrap(),
            SurrealProtocol::Http
        );
        
        assert!(SurrealConfig::detect_protocol("invalid://").is_err());
    }
    
    #[test]
    fn test_auth_requirements() {
        let mut config = SurrealConfig::default();
        assert!(!config.requires_auth());
        
        config.protocol = SurrealProtocol::WebSocket;
        assert!(!config.requires_auth());
        
        config.username = Some("admin".to_string());
        assert!(config.requires_auth());
    }
}