use surrealdb::engine::any::{connect, Any};
use surrealdb::opt::auth::Root;
use surrealdb::{Error as SurrealError, Surreal};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use tracing::{info, warn, error, debug};

use super::config::{SurrealConfig, SurrealProtocol};

/// SurrealDB 连接错误
#[derive(Debug, thiserror::Error)]
pub enum SurrealConnectionError {
    #[error("Database connection error: {0}")]
    Connection(String),
    
    #[error("Authentication error: {0}")]
    Auth(String),
    
    #[error("Query execution error: {0}")]
    Query(String),
    
    #[error("Connection pool exhausted")]
    PoolExhausted,
    
    #[error("Health check failed: {0}")]
    HealthCheck(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("SurrealDB error: {0}")]
    Surreal(#[from] SurrealError),
}

pub type SurrealResult<T> = Result<T, SurrealConnectionError>;

/// SurrealDB 连接池
pub struct SurrealPool {
    connections: Arc<RwLock<Vec<PooledConnection>>>,
    config: SurrealConfig,
    stats: Arc<Mutex<PoolStats>>,
}

/// 池化连接
#[derive(Debug)]
struct PooledConnection {
    db: Arc<Surreal<Any>>,
    created_at: Instant,
    last_used: Instant,
    in_use: Arc<AtomicBool>,
}

/// 连接池统计信息
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total_connections: usize,
    pub active_connections: usize,
    pub idle_connections: usize,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub total_query_time_ms: u64,
}

impl Default for PoolStats {
    fn default() -> Self {
        Self {
            total_connections: 0,
            active_connections: 0,
            idle_connections: 0,
            total_requests: 0,
            failed_requests: 0,
            total_query_time_ms: 0,
        }
    }
}

impl SurrealPool {
    /// 创建新的连接池
    pub async fn new(config: SurrealConfig) -> SurrealResult<Self> {
        info!("初始化 SurrealDB 连接池...");
        debug!("配置: 协议={:?}, 端点={}, 命名空间={}, 数据库={}", 
               config.protocol, config.endpoint, config.namespace, config.database);
        
        let pool = Self {
            connections: Arc::new(RwLock::new(Vec::new())),
            config: config.clone(),
            stats: Arc::new(Mutex::new(PoolStats::default())),
        };
        
        // 预热连接池
        pool.warm_up().await?;
        
        // 启动健康检查任务
        pool.start_health_check().await;
        
        info!("SurrealDB 连接池初始化完成");
        Ok(pool)
    }
    
    /// 获取连接
    pub async fn get_connection(&self) -> SurrealResult<SurrealConnection> {
        debug!("获取数据库连接...");
        
        // 尝试获取空闲连接
        if let Some((conn, in_use_flag)) = self.get_idle_connection().await {
            debug!("复用现有连接");
            return Ok(SurrealConnection::new_with_flag(conn, self.stats.clone(), Some(in_use_flag)));
        }
        
        // 检查是否可以创建新连接
        let connections = self.connections.read().await;
        if connections.len() >= self.config.pool.max_connections {
            error!("连接池已满，无法创建新连接");
            return Err(SurrealConnectionError::PoolExhausted);
        }
        drop(connections);
        
        // 创建新连接
        debug!("创建新的数据库连接");
        let db = self.create_connection().await?;
        let pooled_conn = PooledConnection {
            db: Arc::new(db),
            created_at: Instant::now(),
            last_used: Instant::now(),
            in_use: Arc::new(AtomicBool::new(true)),
        };
        
        // 添加到连接池
        let mut connections = self.connections.write().await;
        connections.push(pooled_conn);
        let conn = connections.last().unwrap();
        
        Ok(SurrealConnection::new_with_flag(conn.db.clone(), self.stats.clone(), Some(conn.in_use.clone())))
    }
    
