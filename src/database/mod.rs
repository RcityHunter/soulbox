pub mod config;
// pub mod connection;  // Legacy sqlx-based connection - disabled in favor of SurrealDB
pub mod models;
pub mod repositories;
pub mod migrations;  // SurrealDB schema initialization
pub mod surrealdb;
pub mod health_check;
pub mod batch_operations;

// Legacy exports for backward compatibility (commented out for SurrealDB migration)
// pub use config::{DatabaseConfig, DatabaseType, PoolConfig};
// pub use connection::{Database, DatabasePool};

// New SurrealDB exports
pub use surrealdb::{
    SurrealConfig, SurrealProtocol, SurrealPool, SurrealConnection,
    SurrealOperations, SurrealSchema, LiveQueries,
    SurrealResult, SurrealConnectionError
};

pub use repositories::*;
pub use migrations::MigrationManager;
pub use health_check::{DatabaseHealthChecker, HealthCheckReport, CheckStatus, PerformanceStats};
pub use batch_operations::BatchOperations;
// use surrealdb::sql::Value;

/// 数据库错误类型
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("Database connection error: {0}")]
    Connection(String),
    
    #[error("Query execution error: {0}")]
    Query(String),
    
    #[error("Migration error: {0}")]
    Migration(String),
    
    #[error("Transaction error: {0}")]
    Transaction(String),
    
    #[error("Record not found")]
    NotFound,
    
    #[error("Duplicate record")]
    Duplicate,
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Database error: {0}")]
    Other(String),
}

// Legacy sqlx compatibility code - commented out for SurrealDB migration
// #[cfg(feature = "sqlx-compat")]
// impl From<sqlx::Error> for DatabaseError {
//     fn from(err: sqlx::Error) -> Self {
//         match err {
//             sqlx::Error::RowNotFound => DatabaseError::NotFound,
//             sqlx::Error::Database(db_err) => {
//                 // 检查是否是唯一约束违规
//                 if db_err.message().contains("UNIQUE") || db_err.message().contains("duplicate") {
//                     DatabaseError::Duplicate
//                 } else {
//                     DatabaseError::Query(db_err.message().to_string())
//                 }
//             }
//             _ => DatabaseError::Other(err.to_string()),
//         }
//     }
// }

// Convert from SurrealDB connection errors
impl From<SurrealConnectionError> for DatabaseError {
    fn from(err: SurrealConnectionError) -> Self {
        match err {
            SurrealConnectionError::Connection(msg) => DatabaseError::Connection(msg),
            SurrealConnectionError::Query(msg) => DatabaseError::Query(msg),
            SurrealConnectionError::PoolExhausted => DatabaseError::Connection("Connection pool exhausted".to_string()),
            SurrealConnectionError::HealthCheck(msg) => DatabaseError::Connection(format!("Health check failed: {}", msg)),
            SurrealConnectionError::Auth(msg) => DatabaseError::Connection(format!("Authentication failed: {}", msg)),
            SurrealConnectionError::Config(msg) => DatabaseError::Connection(format!("Configuration error: {}", msg)),
            SurrealConnectionError::Surreal(e) => DatabaseError::Other(e.to_string()),
        }
    }
}

pub type DatabaseResult<T> = Result<T, DatabaseError>;

/// Database abstraction for SurrealDB  
pub enum DatabaseBackend {
    // Legacy sqlx support removed in favor of SurrealDB
    // #[cfg(feature = "sqlx-compat")]
    // Legacy(Database),
    Surreal(std::sync::Arc<SurrealPool>),
}

impl DatabaseBackend {
    /// Create a new SurrealDB backend
    pub async fn new_surreal(config: SurrealConfig) -> DatabaseResult<Self> {
        let pool = SurrealPool::new(config).await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        Ok(Self::Surreal(std::sync::Arc::new(pool)))
    }
    
    // Legacy sqlx backend support removed
    // #[cfg(feature = "sqlx-compat")]
    // pub async fn new_legacy(config: DatabaseConfig) -> DatabaseResult<Self> {
    //     let db = Database::new(config).await?;
    //     Ok(Self::Legacy(db))
    // }
    
    /// Get SurrealDB pool if using SurrealDB backend
    pub fn surreal_pool(&self) -> Option<std::sync::Arc<SurrealPool>> {
        match self {
            Self::Surreal(pool) => Some(pool.clone()),
            // Legacy support removed
            // #[cfg(feature = "sqlx-compat")]
            // Self::Legacy(_) => None,
        }
    }
    
    // Legacy database access removed
    // #[cfg(feature = "sqlx-compat")]
    // pub fn legacy_db(&self) -> Option<&Database> {
    //     match self {
    //         Self::Legacy(db) => Some(db),
    //         Self::Surreal(_) => None,
    //     }
    // }
    
    /// Health check for the database backend
    pub async fn health_check(&self) -> DatabaseResult<()> {
        match self {
            Self::Surreal(pool) => {
                let conn = pool.get_connection().await
                    .map_err(|e| DatabaseError::Connection(e.to_string()))?;
                // Simple health check query
                let ops = SurrealOperations::new(&conn);
                ops.query::<serde_json::Value>("SELECT 1 as health_check").await
                    .map_err(|e| DatabaseError::Query(e.to_string()))?;
                Ok(())
            }
            // Legacy health check removed
            // #[cfg(feature = "sqlx-compat")]
            // Self::Legacy(db) => db.health_check().await,
        }
    }
}