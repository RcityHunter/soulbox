use sqlx::{postgres::PgPoolOptions, sqlite::SqlitePoolOptions, Pool, Postgres, Sqlite};
use std::sync::Arc;
use tracing::{info, warn, error};

use super::{DatabaseConfig, DatabaseError, DatabaseResult, DatabaseType};

/// 数据库连接池类型
pub enum DatabasePool {
    Postgres(Pool<Postgres>),
    Sqlite(Pool<Sqlite>),
}

impl DatabasePool {
    /// 获取 PostgreSQL 连接池
    pub fn as_postgres(&self) -> Option<&Pool<Postgres>> {
        match self {
            DatabasePool::Postgres(pool) => Some(pool),
            _ => None,
        }
    }
    
    /// 获取 SQLite 连接池
    pub fn as_sqlite(&self) -> Option<&Pool<Sqlite>> {
        match self {
            DatabasePool::Sqlite(pool) => Some(pool),
            _ => None,
        }
    }
    
    /// 关闭连接池
    pub async fn close(&self) {
        match self {
            DatabasePool::Postgres(pool) => pool.close().await,
            DatabasePool::Sqlite(pool) => pool.close().await,
        }
    }
}

/// 数据库连接管理器
pub struct Database {
    pool: Arc<DatabasePool>,
    config: DatabaseConfig,
}

impl Database {
    /// 创建新的数据库连接
    pub async fn new(config: DatabaseConfig) -> DatabaseResult<Self> {
        info!("连接到数据库: {:?}", config.database_type);
        
        let pool = match config.database_type {
            DatabaseType::Postgres => {
                let pool = PgPoolOptions::new()
                    .max_connections(config.pool.max_connections)
                    .min_connections(config.pool.min_connections)
                    .acquire_timeout(config.connect_timeout())
                    .idle_timeout(Some(config.idle_timeout()))
                    .max_lifetime(config.max_lifetime())
                    .connect(&config.url)
                    .await
                    .map_err(|e| DatabaseError::Connection(format!("PostgreSQL 连接失败: {}", e)))?;
                
                info!("PostgreSQL 数据库连接成功");
                DatabasePool::Postgres(pool)
            }
            DatabaseType::Sqlite => {
                // 确保 SQLite 数据库目录存在
                if !config.url.contains(":memory:") {
                    if let Some(path) = config.url.strip_prefix("sqlite:") {
                        if let Some(parent) = std::path::Path::new(path).parent() {
                            std::fs::create_dir_all(parent)
                                .map_err(|e| DatabaseError::Connection(format!("创建数据库目录失败: {}", e)))?;
                        }
                    }
                }
                
                let pool = SqlitePoolOptions::new()
                    .max_connections(config.pool.max_connections)
                    .min_connections(config.pool.min_connections)
                    .acquire_timeout(config.connect_timeout())
                    .idle_timeout(Some(config.idle_timeout()))
                    .max_lifetime(config.max_lifetime())
                    .connect(&config.url)
                    .await
                    .map_err(|e| DatabaseError::Connection(format!("SQLite 连接失败: {}", e)))?;
                
                // 为 SQLite 启用外键约束
                sqlx::query("PRAGMA foreign_keys = ON")
                    .execute(&pool)
                    .await
                    .map_err(|e| DatabaseError::Connection(format!("启用外键约束失败: {}", e)))?;
                
                info!("SQLite 数据库连接成功");
                DatabasePool::Sqlite(pool)
            }
        };
        
        let db = Self {
            pool: Arc::new(pool),
            config: config.clone(),
        };
        
        // 运行迁移
        if config.run_migrations {
            db.run_migrations().await?;
        }
        
        Ok(db)
    }
    
    /// 获取数据库连接池
    pub fn pool(&self) -> Arc<DatabasePool> {
        self.pool.clone()
    }
    
    /// 获取数据库类型
    pub fn database_type(&self) -> &DatabaseType {
        &self.config.database_type
    }
    
