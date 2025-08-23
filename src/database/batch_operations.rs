use tracing::{debug, warn};
use crate::database::{DatabaseResult, DatabaseError};
use crate::database::surrealdb::{SurrealPool, SurrealOperations};

/// 批量操作辅助工具
pub struct BatchOperations {
    pool: std::sync::Arc<SurrealPool>,
    batch_size: usize,
}

impl BatchOperations {
    pub fn new(pool: std::sync::Arc<SurrealPool>) -> Self {
        Self {
            pool,
            batch_size: 100, // 默认批量大小
        }
    }
    
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }
    
    /// 批量创建记录
    pub async fn batch_create<T>(&self, table: &str, records: Vec<T>) -> DatabaseResult<usize>
    where
        T: serde::Serialize + for<'de> serde::Deserialize<'de> + Send + Sync,
    {
        if records.is_empty() {
            return Ok(0);
        }
        
        debug!("批量创建 {} 条记录到表 {}", records.len(), table);
        
        let mut total_created = 0;
        let mut failed_batches = 0;
        
        // 将记录分批处理
        for batch in records.chunks(self.batch_size) {
            match self.create_batch(table, batch).await {
                Ok(count) => {
                    total_created += count;
                    debug!("成功创建 {} 条记录", count);
                }
                Err(e) => {
                    failed_batches += 1;
                    warn!("批量创建失败: {}", e);
                    
                    // 如果批量失败，尝试逐个创建
                    total_created += self.fallback_individual_create(table, batch).await?;
                }
            }
        }
        
        if failed_batches > 0 {
            warn!("批量操作中有 {} 个批次失败，已通过单个创建恢复", failed_batches);
        }
        
        Ok(total_created)
    }
    
    /// 创建单个批次
    async fn create_batch<T>(&self, table: &str, batch: &[T]) -> DatabaseResult<usize>
    where
        T: serde::Serialize + for<'de> serde::Deserialize<'de> + Send + Sync,
    {
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        // 开始事务
        ops.begin_transaction().await
            .map_err(|e| DatabaseError::Transaction(e.to_string()))?;
        
        let mut created_count = 0;
        
        for record in batch {
            match ops.create(table, record).await {
                Ok(_) => created_count += 1,
                Err(e) => {
                    // 回滚事务并返回错误
                    ops.rollback_transaction().await
                        .map_err(|e| DatabaseError::Transaction(e.to_string()))?;
                    return Err(DatabaseError::Query(e.to_string()));
                }
            }
        }
        
        // 提交事务
        ops.commit_transaction().await
            .map_err(|e| DatabaseError::Transaction(e.to_string()))?;
        
        Ok(created_count)
    }
    
    /// 回退到逐个创建（当批量失败时）
    async fn fallback_individual_create<T>(&self, table: &str, batch: &[T]) -> DatabaseResult<usize>
    where
        T: serde::Serialize + for<'de> serde::Deserialize<'de> + Send + Sync,
    {
        debug!("批量创建失败，回退到逐个创建");
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        let mut created_count = 0;
        
        for record in batch {
            match ops.create(table, record).await {
                Ok(_) => created_count += 1,
                Err(e) => {
                    warn!("单个记录创建失败: {}", e);
                    // 继续处理下一个记录，而不是立即失败
                }
            }
        }
        
        Ok(created_count)
    }
    
    /// 批量删除记录
    pub async fn batch_delete(&self, table: &str, ids: Vec<String>) -> DatabaseResult<usize>
    where
    {
        if ids.is_empty() {
            return Ok(0);
        }
        
        debug!("批量删除 {} 条记录从表 {}", ids.len(), table);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        // 构建批量删除查询
        let id_list = ids.iter()
            .map(|id| format!("{}:{}", table, id))
            .collect::<Vec<_>>()
            .join(", ");
            
        let sql = format!("DELETE [{}] RETURN count()", id_list);
        
        let mut response = conn.db()
            .query(&sql)
            .await
            .map_err(|e| DatabaseError::Query(format!("批量删除失败: {}", e)))?;
        
        let result: Vec<serde_json::Value> = response
            .take(0usize)
            .map_err(|e| DatabaseError::Query(format!("解析删除结果失败: {}", e)))?;
        
        let deleted_count = result.first()
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        
        debug!("成功删除 {} 条记录", deleted_count);
        Ok(deleted_count)
    }
    
    /// 批量更新记录状态（常用于沙盒状态更新）
    pub async fn batch_update_status(&self, table: &str, updates: Vec<(String, String)>) -> DatabaseResult<usize>
    where
    {
        if updates.is_empty() {
            return Ok(0);
        }
        
        debug!("批量更新 {} 条记录状态在表 {}", updates.len(), table);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        // 开始事务
        ops.begin_transaction().await
            .map_err(|e| DatabaseError::Transaction(e.to_string()))?;
        
        let mut updated_count = 0;
        
        for (id, status) in updates {
            let sql = format!("UPDATE {}:{} SET status = $status, updated_at = time::now()", table, id);
            
            match conn.db()
                .query(&sql)
                .bind(("status", &status))
                .await
            {
                Ok(_) => updated_count += 1,
                Err(e) => {
                    warn!("更新记录 {} 状态失败: {}", id, e);
                    // 回滚事务
                    ops.rollback_transaction().await
                        .map_err(|e| DatabaseError::Transaction(e.to_string()))?;
                    return Err(DatabaseError::Query(e.to_string()));
                }
            }
        }
        
        // 提交事务
        ops.commit_transaction().await
            .map_err(|e| DatabaseError::Transaction(e.to_string()))?;
        
        debug!("成功更新 {} 条记录状态", updated_count);
        Ok(updated_count)
    }
    
    /// 批量查询记录（基于ID列表）
    pub async fn batch_find_by_ids<T>(&self, table: &str, ids: Vec<String>) -> DatabaseResult<Vec<T>>
    where
        T: for<'de> serde::Deserialize<'de> + Send,
    {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        
        debug!("批量查询 {} 条记录从表 {}", ids.len(), table);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        // 构建批量查询
        let id_list = ids.iter()
            .map(|id| format!("{}:{}", table, id))
            .collect::<Vec<_>>()
            .join(", ");
            
        let sql = format!("SELECT * FROM [{}]", id_list);
        
        let mut response = conn.db()
            .query(&sql)
            .await
            .map_err(|e| DatabaseError::Query(format!("批量查询失败: {}", e)))?;
        
        let records: Vec<T> = response
            .take(0usize)
            .map_err(|e| DatabaseError::Query(format!("解析查询结果失败: {}", e)))?;
        
        debug!("成功查询到 {} 条记录", records.len());
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::surrealdb::SurrealConfig;
    use serde::{Serialize, Deserialize};
    
    #[derive(Serialize, Deserialize, Debug)]
    struct TestRecord {
        id: String,
        name: String,
        value: i32,
    }
    
    #[tokio::test]
    async fn test_batch_operations() {
        let config = SurrealConfig::memory();
        let pool = std::sync::Arc::new(SurrealPool::new(config).await.unwrap());
        
        let batch_ops = BatchOperations::new(pool).with_batch_size(5);
        
        // 创建测试记录
        let test_records = vec![
            TestRecord { id: "test:1".to_string(), name: "Record 1".to_string(), value: 100 },
            TestRecord { id: "test:2".to_string(), name: "Record 2".to_string(), value: 200 },
            TestRecord { id: "test:3".to_string(), name: "Record 3".to_string(), value: 300 },
        ];
        
        // 测试批量创建
        let created_count = batch_ops.batch_create("test_records", test_records).await.unwrap();
        assert_eq!(created_count, 3);
        
        // 测试批量查询
        let ids = vec!["1".to_string(), "2".to_string(), "3".to_string()];
        let found_records: Vec<TestRecord> = batch_ops.batch_find_by_ids("test_records", ids).await.unwrap();
        assert_eq!(found_records.len(), 3);
        
        // 测试批量删除
        let delete_ids = vec!["1".to_string(), "2".to_string()];
        let deleted_count = batch_ops.batch_delete("test_records", delete_ids).await.unwrap();
        assert_eq!(deleted_count, 2);
    }
}