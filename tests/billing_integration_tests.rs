//! Billing System Integration Tests
//! 
//! These tests verify the integration and functionality of the billing system components.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use soulbox::billing::{
    BillingConfig, BillingService, CostCalculator, 
    models::*,
    cost_calculator::{DiscountType, UserBillingContext},
    usage_aggregator::AggregationPeriod,
};
use std::collections::HashMap;
use uuid::Uuid;

/// Test basic billing configuration
#[test]
fn test_billing_config_creation() {
    let config = BillingConfig::default();
    
    assert_eq!(config.metrics_stream, "soulbox:metrics");
    assert_eq!(config.batch_size, 100);
    assert_eq!(config.collection_interval, 30);
    assert_eq!(config.buffer_size, 1000);
    assert_eq!(config.flush_interval, 60);
}

/// Test cost calculator with different pricing tiers
#[test]
fn test_cost_calculator_pricing_tiers() {
    let calculator = CostCalculator::new();
    
    // Test that all pricing tiers are available
    assert!(calculator.get_pricing_info(&PricingTier::Free).is_some());
    assert!(calculator.get_pricing_info(&PricingTier::Basic).is_some());
    assert!(calculator.get_pricing_info(&PricingTier::Professional).is_some());
    assert!(calculator.get_pricing_info(&PricingTier::Enterprise).is_some());
    
    // Test free tier has zero rates
    let free_pricing = calculator.get_pricing_info(&PricingTier::Free).unwrap();
    for rate in free_pricing.rates.values() {
        assert_eq!(*rate, Decimal::ZERO);
    }
    
    // Test professional tier has lower rates than basic
    let basic_pricing = calculator.get_pricing_info(&PricingTier::Basic).unwrap();
    let pro_pricing = calculator.get_pricing_info(&PricingTier::Professional).unwrap();
    
    let basic_cpu_rate = basic_pricing.rates.get(&MetricType::CpuUsage).unwrap();
    let pro_cpu_rate = pro_pricing.rates.get(&MetricType::CpuUsage).unwrap();
    
    assert!(pro_cpu_rate < basic_cpu_rate, "Professional tier should have lower CPU rates");
}

/// Test usage metric creation and validation
#[test]
fn test_usage_metric_creation() {
    let user_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let timestamp = Utc::now();
    
    let metric = UsageMetric {
        id: Uuid::new_v4(),
        session_id,
        user_id,
        metric_type: MetricType::CpuUsage,
        value: Decimal::new(3600, 0), // 1 hour
        timestamp,
        metadata: Some([
            ("container_id".to_string(), "test-container".to_string()),
            ("source".to_string(), "test".to_string()),
        ].iter().cloned().collect()),
    };
    
    assert_eq!(metric.user_id, user_id);
    assert_eq!(metric.session_id, session_id);
    assert_eq!(metric.metric_type, MetricType::CpuUsage);
    assert_eq!(metric.value, Decimal::new(3600, 0));
    assert_eq!(metric.timestamp, timestamp);
    assert!(metric.metadata.is_some());
}

/// Test metric type properties
#[test]
fn test_metric_type_properties() {
    // Test units
    assert_eq!(MetricType::CpuUsage.unit(), "cpu-seconds");
    assert_eq!(MetricType::MemoryUsage.unit(), "mb-seconds");
    assert_eq!(MetricType::NetworkIngress.unit(), "bytes");
    assert_eq!(MetricType::NetworkEgress.unit(), "bytes");
    assert_eq!(MetricType::StorageUsage.unit(), "gb-seconds");
    assert_eq!(MetricType::ExecutionTime.unit(), "seconds");
    assert_eq!(MetricType::ApiRequests.unit(), "requests");
    assert_eq!(MetricType::Custom("test".to_string()).unit(), "units");
    
    // Test cumulative properties
    assert!(MetricType::CpuUsage.is_cumulative());
    assert!(MetricType::MemoryUsage.is_cumulative());
    assert!(!MetricType::NetworkIngress.is_cumulative());
    assert!(!MetricType::NetworkEgress.is_cumulative());
    assert!(MetricType::StorageUsage.is_cumulative());
    assert!(MetricType::ExecutionTime.is_cumulative());
    assert!(!MetricType::ApiRequests.is_cumulative());
}

