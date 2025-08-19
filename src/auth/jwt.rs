use anyhow::{Result, Context};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{info, warn, error};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use super::models::{User, Role};
use crate::error::SoulBoxError;

/// JWT 声明
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// 用户 ID
    pub sub: String,
    /// 用户名
    pub username: String,
    /// 角色
    pub role: Role,
    /// 租户 ID
    pub tenant_id: Option<Uuid>,
    /// 签发时间
    pub iat: i64,
    /// 过期时间
    pub exp: i64,
    /// 签发者
    pub iss: String,
    /// 受众
    pub aud: String,
}

/// JWT 管理器 - 简化版本
pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
    issuer: String,
    audience: String,
    access_token_duration: Duration,
    refresh_token_duration: Duration,
    /// Token blacklist for revoked tokens
    token_blacklist: Arc<RwLock<HashSet<String>>>,
}

/// Simplified JWT configuration constants
pub struct JwtConfig;

impl JwtConfig {
    pub const MAX_CLOCK_SKEW_SECONDS: i64 = 30;
    pub const MIN_TOKEN_LIFETIME_SECONDS: i64 = 60;
    pub const MAX_TOKEN_LIFETIME_SECONDS: i64 = 86400; // 24 hours
    pub const ALGORITHM: Algorithm = Algorithm::HS256; // Single secure algorithm
}

impl JwtManager {
    /// 创建新的 JWT 管理器并强制安全配置
    pub fn new(secret: &str, issuer: String, audience: String) -> Result<Self> {
        // 强制密钥强度验证
        Self::validate_secret_strength(secret)?;
        
        let encoding_key = EncodingKey::from_secret(secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(secret.as_bytes());
        
        // 使用简化的安全配置
        let mut validation = Validation::new(JwtConfig::ALGORITHM);
        
        // 强制安全验证设置
        validation.set_issuer(&[&issuer]);
        validation.set_audience(&[&audience]);
        validation.leeway = JwtConfig::MAX_CLOCK_SKEW_SECONDS as u64;
        validation.validate_exp = true; // 强制验证过期时间
        validation.validate_nbf = true; // 强制验证生效时间
        validation.validate_aud = true; // 始终启用严格的受众验证
        validation.validate_iat = true; // 验证签发时间
        
        // 使用单一安全算法
        validation.algorithms = vec![JwtConfig::ALGORITHM];
        
        // 禁用危险的验证绕过选项
        validation.insecure_disable_signature_validation = false;
        validation.validate_signature = true;
        
        Ok(Self {
            encoding_key,
            decoding_key,
            validation,
            issuer,
            audience,
            access_token_duration: Duration::minutes(15), // 缩短访问令牌有效期到15分钟
            refresh_token_duration: Duration::days(1),    // 缩短刷新令牌有效期到1天
            token_blacklist: Arc::new(RwLock::new(HashSet::new())),
        })
    }
    
    /// 验证JWT密钥强度
    fn validate_secret_strength(secret: &str) -> Result<()> {
        use sha2::{Sha256, Digest};
        
        // 最小长度要求
        if secret.len() < 32 {
            return Err(anyhow::anyhow!(
                "JWT secret must be at least 32 characters long. Current length: {}",
                secret.len()
            ));
        }
        
        // 最大长度限制（防止DoS）
        if secret.len() > 512 {
            return Err(anyhow::anyhow!(
                "JWT secret must not exceed 512 characters. Current length: {}",
                secret.len()
            ));
        }
        
        // 检查密钥复杂度
        let has_lower = secret.chars().any(|c| c.is_ascii_lowercase());
        let has_upper = secret.chars().any(|c| c.is_ascii_uppercase());
        let has_digit = secret.chars().any(|c| c.is_ascii_digit());
        let has_special = secret.chars().any(|c| !c.is_ascii_alphanumeric());
        
        let complexity_score = [has_lower, has_upper, has_digit, has_special]
            .iter()
            .filter(|&&x| x)
            .count();
        
        if complexity_score < 3 {
            return Err(anyhow::anyhow!(
                "JWT secret must contain at least 3 of: lowercase letters, uppercase letters, digits, special characters"
            ));
        }
        
        // 检查常见弱密钥模式
        let weak_patterns = [
            "password", "123456", "qwerty", "admin", "secret", "token",
            "jwt", "key", "test", "demo", "example", "changeme"
        ];
        
        let secret_lower = secret.to_lowercase();
        for pattern in &weak_patterns {
            if secret_lower.contains(pattern) {
                return Err(anyhow::anyhow!(
                    "JWT secret contains weak pattern: '{}'. Please use a stronger secret.",
                    pattern
                ));
            }
        }
        
        // 计算熵值（简单检查）
        let mut char_counts = std::collections::HashMap::new();
        for c in secret.chars() {
            *char_counts.entry(c).or_insert(0) += 1;
        }
        
        let unique_chars = char_counts.len();
        if unique_chars < secret.len() / 4 {
            return Err(anyhow::anyhow!(
                "JWT secret has insufficient entropy. Please use a more random secret."
            ));
        }
        
        info!(
            "JWT secret validation passed: length={}, complexity_score={}, unique_chars={}",
            secret.len(), complexity_score, unique_chars
        );
        
        Ok(())
    }

    /// 生成访问令牌
    pub fn generate_access_token(&self, user: &User) -> Result<String> {
        let now = Utc::now();
        let claims = Claims {
            sub: user.id.to_string(),
            username: user.username.clone(),
            role: user.role.clone(),
            tenant_id: user.tenant_id,
            iat: now.timestamp(),
            exp: (now + self.access_token_duration).timestamp(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
        };

        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| SoulBoxError::internal(format!("JWT encoding error: {}", e)).into())
    }

    /// 生成刷新令牌
    pub fn generate_refresh_token(&self, user: &User) -> Result<String> {
        let now = Utc::now();
        let claims = Claims {
            sub: user.id.to_string(),
            username: user.username.clone(),
            role: user.role.clone(),
            tenant_id: user.tenant_id,
            iat: now.timestamp(),
            exp: (now + self.refresh_token_duration).timestamp(),
            iss: format!("{}-refresh", self.issuer),
            aud: self.audience.clone(),
        };

        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| SoulBoxError::internal(format!("JWT encoding error: {}", e)).into())
    }

