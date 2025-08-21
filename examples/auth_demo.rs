// 认证系统演示
// 展示 JWT 和 API Key 认证功能

use soulbox::auth::{JwtManager, ApiKeyManager, models::{User, Role, Permission}};
use soulbox::auth::api_key::ApiKeyTemplate;
use std::collections::HashSet;
use chrono::Utc;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔐 SoulBox 认证系统演示");
    println!("======================");

    // 1. JWT 管理器演示
    println!("\n📋 JWT 认证演示:");
    let jwt_manager = JwtManager::new(
        "demo-secret-key-do-not-use-in-production",
        "soulbox".to_string(),
        "soulbox-api".to_string(),
    )?;

    // 创建测试用户
    let user = User {
        id: Uuid::new_v4(),
        username: "alice_developer".to_string(),
        email: "alice@example.com".to_string(),
        role: Role::Developer,
        is_active: true,
        created_at: Utc::now(),
        last_login: None,
        tenant_id: Some(Uuid::new_v4()),
    };

    println!("👤 用户信息:");
    println!("   用户名: {}", user.username);
    println!("   角色: {:?}", user.role);
    println!("   权限: {:?}", user.role.permissions());

    // 生成 JWT 令牌
    let access_token = jwt_manager.generate_access_token(&user)?;
    let refresh_token = jwt_manager.generate_refresh_token(&user)?;

    println!("\n🎫 生成的令牌:");
    println!("   访问令牌: {}...", &access_token[..50]);
    println!("   刷新令牌: {}...", &refresh_token[..50]);

    // 验证令牌
    let claims = jwt_manager.validate_access_token(&access_token)?;
    println!("\n✅ 令牌验证成功:");
    println!("   用户ID: {}", claims.sub);
    println!("   用户名: {}", claims.username);
    println!("   角色: {:?}", claims.role);

    // 2. API Key 管理器演示
    println!("\n📋 API Key 认证演示:");
    let api_key_manager = ApiKeyManager::new("sk".to_string());

    // 生成 API Key
    let permissions = ApiKeyTemplate::developer_permissions();
    let (api_key, full_key) = api_key_manager.generate_api_key(
        "Alice的开发密钥".to_string(),
        user.id,
        permissions,
        None, // 永不过期
    )?;

    println!("🔑 生成的 API Key:");
    println!("   名称: {}", api_key.name);
    println!("   密钥: {}", full_key);
    println!("   显示密钥: {}", api_key_manager.extract_display_key(&full_key));
    println!("   权限数量: {}", api_key.permissions.len());

    // 验证 API Key
    let is_valid = api_key_manager.validate_api_key(&full_key, &api_key)?;
    println!("\n✅ API Key 验证: {}", if is_valid { "成功" } else { "失败" });

    // 权限检查演示
    println!("\n📋 权限检查演示:");
    let test_permissions = vec![
        Permission::SandboxCreate,
        Permission::SandboxDelete,
        Permission::UserCreate,
        Permission::SystemConfig,
    ];

    for permission in test_permissions {
        let has_permission = api_key_manager.check_permission(&api_key, &permission);
        println!("   {:?}: {}", permission, if has_permission { "✅ 允许" } else { "❌ 拒绝" });
    }

    // 3. 不同角色的权限演示
    println!("\n📋 角色权限对比:");
    let roles = vec![
        Role::SuperAdmin,
        Role::TenantAdmin,
        Role::Developer,
        Role::User,
        Role::ReadOnly,
    ];

    for role in roles {
        println!("   {:?}: {} 个权限", role, role.permissions().len());
    }

    println!("\n🎉 认证系统演示完成！");
    println!("\n💡 功能特点:");
    println!("   ✅ JWT 访问令牌和刷新令牌");
    println!("   ✅ API Key 生成和验证");
    println!("   ✅ 基于角色的权限控制 (RBAC)");
    println!("   ✅ 细粒度权限管理");
    println!("   ✅ 多租户支持");
    println!("   ✅ 密钥过期和撤销");

    Ok(())
}