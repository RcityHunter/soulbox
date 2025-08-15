pub mod config;
pub mod connection;
pub mod models;
pub mod repositories;
pub mod migrations;

pub use config::{DatabaseConfig, DatabaseType, PoolConfig};
pub use connection::{Database, DatabasePool};
pub use repositories::*;

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

impl From<sqlx::Error> for DatabaseError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => DatabaseError::NotFound,
            sqlx::Error::Database(db_err) => {
                // 检查是否是唯一约束违规
                if db_err.message().contains("UNIQUE") || db_err.message().contains("duplicate") {
                    DatabaseError::Duplicate
                } else {
                    DatabaseError::Query(db_err.message().to_string())
                }
            }
            _ => DatabaseError::Other(err.to_string()),
        }
    }
}

pub type DatabaseResult<T> = Result<T, DatabaseError>;