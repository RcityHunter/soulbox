use anyhow::{Result, Context};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use uuid::Uuid;
use tracing::{info, warn, error};

pub use super::models::{ApiKey, Permission};
use crate::error::SoulBoxError;

/// 数据库API密钥操作trait，确保正确的数据库验证
#[async_trait::async_trait]
pub trait ApiKeyRepository {
    type Error: std::error::Error + Send + Sync + 'static;
    
    /// 根据哈希值查找API密钥
    async fn find_api_key_by_hash(&self, hash: &str) -> Result<Option<ApiKey>, Self::Error>;
    
    /// 更新API密钥最后使用时间
    async fn update_last_used(&self, key_id: uuid::Uuid) -> Result<(), Self::Error>;
    
    /// 检查API密钥权限
    async fn check_key_permissions(&self, key_id: uuid::Uuid, required_permission: &Permission) -> Result<bool, Self::Error>;
}

/// API 密钥管理器
pub struct ApiKeyManager {
    prefix: String,
}

impl ApiKeyManager {
    /// 创建新的 API 密钥管理器
    pub fn new(prefix: String) -> Self {
        Self { prefix }
    }

    /// 生成新的 API 密钥
    pub fn generate_api_key(
        &self,
        name: String,
        user_id: Uuid,
        permissions: HashSet<Permission>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(ApiKey, String)> {
        let id = Uuid::new_v4();
        let key_suffix = self.generate_random_key(32);
        let full_key = format!("{}_{}", self.prefix, key_suffix);
        let key_hash = self.hash_key(&full_key);

        let api_key = ApiKey {
            id,
            name,
            key_prefix: format!("{}_{}", self.prefix, &key_suffix[..8]),
            key_hash,
            user_id,
            permissions,
            is_active: true,
            last_used: None,
            expires_at,
            created_at: Utc::now(),
        };

        Ok((api_key, full_key))
    }

    /// 验证 API 密钥 - 改进为需要数据库验证
    pub async fn validate_api_key_with_db<DB>(&self, provided_key: &str, db: &DB) -> Result<Option<ApiKey>>
    where
        DB: ApiKeyRepository,
    {
        // 检查密钥格式 - 基本格式验证
        if !provided_key.starts_with(&format!("{}_", self.prefix)) {
            warn!("API key validation failed: invalid prefix format");
            return Ok(None);
        }
        
        if provided_key.len() < 20 || provided_key.len() > 200 {
            warn!("API key validation failed: invalid length");
            return Ok(None);
        }

        // 从数据库查找密钥
        let stored_api_key = match db.find_api_key_by_hash(&self.hash_key(provided_key)).await {
            Ok(Some(key)) => key,
            Ok(None) => {
                warn!("API key validation failed: key not found in database");
                return Ok(None);
            }
            Err(e) => {
                error!("Database error during API key validation: {}", e);
                return Err(anyhow::anyhow!("Database validation failed: {}", e));
            }
        };

        // 检查密钥是否激活
        if !stored_api_key.is_active {
            warn!("API key validation failed: key is deactivated");
            return Ok(None);
        }

        // 检查密钥是否过期
        if stored_api_key.is_expired() {
            warn!("API key validation failed: key has expired at {:?}", stored_api_key.expires_at);
            return Ok(None);
        }

        // 验证密钥哈希（双重检查）
        let provided_hash = self.hash_key(provided_key);
        if provided_hash != stored_api_key.key_hash {
            error!("API key hash mismatch - potential security issue");
            return Ok(None);
        }

        info!("API key validation successful for user: {}", stored_api_key.user_id);
        Ok(Some(stored_api_key))
    }

    /// 保留向后兼容的方法，但标记为deprecated
    #[deprecated(note = "Use validate_api_key_with_db for proper database validation")]
    pub fn validate_api_key(&self, provided_key: &str, stored_api_key: &ApiKey) -> Result<bool> {
        warn!("Using deprecated validate_api_key method - migrate to validate_api_key_with_db");
        
        // 检查密钥格式
        if !provided_key.starts_with(&format!("{}_", self.prefix)) {
            return Ok(false);
        }

        // 检查密钥是否激活
        if !stored_api_key.is_active {
            return Ok(false);
        }

        // 检查密钥是否过期
        if stored_api_key.is_expired() {
            return Ok(false);
        }

        // 验证密钥哈希
        let provided_hash = self.hash_key(provided_key);
        Ok(provided_hash == stored_api_key.key_hash)
    }

    /// 撤销 API 密钥
    pub fn revoke_api_key(&self, api_key: &mut ApiKey) {
        api_key.is_active = false;
    }

    /// 更新 API 密钥最后使用时间
    pub fn update_last_used(&self, api_key: &mut ApiKey) {
        api_key.last_used = Some(Utc::now());
    }

    /// 检查 API 密钥是否具有权限 - 增强版本，同时检查数据库状态
    pub async fn check_permission_with_db<DB>(&self, api_key: &ApiKey, permission: &Permission, db: &DB) -> Result<bool>
    where
        DB: ApiKeyRepository,
    {
        // 首先进行本地权限检查
        if !api_key.has_permission(permission) {
            warn!("Permission denied: API key {} does not have permission {:?}", api_key.id, permission);
            return Ok(false);
        }
        
        // 然后检查数据库中的权限状态（防止权限被撤销但本地缓存未更新）
        match db.check_key_permissions(api_key.id, permission).await {
            Ok(has_permission) => {
                if !has_permission {
                    warn!("Permission denied: Database shows API key {} does not have permission {:?}", api_key.id, permission);
                }
                Ok(has_permission)
            }
            Err(e) => {
                error!("Database error checking permissions for API key {}: {}", api_key.id, e);
                // 默认拒绝访问以保证安全
                Ok(false)
            }
        }
    }

    /// 检查 API 密钥是否具有权限 - 保留原有方法用于向后兼容
    pub fn check_permission(&self, api_key: &ApiKey, permission: &Permission) -> bool {
        api_key.has_permission(permission)
    }

    /// 从完整密钥中提取前缀用于显示
    pub fn extract_display_key(&self, full_key: &str) -> String {
        if let Some(suffix_start) = full_key.find('_') {
            let suffix = &full_key[suffix_start + 1..];
            if suffix.len() >= 8 {
                return format!("{}_{}", self.prefix, &suffix[..8]);
            }
        }
        format!("{}_{}", self.prefix, "***")
    }

    /// 生成随机密钥
    fn generate_random_key(&self, length: usize) -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut rng = rand::thread_rng();
        
        (0..length)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// 哈希密钥
    fn hash_key(&self, key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// API 密钥权限模板
pub struct ApiKeyTemplate;

impl ApiKeyTemplate {
    /// 开发者模板权限
    pub fn developer_permissions() -> HashSet<Permission> {
        vec![
            Permission::SandboxCreate,
            Permission::SandboxRead,
            Permission::SandboxUpdate,
            Permission::SandboxDelete,
            Permission::SandboxExecute,
            Permission::FileUpload,
            Permission::FileDownload,
            Permission::FileList,
            Permission::ApiKeyCreate,
            Permission::ApiKeyRevoke,
            Permission::ApiKeyList,
        ].into_iter().collect()
    }

    /// 只读模板权限
    pub fn readonly_permissions() -> HashSet<Permission> {
        vec![
            Permission::SandboxRead,
            Permission::FileList,
        ].into_iter().collect()
    }

    /// 执行模板权限
    pub fn execution_permissions() -> HashSet<Permission> {
        vec![
            Permission::SandboxCreate,
            Permission::SandboxRead,
            Permission::SandboxExecute,
            Permission::FileUpload,
            Permission::FileDownload,
            Permission::FileList,
        ].into_iter().collect()
    }

    /// 文件操作模板权限
    pub fn file_operations_permissions() -> HashSet<Permission> {
        vec![
            Permission::FileUpload,
            Permission::FileDownload,
            Permission::FileDelete,
            Permission::FileList,
        ].into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_generation() {
        let manager = ApiKeyManager::new("sk".to_string());
        let user_id = Uuid::new_v4();
        let permissions = ApiKeyTemplate::developer_permissions();
        
        let (api_key, full_key) = manager.generate_api_key(
            "Test Key".to_string(),
            user_id,
            permissions.clone(),
            None,
        ).unwrap();

        assert_eq!(api_key.name, "Test Key");
        assert_eq!(api_key.user_id, user_id);
        assert_eq!(api_key.permissions, permissions);
        assert!(api_key.is_active);
        assert!(full_key.starts_with("sk_"));
    }

    #[test]
    fn test_api_key_validation() {
        let manager = ApiKeyManager::new("sk".to_string());
        let user_id = Uuid::new_v4();
        let permissions = ApiKeyTemplate::readonly_permissions();
        
        let (api_key, full_key) = manager.generate_api_key(
            "Test Key".to_string(),
            user_id,
            permissions,
            None,
        ).unwrap();

        // 验证正确的密钥
        assert!(manager.validate_api_key(&full_key, &api_key).unwrap());

        // 验证错误的密钥
        let wrong_key = "sk_WRONGKEY123456789012345678901234";
        assert!(!manager.validate_api_key(wrong_key, &api_key).unwrap());
    }

    #[test]
    fn test_expired_api_key() {
        let manager = ApiKeyManager::new("sk".to_string());
        let user_id = Uuid::new_v4();
        let permissions = ApiKeyTemplate::readonly_permissions();
        let expired_time = Utc::now() - Duration::hours(1); // 1小时前过期
        
        let (api_key, full_key) = manager.generate_api_key(
            "Expired Key".to_string(),
            user_id,
            permissions,
            Some(expired_time),
        ).unwrap();

        // 过期的密钥应该验证失败
        assert!(!manager.validate_api_key(&full_key, &api_key).unwrap());
        assert!(api_key.is_expired());
    }

    #[test]
    fn test_revoked_api_key() {
        let manager = ApiKeyManager::new("sk".to_string());
        let user_id = Uuid::new_v4();
        let permissions = ApiKeyTemplate::readonly_permissions();
        
        let (mut api_key, full_key) = manager.generate_api_key(
            "Test Key".to_string(),
            user_id,
            permissions,
            None,
        ).unwrap();

        // 撤销密钥
        manager.revoke_api_key(&mut api_key);
        
        // 撤销的密钥应该验证失败
        assert!(!manager.validate_api_key(&full_key, &api_key).unwrap());
        assert!(!api_key.is_active);
    }

    #[test]
    fn test_permission_checking() {
        let manager = ApiKeyManager::new("sk".to_string());
        let user_id = Uuid::new_v4();
        let permissions = ApiKeyTemplate::readonly_permissions();
        
        let (api_key, _) = manager.generate_api_key(
            "ReadOnly Key".to_string(),
            user_id,
            permissions,
            None,
        ).unwrap();

        // 应该有只读权限
        assert!(manager.check_permission(&api_key, &Permission::SandboxRead));
        assert!(manager.check_permission(&api_key, &Permission::FileList));
        
        // 不应该有写入权限
        assert!(!manager.check_permission(&api_key, &Permission::SandboxCreate));
        assert!(!manager.check_permission(&api_key, &Permission::FileUpload));
    }

    #[test]
    fn test_display_key_extraction() {
        let manager = ApiKeyManager::new("sk".to_string());
        let full_key = "sk_ABCDEFGH12345678901234567890";
        
        let display_key = manager.extract_display_key(full_key);
        assert_eq!(display_key, "sk_ABCDEFGH");
    }
}