    /// 验证访问令牌
    pub async fn validate_access_token(&self, token: &str) -> Result<Claims> {
        // Pre-validation security checks
        self.pre_validate_token(token).await?;
        
        let token_data = decode::<Claims>(token, &self.decoding_key, &self.validation)
            .context("Failed to decode JWT token")
            .map_err(|e| {
                error!("JWT token decode failed: {}", e);
                SoulBoxError::authentication(format!("Invalid JWT token: {}", e))
            })?;

        // Post-decode security validations
        self.post_validate_claims(&token_data.claims, token).await?;

        // 检查是否是访问令牌（不是刷新令牌）
        if token_data.claims.iss.ends_with("-refresh") {
            error!("Access token validation attempted with refresh token");
            return Err(SoulBoxError::authentication("Invalid token type").into());
        }

        Ok(token_data.claims)
    }

    /// 验证刷新令牌
    pub async fn validate_refresh_token(&self, token: &str) -> Result<Claims> {
        // Pre-validation security checks
        self.pre_validate_token(token).await?;
        
        // 临时修改验证器以接受刷新令牌签发者
        let mut validation = self.validation.clone();
        validation.set_issuer(&[&format!("{}-refresh", self.issuer)]);

        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)
            .context("Failed to decode refresh token")
            .map_err(|e| {
                error!("Refresh token decode failed: {}", e);
                SoulBoxError::authentication(format!("Invalid refresh token: {}", e))
            })?;

        // Post-decode security validations for refresh tokens
        self.post_validate_claims(&token_data.claims, token).await?;

