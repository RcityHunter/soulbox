// RBAC 权限管理演示
// 展示 REST API 权限检查和管理功能

use reqwest::Client;
use serde_json::{json, Value};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🛡️  SoulBox RBAC 权限管理演示");
    println!("=============================");

    let client = Client::new();
    let base_url = "http://localhost:3000";

    // 1. 以开发者身份登录
    println!("\n📋 1. 开发者登录:");
    let login_response = client
        .post(&format!("{}/api/v1/auth/login", base_url))
        .json(&json!({
            "username": "developer",
            "password": "password123"
        }))
        .send()
        .await?;

    if !login_response.status().is_success() {
        println!("❌ 登录失败: {}", login_response.status());
        return Ok(());
    }

    let login_data: Value = login_response.json().await?;
    let developer_token = login_data["access_token"].as_str().unwrap();
    println!("✅ 开发者登录成功");

    // 2. 查看角色权限
    println!("\n📋 2. 查看所有角色权限:");
    let roles_response = client
        .get(&format!("{}/api/v1/permissions/roles", base_url))
        .header("Authorization", format!("Bearer {}", developer_token))
        .send()
        .await?;

    if roles_response.status().is_success() {
        let roles_data: Value = roles_response.json().await?;
        println!("✅ 角色权限信息:");
        for role in roles_data.as_array().unwrap() {
            println!("   {} - {} 个权限", 
                role["role"].as_str().unwrap_or("Unknown"),
                role["permission_count"].as_u64().unwrap_or(0));
        }
    } else {
        println!("❌ 获取角色权限失败: {}", roles_response.status());
    }

    // 3. 查看用户权限
    println!("\n📋 3. 查看用户权限:");
    let user_id = login_data["user"]["id"].as_str().unwrap();
    let user_perms_response = client
        .get(&format!("{}/api/v1/permissions/users/{}", base_url, user_id))
        .header("Authorization", format!("Bearer {}", developer_token))
        .send()
        .await?;

    if user_perms_response.status().is_success() {
        let user_perms_data: Value = user_perms_response.json().await?;
        println!("✅ 用户权限信息:");
        println!("   用户: {}", user_perms_data["username"].as_str().unwrap_or("Unknown"));
        println!("   角色: {:?}", user_perms_data["role"]);
        println!("   权限数量: {}", user_perms_data["permissions"].as_array().unwrap().len());
    } else {
        println!("❌ 获取用户权限失败: {}", user_perms_response.status());
    }

    // 4. 权限检查测试
    println!("\n📋 4. 权限检查测试:");
    let test_permissions = vec![
        "SandboxCreate",
        "SandboxDelete",
        "UserCreate",
        "SystemConfig"
    ];

    for permission in test_permissions {
        let check_response = client
            .post(&format!("{}/api/v1/permissions/check", base_url))
            .header("Authorization", format!("Bearer {}", developer_token))
            .json(&json!({
                "user_id": user_id,
                "permission": permission
            }))
            .send()
            .await?;

        if check_response.status().is_success() {
            let check_data: Value = check_response.json().await?;
            let has_permission = check_data["has_permission"].as_bool().unwrap_or(false);
            println!("   {}: {}", permission, if has_permission { "✅ 允许" } else { "❌ 拒绝" });
        } else {
            println!("   {}: ❌ 检查失败", permission);
        }
    }

    // 5. 测试沙盒权限 - 创建沙盒
    println!("\n📋 5. 测试沙盒权限:");
    let sandbox_create_response = client
        .post(&format!("{}/api/v1/sandboxes", base_url))
        .header("Authorization", format!("Bearer {}", developer_token))
        .json(&json!({
            "template": "python:3.11",
            "resource_limits": {
                "cpu": 1.0,
                "memory": "512MB"
            }
        }))
        .send()
        .await?;

    if sandbox_create_response.status().is_success() {
        let sandbox_data: Value = sandbox_create_response.json().await?;
        println!("✅ 沙盒创建成功: {}", sandbox_data["id"].as_str().unwrap_or("unknown"));
        
        // 测试获取沙盒
        let sandbox_id = sandbox_data["id"].as_str().unwrap();
        let sandbox_get_response = client
            .get(&format!("{}/api/v1/sandboxes/{}", base_url, sandbox_id))
            .header("Authorization", format!("Bearer {}", developer_token))
            .send()
            .await?;

        if sandbox_get_response.status().is_success() {
            println!("✅ 沙盒查询成功");
        } else {
            println!("❌ 沙盒查询失败: {}", sandbox_get_response.status());
        }

        // 测试删除沙盒
        let sandbox_delete_response = client
            .delete(&format!("{}/api/v1/sandboxes/{}", base_url, sandbox_id))
            .header("Authorization", format!("Bearer {}", developer_token))
            .send()
            .await?;

        if sandbox_delete_response.status().is_success() {
            println!("✅ 沙盒删除成功");
        } else {
            println!("❌ 沙盒删除失败: {}", sandbox_delete_response.status());
        }

    } else {
        println!("❌ 沙盒创建失败: {}", sandbox_create_response.status());
    }

    // 6. 以只读用户身份登录测试限制
    println!("\n📋 6. 只读用户权限测试:");
    let readonly_login_response = client
        .post(&format!("{}/api/v1/auth/login", base_url))
        .json(&json!({
            "username": "readonly",
            "password": "password123"
        }))
        .send()
        .await?;

    if readonly_login_response.status().is_success() {
        let readonly_data: Value = readonly_login_response.json().await?;
        let readonly_token = readonly_data["access_token"].as_str().unwrap();
        println!("✅ 只读用户登录成功");

        // 尝试创建沙盒（应该失败）
        let forbidden_create_response = client
            .post(&format!("{}/api/v1/sandboxes", base_url))
            .header("Authorization", format!("Bearer {}", readonly_token))
            .json(&json!({
                "template": "python:3.11"
            }))
            .send()
            .await?;

        if forbidden_create_response.status() == 403 {
            println!("✅ 只读用户正确被拒绝创建沙盒 (403 Forbidden)");
        } else {
            println!("❌ 只读用户权限检查异常: {}", forbidden_create_response.status());
        }

        // 尝试分配角色（应该失败）
        let forbidden_role_response = client
            .put(&format!("{}/api/v1/permissions/assign-role", base_url))
            .header("Authorization", format!("Bearer {}", readonly_token))
            .json(&json!({
                "user_id": user_id,
                "role": "Developer"
            }))
            .send()
            .await?;

        if forbidden_role_response.status() == 403 {
            println!("✅ 只读用户正确被拒绝分配角色 (403 Forbidden)");
        } else {
            println!("❌ 只读用户角色管理权限检查异常: {}", forbidden_role_response.status());
        }

    } else {
        println!("❌ 只读用户登录失败: {}", readonly_login_response.status());
    }

    // 7. 以超级管理员身份测试系统权限
    println!("\n📋 7. 超级管理员系统权限测试:");
    let admin_login_response = client
        .post(&format!("{}/api/v1/auth/login", base_url))
        .json(&json!({
            "username": "admin",
            "password": "password123"
        }))
        .send()
        .await?;

    if admin_login_response.status().is_success() {
        let admin_data: Value = admin_login_response.json().await?;
        let admin_token = admin_data["access_token"].as_str().unwrap();
        println!("✅ 超级管理员登录成功");

        // 测试系统权限端点
        let system_perms_response = client
            .get(&format!("{}/api/v1/permissions/system/all", base_url))
            .header("Authorization", format!("Bearer {}", admin_token))
            .send()
            .await?;

        if system_perms_response.status().is_success() {
            let system_data: Value = system_perms_response.json().await?;
            println!("✅ 系统权限查询成功 - 共 {} 个权限", 
                system_data["total_permissions"].as_u64().unwrap_or(0));
        } else {
            println!("❌ 系统权限查询失败: {}", system_perms_response.status());
        }

        // 测试角色分配
        let assign_role_response = client
            .put(&format!("{}/api/v1/permissions/assign-role", base_url))
            .header("Authorization", format!("Bearer {}", admin_token))
            .json(&json!({
                "user_id": user_id,
                "role": "TenantAdmin",
                "reason": "演示角色分配功能"
            }))
            .send()
            .await?;

        if assign_role_response.status().is_success() {
            println!("✅ 角色分配成功");
        } else {
            println!("❌ 角色分配失败: {}", assign_role_response.status());
        }

    } else {
        println!("❌ 超级管理员登录失败: {}", admin_login_response.status());
    }

    println!("\n🎉 RBAC 权限管理演示完成！");
    println!("\n💡 测试内容:");
    println!("   ✅ 多角色权限查询");
    println!("   ✅ 用户权限检查");
    println!("   ✅ 细粒度权限验证");
    println!("   ✅ 受保护的沙盒操作");
    println!("   ✅ 权限拒绝机制");
    println!("   ✅ 角色层级权限");
    println!("   ✅ 系统管理员特权");
    println!("   ✅ 角色分配管理");

    Ok(())
}