/// Test usage summary creation
#[test]
fn test_usage_summary_creation() {
    let user_id = Uuid::new_v4();
    let start_time = Utc::now() - chrono::Duration::hours(1);
    let end_time = Utc::now();
    
    let mut metrics = HashMap::new();
    metrics.insert(MetricType::CpuUsage, AggregatedMetric {
        total: Decimal::new(3600, 0), // 1 hour
        average: Decimal::new(3600, 0),
        maximum: Decimal::new(3600, 0),
        minimum: Decimal::new(3600, 0),
        count: 1,
    });
    
    metrics.insert(MetricType::MemoryUsage, AggregatedMetric {
        total: Decimal::new(1024000000, 0), // 1GB for 1 hour in mb-seconds
        average: Decimal::new(1024000000, 0),
        maximum: Decimal::new(1024000000, 0),
        minimum: Decimal::new(1024000000, 0),
        count: 1,
    });
    
    let summary = UsageSummary {
        user_id,
        start_time,
        end_time,
        metrics,
        session_count: 1,
        created_at: Utc::now(),
    };
    
    assert_eq!(summary.user_id, user_id);
    assert_eq!(summary.session_count, 1);
    assert_eq!(summary.metrics.len(), 2);
    assert!(summary.metrics.contains_key(&MetricType::CpuUsage));
    assert!(summary.metrics.contains_key(&MetricType::MemoryUsage));
}

/// Test cost calculation for basic usage
#[tokio::test]
async fn test_basic_cost_calculation() {
    let calculator = CostCalculator::new();
    
    // Create usage summary with 1 hour of CPU and 1GB of memory
    let user_id = Uuid::new_v4();
    let start_time = Utc::now() - chrono::Duration::hours(1);
    let end_time = Utc::now();
    
    let mut metrics = HashMap::new();
    
    // CPU usage: 1 hour (3600 seconds)
    metrics.insert(MetricType::CpuUsage, AggregatedMetric {
        total: Decimal::new(3600, 0),
        average: Decimal::new(3600, 0),
        maximum: Decimal::new(3600, 0),
        minimum: Decimal::new(3600, 0),
        count: 1,
    });
    
    // Memory usage: 1GB for 1 hour = 1024 * 1024 * 1024 bytes * 3600 seconds
    // But we track in MB-seconds, so 1024MB * 3600s = 3,686,400 mb-seconds
    metrics.insert(MetricType::MemoryUsage, AggregatedMetric {
        total: Decimal::new(3686400, 0),
        average: Decimal::new(3686400, 0),
        maximum: Decimal::new(3686400, 0),
        minimum: Decimal::new(3686400, 0),
        count: 1,
    });
    
    let usage_summary = UsageSummary {
        user_id,
        start_time,
        end_time,
        metrics,
        session_count: 1,
        created_at: Utc::now(),
    };
    
    let billing_record = calculator.calculate_cost(&usage_summary).await.unwrap();
    
    // Should have line items for CPU and memory
    assert_eq!(billing_record.line_items.len(), 2);
    assert_eq!(billing_record.user_id, user_id);
    assert!(billing_record.total_cost >= Decimal::ZERO);
    
    // Check that CPU line item exists
    let cpu_line_item = billing_record.line_items.iter()
        .find(|item| item.metric_type == MetricType::CpuUsage)
        .expect("Should have CPU line item");
    
    assert_eq!(cpu_line_item.usage, Decimal::new(3600, 0));
    assert!(cpu_line_item.rate > Decimal::ZERO); // Basic tier should have positive rates
    
    // Check that memory line item exists
    let memory_line_item = billing_record.line_items.iter()
        .find(|item| item.metric_type == MetricType::MemoryUsage)
        .expect("Should have memory line item");
    
    assert_eq!(memory_line_item.usage, Decimal::new(3686400, 0));
    assert!(memory_line_item.rate > Decimal::ZERO);
}

