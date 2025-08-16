//! SurrealDB 数据库模块
//! 
//! 提供 SurrealDB 连接管理、模式定义、基础操作等功能

pub mod config;
pub mod connection;
pub mod schema;
pub mod operations;
pub mod realtime;

pub use config::{SurrealConfig, SurrealProtocol, SurrealPoolConfig, RetryConfig};
pub use connection::{SurrealPool, SurrealConnection, SurrealConnectionError, SurrealResult, PoolStats};
pub use schema::{SurrealSchema, LiveQueries};
pub use operations::{SurrealOperations, PaginationResult, uuid_to_record_id, record_id_to_uuid};
pub use realtime::{RealtimeManager, RealtimeNotification, RealtimeEvent, SubscriptionManager, SubscriptionConfig};