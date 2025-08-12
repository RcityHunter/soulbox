// SoulBox 认证 API 演示
// 演示如何使用认证端点进行登录和 API 操作

use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔐 SoulBox 认证 API 演示");
    println!("=======================");

    // 创建 HTTP 客户端
    let client = Client::new();
    let base_url = "http://localhost:3000";

    println!("\n📋 测试健康检查:");
    let health_response = client
        .get(&format!("{}/health", base_url))
        .send()
        .await?;
    
    println!("健康检查状态: {}", health_response.status());
    let health_body: Value = health_response.json().await?;
    println!("响应: {}", serde_json::to_string_pretty(&health_body)?);

    println!("\n📋 用户登录:");
    let login_payload = json!({
        "username": "developer",
        "password": "password123"
    });

    let login_response = client
        .post(&format!("{}/api/v1/auth/login", base_url))
        .json(&login_payload)
        .send()
        .await?;

    println!("登录状态: {}", login_response.status());
    
    if login_response.status().is_success() {
        let login_body: Value = login_response.json().await?;
        println!("登录成功: {}", serde_json::to_string_pretty(&login_body)?);

        // 提取访问令牌
        let access_token = login_body["access_token"].as_str().unwrap();
        println!("\n🎫 获取访问令牌: {}...", &access_token[..50]);

        println!("\n📋 测试受保护的用户资料端点:");
        let profile_response = client
            .get(&format!("{}/api/v1/auth/profile", base_url))
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await?;

        println!("用户资料状态: {}", profile_response.status());
        if profile_response.status().is_success() {
            let profile_body: Value = profile_response.json().await?;
            println!("用户资料: {}", serde_json::to_string_pretty(&profile_body)?);
        } else {
            let error_body = profile_response.text().await?;
            println!("错误: {}", error_body);
        }

        println!("\n📋 创建 API 密钥:");
        let api_key_payload = json!({
            "name": "测试开发密钥",
            "expires_days": 30
        });

        let api_key_response = client
            .post(&format!("{}/api/v1/auth/api-keys", base_url))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&api_key_payload)
            .send()
            .await?;

        println!("API 密钥创建状态: {}", api_key_response.status());
        if api_key_response.status().is_success() {
            let api_key_body: Value = api_key_response.json().await?;
            println!("API 密钥创建成功: {}", serde_json::to_string_pretty(&api_key_body)?);
        } else {
            let error_body = api_key_response.text().await?;
            println!("错误: {}", error_body);
        }

        println!("\n📋 测试受保护的沙盒端点:");
        let sandbox_payload = json!({
            "template": "python:3.11",
            "environment_vars": {},
            "resource_limits": {
                "cpu": 1.0,
                "memory": "512MB"
            }
        });

        let sandbox_response = client
            .post(&format!("{}/api/v1/sandboxes", base_url))
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&sandbox_payload)
            .send()
            .await?;

        println!("沙盒创建状态: {}", sandbox_response.status());
        if sandbox_response.status().is_success() {
            let sandbox_body: Value = sandbox_response.json().await?;
            println!("沙盒创建成功: {}", serde_json::to_string_pretty(&sandbox_body)?);
        } else {
            let error_body = sandbox_response.text().await?;
            println!("错误: {}", error_body);
        }

        println!("\n📋 刷新令牌:");
        let refresh_payload = json!({
            "refresh_token": login_body["refresh_token"].as_str().unwrap()
        });

        let refresh_response = client
            .post(&format!("{}/api/v1/auth/refresh", base_url))
            .json(&refresh_payload)
            .send()
            .await?;

        println!("刷新令牌状态: {}", refresh_response.status());
        if refresh_response.status().is_success() {
            let refresh_body: Value = refresh_response.json().await?;
            println!("令牌刷新成功: {}", serde_json::to_string_pretty(&refresh_body)?);
        } else {
            let error_body = refresh_response.text().await?;
            println!("错误: {}", error_body);
        }

        println!("\n📋 注销:");
        let logout_response = client
            .post(&format!("{}/api/v1/auth/logout", base_url))
            .header("Authorization", format!("Bearer {}", access_token))
            .send()
            .await?;

        println!("注销状态: {}", logout_response.status());
        if logout_response.status().is_success() {
            let logout_body: Value = logout_response.json().await?;
            println!("注销成功: {}", serde_json::to_string_pretty(&logout_body)?);
        }

    } else {
        let error_body = login_response.text().await?;
        println!("登录失败: {}", error_body);
    }

    println!("\n📋 测试未认证访问（应该失败）:");
    let unauth_response = client
        .get(&format!("{}/api/v1/auth/profile", base_url))
        .send()
        .await?;

    println!("未认证访问状态: {} (预期: 401 Unauthorized)", unauth_response.status());

    println!("\n🎉 认证 API 演示完成！");
    println!("\n💡 测试内容:");
    println!("   ✅ 健康检查端点");
    println!("   ✅ 用户登录");
    println!("   ✅ JWT 令牌验证");
    println!("   ✅ 受保护的资源访问");
    println!("   ✅ API 密钥管理");
    println!("   ✅ 令牌刷新");
    println!("   ✅ 用户注销");
    println!("   ✅ 未认证访问拒绝");

    Ok(())
}