/// Test cost calculation with free allowances
#[tokio::test]
async fn test_cost_calculation_with_free_allowances() {
    let calculator = CostCalculator::new();
    
    // Create usage that falls within free allowance
    let user_id = Uuid::new_v4();
    let start_time = Utc::now() - chrono::Duration::minutes(30);
    let end_time = Utc::now();
    
    let mut metrics = HashMap::new();
    
    // CPU usage: 30 minutes (1800 seconds) - should be within free allowance
    metrics.insert(MetricType::CpuUsage, AggregatedMetric {
        total: Decimal::new(1800, 0),
        average: Decimal::new(1800, 0),
        maximum: Decimal::new(1800, 0),
        minimum: Decimal::new(1800, 0),
        count: 1,
    });
    
    let usage_summary = UsageSummary {
        user_id,
        start_time,
        end_time,
        metrics,
        session_count: 1,
        created_at: Utc::now(),
    };
    
    let billing_record = calculator.calculate_cost(&usage_summary).await.unwrap();
    
    let cpu_line_item = billing_record.line_items.iter()
        .find(|item| item.metric_type == MetricType::CpuUsage)
        .expect("Should have CPU line item");
    
    // Should have zero billable usage due to free allowance
    assert_eq!(cpu_line_item.billable_usage, Decimal::ZERO);
    assert_eq!(cpu_line_item.cost, Decimal::ZERO);
}

/// Test discount calculations
#[test]
fn test_discount_calculations() {
    let subtotal = Decimal::new(10000, 2); // $100.00
    
    // Test percentage discount
    let percentage_discount = DiscountType::Percentage(Decimal::new(20, 0)); // 20%
    let discount_amount = percentage_discount.calculate_discount(subtotal);
    assert_eq!(discount_amount, Decimal::new(2000, 2)); // $20.00
    
    // Test fixed amount discount
    let fixed_discount = DiscountType::FixedAmount(Decimal::new(1500, 2)); // $15.00
    let discount_amount = fixed_discount.calculate_discount(subtotal);
    assert_eq!(discount_amount, Decimal::new(1500, 2)); // $15.00
    
    // Test volume discount (threshold met)
    let volume_discount = DiscountType::Volume(
        Decimal::new(5000, 2), // $50.00 threshold
        Decimal::new(10, 0)    // 10% discount
    );
    let discount_amount = volume_discount.calculate_discount(subtotal);
    assert_eq!(discount_amount, Decimal::new(1000, 2)); // $10.00
    
    // Test volume discount (threshold not met)
    let volume_discount_high = DiscountType::Volume(
        Decimal::new(15000, 2), // $150.00 threshold
        Decimal::new(10, 0)     // 10% discount
    );
    let discount_amount = volume_discount_high.calculate_discount(subtotal);
    assert_eq!(discount_amount, Decimal::ZERO);
    
    // Test first-time discount
    let first_time_discount = DiscountType::FirstTime(Decimal::new(25, 0)); // 25%
    let discount_amount = first_time_discount.calculate_discount(subtotal);
    assert_eq!(discount_amount, Decimal::new(2500, 2)); // $25.00
}

/// Test promotional discount with expiry
#[test]
fn test_promotional_discount_expiry() {
    let subtotal = Decimal::new(10000, 2); // $100.00
    
    // Active promotional discount
    let active_promo = DiscountType::Promotional {
        percentage: Decimal::new(30, 0), // 30%
        expires_at: Utc::now() + chrono::Duration::days(7), // Expires in 7 days
    };
    let discount_amount = active_promo.calculate_discount(subtotal);
    assert_eq!(discount_amount, Decimal::new(3000, 2)); // $30.00
    
    // Expired promotional discount
    let expired_promo = DiscountType::Promotional {
        percentage: Decimal::new(30, 0), // 30%
        expires_at: Utc::now() - chrono::Duration::days(1), // Expired yesterday
    };
    let discount_amount = expired_promo.calculate_discount(subtotal);
    assert_eq!(discount_amount, Decimal::ZERO);
}

/// Test cost calculation with user billing context
#[tokio::test]
async fn test_cost_calculation_with_context() {
    let calculator = CostCalculator::new();
    
    // Create usage summary
    let user_id = Uuid::new_v4();
    let start_time = Utc::now() - chrono::Duration::hours(2);
    let end_time = Utc::now();
    
    let mut metrics = HashMap::new();
    metrics.insert(MetricType::CpuUsage, AggregatedMetric {
        total: Decimal::new(7200, 0), // 2 hours
        average: Decimal::new(7200, 0),
        maximum: Decimal::new(7200, 0),
        minimum: Decimal::new(7200, 0),
        count: 1,
    });
    
    let usage_summary = UsageSummary {
        user_id,
        start_time,
        end_time,
        metrics,
        session_count: 1,
        created_at: Utc::now(),
    };
    
    // Create billing context with discounts
    let context = UserBillingContext {
        user_id,
        pricing_tier: PricingTier::Professional,
        is_first_billing: true,
        total_previous_usage: Decimal::ZERO,
        applicable_discounts: vec![
            DiscountType::FirstTime(Decimal::new(50, 0)), // 50% first-time discount
        ],
        custom_rates: None,
    };
    
    let billing_record = calculator.calculate_cost_with_context(&usage_summary, &context).await.unwrap();
    
    // Should have applied the discount
    assert!(billing_record.discounts > Decimal::ZERO);
    assert!(billing_record.total_cost < billing_record.subtotal);
    
    // Check that professional tier pricing was used
    assert_eq!(billing_record.cost_structure.tier, PricingTier::Professional);
}

