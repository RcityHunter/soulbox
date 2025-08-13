use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 数据库类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    Postgres,
    Sqlite,
}

/// 数据库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// 数据库类型
    pub database_type: DatabaseType,
    
    /// 数据库连接URL
    /// PostgreSQL: postgres://user:password@host:port/database
    /// SQLite: sqlite://path/to/database.db or sqlite::memory:
    pub url: String,
    
    /// 连接池配置
    pub pool: PoolConfig,
    
    /// 是否运行迁移
    pub run_migrations: bool,
    
    /// 是否在开发模式下重置数据库
    pub reset_on_startup: bool,
}

/// 连接池配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// 最大连接数
    pub max_connections: u32,
    
    /// 最小空闲连接数
    pub min_connections: u32,
    
    /// 连接超时时间（秒）
    pub connect_timeout_secs: u64,
    
    /// 空闲连接超时时间（秒）
    pub idle_timeout_secs: u64,
    
    /// 连接最大生命周期（秒）
    pub max_lifetime_secs: Option<u64>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            database_type: DatabaseType::Sqlite,
            url: "sqlite:./data/soulbox.db".to_string(),
            pool: PoolConfig::default(),
            run_migrations: true,
            reset_on_startup: false,
        }
    }
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 1,
            connect_timeout_secs: 30,
            idle_timeout_secs: 600,
            max_lifetime_secs: Some(1800), // 30 minutes
        }
    }
}

impl DatabaseConfig {
    /// 从环境变量创建配置
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:./data/soulbox.db".to_string());
        
        let database_type = if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
            DatabaseType::Postgres
        } else if database_url.starts_with("sqlite:") {
            DatabaseType::Sqlite
        } else {
            anyhow::bail!("Unsupported database URL: {}", database_url);
        };
        
        Ok(Self {
            database_type,
            url: database_url,
            pool: PoolConfig {
                max_connections: std::env::var("DB_MAX_CONNECTIONS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10),
                min_connections: std::env::var("DB_MIN_CONNECTIONS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1),
                connect_timeout_secs: std::env::var("DB_CONNECT_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
                idle_timeout_secs: std::env::var("DB_IDLE_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(600),
                max_lifetime_secs: std::env::var("DB_MAX_LIFETIME")
                    .ok()
                    .and_then(|s| s.parse().ok()),
            },
            run_migrations: std::env::var("DB_RUN_MIGRATIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(true),
            reset_on_startup: std::env::var("DB_RESET_ON_STARTUP")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(false),
        })
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
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_database_config_default() {
        let config = DatabaseConfig::default();
        assert_eq!(config.database_type, DatabaseType::Sqlite);
        assert_eq!(config.url, "sqlite:./data/soulbox.db");
        assert_eq!(config.pool.max_connections, 10);
    }
    
    #[test]
    fn test_database_type_detection() {
        let config = DatabaseConfig::from_env();
        assert!(config.is_ok());
    }
}