    /// 获取空闲连接
    async fn get_idle_connection(&self) -> Option<(Arc<Surreal<Any>>, Arc<AtomicBool>)> {
        let mut connections = self.connections.write().await;
        
        for conn in connections.iter_mut() {
            // Try to atomically acquire the connection
            if conn.in_use.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                // 检查连接是否过期
                if let Some(max_lifetime) = self.config.max_lifetime() {
                    if conn.created_at.elapsed() > max_lifetime {
                        conn.in_use.store(false, Ordering::SeqCst);
                        continue;
                    }
                }
                
                // 检查连接是否空闲太久
                if conn.last_used.elapsed() > self.config.idle_timeout() {
                    conn.in_use.store(false, Ordering::SeqCst);
                    continue;
                }
                
                conn.last_used = Instant::now();
                return Some((conn.db.clone(), conn.in_use.clone()));
            }
        }
        
        None
    }
    
    /// 创建新的数据库连接
    async fn create_connection(&self) -> SurrealResult<Surreal<Any>> {
        debug!("连接到 SurrealDB: {}", self.config.endpoint);
        
        let db = self.connect_with_retry().await?;
        
        // 设置命名空间和数据库
        db.use_ns(&self.config.namespace)
            .use_db(&self.config.database)
            .await
            .map_err(|e| SurrealConnectionError::Connection(format!("设置命名空间/数据库失败: {}", e)))?;
        
        debug!("数据库连接建立成功");
        Ok(db)
    }
    
    /// 带重试的连接
    async fn connect_with_retry(&self) -> SurrealResult<Surreal<Any>> {
        let mut attempts = 0;
        let mut interval = self.config.initial_retry_interval();
        
        loop {
            match self.try_connect().await {
                Ok(db) => return Ok(db),
                Err(e) => {
                    attempts += 1;
                    if attempts >= self.config.retry.max_attempts {
                        error!("连接失败，已达到最大重试次数: {}", e);
                        return Err(e);
                    }
                    
                    warn!("连接失败，{} 秒后重试 (尝试 {}/{}): {}", 
                          interval.as_secs(), attempts, self.config.retry.max_attempts, e);
                    
                    sleep(interval).await;
                    
                    // 指数退避
                    if self.config.retry.exponential_backoff {
                        interval = Duration::from_millis(
                            (interval.as_millis() as f64 * self.config.retry.multiplier) as u64
                        ).min(self.config.max_retry_interval());
                    }
                }
            }
        }
    }
    
    /// 尝试连接
    async fn try_connect(&self) -> SurrealResult<Surreal<Any>> {
        let db = connect(&self.config.endpoint).await
            .map_err(|e| SurrealConnectionError::Connection(format!("连接失败: {}", e)))?;
        
        // 如果需要认证
        if self.config.requires_auth() {
            if let (Some(username), Some(password)) = (&self.config.username, &self.config.password) {
                db.signin(Root {
                    username,
                    password,
                })
                .await
                .map_err(|e| SurrealConnectionError::Auth(format!("认证失败: {}", e)))?;
            }
        }
        
        Ok(db)
    }
    
    /// 预热连接池
    async fn warm_up(&self) -> SurrealResult<()> {
        info!("预热连接池，创建 {} 个连接...", self.config.pool.min_connections);
        
        let mut connections = self.connections.write().await;
        for i in 0..self.config.pool.min_connections {
            debug!("创建预热连接 {}/{}", i + 1, self.config.pool.min_connections);
            
            let db = self.create_connection().await
                .map_err(|e| {
                    error!("预热连接创建失败: {}", e);
                    e
                })?;
            
            connections.push(PooledConnection {
                db: Arc::new(db),
                created_at: Instant::now(),
                last_used: Instant::now(),
                in_use: Arc::new(AtomicBool::new(false)),
            });
        }
        
        info!("连接池预热完成");
        Ok(())
    }
    
    /// 启动健康检查任务
    async fn start_health_check(&self) {
        let connections = self.connections.clone();
        let config = self.config.clone();
        let stats = self.stats.clone();
        
        tokio::spawn(async move {
            let interval = config.health_check_interval();
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                debug!("执行连接池健康检查...");
                
                let mut conn_guard = connections.write().await;
                let mut to_remove = Vec::new();
                
                for (i, conn) in conn_guard.iter_mut().enumerate() {
                    // 检查连接是否过期或空闲太久
                    let should_remove = if let Some(max_lifetime) = config.max_lifetime() {
                        conn.created_at.elapsed() > max_lifetime
                    } else {
                        false
                    } || (!conn.in_use.load(Ordering::SeqCst) && conn.last_used.elapsed() > config.idle_timeout());
                    
                    if should_remove {
                        debug!("移除过期连接 {}", i);
                        to_remove.push(i);
                        continue;
                    }
                    
                    // 对空闲连接进行健康检查
                    if !conn.in_use.load(Ordering::SeqCst) {
                        if let Err(e) = Self::health_check_connection(&conn.db).await {
                            warn!("连接健康检查失败，移除连接: {}", e);
                            to_remove.push(i);
                        }
                    }
                }
                
                // 倒序移除，避免索引问题
                for &i in to_remove.iter().rev() {
                    conn_guard.remove(i);
                }
                
                // 更新统计信息
                let mut stats_guard = stats.lock().await;
                stats_guard.total_connections = conn_guard.len();
                stats_guard.active_connections = conn_guard.iter().filter(|c| c.in_use.load(Ordering::SeqCst)).count();
                stats_guard.idle_connections = stats_guard.total_connections - stats_guard.active_connections;
                
                debug!("健康检查完成: 总连接数={}, 活跃={}, 空闲={}", 
                       stats_guard.total_connections, 
                       stats_guard.active_connections, 
                       stats_guard.idle_connections);
            }
        });
    }
    
    /// 单个连接健康检查
    async fn health_check_connection(db: &Surreal<Any>) -> SurrealResult<()> {
        // 1. Basic connectivity test
        let mut result = db.query("SELECT 1 as health_check")
            .await
            .map_err(|e| SurrealConnectionError::HealthCheck(format!("基础连接检查失败: {}", e)))?;
        
        // Try to take the first result to check if query was successful
        let check_result: Result<Option<serde_json::Value>, _> = result.take(0);
        match check_result {
            Ok(Some(_)) => {}, // Query successful
            Ok(None) | Err(_) => return Err(SurrealConnectionError::HealthCheck("查询未返回结果".to_string())),
        }
        
        // 2. Check namespace and database access
        let mut result = db.query("INFO FOR DB")
            .await
            .map_err(|e| SurrealConnectionError::HealthCheck(format!("数据库信息查询失败: {}", e)))?;
        
        // Try to take the first result to check if query was successful
        let check_result: Result<Option<serde_json::Value>, _> = result.take(0);
        match check_result {
            Ok(Some(_)) => {}, // Query successful
            Ok(None) | Err(_) => return Err(SurrealConnectionError::HealthCheck("无法获取数据库信息".to_string())),
        }
        
        // 3. Check write permissions by creating a temporary record
        let test_record_id = format!("health_check:{}", uuid::Uuid::new_v4());
        
        // Create test record
        db.query("CREATE $record_id SET test = true, timestamp = time::now()")
            .bind(("record_id", &test_record_id))
            .await
            .map_err(|e| SurrealConnectionError::HealthCheck(format!("写入权限检查失败: {}", e)))?;
        
        // Read test record
        let mut result = db.query("SELECT * FROM $record_id")
            .bind(("record_id", &test_record_id))
            .await
            .map_err(|e| SurrealConnectionError::HealthCheck(format!("读取权限检查失败: {}", e)))?;
        
        // Try to take the first result to check if query was successful
        let check_result: Result<Option<serde_json::Value>, _> = result.take(0);
        match check_result {
            Ok(Some(_)) => {}, // Query successful
            Ok(None) | Err(_) => return Err(SurrealConnectionError::HealthCheck("无法读取测试记录".to_string())),
        }
        
        // Delete test record
        db.query("DELETE $record_id")
            .bind(("record_id", &test_record_id))
            .await
            .map_err(|e| SurrealConnectionError::HealthCheck(format!("删除权限检查失败: {}", e)))?;
        
        debug!("数据库连接健康检查通过");
        Ok(())
    }
    
    /// 获取连接池统计信息
    pub async fn stats(&self) -> PoolStats {
        self.stats.lock().await.clone()
    }
    
    /// 关闭连接池
    pub async fn close(&self) {
        info!("关闭 SurrealDB 连接池...");
        
        let mut connections = self.connections.write().await;
        connections.clear();
        
        info!("SurrealDB connection pool closed ");
    }
}