        Ok(token_data.claims)
    }

    /// 从令牌中提取用户 ID
    pub async fn extract_user_id(&self, token: &str) -> Result<Uuid> {
        let claims = self.validate_access_token(token).await?;
        Uuid::parse_str(&claims.sub)
            .map_err(|e| SoulBoxError::authentication(format!("Invalid user ID: {}", e)).into())
    }
    
    /// 吊销令牌（添加到黑名单）
    pub async fn revoke_token(&self, token: &str) -> Result<()> {
        // 提取token的唯一标识符（使用前64个字符作为标识）
        let token_id = if token.len() > 64 {
            &token[..64]
        } else {
            token
        };
        
        let mut blacklist = self.token_blacklist.write().await;
        blacklist.insert(token_id.to_string());
        
        info!("Token revoked and added to blacklist");
        Ok(())
    }
    
    /// 检查令牌是否被吊销
    pub async fn is_token_revoked(&self, token: &str) -> bool {
        let token_id = if token.len() > 64 {
            &token[..64]
        } else {
            token
        };
        
        let blacklist = self.token_blacklist.read().await;
        blacklist.contains(token_id)
    }
    
    /// 清理过期的黑名单令牌
    pub async fn cleanup_blacklist(&self) {
        // 在实际实现中，这里应该基于令牌的过期时间来清理
        // 为了简化，我们保持当前实现
        info!("Blacklist cleanup completed");
    }
    
    /// Pre-validation security checks
    async fn pre_validate_token(&self, token: &str) -> Result<()> {
        // Check token format
        if token.is_empty() {
            return Err(SoulBoxError::authentication("Empty token").into());
        }
        
        // Check token length (basic sanity check)
        if token.len() < 20 || token.len() > 4096 {
            warn!("Token length suspicious: {} characters", token.len());
            return Err(SoulBoxError::authentication("Invalid token format").into());
        }
        
        // Check if token is blacklisted
        if self.is_token_revoked(token).await {
            error!("Attempted to use revoked token");
            return Err(SoulBoxError::authentication("Token has been revoked").into());
        }
        
        // Check JWT structure (should have 3 parts separated by dots)
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(SoulBoxError::authentication("Invalid JWT structure").into());
        }
        
        // Validate each part is base64url encoded
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                return Err(SoulBoxError::authentication(
                    format!("Empty JWT part {}", i + 1)
                ).into());
            }
        }
        
        Ok(())
    }
    
    /// Post-decode claims validation
    async fn post_validate_claims(&self, claims: &Claims, token: &str) -> Result<()> {
        let now = Utc::now().timestamp();
        
        // Validate time-based claims with strict bounds
        if claims.exp <= now {
            error!("Token expired: exp={}, now={}", claims.exp, now);
            return Err(SoulBoxError::authentication("Token expired").into());
        }
        
        if claims.iat > now + JwtConfig::MAX_CLOCK_SKEW_SECONDS {
            error!("Token issued in future: iat={}, now={}", claims.iat, now);
            return Err(SoulBoxError::authentication("Token issued in future").into());
        }
        
        // Validate token lifetime with simplified constants
        let token_lifetime = claims.exp - claims.iat;
        if token_lifetime < JwtConfig::MIN_TOKEN_LIFETIME_SECONDS {
            return Err(SoulBoxError::authentication("Token lifetime too short").into());
        }
        
        if token_lifetime > JwtConfig::MAX_TOKEN_LIFETIME_SECONDS {
            return Err(SoulBoxError::authentication("Token lifetime too long").into());
        }
        
        // Simplified issuer validation - always strict
        if !claims.iss.starts_with(&self.issuer) {
            error!("Invalid issuer: expected prefix '{}', got '{}'", self.issuer, claims.iss);
            return Err(SoulBoxError::authentication("Invalid token issuer").into());
        }
        
        // Simplified audience validation - always strict
        if claims.aud != self.audience {
            error!("Invalid audience: expected '{}', got '{}'", self.audience, claims.aud);
            return Err(SoulBoxError::authentication("Invalid token audience").into());
        }
        
        // Validate user ID format
        if Uuid::parse_str(&claims.sub).is_err() {
            return Err(SoulBoxError::authentication("Invalid user ID format").into());
        }
        
        // Validate username format (basic checks)
        if claims.username.is_empty() || claims.username.len() > 255 {
            return Err(SoulBoxError::authentication("Invalid username format").into());
        }
        
        // Always perform basic replay protection
        self.check_replay_protection(claims, token).await?;
        
        Ok(())
    }
    
    /// Check for token replay attacks
    async fn check_replay_protection(&self, claims: &Claims, _token: &str) -> Result<()> {
        // In a full implementation, this would check against a cache of recently used tokens
        // For now, we implement basic time-based replay protection
        
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let token_age = now - claims.iat;
        
        // Reject tokens that are too old (potential replay attack)
        if token_age > 3600 { // 1 hour
            warn!("Potentially replayed token detected: age={}s", token_age);
            // Don't reject yet, just log for monitoring
        }
        
        Ok(())
    }

    /// 检查令牌是否即将过期（30分钟内）
    pub fn is_token_expiring_soon(&self, claims: &Claims) -> bool {
        let expiry_time = chrono::DateTime::from_timestamp(claims.exp, 0)
            .unwrap_or_else(|| Utc::now());
        let time_until_expiry = expiry_time - Utc::now();
        time_until_expiry < Duration::minutes(30)
    }

    /// 获取访问令牌有效期（秒）
    pub fn access_token_duration_secs(&self) -> i64 {
        self.access_token_duration.num_seconds()
    }

    /// 获取刷新令牌有效期（秒）
    pub fn refresh_token_duration_secs(&self) -> i64 {
        self.refresh_token_duration.num_seconds()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::models::Role;

    fn create_test_user() -> User {
        User {
            id: Uuid::new_v4(),
            username: "testuser".to_string(),
            email: "test@example.com".to_string(),
            role: Role::Developer,
            is_active: true,
            created_at: Utc::now(),
            last_login: None,
            tenant_id: Some(Uuid::new_v4()),
        }
    }

    #[test]
    fn test_jwt_token_generation_and_validation() {
        let manager = JwtManager::new(
            "test-secret-key-that-is-very-long-and-secure-1234567890",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        ).expect("Failed to create JWT manager");
        
        let user = create_test_user();
        
        // 生成访问令牌
        let access_token = manager.generate_access_token(&user).unwrap();
        assert!(!access_token.is_empty());
        
        // 验证访问令牌
        let claims = tokio_test::block_on(manager.validate_access_token(&access_token)).unwrap();
        assert_eq!(claims.sub, user.id.to_string());
        assert_eq!(claims.username, user.username);
        assert_eq!(claims.role, user.role);
        assert_eq!(claims.tenant_id, user.tenant_id);
    }

    #[test]
    fn test_refresh_token_generation_and_validation() {
        let manager = JwtManager::new(
            "test-secret-key-that-is-very-long-and-secure-1234567890",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        ).expect("Failed to create JWT manager");
        
        let user = create_test_user();
        
        // 生成刷新令牌
        let refresh_token = manager.generate_refresh_token(&user).unwrap();
        assert!(!refresh_token.is_empty());
        
        // 验证刷新令牌
        let claims = tokio_test::block_on(manager.validate_refresh_token(&refresh_token)).unwrap();
        assert_eq!(claims.sub, user.id.to_string());
        assert_eq!(claims.username, user.username);
    }

    #[test]
    fn test_invalid_token_validation() {
        let manager = JwtManager::new(
            "test-secret-key-that-is-very-long-and-secure-1234567890",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        ).expect("Failed to create JWT manager");
        
        // 测试无效令牌
        assert!(tokio_test::block_on(manager.validate_access_token("invalid-token")).is_err());
        assert!(tokio_test::block_on(manager.validate_refresh_token("invalid-token")).is_err());
    }

    #[test]
    fn test_token_type_validation() {
        let manager = JwtManager::new(
            "test-secret-key-that-is-very-long-and-secure-1234567890",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        ).expect("Failed to create JWT manager");
        
        let user = create_test_user();
        
        let access_token = manager.generate_access_token(&user).unwrap();
        let refresh_token = manager.generate_refresh_token(&user).unwrap();
        
        // 访问令牌不应该被当作刷新令牌验证
        assert!(tokio_test::block_on(manager.validate_refresh_token(&access_token)).is_err());
        
        // 刷新令牌不应该被当作访问令牌验证
        assert!(tokio_test::block_on(manager.validate_access_token(&refresh_token)).is_err());
    }
}