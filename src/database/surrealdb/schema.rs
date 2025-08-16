use tracing::{info, warn};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;

use super::connection::SurrealResult;

/// SurrealDB 数据库初始化器
pub struct SurrealSchema;

impl SurrealSchema {
    /// 初始化数据库模式
    pub async fn initialize(db: &Surreal<Any>) -> SurrealResult<()> {
        info!("初始化 SurrealDB 数据库模式...");
        
        // 创建表和约束
        Self::create_users_table(db).await?;
        Self::create_api_keys_table(db).await?;
        Self::create_sessions_table(db).await?;
        Self::create_tenants_table(db).await?;
        Self::create_sandboxes_table(db).await?;
        Self::create_templates_table(db).await?;
        Self::create_audit_logs_table(db).await?;
        
        // 创建索引
        Self::create_indexes(db).await?;
        
        // 创建关系定义
        Self::create_relationships(db).await?;
        
        info!("SurrealDB 数据库模式初始化完成");
        Ok(())
    }
    
    /// 创建用户表
    async fn create_users_table(db: &Surreal<Any>) -> SurrealResult<()> {
        info!("创建用户表...");
        
        let sql = r#"
        DEFINE TABLE users SCHEMAFULL;
        
        DEFINE FIELD id ON TABLE users TYPE record<users> VALUE $value OR uuid::new();
        DEFINE FIELD username ON TABLE users TYPE string ASSERT string::len($value) > 2;
        DEFINE FIELD email ON TABLE users TYPE string ASSERT string::is::email($value);
        DEFINE FIELD password_hash ON TABLE users TYPE string;
        DEFINE FIELD role ON TABLE users TYPE string VALUE $value OR "User" ASSERT $value IN ["Admin", "Manager", "User"];
        DEFINE FIELD is_active ON TABLE users TYPE bool VALUE $value OR true;
        DEFINE FIELD tenant_id ON TABLE users TYPE option<record<tenants>>;
        DEFINE FIELD created_at ON TABLE users TYPE datetime VALUE $value OR time::now();
        DEFINE FIELD updated_at ON TABLE users TYPE datetime VALUE time::now();
        DEFINE FIELD last_login ON TABLE users TYPE option<datetime>;
        
        DEFINE INDEX idx_users_username ON TABLE users COLUMNS username UNIQUE;
        DEFINE INDEX idx_users_email ON TABLE users COLUMNS email UNIQUE;
        DEFINE INDEX idx_users_tenant ON TABLE users COLUMNS tenant_id;
        DEFINE INDEX idx_users_role ON TABLE users COLUMNS role;
        DEFINE INDEX idx_users_active ON TABLE users COLUMNS is_active;
        "#;
        
        db.query(sql).await?;
        Ok(())
    }
    
    /// 创建API密钥表
    async fn create_api_keys_table(db: &Surreal<Any>) -> SurrealResult<()> {
        info!("创建API密钥表...");
        
        let sql = r#"
        DEFINE TABLE api_keys SCHEMAFULL;
        
        DEFINE FIELD id ON TABLE api_keys TYPE record<api_keys> VALUE $value OR uuid::new();
        DEFINE FIELD key_hash ON TABLE api_keys TYPE string;
        DEFINE FIELD prefix ON TABLE api_keys TYPE string;
        DEFINE FIELD name ON TABLE api_keys TYPE string;
        DEFINE FIELD user_id ON TABLE api_keys TYPE record<users>;
        DEFINE FIELD permissions ON TABLE api_keys TYPE array<string>;
        DEFINE FIELD expires_at ON TABLE api_keys TYPE option<datetime>;
        DEFINE FIELD last_used_at ON TABLE api_keys TYPE option<datetime>;
        DEFINE FIELD is_active ON TABLE api_keys TYPE bool VALUE $value OR true;
        DEFINE FIELD created_at ON TABLE api_keys TYPE datetime VALUE $value OR time::now();
        DEFINE FIELD updated_at ON TABLE api_keys TYPE datetime VALUE time::now();
        
        DEFINE INDEX idx_api_keys_hash ON TABLE api_keys COLUMNS key_hash UNIQUE;
        DEFINE INDEX idx_api_keys_prefix ON TABLE api_keys COLUMNS prefix UNIQUE;
        DEFINE INDEX idx_api_keys_user ON TABLE api_keys COLUMNS user_id;
        DEFINE INDEX idx_api_keys_active ON TABLE api_keys COLUMNS is_active;
        "#;
        
        db.query(sql).await?;
        Ok(())
    }
    