/// 包装的数据库连接
pub struct SurrealConnection {
    db: Arc<Surreal<Any>>,
    stats: Arc<Mutex<PoolStats>>,
    created_at: Instant,
    in_use_flag: Option<Arc<AtomicBool>>,
}

impl SurrealConnection {
    fn new(db: Arc<Surreal<Any>>, stats: Arc<Mutex<PoolStats>>) -> Self {
        Self {
            db,
            stats,
            created_at: Instant::now(),
            in_use_flag: None,
        }
    }
    
    fn new_with_flag(db: Arc<Surreal<Any>>, stats: Arc<Mutex<PoolStats>>, in_use_flag: Option<Arc<AtomicBool>>) -> Self {
        Self {
            db,
            stats,
            created_at: Instant::now(),
            in_use_flag,
        }
    }
    
    /// 获取底层数据库连接
    pub fn db(&self) -> &Surreal<Any> {
        &self.db
    }
    
    /// 执行查询并更新统计信息
    pub async fn query<T>(&self, sql: &str) -> SurrealResult<T>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        let start = Instant::now();
        
        let mut stats = self.stats.lock().await;
        stats.total_requests += 1;
        drop(stats);
        
        let result = self.db
            .query(sql)
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("Query execution failed: {}", e)))?
            .take(0)
            .map_err(|e| SurrealConnectionError::Query(format!("Result parsing failed: {}", e)));
        
        let elapsed = start.elapsed();
        
        let mut stats = self.stats.lock().await;
        stats.total_query_time_ms += elapsed.as_millis() as u64;
        if result.is_err() {
            stats.failed_requests += 1;
        }
        
        result
    }
}

