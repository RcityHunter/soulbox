use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, error, debug};
use uuid::Uuid;

use crate::auth::models::{User, Role};
use crate::database::surrealdb::{
    SurrealPool, SurrealOperations, uuid_to_record_id, record_id_to_uuid
};
// use surrealdb::sql::Value;
use crate::database::{DatabaseError, DatabaseResult, models::DbUser};

/// SurrealDB 用户模型
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SurrealUser {
    pub id: String, // SurrealDB record ID
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub is_active: bool,
    pub tenant_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
}

/// 用户仓库
pub struct UserRepository {
    pool: Arc<SurrealPool>,
}

impl UserRepository {
    pub fn new(pool: Arc<SurrealPool>) -> Self {
        Self { pool }
    }
    
    /// 创建用户
    pub async fn create(&self, user: &User, password_hash: &str) -> DatabaseResult<()> {
        debug!("创建用户: {}", user.username);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let ops = SurrealOperations::new(&conn);
        
        let surreal_user = SurrealUser {
            id: uuid_to_record_id("users", user.id),
            username: user.username.clone(),
            email: user.email.clone(),
            password_hash: password_hash.to_string(),
            role: format!("{:?}", user.role),
            is_active: user.is_active,
            tenant_id: user.tenant_id.map(|id| uuid_to_record_id("tenants", id)),
            created_at: user.created_at,
            updated_at: user.created_at,
            last_login: user.last_login,
        };
        
        ops.create("users", &surreal_user).await
            .map_err(|e| DatabaseError::Query(e.to_string()))?;
        
        info!("Created user: {}", user.username);
        Ok(())
    }
    
    /// 根据ID查找用户
    pub async fn find_by_id(&self, id: Uuid) -> DatabaseResult<Option<DbUser>> {
        debug!("根据ID查找用户: {}", id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let record_id = uuid_to_record_id("users", id);
        
        // Use direct SurrealQL with proper record ID format
        let sql = "SELECT * FROM $record_id";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("record_id", &record_id))
            .await
            .map_err(|e| DatabaseError::Query(format!("查询用户失败: {}", e)))?;
        
        let users: Vec<SurrealUser> = response
            .take(0)
            .map_err(|e| DatabaseError::Query(format!("解析查询结果失败: {}", e)))?;
        
        if let Some(surreal_user) = users.into_iter().next() {
            let db_user = self.surreal_to_db_user(surreal_user)?;
            Ok(Some(db_user))
        } else {
            Ok(None)
        }
    }
    
    /// 根据用户名查找用户
    pub async fn find_by_username(&self, username: &str) -> DatabaseResult<Option<DbUser>> {
        debug!("根据用户名查找用户: {}", username);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        // Use direct SurrealQL with parameter binding to prevent SQL injection
        let sql = "SELECT * FROM users WHERE username = $username";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("username", username))
            .await
            .map_err(|e| DatabaseError::Query(format!("查询用户失败: {}", e)))?;
        
        let users: Vec<SurrealUser> = response
            .take(0)
            .map_err(|e| DatabaseError::Query(format!("解析查询结果失败: {}", e)))?;
        
