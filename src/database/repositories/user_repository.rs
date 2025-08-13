use chrono::{DateTime, Utc};
use sqlx::Row;
use std::sync::Arc;
use tracing::{info, error};
use uuid::Uuid;

use crate::auth::models::{User, Role};
use crate::database::{Database, DatabaseError, DatabasePool, DatabaseResult, models::DbUser};

/// 用户仓库
pub struct UserRepository {
    db: Arc<Database>,
}

impl UserRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
    
    /// 创建用户
    pub async fn create(&self, user: &User, password_hash: &str) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                sqlx::query!(
                    r#"
                    INSERT INTO users (id, username, email, password_hash, role, is_active, tenant_id, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                    "#,
                    user.id,
                    user.username,
                    user.email,
                    password_hash,
                    format!("{:?}", user.role),
                    user.is_active,
                    user.tenant_id,
                    user.created_at,
                    user.created_at, // updated_at = created_at initially
                )
                .execute(pool)
                .await?;
            }
            DatabasePool::Sqlite(pool) => {
                let id = user.id.to_string();
                let role = format!("{:?}", user.role);
                let tenant_id = user.tenant_id.map(|t| t.to_string());
                let created_at = user.created_at.to_rfc3339();
                
                sqlx::query!(
                    r#"
                    INSERT INTO users (id, username, email, password_hash, role, is_active, tenant_id, created_at, updated_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                    "#,
                    id,
                    user.username,
                    user.email,
                    password_hash,
                    role,
                    user.is_active,
                    tenant_id,
                    created_at,
                    created_at, // updated_at = created_at initially
                )
                .execute(pool)
                .await?;
            }
        }
        
        info!("Created user: {}", user.username);
        Ok(())
    }
    
    /// 根据ID查找用户
    pub async fn find_by_id(&self, id: Uuid) -> DatabaseResult<Option<DbUser>> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let user = sqlx::query_as!(
                    DbUser,
                    "SELECT * FROM users WHERE id = $1",
                    id
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(user)
            }
            DatabasePool::Sqlite(pool) => {
                let id_str = id.to_string();
                let user = sqlx::query_as!(
                    DbUser,
                    "SELECT * FROM users WHERE id = ?1",
                    id_str
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(user)
            }
        }
    }
    
    /// 根据用户名查找用户
    pub async fn find_by_username(&self, username: &str) -> DatabaseResult<Option<DbUser>> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let user = sqlx::query_as!(
                    DbUser,
                    "SELECT * FROM users WHERE username = $1",
                    username
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(user)
            }
            DatabasePool::Sqlite(pool) => {
                let user = sqlx::query_as!(
                    DbUser,
                    "SELECT * FROM users WHERE username = ?1",
                    username
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(user)
            }
        }
    }
    
    /// 根据邮箱查找用户
    pub async fn find_by_email(&self, email: &str) -> DatabaseResult<Option<DbUser>> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let user = sqlx::query_as!(
                    DbUser,
                    "SELECT * FROM users WHERE email = $1",
                    email
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(user)
            }
            DatabasePool::Sqlite(pool) => {
                let user = sqlx::query_as!(
                    DbUser,
                    "SELECT * FROM users WHERE email = ?1",
                    email
                )
                .fetch_optional(pool)
                .await?;
                
                Ok(user)
            }
        }
    }
    
    /// 更新用户
    pub async fn update(&self, user: &User) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query!(
                    r#"
                    UPDATE users 
                    SET username = $2, email = $3, role = $4, is_active = $5, 
                        tenant_id = $6, updated_at = NOW()
                    WHERE id = $1
                    "#,
                    user.id,
                    user.username,
                    user.email,
                    format!("{:?}", user.role),
                    user.is_active,
                    user.tenant_id,
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
            DatabasePool::Sqlite(pool) => {
                let id = user.id.to_string();
                let role = format!("{:?}", user.role);
                let tenant_id = user.tenant_id.map(|t| t.to_string());
                
                let result = sqlx::query!(
                    r#"
                    UPDATE users 
                    SET username = ?2, email = ?3, role = ?4, is_active = ?5, 
                        tenant_id = ?6, updated_at = datetime('now')
                    WHERE id = ?1
                    "#,
                    id,
                    user.username,
                    user.email,
                    role,
                    user.is_active,
                    tenant_id,
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
        }
        
        info!("Updated user: {}", user.username);
        Ok(())
    }
    
    /// 更新用户密码
    pub async fn update_password(&self, user_id: Uuid, password_hash: &str) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query!(
                    "UPDATE users SET password_hash = $2, updated_at = NOW() WHERE id = $1",
                    user_id,
                    password_hash
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
            DatabasePool::Sqlite(pool) => {
                let id = user_id.to_string();
                let result = sqlx::query!(
                    "UPDATE users SET password_hash = ?2, updated_at = datetime('now') WHERE id = ?1",
                    id,
                    password_hash
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
        }
        
        Ok(())
    }
    
    /// 更新最后登录时间
    pub async fn update_last_login(&self, user_id: Uuid) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                sqlx::query!(
                    "UPDATE users SET last_login = NOW() WHERE id = $1",
                    user_id
                )
                .execute(pool)
                .await?;
            }
            DatabasePool::Sqlite(pool) => {
                let id = user_id.to_string();
                sqlx::query!(
                    "UPDATE users SET last_login = datetime('now') WHERE id = ?1",
                    id
                )
                .execute(pool)
                .await?;
            }
        }
        
        Ok(())
    }
    
    /// 删除用户
    pub async fn delete(&self, user_id: Uuid) -> DatabaseResult<()> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query!(
                    "DELETE FROM users WHERE id = $1",
                    user_id
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
            DatabasePool::Sqlite(pool) => {
                let id = user_id.to_string();
                let result = sqlx::query!(
                    "DELETE FROM users WHERE id = ?1",
                    id
                )
                .execute(pool)
                .await?;
                
                if result.rows_affected() == 0 {
                    return Err(DatabaseError::NotFound);
                }
            }
        }
        
        info!("Deleted user: {}", user_id);
        Ok(())
    }
    
    /// 列出租户下的用户
    pub async fn list_by_tenant(&self, tenant_id: Uuid, page: u32, page_size: u32) -> DatabaseResult<Vec<DbUser>> {
        let offset = ((page - 1) * page_size) as i64;
        let limit = page_size as i64;
        
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let users = sqlx::query_as!(
                    DbUser,
                    r#"
                    SELECT * FROM users 
                    WHERE tenant_id = $1 
                    ORDER BY created_at DESC 
                    LIMIT $2 OFFSET $3
                    "#,
                    tenant_id,
                    limit,
                    offset
                )
                .fetch_all(pool)
                .await?;
                
                Ok(users)
            }
            DatabasePool::Sqlite(pool) => {
                let tenant_id_str = tenant_id.to_string();
                let users = sqlx::query_as!(
                    DbUser,
                    r#"
                    SELECT * FROM users 
                    WHERE tenant_id = ?1 
                    ORDER BY created_at DESC 
                    LIMIT ?2 OFFSET ?3
                    "#,
                    tenant_id_str,
                    limit,
                    offset
                )
                .fetch_all(pool)
                .await?;
                
                Ok(users)
            }
        }
    }
    
    /// 统计租户下的用户数
    pub async fn count_by_tenant(&self, tenant_id: Uuid) -> DatabaseResult<i64> {
        match &*self.db.pool() {
            DatabasePool::Postgres(pool) => {
                let count = sqlx::query!(
                    "SELECT COUNT(*) as count FROM users WHERE tenant_id = $1",
                    tenant_id
                )
                .fetch_one(pool)
                .await?
                .count
                .unwrap_or(0);
                
                Ok(count)
            }
            DatabasePool::Sqlite(pool) => {
                let tenant_id_str = tenant_id.to_string();
                let count = sqlx::query!(
                    "SELECT COUNT(*) as count FROM users WHERE tenant_id = ?1",
                    tenant_id_str
                )
                .fetch_one(pool)
                .await?
                .count;
                
                Ok(count)
            }
        }
    }
}

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