    /// 创建会话表
    async fn create_sessions_table(db: &Surreal<Any>) -> SurrealResult<()> {
        info!("创建会话表...");
        
        let sql = r#"
        DEFINE TABLE sessions SCHEMAFULL;
        
        DEFINE FIELD id ON TABLE sessions TYPE record<sessions> VALUE $value OR uuid::new();
        DEFINE FIELD user_id ON TABLE sessions TYPE record<users>;
        DEFINE FIELD token_hash ON TABLE sessions TYPE string;
        DEFINE FIELD refresh_token_hash ON TABLE sessions TYPE option<string>;
        DEFINE FIELD ip_address ON TABLE sessions TYPE option<string>;
        DEFINE FIELD user_agent ON TABLE sessions TYPE option<string>;
        DEFINE FIELD expires_at ON TABLE sessions TYPE datetime;
        DEFINE FIELD created_at ON TABLE sessions TYPE datetime VALUE $value OR time::now();
        DEFINE FIELD last_accessed_at ON TABLE sessions TYPE datetime VALUE time::now();
        DEFINE FIELD is_active ON TABLE sessions TYPE bool VALUE $value OR true;
        
        DEFINE INDEX idx_sessions_token ON TABLE sessions COLUMNS token_hash UNIQUE;
        DEFINE INDEX idx_sessions_refresh ON TABLE sessions COLUMNS refresh_token_hash;
        DEFINE INDEX idx_sessions_user ON TABLE sessions COLUMNS user_id;
        DEFINE INDEX idx_sessions_active ON TABLE sessions COLUMNS is_active;
        "#;
        
        db.query(sql).await?;
        Ok(())
    }
    
    /// 创建租户表
    async fn create_tenants_table(db: &Surreal<Any>) -> SurrealResult<()> {
        info!("创建租户表...");
        
        let sql = r#"
        DEFINE TABLE tenants SCHEMAFULL;
        
        DEFINE FIELD id ON TABLE tenants TYPE record<tenants> VALUE $value OR uuid::new();
        DEFINE FIELD name ON TABLE tenants TYPE string;
        DEFINE FIELD slug ON TABLE tenants TYPE string ASSERT string::is::alphanum($value);
        DEFINE FIELD description ON TABLE tenants TYPE option<string>;
        DEFINE FIELD settings ON TABLE tenants TYPE option<object>;
        DEFINE FIELD is_active ON TABLE tenants TYPE bool VALUE $value OR true;
        DEFINE FIELD created_at ON TABLE tenants TYPE datetime VALUE $value OR time::now();
        DEFINE FIELD updated_at ON TABLE tenants TYPE datetime VALUE time::now();
        
        DEFINE INDEX idx_tenants_slug ON TABLE tenants COLUMNS slug UNIQUE;
        DEFINE INDEX idx_tenants_active ON TABLE tenants COLUMNS is_active;
        "#;
        
        db.query(sql).await?;
        Ok(())
    }
    