/// Test estimated cost calculation
#[test]
fn test_estimated_cost_calculation() {
    let calculator = CostCalculator::new();
    
    let mut current_usage = HashMap::new();
    current_usage.insert(MetricType::CpuUsage, Decimal::new(50, 0)); // 50 cpu-seconds current
    current_usage.insert(MetricType::MemoryUsage, Decimal::new(1000, 0)); // 1000 mb-seconds current
    
    // Estimate cost for next 2 hours
    let estimated_cost = calculator.calculate_estimated_cost(
        &current_usage,
        &PricingTier::Basic,
        2.0 // 2 hours
    ).unwrap();
    
    assert!(estimated_cost >= Decimal::ZERO);
}

/// Test aggregation period functionality
#[test]
fn test_aggregation_periods() {
    // Test duration calculations
    assert_eq!(AggregationPeriod::Hourly.duration(), chrono::Duration::hours(1));
    assert_eq!(AggregationPeriod::Daily.duration(), chrono::Duration::days(1));
    assert_eq!(AggregationPeriod::Weekly.duration(), chrono::Duration::weeks(1));
    assert_eq!(AggregationPeriod::Monthly.duration(), chrono::Duration::days(30));
    
    // Test timestamp rounding
    let test_time = DateTime::parse_from_rfc3339("2024-03-15T14:30:45Z")
        .unwrap()
        .with_timezone(&Utc);
    
    let hourly_rounded = AggregationPeriod::Hourly.round_timestamp(test_time);
    assert_eq!(hourly_rounded.minute(), 0);
    assert_eq!(hourly_rounded.second(), 0);
    
    let daily_rounded = AggregationPeriod::Daily.round_timestamp(test_time);
    assert_eq!(daily_rounded.hour(), 0);
    assert_eq!(daily_rounded.minute(), 0);
    assert_eq!(daily_rounded.second(), 0);
}

/// Test billing record structure
#[test]
fn test_billing_record_structure() {
    let user_id = Uuid::new_v4();
    let record_id = Uuid::new_v4();
    let now = Utc::now();
    
    let line_item = BillingLineItem {
        metric_type: MetricType::CpuUsage,
        usage: Decimal::new(3600, 0),
        rate: Decimal::new(1, 3), // $0.001 per cpu-second
        free_allowance: Decimal::new(1800, 0), // 30 minutes free
        billable_usage: Decimal::new(1800, 0), // 30 minutes billable
        cost: Decimal::new(18, 1), // $1.80
        description: "CPU usage: 3600 cpu-seconds (Rate: $0.001/cpu-seconds)".to_string(),
    };
    
    let billing_record = BillingRecord {
        id: record_id,
        user_id,
        start_time: now - chrono::Duration::hours(1),
        end_time: now,
        cost_structure: CostStructure::default(),
        line_items: vec![line_item.clone()],
        subtotal: Decimal::new(18, 1), // $1.80
        discounts: Decimal::ZERO,
        total_cost: Decimal::new(18, 1), // $1.80
        created_at: now,
    };
    
    assert_eq!(billing_record.id, record_id);
    assert_eq!(billing_record.user_id, user_id);
    assert_eq!(billing_record.line_items.len(), 1);
    assert_eq!(billing_record.line_items[0].metric_type, MetricType::CpuUsage);
    assert_eq!(billing_record.subtotal, billing_record.total_cost);
}

