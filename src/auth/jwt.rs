use anyhow::{Result, Context};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::info;

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

/// JWT 管理器
pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    validation: Validation,
    issuer: String,
    audience: String,
    access_token_duration: Duration,
    refresh_token_duration: Duration,
}

impl JwtManager {
    /// 创建新的 JWT 管理器并强制安全配置
    pub fn new(secret: &str, issuer: String, audience: String) -> Result<Self> {
        // 强制密钥强度验证
        Self::validate_secret_strength(secret)?;
        
        let encoding_key = EncodingKey::from_secret(secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(secret.as_bytes());
        
        // 使用安全的算法配置
        let mut validation = Validation::new(Algorithm::HS256);
        
        // 强制安全验证设置
        validation.set_issuer(&[&issuer]);
        validation.set_audience(&[&audience]);
        validation.leeway = 30; // 减少时钟偏差容忍度到30秒
        validation.validate_exp = true; // 强制验证过期时间
        validation.validate_nbf = true; // 强制验证生效时间
        validation.validate_aud = true; // 强制验证受众
        validation.validate_iss = true; // 强制验证发行者
        
        // 禁用不安全的算法
        validation.algorithms = vec![Algorithm::HS256, Algorithm::HS384, Algorithm::HS512];
        
        Ok(Self {
            encoding_key,
            decoding_key,
            validation,
            issuer,
            audience,
            access_token_duration: Duration::minutes(15), // 缩短访问令牌有效期到15分钟
            refresh_token_duration: Duration::days(1),    // 缩短刷新令牌有效期到1天
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
    pub fn validate_access_token(&self, token: &str) -> Result<Claims> {
        let token_data = decode::<Claims>(token, &self.decoding_key, &self.validation)
            .context("Failed to decode JWT token")
            .map_err(|e| SoulBoxError::authentication(format!("Invalid JWT token: {}", e)))?;

        // 检查是否是访问令牌（不是刷新令牌）
        if token_data.claims.iss.ends_with("-refresh") {
            return Err(SoulBoxError::authentication("Invalid token type").into());
        }

        Ok(token_data.claims)
    }

    /// 验证刷新令牌
    pub fn validate_refresh_token(&self, token: &str) -> Result<Claims> {
        // 临时修改验证器以接受刷新令牌签发者
        let mut validation = self.validation.clone();
        validation.set_issuer(&[&format!("{}-refresh", self.issuer)]);

        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)
            .context("Failed to decode refresh token")
            .map_err(|e| SoulBoxError::authentication(format!("Invalid refresh token: {}", e)))?;

        Ok(token_data.claims)
    }

    /// 从令牌中提取用户 ID
    pub fn extract_user_id(&self, token: &str) -> Result<Uuid> {
        let claims = self.validate_access_token(token)?;
        Uuid::parse_str(&claims.sub)
            .map_err(|e| SoulBoxError::authentication(format!("Invalid user ID: {}", e)).into())
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
            "test-secret-key",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        );
        
        let user = create_test_user();
        
        // 生成访问令牌
        let access_token = manager.generate_access_token(&user).unwrap();
        assert!(!access_token.is_empty());
        
        // 验证访问令牌
        let claims = manager.validate_access_token(&access_token).unwrap();
        assert_eq!(claims.sub, user.id.to_string());
        assert_eq!(claims.username, user.username);
        assert_eq!(claims.role, user.role);
        assert_eq!(claims.tenant_id, user.tenant_id);
    }

    #[test]
    fn test_refresh_token_generation_and_validation() {
        let manager = JwtManager::new(
            "test-secret-key",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        );
        
        let user = create_test_user();
        
        // 生成刷新令牌
        let refresh_token = manager.generate_refresh_token(&user).unwrap();
        assert!(!refresh_token.is_empty());
        
        // 验证刷新令牌
        let claims = manager.validate_refresh_token(&refresh_token).unwrap();
        assert_eq!(claims.sub, user.id.to_string());
        assert_eq!(claims.username, user.username);
    }

    #[test]
    fn test_invalid_token_validation() {
        let manager = JwtManager::new(
            "test-secret-key",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        );
        
        // 测试无效令牌
        assert!(manager.validate_access_token("invalid-token").is_err());
        assert!(manager.validate_refresh_token("invalid-token").is_err());
    }

    #[test]
    fn test_token_type_validation() {
        let manager = JwtManager::new(
            "test-secret-key",
            "soulbox".to_string(),
            "soulbox-api".to_string(),
        );
        
        let user = create_test_user();
        
        let access_token = manager.generate_access_token(&user).unwrap();
        let refresh_token = manager.generate_refresh_token(&user).unwrap();
        
        // 访问令牌不应该被当作刷新令牌验证
        assert!(manager.validate_refresh_token(&access_token).is_err());
        
        // 刷新令牌不应该被当作访问令牌验证
        assert!(manager.validate_access_token(&refresh_token).is_err());
    }
}