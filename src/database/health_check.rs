use tracing::{info, debug, error};
use crate::database::{DatabaseResult, DatabaseError};
use crate::database::surrealdb::{SurrealPool, SurrealOperations};

/// 数据库健康检查服务
pub struct DatabaseHealthChecker {
    pool: std::sync::Arc<SurrealPool>,
}

impl DatabaseHealthChecker {
    pub fn new(pool: std::sync::Arc<SurrealPool>) -> Self {
        Self { pool }
    }

    /// 执行完整的数据库健康检查
    pub async fn full_health_check(&self) -> DatabaseResult<HealthCheckReport> {
        info!("开始执行数据库健康检查");
        
        let mut report = HealthCheckReport::new();
        
        // 1. 基础连接测试
        match self.check_basic_connectivity().await {
            Ok(_) => {
                report.connectivity = CheckStatus::Passed;
                debug!("✓ 基础连接测试通过");
            }
            Err(e) => {
                report.connectivity = CheckStatus::Failed(e.to_string());
                error!("✗ 基础连接测试失败: {}", e);
            }
        }
        
        // 2. 表结构验证
        match self.check_table_schemas().await {
            Ok(_) => {
                report.schema_integrity = CheckStatus::Passed;
                debug!("✓ 表结构验证通过");
            }
            Err(e) => {
                report.schema_integrity = CheckStatus::Failed(e.to_string());
                error!("✗ 表结构验证失败: {}", e);
            }
        }
        
        // 3. 索引完整性检查
        match self.check_indexes().await {
            Ok(_) => {
                report.index_integrity = CheckStatus::Passed;
                debug!("✓ 索引完整性检查通过");
            }
            Err(e) => {
                report.index_integrity = CheckStatus::Failed(e.to_string());
                error!("✗ 索引完整性检查失败: {}", e);
            }
        }
        
        // 4. 数据一致性检查
        match self.check_data_consistency().await {
            Ok(_) => {
                report.data_consistency = CheckStatus::Passed;
                debug!("✓ 数据一致性检查通过");
            }
            Err(e) => {
                report.data_consistency = CheckStatus::Failed(e.to_string());
                error!("✗ 数据一致性检查失败: {}", e);
            }
        }
        
        // 5. 性能基准测试
        match self.performance_benchmark().await {
            Ok(stats) => {
                report.performance = CheckStatus::Passed;
                report.performance_stats = Some(stats);
                debug!("✓ 性能基准测试通过");
            }
            Err(e) => {
                report.performance = CheckStatus::Failed(e.to_string());
                error!("✗ 性能基准测试失败: {}", e);
            }
        }
        
        // 计算整体状态
        report.overall_status = if report.all_checks_passed() {
            CheckStatus::Passed
        } else {
            CheckStatus::Failed("部分检查失败".to_string())
        };
        
        info!("数据库健康检查完成，整体状态: {:?}", report.overall_status);
        Ok(report)
    }

    /// 基础连接测试
    async fn check_basic_connectivity(&self) -> DatabaseResult<()> {
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        // 简单查询测试
        ops.query::<serde_json::Value>("SELECT 1 as health_check").await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        Ok(())
    }

    /// 检查表结构
    async fn check_table_schemas(&self) -> DatabaseResult<()> {
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let required_tables = vec![
            "users", "api_keys", "sessions", "tenants", 
            "sandboxes", "templates", "audit_logs"
        ];
        
        for table in required_tables {
            let sql = format!("INFO FOR TABLE {}", table);
            
            let mut response = conn.db()
                .query(&sql)
                .await
                .map_err(|e| DatabaseError::Query(format!("检查表 {} 失败: {}", table, e)))?;
            
            let result: Vec<serde_json::Value> = response
                .take(0usize)
                .map_err(|e| DatabaseError::Query(format!("解析表信息失败: {}", e)))?;
            
            if result.is_empty() {
                return Err(DatabaseError::Other(format!("表 {} 不存在", table)));
            }
        }
        
        Ok(())
    }