        if let Some(surreal_user) = users.into_iter().next() {
            let db_user = self.surreal_to_db_user(surreal_user)?;
            Ok(Some(db_user))
        } else {
            Ok(None)
        }
    }
    
    /// 根据邮箱查找用户
    pub async fn find_by_email(&self, email: &str) -> DatabaseResult<Option<DbUser>> {
        debug!("根据邮箱查找用户: {}", email);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        // Use direct SurrealQL with parameter binding to prevent SQL injection
        let sql = "SELECT * FROM users WHERE email = $email";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("email", email))
            .await
            .map_err(|e| DatabaseError::Query(format!("查询用户失败: {}", e)))?;
        
        let users: Vec<SurrealUser> = response
            .take(0)
            .map_err(|e| DatabaseError::Query(format!("解析查询结果失败: {}", e)))?;
        
        if let Some(surreal_user) = users.into_iter().next() {
            let db_user = self.surreal_to_db_user(surreal_user)?;
            Ok(Some(db_user))
        } else {
            Ok(None)
        }
    }
    
    /// 更新用户
    pub async fn update(&self, user: &User) -> DatabaseResult<()> {
        debug!("更新用户: {}", user.username);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let record_id = uuid_to_record_id("users", user.id);
        
        // Use direct SurrealQL with parameter binding to prevent SQL injection
        let sql = if let Some(tenant_id) = user.tenant_id {
            "UPDATE $record_id SET username = $username, email = $email, role = $role, is_active = $is_active, tenant_id = $tenant_id, updated_at = time::now()"
        } else {
            "UPDATE $record_id SET username = $username, email = $email, role = $role, is_active = $is_active, tenant_id = NONE, updated_at = time::now()"
        };
        
        let mut query = conn.db()
            .query(sql)
            .bind(("record_id", &record_id))
            .bind(("username", &user.username))
            .bind(("email", &user.email))
            .bind(("role", format!("{:?}", user.role)))
            .bind(("is_active", user.is_active));
        
        if let Some(tenant_id) = user.tenant_id {
            let tenant_record_id = uuid_to_record_id("tenants", tenant_id);
            query = query.bind(("tenant_id", tenant_record_id));
        }
        
        let response = query.await
            .map_err(|e| DatabaseError::Query(format!("更新用户失败: {}", e)))?;
        
        if response.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        
        info!("Updated user: {}", user.username);
        Ok(())
    }
    
    /// 更新用户密码
    pub async fn update_password(&self, user_id: Uuid, password_hash: &str) -> DatabaseResult<()> {
        debug!("更新用户密码: {}", user_id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let record_id = uuid_to_record_id("users", user_id);
        
        // Use direct SurrealQL with parameter binding to prevent SQL injection
        let sql = "UPDATE $record_id SET password_hash = $password_hash, updated_at = time::now()";
        
        let response = conn.db()
            .query(sql)
            .bind(("record_id", &record_id))
            .bind(("password_hash", password_hash))
            .await
            .map_err(|e| DatabaseError::Query(format!("更新密码失败: {}", e)))?;
        
        if response.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        
        Ok(())
    }
    
    /// 更新最后登录时间
    pub async fn update_last_login(&self, user_id: Uuid) -> DatabaseResult<()> {
        debug!("更新最后登录时间: {}", user_id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let record_id = uuid_to_record_id("users", user_id);
        
        // Use direct SurrealQL with parameter binding
        let sql = "UPDATE $record_id SET last_login = time::now()";
        
        conn.db()
            .query(sql)
            .bind(("record_id", &record_id))
            .await
            .map_err(|e| DatabaseError::Query(format!("更新登录时间失败: {}", e)))?;
        
        Ok(())
    }
    
    /// 删除用户
    pub async fn delete(&self, user_id: Uuid) -> DatabaseResult<()> {
        debug!("删除用户: {}", user_id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let record_id = uuid_to_record_id("users", user_id);
        
        // Use direct SurrealQL with proper record ID format
        let sql = "DELETE $record_id";
        
        let response = conn.db()
            .query(sql)
            .bind(("record_id", &record_id))
            .await
            .map_err(|e| DatabaseError::Query(format!("删除用户失败: {}", e)))?;
        
        if response.is_empty() {
            return Err(DatabaseError::NotFound);
        }
        
        info!("Deleted user: {}", user_id);
        Ok(())
    }
    
    /// 列出租户下的用户
    pub async fn list_by_tenant(&self, tenant_id: Uuid, page: u32, page_size: u32) -> DatabaseResult<Vec<DbUser>> {
        debug!("列出租户 {} 下的用户，页码: {}, 大小: {}", tenant_id, page, page_size);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let tenant_record_id = uuid_to_record_id("tenants", tenant_id);
        let offset = ((page - 1) * page_size) as usize;
        
        // Use direct SurrealQL with parameter binding
        let sql = "SELECT * FROM users WHERE tenant_id = $tenant_id ORDER BY created_at DESC LIMIT $limit START $offset";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("tenant_id", &tenant_record_id))
            .bind(("limit", page_size as usize))
            .bind(("offset", offset))
            .await
            .map_err(|e| DatabaseError::Query(format!("查询租户用户失败: {}", e)))?;
        
        let surreal_users: Vec<SurrealUser> = response
            .take(0)
            .map_err(|e| DatabaseError::Query(format!("解析查询结果失败: {}", e)))?;
        
        let mut db_users = Vec::new();
        for surreal_user in surreal_users {
            db_users.push(self.surreal_to_db_user(surreal_user)?);
        }
        
        Ok(db_users)
    }
    
    /// 统计租户下的用户数
    pub async fn count_by_tenant(&self, tenant_id: Uuid) -> DatabaseResult<i64> {
        debug!("统计租户 {} 下的用户数", tenant_id);
        
        let conn = self.pool.get_connection().await
            .map_err(|e| DatabaseError::Connection(e.to_string()))?;
        
        let tenant_record_id = uuid_to_record_id("tenants", tenant_id);
        
        // Use direct SurrealQL with parameter binding
        let sql = "SELECT count() FROM users WHERE tenant_id = $tenant_id GROUP ALL";
        
        let mut response = conn.db()
            .query(sql)
            .bind(("tenant_id", &tenant_record_id))
            .await
            .map_err(|e| DatabaseError::Query(format!("统计用户数失败: {}", e)))?;
        
        let results: Vec<serde_json::Value> = response
            .take(0)
            .map_err(|e| DatabaseError::Query(format!("解析统计结果失败: {}", e)))?;
        
        let count = if let Some(count_value) = results.first() {
            count_value.as_int() as i64
        } else {
            0
        };
        
        Ok(count)
    }
    
    /// 将 SurrealUser 转换为 DbUser
    fn surreal_to_db_user(&self, surreal_user: SurrealUser) -> DatabaseResult<DbUser> {
        let id_str = surreal_user.id.split(':').last()
            .ok_or_else(|| DatabaseError::Other("Invalid record ID format".to_string()))?;
        
        let id = id_str.parse::<Uuid>()
            .map_err(|e| DatabaseError::Other(format!("Failed to parse UUID: {}", e)))?;
        
        let tenant_id = if let Some(tenant_record_id) = &surreal_user.tenant_id {
            let tenant_id_str = tenant_record_id.split(':').last()
                .ok_or_else(|| DatabaseError::Other("Invalid tenant record ID format".to_string()))?;
            Some(tenant_id_str.parse::<Uuid>()
                .map_err(|e| DatabaseError::Other(format!("Failed to parse tenant UUID: {}", e)))?)
        } else {
            None
        };
        
        Ok(DbUser {
            id,
            username: surreal_user.username,
            email: surreal_user.email,
            password_hash: surreal_user.password_hash,
            role: surreal_user.role,
            is_active: surreal_user.is_active,
            tenant_id,
            created_at: surreal_user.created_at,
            updated_at: surreal_user.updated_at,
            last_login: surreal_user.last_login,
        })
    }
}

// Note: Error conversion is now handled in src/database/mod.rs to avoid duplication

impl DbUser {
    /// 转换为领域模型
    pub fn to_domain_model(self) -> anyhow::Result<User> {
        let role = self.role.parse::<Role>()
            .unwrap_or(Role::User);
        
        Ok(User {
            id: self.id,
            username: self.username,
            email: self.email,
            role,
            is_active: self.is_active,
            created_at: self.created_at,
            last_login: self.last_login,
            tenant_id: self.tenant_id,
        })
    }
}