    /// 创建沙盒表
    async fn create_sandboxes_table(db: &Surreal<Any>) -> SurrealResult<()> {
        info!("创建沙盒表...");
        
        let sql = r#"
        DEFINE TABLE sandboxes SCHEMAFULL;
        
        DEFINE FIELD id ON TABLE sandboxes TYPE record<sandboxes> VALUE $value OR uuid::new();
        DEFINE FIELD name ON TABLE sandboxes TYPE string;
        DEFINE FIELD runtime_type ON TABLE sandboxes TYPE string VALUE $value OR "docker" ASSERT $value IN ["docker", "firecracker"];
        DEFINE FIELD template ON TABLE sandboxes TYPE string;
        DEFINE FIELD status ON TABLE sandboxes TYPE string VALUE $value OR "created" ASSERT $value IN ["created", "starting", "running", "stopping", "stopped", "error"];
        DEFINE FIELD owner_id ON TABLE sandboxes TYPE record<users>;
        DEFINE FIELD tenant_id ON TABLE sandboxes TYPE option<record<tenants>>;
        
        -- 资源限制
        DEFINE FIELD cpu_limit ON TABLE sandboxes TYPE option<int>;
        DEFINE FIELD memory_limit ON TABLE sandboxes TYPE option<int>;
        DEFINE FIELD disk_limit ON TABLE sandboxes TYPE option<int>;
        
        -- 运行时信息
        DEFINE FIELD container_id ON TABLE sandboxes TYPE option<string>;
        DEFINE FIELD vm_id ON TABLE sandboxes TYPE option<string>;
        DEFINE FIELD ip_address ON TABLE sandboxes TYPE option<string>;
        DEFINE FIELD port_mappings ON TABLE sandboxes TYPE option<object>;
        
        -- 时间戳
        DEFINE FIELD created_at ON TABLE sandboxes TYPE datetime VALUE $value OR time::now();
        DEFINE FIELD updated_at ON TABLE sandboxes TYPE datetime VALUE time::now();
        DEFINE FIELD started_at ON TABLE sandboxes TYPE option<datetime>;
        DEFINE FIELD stopped_at ON TABLE sandboxes TYPE option<datetime>;
        DEFINE FIELD expires_at ON TABLE sandboxes TYPE option<datetime>;
        
        DEFINE INDEX idx_sandboxes_owner ON TABLE sandboxes COLUMNS owner_id;
        DEFINE INDEX idx_sandboxes_tenant ON TABLE sandboxes COLUMNS tenant_id;
        DEFINE INDEX idx_sandboxes_status ON TABLE sandboxes COLUMNS status;
        DEFINE INDEX idx_sandboxes_runtime ON TABLE sandboxes COLUMNS runtime_type;
        DEFINE INDEX idx_sandboxes_container ON TABLE sandboxes COLUMNS container_id;
        DEFINE INDEX idx_sandboxes_vm ON TABLE sandboxes COLUMNS vm_id;
        "#;
        
        db.query(sql).await?;
        Ok(())
    }
    
    /// 创建模板表
    async fn create_templates_table(db: &Surreal<Any>) -> SurrealResult<()> {
        info!("创建模板表...");
        
        let sql = r#"
        DEFINE TABLE templates SCHEMAFULL;
        
        DEFINE FIELD id ON TABLE templates TYPE record<templates> VALUE $value OR uuid::new();
        DEFINE FIELD name ON TABLE templates TYPE string;
        DEFINE FIELD slug ON TABLE templates TYPE string ASSERT string::is::alphanum($value);
        DEFINE FIELD description ON TABLE templates TYPE option<string>;
        DEFINE FIELD runtime_type ON TABLE templates TYPE string VALUE $value OR "docker" ASSERT $value IN ["docker", "firecracker"];
        DEFINE FIELD base_image ON TABLE templates TYPE string;
        DEFINE FIELD default_command ON TABLE templates TYPE option<string>;
        DEFINE FIELD environment_vars ON TABLE templates TYPE option<object>;
        DEFINE FIELD resource_limits ON TABLE templates TYPE option<object>;
        DEFINE FIELD is_public ON TABLE templates TYPE bool VALUE $value OR false;
        DEFINE FIELD owner_id ON TABLE templates TYPE option<record<users>>;
        DEFINE FIELD created_at ON TABLE templates TYPE datetime VALUE $value OR time::now();
        DEFINE FIELD updated_at ON TABLE templates TYPE datetime VALUE time::now();
        
        DEFINE INDEX idx_templates_slug ON TABLE templates COLUMNS slug UNIQUE;
        DEFINE INDEX idx_templates_runtime ON TABLE templates COLUMNS runtime_type;
        DEFINE INDEX idx_templates_public ON TABLE templates COLUMNS is_public;
        DEFINE INDEX idx_templates_owner ON TABLE templates COLUMNS owner_id;
        "#;
        
        db.query(sql).await?;
        Ok(())
    }
    
