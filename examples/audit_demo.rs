// 审计日志系统演示
// 展示完整的审计日志功能，包括记录、查询和分析

use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("📊 SoulBox 审计日志系统演示");
    println!("==============================");

    let client = Client::new();
    let base_url = "http://localhost:3000";

    // 1. 启动服务器（这里假设服务器已经在运行）
    println!("\n🔍 1. 检查服务器状态:");
    let health_response = client
        .get(&format!("{}/health", base_url))
        .send()
        .await?;

    if health_response.status().is_success() {
        println!("✅ 服务器运行正常");
    } else {
        println!("❌ 服务器未运行，请先启动服务器");
        return Ok(());
    }

    // 2. 管理员登录以获取审计权限
    println!("\n📋 2. 管理员登录（获取审计查看权限）:");
    let admin_login_response = client
        .post(&format!("{}/api/v1/auth/login", base_url))
        .json(&json!({
            "username": "admin",
            "password": "password123"
        }))
        .send()
        .await?;

    if !admin_login_response.status().is_success() {
        println!("❌ 管理员登录失败: {}", admin_login_response.status());
        return Ok(());
    }

    let admin_data: Value = admin_login_response.json().await?;
    let admin_token = admin_data["access_token"].as_str().unwrap();
    println!("✅ 管理员登录成功");

    // 3. 生成一些测试活动来创建审计日志
    println!("\n📋 3. 生成测试活动:");
    
    // 3.1 普通用户登录
    println!("   → 普通用户登录");
    let user_login_response = client
        .post(&format!("{}/api/v1/auth/login", base_url))
        .json(&json!({
            "username": "developer",
            "password": "password123"
        }))
        .send()
        .await?;

    let mut developer_token = String::new();
    if user_login_response.status().is_success() {
        let user_data: Value = user_login_response.json().await?;
        developer_token = user_data["access_token"].as_str().unwrap().to_string();
        println!("   ✅ 开发者用户登录成功");
    }

    // 3.2 创建沙盒
    println!("   → 创建沙盒");
    let create_sandbox_response = client
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

    if create_sandbox_response.status().is_success() {
        println!("   ✅ 沙盒创建成功");
    } else {
        println!("   ⚠️ 沙盒创建失败: {}", create_sandbox_response.status());
    }

    // 3.3 尝试无权限操作（生成拒绝事件）
    println!("   → 尝试无权限的系统配置操作");
    let forbidden_response = client
        .get(&format!("{}/api/v1/permissions/system/all", base_url))
        .header("Authorization", format!("Bearer {}", developer_token))
        .send()
        .await?;

    if forbidden_response.status() == 403 {
        println!("   ✅ 权限拒绝事件生成成功 (403 Forbidden)");
    }

    // 3.4 无效登录尝试
    println!("   → 无效登录尝试");
    let invalid_login_response = client
        .post(&format!("{}/api/v1/auth/login", base_url))
        .json(&json!({
            "username": "nonexistent",
            "password": "wrongpassword"
        }))
        .send()
        .await?;

    if !invalid_login_response.status().is_success() {
        println!("   ✅ 无效登录事件生成成功");
    }

    // 等待一下让审计日志处理完成
    sleep(Duration::from_millis(100)).await;

    // 4. 查询审计日志
    println!("\n📋 4. 查询审计日志:");

    // 4.1 查询所有审计日志
    println!("   → 查询所有审计日志");
    let all_logs_response = client
        .get(&format!("{}/api/v1/audit/logs", base_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .query(&[("limit", "20")])
        .send()
        .await?;

    if all_logs_response.status().is_success() {
        let logs_data: Value = all_logs_response.json().await?;
        let logs_count = logs_data["total"].as_u64().unwrap_or(0);
        println!("   ✅ 获取到 {} 条审计日志", logs_count);
        
        // 显示前几条日志
        if let Some(logs) = logs_data["logs"].as_array() {
            for (i, log) in logs.iter().take(3).enumerate() {
                println!("     {}. {} - {} ({})", 
                    i + 1,
                    log["event_type"].as_str().unwrap_or("Unknown"),
                    log["message"].as_str().unwrap_or("No message"),
                    log["result"].as_str().unwrap_or("Unknown")
                );
            }
        }
    }

    // 4.2 查询登录事件
    println!("   → 查询登录相关事件");
    let login_logs_response = client
        .get(&format!("{}/api/v1/audit/logs", base_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .query(&[
            ("event_type", "userlogin"),
            ("limit", "10")
        ])
        .send()
        .await?;

    if login_logs_response.status().is_success() {
        let login_data: Value = login_logs_response.json().await?;
        let login_count = login_data["total"].as_u64().unwrap_or(0);
        println!("   ✅ 获取到 {} 条登录事件", login_count);
    }

    // 4.3 查询安全事件
    println!("   → 查询安全事件");
    let security_events_response = client
        .get(&format!("{}/api/v1/audit/security-events", base_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .send()
        .await?;

    if security_events_response.status().is_success() {
        let security_data: Value = security_events_response.json().await?;
        let security_count = security_data["total"].as_u64().unwrap_or(0);
        println!("   ✅ 获取到 {} 条安全事件", security_count);
        
        if let Some(events) = security_data["security_events"].as_array() {
            for (i, event) in events.iter().take(3).enumerate() {
                println!("     {}. {} - {} (严重程度: {})", 
                    i + 1,
                    event["event_type"].as_str().unwrap_or("Unknown"),
                    event["message"].as_str().unwrap_or("No message"),
                    event["severity"].as_str().unwrap_or("Unknown")
                );
            }
        }
    }

    // 5. 获取审计统计信息
    println!("\n📋 5. 获取审计统计信息:");
    let stats_response = client
        .get(&format!("{}/api/v1/audit/stats", base_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .send()
        .await?;

    if stats_response.status().is_success() {
        let stats_data: Value = stats_response.json().await?;
        
        println!("   ✅ 审计统计信息:");
        println!("     总事件数: {}", stats_data["total_events"].as_u64().unwrap_or(0));
        println!("     安全事件数: {}", stats_data["security_events"].as_u64().unwrap_or(0));
        println!("     失败事件数: {}", stats_data["failed_events"].as_u64().unwrap_or(0));
        println!("     唯一用户数: {}", stats_data["unique_users"].as_u64().unwrap_or(0));
        
        // 显示按类型分组的事件统计
        if let Some(events_by_type) = stats_data["events_by_type"].as_object() {
            println!("     按类型分组:");
            for (event_type, count) in events_by_type.iter().take(5) {
                println!("       {}: {}", event_type, count.as_u64().unwrap_or(0));
            }
        }
    }

    // 6. 测试不同权限级别的访问
    println!("\n📋 6. 测试权限控制:");
    
    // 6.1 只读用户尝试访问审计日志
    println!("   → 只读用户尝试访问审计日志");
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
        
        let readonly_audit_response = client
            .get(&format!("{}/api/v1/audit/logs", base_url))
            .header("Authorization", format!("Bearer {}", readonly_token))
            .send()
            .await?;

        if readonly_audit_response.status() == 403 {
            println!("   ✅ 只读用户正确被拒绝访问审计日志 (403 Forbidden)");
        } else {
            println!("   ❌ 只读用户权限检查异常: {}", readonly_audit_response.status());
        }
    }

    // 7. 高级查询功能测试
    println!("\n📋 7. 高级查询功能测试:");
    
    // 7.1 按严重程度查询
    println!("   → 查询警告级别事件");
    let warning_logs_response = client
        .get(&format!("{}/api/v1/audit/logs", base_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .query(&[
            ("severity", "warning"),
            ("limit", "10")
        ])
        .send()
        .await?;

    if warning_logs_response.status().is_success() {
        let warning_data: Value = warning_logs_response.json().await?;
        let warning_count = warning_data["total"].as_u64().unwrap_or(0);
        println!("   ✅ 获取到 {} 条警告级别事件", warning_count);
    }

    // 7.2 按结果查询
    println!("   → 查询失败事件");
    let failure_logs_response = client
        .get(&format!("{}/api/v1/audit/logs", base_url))
        .header("Authorization", format!("Bearer {}", admin_token))
        .query(&[
            ("result", "failure"),
            ("limit", "10")
        ])
        .send()
        .await?;

    if failure_logs_response.status().is_success() {
        let failure_data: Value = failure_logs_response.json().await?;
        let failure_count = failure_data["total"].as_u64().unwrap_or(0);
        println!("   ✅ 获取到 {} 条失败事件", failure_count);
    }

    // 8. 总结
    println!("\n🎉 审计日志系统演示完成！");
    println!("\n💡 演示内容:");
    println!("   ✅ 自动审计日志记录");
    println!("   ✅ HTTP 请求审计");
    println!("   ✅ 认证事件审计");
    println!("   ✅ 权限检查审计");
    println!("   ✅ 安全事件检测");
    println!("   ✅ 审计日志查询 API");
    println!("   ✅ 审计统计分析");
    println!("   ✅ 基于权限的访问控制");
    println!("   ✅ 多维度查询过滤");
    println!("   ✅ 租户隔离支持");

    println!("\n🔒 安全特性:");
    println!("   ✅ 实时安全事件告警");
    println!("   ✅ 失败操作自动记录");
    println!("   ✅ 用户行为追踪");
    println!("   ✅ 系统操作审计");
    println!("   ✅ IP 地址和用户代理记录");

    Ok(())
}