    /// 检查索引完整性
    async fn check_indexes(&self) -> DatabaseResult<()> {
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        // 检查关键索引是否存在
        let critical_indexes = vec![
            ("users", "idx_users_username"),
            ("users", "idx_users_email"),
            ("sandboxes", "idx_sandboxes_owner"),
            ("audit_logs", "idx_audit_logs_timestamp"),
        ];
        
        for (table, index) in critical_indexes {
            let sql = format!("INFO FOR INDEX {} ON TABLE {}", index, table);
            
            match conn.db().query(&sql).await {
                Ok(_) => continue,
                Err(e) => {
                    return Err(DatabaseError::Other(
                        format!("关键索引 {} 在表 {} 中缺失: {}", index, table, e)
                    ));
                }
            }
        }
        
        Ok(())
    }

    /// 检查数据一致性
    async fn check_data_consistency(&self) -> DatabaseResult<()> {
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        // 检查外键关系的一致性
        // 1. 检查沙盒的拥有者是否存在
        let sql = "SELECT count() FROM sandboxes WHERE owner_id NOT IN (SELECT id FROM users) GROUP ALL";
        
        let mut response = conn.db()
            .query(sql)
            .await
            .map_err(|e| DatabaseError::Query(format!("检查数据一致性失败: {}", e)))?;
        
        let result: Vec<serde_json::Value> = response
            .take(0usize)
            .map_err(|e| DatabaseError::Query(format!("解析一致性检查结果失败: {}", e)))?;
        
        let orphaned_count = result.first()
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        
        if orphaned_count > 0 {
            return Err(DatabaseError::Other(
                format!("发现 {} 个孤立的沙盒记录", orphaned_count)
            ));
        }
        
        Ok(())
    }

    /// 性能基准测试
    async fn performance_benchmark(&self) -> DatabaseResult<PerformanceStats> {
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let start = std::time::Instant::now();
        
        // 执行一系列基准查询
        let queries = vec![
            "SELECT count() FROM users GROUP ALL",
            "SELECT count() FROM sandboxes GROUP ALL",
            "SELECT count() FROM audit_logs GROUP ALL",
        ];
        
        let mut total_time = std::time::Duration::default();
        let mut query_count = 0;
        
        for query in queries {
            let query_start = std::time::Instant::now();
            
            conn.db()
                .query(query)
                .await
                .map_err(|e| DatabaseError::Query(format!("基准查询失败: {}", e)))?;
            
            let query_time = query_start.elapsed();
            total_time += query_time;
            query_count += 1;
        }
        
        let avg_query_time = total_time / query_count;
        let total_elapsed = start.elapsed();
        
        Ok(PerformanceStats {
            total_time_ms: total_elapsed.as_millis() as u64,
            average_query_time_ms: avg_query_time.as_millis() as u64,
            queries_executed: query_count,
        })
    }
}

/// 健康检查报告
#[derive(Debug)]
pub struct HealthCheckReport {
    pub overall_status: CheckStatus,
    pub connectivity: CheckStatus,
    pub schema_integrity: CheckStatus,
    pub index_integrity: CheckStatus,
    pub data_consistency: CheckStatus,
    pub performance: CheckStatus,
    pub performance_stats: Option<PerformanceStats>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl HealthCheckReport {
    fn new() -> Self {
        Self {
            overall_status: CheckStatus::Unknown,
            connectivity: CheckStatus::Unknown,
            schema_integrity: CheckStatus::Unknown,
            index_integrity: CheckStatus::Unknown,
            data_consistency: CheckStatus::Unknown,
            performance: CheckStatus::Unknown,
            performance_stats: None,
            timestamp: chrono::Utc::now(),
        }
    }
    
    fn all_checks_passed(&self) -> bool {
        matches!(self.connectivity, CheckStatus::Passed) &&
        matches!(self.schema_integrity, CheckStatus::Passed) &&
        matches!(self.index_integrity, CheckStatus::Passed) &&
        matches!(self.data_consistency, CheckStatus::Passed) &&
        matches!(self.performance, CheckStatus::Passed)
    }
}

/// 检查状态
#[derive(Debug)]
pub enum CheckStatus {
    Unknown,
    Passed,
    Failed(String),
}

/// 性能统计
#[derive(Debug)]
pub struct PerformanceStats {
    pub total_time_ms: u64,
    pub average_query_time_ms: u64,
    pub queries_executed: u32,
}