    /// 运行数据库迁移
    pub async fn run_migrations(&self) -> DatabaseResult<()> {
        info!("运行数据库迁移...");
        
        match &*self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::migrate!("./migrations/postgres")
                    .run(pool)
                    .await
                    .map_err(|e| DatabaseError::Migration(format!("PostgreSQL 迁移失败: {}", e)))?;
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::migrate!("./migrations/sqlite")
                    .run(pool)
                    .await
                    .map_err(|e| DatabaseError::Migration(format!("SQLite 迁移失败: {}", e)))?;
            }
        }
        
        info!("数据库迁移完成");
        Ok(())
    }
    
    /// 重置数据库（仅用于开发环境）
    pub async fn reset(&self) -> DatabaseResult<()> {
        if !self.config.reset_on_startup {
            return Ok(());
        }
        
        warn!("⚠️ 重置数据库...");
        
        // 删除所有表
        match &*self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::query("DROP SCHEMA public CASCADE; CREATE SCHEMA public;")
                    .execute(pool)
                    .await
                    .map_err(|e| DatabaseError::Other(format!("重置 PostgreSQL 失败: {}", e)))?;
            }
            DatabasePool::Sqlite(_) => {
                // SQLite: 删除文件并重新连接
                if !self.config.url.contains(":memory:") {
                    if let Some(path) = self.config.url.strip_prefix("sqlite:") {
                        if std::path::Path::new(path).exists() {
                            std::fs::remove_file(path)
                                .map_err(|e| DatabaseError::Other(format!("删除 SQLite 文件失败: {}", e)))?;
                        }
                    }
                }
            }
        }
        
        // 重新运行迁移
        self.run_migrations().await?;
        
        info!("数据库重置完成");
        Ok(())
    }
    
    /// 检查数据库连接
    pub async fn health_check(&self) -> DatabaseResult<()> {
        match &*self.pool {
            DatabasePool::Postgres(pool) => {
                sqlx::query("SELECT 1")
                    .execute(pool)
                    .await
                    .map_err(|e| DatabaseError::Connection(format!("PostgreSQL 健康检查失败: {}", e)))?;
            }
            DatabasePool::Sqlite(pool) => {
                sqlx::query("SELECT 1")
                    .execute(pool)
                    .await
                    .map_err(|e| DatabaseError::Connection(format!("SQLite 健康检查失败: {}", e)))?;
            }
        }
        Ok(())
    }
    
    /// 开始事务
    pub async fn begin_transaction(&self) -> DatabaseResult<Transaction> {
        match &*self.pool {
            DatabasePool::Postgres(pool) => {
                let tx = pool.begin().await
                    .map_err(|e| DatabaseError::Transaction(format!("开始 PostgreSQL 事务失败: {}", e)))?;
                Ok(Transaction::Postgres(tx))
            }
            DatabasePool::Sqlite(pool) => {
                let tx = pool.begin().await
                    .map_err(|e| DatabaseError::Transaction(format!("开始 SQLite 事务失败: {}", e)))?;
                Ok(Transaction::Sqlite(tx))
            }
        }
    }
}

/// 数据库事务
pub enum Transaction {
    Postgres(sqlx::Transaction<'static, Postgres>),
    Sqlite(sqlx::Transaction<'static, Sqlite>),
}

impl Transaction {
    /// 提交事务
    pub async fn commit(self) -> DatabaseResult<()> {
        match self {
            Transaction::Postgres(tx) => tx.commit().await
                .map_err(|e| DatabaseError::Transaction(format!("提交 PostgreSQL 事务失败: {}", e)))?,
            Transaction::Sqlite(tx) => tx.commit().await
                .map_err(|e| DatabaseError::Transaction(format!("提交 SQLite 事务失败: {}", e)))?,
        }
        Ok(())
    }
    
    /// 回滚事务
    pub async fn rollback(self) -> DatabaseResult<()> {
        match self {
            Transaction::Postgres(tx) => tx.rollback().await
                .map_err(|e| DatabaseError::Transaction(format!("回滚 PostgreSQL 事务失败: {}", e)))?,
            Transaction::Sqlite(tx) => tx.rollback().await
                .map_err(|e| DatabaseError::Transaction(format!("回滚 SQLite 事务失败: {}", e)))?,
        }
        Ok(())
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_sqlite_memory_connection() {
        let config = DatabaseConfig {
            database_type: DatabaseType::Sqlite,
            url: "sqlite::memory:".to_string(),
            pool: Default::default(),
            run_migrations: false,
            reset_on_startup: false,
        };
        
        let db = Database::new(config).await;
        assert!(db.is_ok());
        
        let db = db.unwrap();
        let health = db.health_check().await;
        assert!(health.is_ok());
    }
}