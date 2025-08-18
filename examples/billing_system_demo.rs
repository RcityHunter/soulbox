//! Billing System Demonstration
//! 
//! This example demonstrates the fixed billing system with improved security,
//! thread safety, and precision handling.

use chrono::Utc;
use rust_decimal::Decimal;
use soulbox::billing::{
    models::{UsageMetric, MetricType},
    precision::FinancialPrecision,
    error_handling::{ValidationHelper, RetryConfig, ErrorRecoveryService},
    BillingConfig,
};
use std::sync::Arc;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::init();
    
    println!("🔧 SoulBox Billing System - Security Fixes Demo");
    println!("================================================");
    
    // 1. Demonstrate precision handling
    demonstrate_precision_handling().await?;
    
    // 2. Demonstrate error handling and recovery
    demonstrate_error_handling().await?;
    
    // 3. Demonstrate thread-safe operations
    demonstrate_thread_safety().await?;
    
    // 4. Demonstrate validation
    demonstrate_validation().await?;
    
    println!("\n✅ All demonstrations completed successfully!");
    println!("📊 Key fixes implemented:");
    println!("   • Thread-safe data structures with atomic operations");
    println!("   • Parameterized queries preventing SQL injection");
    println!("   • Proper resource management with Drop traits");
    println!("   • Simplified architecture without Redis Streams");
    println!("   • Unified financial precision handling");
    println!("   • Comprehensive error recovery mechanisms");
    
    Ok(())
}

async fn demonstrate_precision_handling() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n💰 1. Financial Precision Handling");
    println!("----------------------------------");
    
    // Safe monetary calculations
    let amount1 = Decimal::new(12345, 4); // $1.2345
    let amount2 = Decimal::new(67890, 4); // $6.7890
    
    println!("Amount 1: {}", FinancialPrecision::format_currency(amount1, "USD"));
    println!("Amount 2: {}", FinancialPrecision::format_currency(amount2, "USD"));
    
    // Safe addition with overflow protection
    match FinancialPrecision::safe_add(amount1, amount2) {
        Ok(sum) => println!("Safe sum: {}", FinancialPrecision::format_currency(sum, "USD")),
        Err(e) => println!("Addition error: {}", e),
    }
    
    // Safe multiplication
    let usage = Decimal::new(3600, 0); // 1 hour in seconds
    let rate = Decimal::new(1, 3); // $0.001 per second
    
    match FinancialPrecision::safe_multiply(usage, rate) {
        Ok(cost) => println!("Usage cost: {}", FinancialPrecision::format_currency(cost, "USD")),
        Err(e) => println!("Multiplication error: {}", e),
    }
    
    // Demonstrate precision rounding for different metric types
    let cpu_usage = Decimal::new(123456789, 8); // High precision CPU usage
    let rounded_cpu = FinancialPrecision::round_cpu(cpu_usage);
    println!("CPU usage precision: {} -> {}", cpu_usage, rounded_cpu);
    
    Ok(())
}

async fn demonstrate_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⚡ 2. Error Handling and Recovery");
    println!("--------------------------------");
    
    let recovery_service = Arc::new(ErrorRecoveryService::new(RetryConfig::default()));
    
    // Simulate an operation that fails initially but succeeds on retry
    let mut attempt_count = 0;
    let result = recovery_service.execute_with_recovery("demo_operation", || {
        attempt_count += 1;
        if attempt_count < 3 {
            Err("temporary network error")
        } else {
            Ok("success")
        }
    }).await;
    
    match result {
        Ok(value) => println!("Operation succeeded after {} attempts: {}", attempt_count, value),
        Err(e) => println!("Operation failed: {}", e),
    }
    
    // Demonstrate error statistics
    let stats = recovery_service.get_error_stats();
    println!("Total errors handled: {}", stats.total_errors);
    println!("Total recoveries: {}", stats.total_recoveries);
    
    Ok(())
}

async fn demonstrate_thread_safety() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔒 3. Thread Safety Demonstration");
    println!("--------------------------------");
    
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::sync::Mutex;
    use tokio::task;
    
    // Simulate concurrent metric collection
    let metric_count = Arc::new(AtomicU64::new(0));
    let metrics_buffer = Arc::new(Mutex::new(Vec::<UsageMetric>::new()));
    
    let mut handles = Vec::new();
    
    // Spawn multiple tasks to simulate concurrent metric collection
    for i in 0..10 {
        let count = metric_count.clone();
        let buffer = metrics_buffer.clone();
        
        let handle = task::spawn(async move {
            // Simulate metric creation
            let metric = UsageMetric {
                id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                metric_type: MetricType::CpuUsage,
                value: Decimal::new(i * 100, 2),
                timestamp: Utc::now(),
                metadata: None,
            };
            
            // Thread-safe operations
            count.fetch_add(1, Ordering::SeqCst);
            
            let mut buffer_guard = buffer.lock().await;
            buffer_guard.push(metric);
            drop(buffer_guard);
            
            i
        });
        
        handles.push(handle);
    }
    
    // Wait for all tasks to complete
    for handle in handles {
        handle.await?;
    }
    
    let final_count = metric_count.load(Ordering::SeqCst);
    let buffer_size = metrics_buffer.lock().await.len();
    
    println!("Concurrent operations completed:");
    println!("  - Final metric count: {}", final_count);
    println!("  - Buffer size: {}", buffer_size);
    println!("  - Data consistency: {}", if final_count == buffer_size as u64 { "✅ Maintained" } else { "❌ Lost" });
    
    Ok(())
}

async fn demonstrate_validation() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🛡️  4. Input Validation");
    println!("----------------------");
    
    // Test amount validation
    let valid_amount = Decimal::new(12345, 2); // $123.45
    match ValidationHelper::validate_amount(valid_amount, "test_amount") {
        Ok(amount) => println!("✅ Valid amount: ${}", amount),
        Err(e) => println!("❌ Invalid amount: {}", e),
    }
    
    let invalid_amount = Decimal::new(-100, 2); // -$1.00
    match ValidationHelper::validate_amount(invalid_amount, "test_amount") {
        Ok(amount) => println!("✅ Valid amount: ${}", amount),
        Err(e) => println!("❌ Invalid amount: {}", e),
    }
    
    // Test user ID validation
    let valid_user_id = Uuid::new_v4();
    match ValidationHelper::validate_user_id(valid_user_id) {
        Ok(_) => println!("✅ Valid user ID: {}", valid_user_id),
        Err(e) => println!("❌ Invalid user ID: {}", e),
    }
    
    let nil_user_id = Uuid::nil();
    match ValidationHelper::validate_user_id(nil_user_id) {
        Ok(_) => println!("✅ Valid user ID: {}", nil_user_id),
        Err(e) => println!("❌ Invalid user ID: {}", e),
    }
    
    // Test time range validation
    let now = Utc::now();
    let valid_start = now - chrono::Duration::hours(1);
    let valid_end = now;
    
    match ValidationHelper::validate_time_range(valid_start, valid_end) {
        Ok((start, end)) => println!("✅ Valid time range: {} to {}", start.format("%H:%M:%S"), end.format("%H:%M:%S")),
        Err(e) => println!("❌ Invalid time range: {}", e),
    }
    
    let invalid_start = now;
    let invalid_end = now - chrono::Duration::hours(1); // End before start
    
    match ValidationHelper::validate_time_range(invalid_start, invalid_end) {
        Ok((start, end)) => println!("✅ Valid time range: {} to {}", start.format("%H:%M:%S"), end.format("%H:%M:%S")),
        Err(e) => println!("❌ Invalid time range: {}", e),
    }
    
    Ok(())
}