impl Drop for SurrealConnection {
    fn drop(&mut self) {
        debug!("Releasing database connection, usage time: {:?}", self.created_at.elapsed());
        
        // Release the connection back to the pool by setting in_use to false
        if let Some(ref in_use_flag) = self.in_use_flag {
            in_use_flag.store(false, std::sync::atomic::Ordering::SeqCst);
            debug!("Connection released back to pool ");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_memory_connection_pool() {
        let config = SurrealConfig {
            protocol: SurrealProtocol::Memory,
            endpoint: "mem://".to_string(),
            namespace: "test".to_string(),
            database: "test".to_string(),
            ..Default::default()
        };
        
        let pool = SurrealPool::new(config).await;
        assert!(pool.is_ok());
        
        let pool = pool.unwrap();
        let connection = pool.get_connection().await;
        assert!(connection.is_ok());
        
        let stats = pool.stats().await;
        assert!(stats.total_connections > 0);
    }
    
    #[tokio::test]
    async fn test_connection_health_check() {
        let config = SurrealConfig {
            protocol: SurrealProtocol::Memory,
            endpoint: "mem://".to_string(),
            namespace: "test".to_string(),
            database: "test".to_string(),
            ..Default::default()
        };
        
        let pool = SurrealPool::new(config).await.unwrap();
        let conn = pool.get_connection().await.unwrap();
        
        let result: Result<Vec<surrealdb::Value>, _> = conn.query("SELECT 1 as test").await;
        assert!(result.is_ok());
    }
}