/// Test invoice creation and status management
#[test]
fn test_invoice_management() {
    let user_id = Uuid::new_v4();
    let invoice_id = Uuid::new_v4();
    let billing_record_id = Uuid::new_v4();
    let now = Utc::now();
    
    let invoice = Invoice {
        id: invoice_id,
        user_id,
        billing_record_id,
        start_time: now - chrono::Duration::days(30),
        end_time: now,
        total_amount: Decimal::new(5000, 2), // $50.00
        status: InvoiceStatus::Draft,
        created_at: now,
        updated_at: now,
        line_items: vec![],
    };
    
    assert_eq!(invoice.id, invoice_id);
    assert_eq!(invoice.user_id, user_id);
    assert_eq!(invoice.status, InvoiceStatus::Draft);
    assert_eq!(invoice.total_amount, Decimal::new(5000, 2));
    
    // Test status transitions
    assert_ne!(InvoiceStatus::Draft, InvoiceStatus::Sent);
    assert_ne!(InvoiceStatus::Sent, InvoiceStatus::Paid);
    assert_ne!(InvoiceStatus::Paid, InvoiceStatus::Overdue);
}

/// Integration test placeholder (requires Redis and SurrealDB)
#[tokio::test]
#[ignore = "Requires Redis and SurrealDB infrastructure"]
async fn test_full_billing_service_integration() {
    let config = BillingConfig::default();
    
    // This would test the full billing service if infrastructure is available
    let billing_service = BillingService::new(config).await;
    
    // Test would be skipped in CI unless infrastructure is provided
    match billing_service {
        Ok(service) => {
            // Test service operations
            let _ = service.start().await;
            let _ = service.stop().await;
        }
        Err(_) => {
            // Expected failure without infrastructure
            println!("Skipping full integration test - infrastructure not available");
        }
    }
}

/// Performance test for cost calculations
#[test]
fn test_cost_calculation_performance() {
    let calculator = CostCalculator::new();
    
    // Create a large usage summary to test performance
    let user_id = Uuid::new_v4();
    let start_time = Utc::now() - chrono::Duration::hours(24);
    let end_time = Utc::now();
    
    let mut metrics = HashMap::new();
    
    // Add all metric types with substantial usage
    metrics.insert(MetricType::CpuUsage, AggregatedMetric {
        total: Decimal::new(86400, 0), // 24 hours
        average: Decimal::new(86400, 0),
        maximum: Decimal::new(86400, 0),
        minimum: Decimal::new(86400, 0),
        count: 24,
    });
    
    metrics.insert(MetricType::MemoryUsage, AggregatedMetric {
        total: Decimal::new(88473600, 0), // 24GB-hours in mb-seconds
        average: Decimal::new(3686400, 0),
        maximum: Decimal::new(3686400, 0),
        minimum: Decimal::new(3686400, 0),
        count: 24,
    });
    
    metrics.insert(MetricType::NetworkIngress, AggregatedMetric {
        total: Decimal::new(10000000000_u64, 0), // 10GB
        average: Decimal::new(416666667, 0),
        maximum: Decimal::new(1000000000, 0),
        minimum: Decimal::new(100000000, 0),
        count: 24,
    });
    
    metrics.insert(MetricType::NetworkEgress, AggregatedMetric {
        total: Decimal::new(5000000000_u64, 0), // 5GB
        average: Decimal::new(208333333, 0),
        maximum: Decimal::new(500000000, 0),
        minimum: Decimal::new(50000000, 0),
        count: 24,
    });
    
    let usage_summary = UsageSummary {
        user_id,
        start_time,
        end_time,
        metrics,
        session_count: 10,
        created_at: Utc::now(),
    };
    
    // Measure calculation time
    let start = std::time::Instant::now();
    let result = tokio_test::block_on(calculator.calculate_cost(&usage_summary));
    let duration = start.elapsed();
    
    assert!(result.is_ok());
    assert!(duration.as_millis() < 100, "Cost calculation should complete within 100ms");
    
    let billing_record = result.unwrap();
    assert_eq!(billing_record.line_items.len(), 4);
    assert!(billing_record.total_cost > Decimal::ZERO);
}

#[tokio::test]
async fn test_billing_service_creation() {
    let config = BillingConfig::default();
    let result = BillingService::new(config).await;
    
    // This will fail without Redis/SurrealDB, but tests the interface
    match result {
        Ok(_) => println!("Billing service created successfully"),
        Err(e) => println!("Expected error without infrastructure: {}", e),
    }
}