    /// 创建审计日志表
    async fn create_audit_logs_table(db: &Surreal<Any>) -> SurrealResult<()> {
        info!("创建审计日志表...");
        
        let sql = r#"
        DEFINE TABLE audit_logs SCHEMAFULL;
        
        DEFINE FIELD id ON TABLE audit_logs TYPE record<audit_logs> VALUE $value OR uuid::new();
        DEFINE FIELD event_type ON TABLE audit_logs TYPE string;
        DEFINE FIELD severity ON TABLE audit_logs TYPE string VALUE $value OR "Info" ASSERT $value IN ["Debug", "Info", "Warn", "Error", "Critical"];
        DEFINE FIELD result ON TABLE audit_logs TYPE string VALUE $value OR "Success" ASSERT $value IN ["Success", "Failure", "Partial"];
        DEFINE FIELD timestamp ON TABLE audit_logs TYPE datetime VALUE $value OR time::now();
        
        -- 用户信息
        DEFINE FIELD user_id ON TABLE audit_logs TYPE option<record<users>>;
        DEFINE FIELD username ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD user_role ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD tenant_id ON TABLE audit_logs TYPE option<record<tenants>>;
        
        -- 请求信息
        DEFINE FIELD request_id ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD ip_address ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD user_agent ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD request_path ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD http_method ON TABLE audit_logs TYPE option<string>;
        
        -- 资源信息
        DEFINE FIELD resource_type ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD resource_id ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD resource_name ON TABLE audit_logs TYPE option<string>;
        
        -- 权限信息
        DEFINE FIELD permission ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD old_role ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD new_role ON TABLE audit_logs TYPE option<string>;
        
        -- 事件详情
        DEFINE FIELD message ON TABLE audit_logs TYPE string;
        DEFINE FIELD details ON TABLE audit_logs TYPE option<object>;
        DEFINE FIELD error_code ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD error_message ON TABLE audit_logs TYPE option<string>;
        
        -- 元数据
        DEFINE FIELD session_id ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD correlation_id ON TABLE audit_logs TYPE option<string>;
        DEFINE FIELD tags ON TABLE audit_logs TYPE option<array<string>>;
        
        DEFINE INDEX idx_audit_logs_user ON TABLE audit_logs COLUMNS user_id;
        DEFINE INDEX idx_audit_logs_tenant ON TABLE audit_logs COLUMNS tenant_id;
        DEFINE INDEX idx_audit_logs_timestamp ON TABLE audit_logs COLUMNS timestamp;
        DEFINE INDEX idx_audit_logs_event_type ON TABLE audit_logs COLUMNS event_type;
        DEFINE INDEX idx_audit_logs_severity ON TABLE audit_logs COLUMNS severity;
        DEFINE INDEX idx_audit_logs_result ON TABLE audit_logs COLUMNS result;
        DEFINE INDEX idx_audit_logs_resource ON TABLE audit_logs COLUMNS resource_type, resource_id;
        "#;
        
        db.query(sql).await?;
        Ok(())
    }
    
    /// 创建额外的索引
    async fn create_indexes(db: &Surreal<Any>) -> SurrealResult<()> {
        info!("创建额外索引...");
        
        let sql = r#"
        -- 复合索引
        DEFINE INDEX idx_users_tenant_role ON TABLE users COLUMNS tenant_id, role;
        DEFINE INDEX idx_sandboxes_owner_status ON TABLE sandboxes COLUMNS owner_id, status;
        DEFINE INDEX idx_sandboxes_tenant_status ON TABLE sandboxes COLUMNS tenant_id, status;
        DEFINE INDEX idx_audit_logs_user_timestamp ON TABLE audit_logs COLUMNS user_id, timestamp;
        DEFINE INDEX idx_audit_logs_tenant_timestamp ON TABLE audit_logs COLUMNS tenant_id, timestamp;
        
        -- 时间范围查询索引
        DEFINE INDEX idx_sandboxes_created_at ON TABLE sandboxes COLUMNS created_at;
        DEFINE INDEX idx_sandboxes_expires_at ON TABLE sandboxes COLUMNS expires_at;
        DEFINE INDEX idx_sessions_expires_at ON TABLE sessions COLUMNS expires_at;
        DEFINE INDEX idx_api_keys_expires_at ON TABLE api_keys COLUMNS expires_at;
        "#;
        
        db.query(sql).await?;
        Ok(())
    }
    
