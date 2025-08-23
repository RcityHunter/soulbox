use serde::{Deserialize, Serialize};
use tracing::{debug, trace};
use uuid::Uuid;

use super::connection::{SurrealConnection, SurrealResult, SurrealConnectionError};

/// 基础 CRUD 操作包装器
pub struct SurrealOperations<'a> {
    conn: &'a SurrealConnection,
}

// Note: QueryBuilder and UpdateBuilder have been removed to eliminate over-engineering.
// Use direct SurrealQL queries with parameter binding instead for better security and simplicity.

/// 分页结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationResult<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

impl<'a> SurrealOperations<'a> {
    /// 创建新的操作实例
    pub fn new(conn: &'a SurrealConnection) -> Self {
        Self { conn }
    }
    
    /// 创建记录
    pub async fn create<T>(&self, table: &str, data: &T) -> SurrealResult<T>
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        debug!("创建记录: 表={}", table);
        trace!("数据: {:?}", serde_json::to_string(data));
        
        let sql = format!("CREATE {} CONTENT $data", table);
        
        let mut response = self.conn.db()
            .query(sql)
            .bind(("data", data))
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("创建记录失败: {}", e)))?;
        
        let result: Option<T> = response
            .take(0usize)
            .map_err(|e| SurrealConnectionError::Query(format!("解析创建结果失败: {}", e)))?;
        
        result.ok_or_else(|| SurrealConnectionError::Query("创建记录返回空结果".to_string()))
    }
    
    /// 根据ID查找记录
    pub async fn find_by_id<T>(&self, table: &str, id: &str) -> SurrealResult<Option<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        debug!("根据ID查找记录: 表={}, ID={}", table, id);
        
        let sql = format!("SELECT * FROM {}:{}", table, id);
        
        let mut response = self.conn.db()
            .query(sql)
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("查询记录失败: {}", e)))?;
        
        let result: Option<T> = response
            .take(0usize)
            .map_err(|e| SurrealConnectionError::Query(format!("解析查询结果失败: {}", e)))?;
        
        Ok(result)
    }
    
    /// 更新记录
    pub async fn update<T>(&self, table: &str, id: &str, data: &T) -> SurrealResult<T>
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        debug!("更新记录: 表={}, ID={}", table, id);
        
        let sql = format!("UPDATE {}:{} MERGE $data", table, id);
        
        let mut response = self.conn.db()
            .query(sql)
            .bind(("data", data))
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("更新记录失败: {}", e)))?;
        
        let result: Option<T> = response
            .take(0usize)
            .map_err(|e| SurrealConnectionError::Query(format!("解析更新结果失败: {}", e)))?;
        
        result.ok_or_else(|| SurrealConnectionError::Query("更新记录返回空结果".to_string()))
    }
    
    /// 删除记录
    pub async fn delete(&self, table: &str, id: &str) -> SurrealResult<bool> {
        debug!("删除记录: 表={}, ID={}", table, id);
        
        let sql = format!("DELETE {}:{}", table, id);
        
        let mut response = self.conn.db()
            .query(sql)
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("删除记录失败: {}", e)))?;
        
        // 检查是否有记录被删除
        let result: Result<Option<serde_json::Value>, _> = response.take::<Option<serde_json::Value>>(0usize);
        Ok(result.is_ok() && result.unwrap().is_some())
    }
    
    /// Execute custom SurrealQL query
    /// 
    /// This replaces the over-engineered QueryBuilder approach
    pub async fn query_custom<T>(&self, sql: &str) -> SurrealResult<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        debug!("自定义查询: {}", sql);
        
        let mut response = self.conn.db()
            .query(sql)
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("查询失败: {}", e)))?;
        
        let result: Vec<T> = response
            .take(0usize)
            .map_err(|e| SurrealConnectionError::Query(format!("解析查询结果失败: {}", e)))?;
        
        Ok(result)
    }
    
    /// Execute paginated query with custom SQL
    /// 
    /// This replaces the over-engineered QueryBuilder approach
    pub async fn query_paginated<T>(
        &self, 
        data_sql: &str,
        count_sql: &str,
        page: usize, 
        page_size: usize
    ) -> SurrealResult<PaginationResult<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        debug!("分页查询: 页={}, 大小={}", page, page_size);
        
        let mut response = self.conn.db()
            .query(data_sql)
            .query(count_sql)
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("分页查询失败: {}", e)))?;
        
        let items: Vec<T> = response
            .take(0usize)
            .map_err(|e| SurrealConnectionError::Query(format!("解析查询结果失败: {}", e)))?;
        
        let count_result: Vec<serde_json::Value> = response
            .take(1)
            .map_err(|e| SurrealConnectionError::Query(format!("解析计数结果失败: {}", e)))?;
        
        let total = if let Some(count_value) = count_result.first() {
            count_value.as_i64().unwrap_or(0)
        } else {
            0
        };
        
        let total_pages = ((total as f64) / (page_size as f64)).ceil() as usize;
        
        Ok(PaginationResult {
            items,
            total,
            page,
            page_size,
            total_pages,
        })
    }
    
    /// Execute custom update query
    /// 
    /// This replaces the over-engineered UpdateBuilder approach  
    pub async fn update_custom<T>(&self, sql: &str) -> SurrealResult<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        debug!("自定义更新: {}", sql);
        
        let mut response = self.conn.db()
            .query(sql)
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("更新失败: {}", e)))?;
        
        let result: Vec<T> = response
            .take(0usize)
            .map_err(|e| SurrealConnectionError::Query(format!("解析更新结果失败: {}", e)))?;
        
        Ok(result)
    }
    
    /// 执行原始 SurrealQL 查询
    pub async fn query<T>(&self, sql: &str) -> SurrealResult<Vec<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        debug!("执行原始查询: {}", sql);
        
        let mut response = self.conn.db()
            .query(sql)
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("原始查询失败: {}", e)))?;
        
        let result: Vec<T> = response
            .take(0usize)
            .map_err(|e| SurrealConnectionError::Query(format!("解析查询结果失败: {}", e)))?;
        
        Ok(result)
    }
    
    /// 开始事务
    pub async fn begin_transaction(&self) -> SurrealResult<()> {
        debug!("开始事务");
        
        self.conn.db()
            .query("BEGIN TRANSACTION")
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("开始事务失败: {}", e)))?;
        
        Ok(())
    }
    
    /// 提交事务
    pub async fn commit_transaction(&self) -> SurrealResult<()> {
        debug!("提交事务");
        
        self.conn.db()
            .query("COMMIT TRANSACTION")
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("提交事务失败: {}", e)))?;
        
        Ok(())
    }
    
    /// 回滚事务
    pub async fn rollback_transaction(&self) -> SurrealResult<()> {
        debug!("回滚事务");
        
        self.conn.db()
            .query("CANCEL TRANSACTION")
            .await
            .map_err(|e| SurrealConnectionError::Query(format!("回滚事务失败: {}", e)))?;
        
        Ok(())
    }
}

// Note: QueryBuilder and UpdateBuilder implementations have been removed.
// They were over-engineered and created SQL injection vulnerabilities.
// Use direct SurrealQL with parameter binding instead.

/// 辅助函数：将 UUID 转换为 SurrealDB 记录 ID
pub fn uuid_to_record_id(table: &str, uuid: Uuid) -> String {
    format!("{}:{}", table, uuid)
}

/// 辅助函数：从 SurrealDB 记录 ID 提取 UUID
pub fn record_id_to_uuid(record_id: &str) -> Result<Uuid, uuid::Error> {
    if let Some(uuid_str) = record_id.split(':').last() {
        uuid_str.parse()
    } else {
        // Return an error by trying to parse an invalid string
        "invalid-uuid".parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_uuid_record_id_conversion() {
        let uuid = Uuid::new_v4();
        let record_id = uuid_to_record_id("users", uuid);
        let parsed_uuid = record_id_to_uuid(&record_id).unwrap();
        
        assert_eq!(uuid, parsed_uuid);
    }
}