    /// 创建关系定义
    async fn create_relationships(db: &Surreal<Any>) -> SurrealResult<()> {
        info!("创建关系定义...");
        
        let sql = r#"
        -- 用户-租户关系
        DEFINE TABLE user_tenant_relation SCHEMAFULL;
        RELATE users:$user->belongs_to->tenants:$tenant
        WHERE $user = users.id AND $tenant = tenants.id;
        
        -- 用户-沙盒关系
        DEFINE TABLE user_sandbox_relation SCHEMAFULL;
        RELATE users:$user->owns->sandboxes:$sandbox
        WHERE $user = users.id AND $sandbox = sandboxes.id;
        
        -- 用户-模板关系
        DEFINE TABLE user_template_relation SCHEMAFULL;
        RELATE users:$user->creates->templates:$template
        WHERE $user = users.id AND $template = templates.id;
        
        -- 沙盒-模板关系
        DEFINE TABLE sandbox_template_relation SCHEMAFULL;
        RELATE sandboxes:$sandbox->uses->templates:$template
        WHERE $sandbox = sandboxes.id AND $template = templates.id;
        "#;
        
        db.query(sql).await?;
        Ok(())
    }
    
    /// 重置数据库（仅用于开发环境）
    pub async fn reset_database(db: &Surreal<Any>) -> SurrealResult<()> {
        warn!("⚠️ 重置数据库...");
        
        let tables = vec![
            "audit_logs",
            "sessions", 
            "api_keys",
            "sandboxes",
            "templates",
            "users",
            "tenants",
            "user_tenant_relation",
            "user_sandbox_relation", 
            "user_template_relation",
            "sandbox_template_relation",
        ];
        
        for table in tables {
            let sql = format!("REMOVE TABLE {}", table);
            if let Err(e) = db.query(sql).await {
                warn!("删除表 {} 失败: {}", table, e);
            }
        }
        
        // 重新初始化模式
        Self::initialize(db).await?;
        
        info!("数据库重置完成");
        Ok(())
    }
}

/// 实时查询辅助工具
pub struct LiveQueries;

impl LiveQueries {
    /// 监听沙盒状态变化
    pub async fn watch_sandbox_status(db: &Surreal<Any>, sandbox_id: &str) -> SurrealResult<()> {
        let sql = format!(
            "LIVE SELECT * FROM sandboxes WHERE id = sandboxes:{} AND status != $old.status",
            sandbox_id
        );
        
        db.query(sql).await?;
        Ok(())
    }
    
    /// 监听用户活动
    pub async fn watch_user_activity(db: &Surreal<Any>, user_id: &str) -> SurrealResult<()> {
        let sql = format!(
            "LIVE SELECT * FROM audit_logs WHERE user_id = users:{}",
            user_id
        );
        
        db.query(sql).await?;
        Ok(())
    }
    
    /// 监听租户资源使用
    pub async fn watch_tenant_resources(db: &Surreal<Any>, tenant_id: &str) -> SurrealResult<()> {
        let sql = format!(
            "LIVE SELECT count() FROM sandboxes WHERE tenant_id = tenants:{} GROUP BY status",
            tenant_id
        );
        
        db.query(sql).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::surrealdb::config::{SurrealConfig, SurrealProtocol};
    use crate::database::surrealdb::connection::SurrealPool;
    
    #[tokio::test]
    async fn test_schema_initialization() {
        let config = SurrealConfig {
            protocol: SurrealProtocol::Memory,
            endpoint: "mem://".to_string(),
            namespace: "test".to_string(),
            database: "test".to_string(),
            ..Default::default()
        };
        
        let pool = SurrealPool::new(config).await.unwrap();
        let conn = pool.get_connection().await.unwrap();
        
        let result = SurrealSchema::initialize(conn.db()).await;
        assert!(result.is_ok());
    }
}