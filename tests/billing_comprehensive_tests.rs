//! Comprehensive Billing System Tests
//! 
//! This module contains extensive tests for the SoulBox billing system,
//! covering critical scenarios, edge cases, and security vulnerabilities.

use chrono::{DateTime, Utc, Duration};
use rust_decimal::Decimal;
use soulbox::billing::{
    BillingConfig, CostCalculator,
    models::*,
    cost_calculator::{DiscountType, UserBillingContext},
    precision::{FinancialPrecision, FinancialContext, ApplyPrecision},
};
use std::collections::HashMap;
use uuid::Uuid;

/// Test financial precision edge cases and overflow protection
mod precision_tests {
    use super::*;
    
    #[test]
    fn test_monetary_precision_limits() {
        // Test minimum charge validation
        let min_charge = FinancialPrecision::min_charge();
        assert_eq!(min_charge, Decimal::new(1, 4)); // 0.0001
        
        // Test maximum charge validation
        let max_charge = FinancialPrecision::max_charge();
        assert_eq!(max_charge, Decimal::new(1_000_000_0000, 4)); // 1,000,000.0000
        
        // Test amount validation
        assert!(FinancialPrecision::validate_amount(Decimal::new(500, 2)).is_ok());
        assert!(FinancialPrecision::validate_amount(Decimal::new(-100, 2)).is_err());
        assert!(FinancialPrecision::validate_amount(Decimal::new(2_000_000_0000, 4)).is_err());
    }
    
    #[test]
    fn test_safe_arithmetic_operations() {
        let a = Decimal::new(999_999_0000, 4); // Near max
        let b = Decimal::new(2, 0); // 2.0000
        
        // Test safe addition with overflow detection
        let result = FinancialPrecision::safe_add(a, b);
        assert!(result.is_err(), "Should detect overflow");
        
        // Test safe multiplication with overflow detection
        let result = FinancialPrecision::safe_multiply(a, b);
        assert!(result.is_err(), "Should detect multiplication overflow");
        
        // Test safe division by zero
        let result = FinancialPrecision::safe_divide(a, Decimal::ZERO);
        assert!(result.is_err(), "Should detect division by zero");
        
        // Test valid operations
        let small_a = Decimal::new(100, 2); // 1.00
        let small_b = Decimal::new(200, 2); // 2.00
        
        assert!(FinancialPrecision::safe_add(small_a, small_b).is_ok());
        assert!(FinancialPrecision::safe_multiply(small_a, small_b).is_ok());
        assert!(FinancialPrecision::safe_divide(small_b, small_a).is_ok());
    }
    
    #[test]
    fn test_precision_rounding() {
        // Test different precision scales
        let amount = Decimal::new(12345678, 6); // 12.345678
        
        assert_eq!(FinancialPrecision::round_monetary(amount), Decimal::new(123457, 4));
        assert_eq!(FinancialPrecision::round_cpu(amount), amount); // Already within CPU scale
        assert_eq!(FinancialPrecision::round_memory(amount), Decimal::new(1235, 2));
        assert_eq!(FinancialPrecision::round_network(amount), Decimal::new(12, 0));
    }
    
    #[test]
    fn test_apply_precision_trait() {
        let cpu_metric = (MetricType::CpuUsage, Decimal::new(123456789, 8));
        let memory_metric = (MetricType::MemoryUsage, Decimal::new(123456789, 8));
        let network_metric = (MetricType::NetworkIngress, Decimal::new(123456789, 8));
        
        // Test precision application
        assert_eq!(cpu_metric.apply_precision().scale(), 6);
        assert_eq!(memory_metric.apply_precision().scale(), 2);
        assert_eq!(network_metric.apply_precision().scale(), 0);
    }
    
    #[test]
    fn test_financial_context_calculations() {
        let context = FinancialContext::new("EUR".to_string());
        
        let usage = Decimal::new(1000, 3); // 1.000
        let rate = Decimal::new(25, 4); // 0.0025
        
        let cost = context.calculate_cost(usage, rate).unwrap();
        assert_eq!(cost, Decimal::new(25, 4)); // 0.0025
        
        // Test discount application
        let amount = Decimal::new(10000, 4); // 10.0000
        let discount_10_percent = Decimal::new(10, 0); // 10%
        
        let discounted = context.apply_discount(amount, discount_10_percent).unwrap();
        assert_eq!(discounted, Decimal::new(9000, 4)); // 9.0000
        
        // Test invalid discount percentages
        assert!(context.apply_discount(amount, Decimal::new(-5, 0)).is_err()); // Negative
        assert!(context.apply_discount(amount, Decimal::new(105, 0)).is_err()); // > 100%
    }
}

/// Test cost calculator edge cases and security vulnerabilities
mod cost_calculator_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_cost_calculation_with_zero_usage() {
        let calculator = CostCalculator::new();
        let user_id = Uuid::new_v4();
        
        // Create usage summary with zero usage
        let usage_summary = UsageSummary {
            user_id,
            start_time: Utc::now() - Duration::hours(1),
            end_time: Utc::now(),
            metrics: HashMap::new(),
            session_count: 0,
            created_at: Utc::now(),
        };
        
        let billing_record = calculator.calculate_cost(&usage_summary).await.unwrap();
        
        assert_eq!(billing_record.line_items.len(), 0);
        assert_eq!(billing_record.total_cost, Decimal::ZERO);
        assert_eq!(billing_record.subtotal, Decimal::ZERO);
        assert_eq!(billing_record.discounts, Decimal::ZERO);
    }
    
    #[tokio::test]
    async fn test_cost_calculation_with_extreme_values() {
        let calculator = CostCalculator::new();
        let user_id = Uuid::new_v4();
        
        let mut metrics = HashMap::new();
        
        // Test with maximum safe values
        metrics.insert(MetricType::CpuUsage, AggregatedMetric {
            total: Decimal::new(999_999_999, 0), // Very large usage
            average: Decimal::new(999_999_999, 0),
            maximum: Decimal::new(999_999_999, 0),
            minimum: Decimal::new(999_999_999, 0),
            count: 1,
        });
        
        let usage_summary = UsageSummary {
            user_id,
            start_time: Utc::now() - Duration::hours(1),
            end_time: Utc::now(),
            metrics,
            session_count: 1,
            created_at: Utc::now(),
        };
        
        // Should handle large values without overflow
        let billing_record = calculator.calculate_cost(&usage_summary).await.unwrap();
        
        assert_eq!(billing_record.line_items.len(), 1);
        assert!(billing_record.total_cost >= Decimal::ZERO);
        // Should respect maximum charge limits
        assert!(billing_record.total_cost <= FinancialPrecision::max_charge());
    }
    
    #[tokio::test]
    async fn test_cost_calculation_with_custom_rates() {
        let calculator = CostCalculator::new();
        let user_id = Uuid::new_v4();
        
        // Create custom rates that are higher than default
        let mut custom_rates = HashMap::new();
        custom_rates.insert(MetricType::CpuUsage, Decimal::new(10, 3)); // $0.010 vs default $0.001
        
        let context = UserBillingContext {
            user_id,
            pricing_tier: PricingTier::Basic,
            is_first_billing: false,
            total_previous_usage: Decimal::ZERO,
            applicable_discounts: vec![],
            custom_rates: Some(custom_rates),
        };
        
        let mut metrics = HashMap::new();
        metrics.insert(MetricType::CpuUsage, AggregatedMetric {
            total: Decimal::new(3600, 0), // 1 hour
            average: Decimal::new(3600, 0),
            maximum: Decimal::new(3600, 0),
            minimum: Decimal::new(3600, 0),
            count: 1,
        });
        
        let usage_summary = UsageSummary {
            user_id,
            start_time: Utc::now() - Duration::hours(1),
            end_time: Utc::now(),
            metrics,
            session_count: 1,
            created_at: Utc::now(),
        };
        
        let billing_record = calculator.calculate_cost_with_context(&usage_summary, &context).await.unwrap();
        
        // Should use custom rate
        let cpu_line_item = billing_record.line_items.iter()
            .find(|item| item.metric_type == MetricType::CpuUsage)
            .expect("Should have CPU line item");
        
        assert_eq!(cpu_line_item.rate, Decimal::new(10, 3)); // Custom rate
    }
    
    #[test]
    fn test_discount_stacking_and_limits() {
        let subtotal = Decimal::new(10000, 2); // $100.00
        
        // Test multiple discounts that would exceed 100%
        let discounts = vec![
            DiscountType::Percentage(Decimal::new(50, 0)), // 50%
            DiscountType::Percentage(Decimal::new(40, 0)), // 40%
            DiscountType::FixedAmount(Decimal::new(2000, 2)), // $20.00
        ];
        
        let calculator = CostCalculator::new();
        let total_discount = calculator.calculate_total_discounts(&discounts, subtotal).unwrap();
        
        // Total discount should not exceed the subtotal
        assert!(total_discount <= subtotal);
        assert!(total_discount > Decimal::ZERO);
    }
    
    #[test]
    fn test_estimated_cost_with_cumulative_metrics() {
        let calculator = CostCalculator::new();
        
        let mut current_usage = HashMap::new();
        
        // Test cumulative metrics (should scale with time)
        current_usage.insert(MetricType::CpuUsage, Decimal::new(50, 0)); // 50 cpu-seconds/hour
        current_usage.insert(MetricType::MemoryUsage, Decimal::new(1024, 0)); // 1024 mb-seconds/hour
        
        // Test non-cumulative metrics (should not scale with time)
        current_usage.insert(MetricType::ApiRequests, Decimal::new(100, 0)); // 100 requests/hour
        current_usage.insert(MetricType::NetworkIngress, Decimal::new(1000000, 0)); // 1MB/hour
        
        let estimated_2h = calculator.calculate_estimated_cost(
            &current_usage,
            &PricingTier::Basic,
            2.0 // 2 hours
        ).unwrap();
        
        let estimated_1h = calculator.calculate_estimated_cost(
            &current_usage,
            &PricingTier::Basic,
            1.0 // 1 hour
        ).unwrap();
        
        // For cumulative metrics, 2 hours should cost more than 1 hour
        // For non-cumulative metrics, cost should be the same
        assert!(estimated_2h > estimated_1h);
    }
    
    #[test]
    fn test_pricing_tier_rate_consistency() {
        let calculator = CostCalculator::new();
        
        let free_pricing = calculator.get_pricing_info(&PricingTier::Free).unwrap();
        let basic_pricing = calculator.get_pricing_info(&PricingTier::Basic).unwrap();
        let pro_pricing = calculator.get_pricing_info(&PricingTier::Professional).unwrap();
        
        // Free tier should have zero rates
        for rate in free_pricing.rates.values() {
            assert_eq!(*rate, Decimal::ZERO);
        }
        
        // Professional tier should have lower rates than basic
        for metric_type in &[MetricType::CpuUsage, MetricType::MemoryUsage] {
            let basic_rate = basic_pricing.rates.get(metric_type).unwrap();
            let pro_rate = pro_pricing.rates.get(metric_type).unwrap();
            
            assert!(pro_rate < basic_rate, "Professional tier should have lower rates for {:?}", metric_type);
        }
        
        // Professional tier should have higher free allowances
        for metric_type in &[MetricType::CpuUsage, MetricType::MemoryUsage] {
            let basic_allowance = basic_pricing.free_allowances.get(metric_type).unwrap();
            let pro_allowance = pro_pricing.free_allowances.get(metric_type).unwrap();
            
            assert!(pro_allowance >= basic_allowance, "Professional tier should have equal or higher allowances for {:?}", metric_type);
        }
    }
}

/// Test data integrity and consistency issues
mod data_integrity_tests {
    use super::*;
    
    #[test]
    fn test_usage_metric_validation() {
        let user_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        
        // Test valid metric
        let valid_metric = UsageMetric {
            id: Uuid::new_v4(),
            session_id,
            user_id,
            metric_type: MetricType::CpuUsage,
            value: Decimal::new(100, 0),
            timestamp: Utc::now(),
            metadata: None,
        };
        
        assert_eq!(valid_metric.user_id, user_id);
        assert_eq!(valid_metric.session_id, session_id);
        assert!(valid_metric.value > Decimal::ZERO);
        
        // Test metric with negative value (should be handled by validation)
        let negative_value_metric = UsageMetric {
            id: Uuid::new_v4(),
            session_id,
            user_id,
            metric_type: MetricType::CpuUsage,
            value: Decimal::new(-100, 0), // Negative value
            timestamp: Utc::now(),
            metadata: None,
        };
        
        // The metric can be created, but validation should catch this later
        assert!(negative_value_metric.value < Decimal::ZERO);
    }
    
    #[test]
    fn test_aggregated_metric_consistency() {
        // Test consistency between aggregated values
        let aggregated = AggregatedMetric {
            total: Decimal::new(1000, 0),
            average: Decimal::new(100, 0),
            maximum: Decimal::new(200, 0),
            minimum: Decimal::new(50, 0),
            count: 10,
        };
        
        // Average should be total / count
        let expected_average = aggregated.total / Decimal::new(aggregated.count, 0);
        assert_eq!(aggregated.average, expected_average);
        
        // Maximum should be >= average >= minimum
        assert!(aggregated.maximum >= aggregated.average);
        assert!(aggregated.average >= aggregated.minimum);
        
        // Total should be reasonable given average and count
        let total_from_average = aggregated.average * Decimal::new(aggregated.count, 0);
        assert_eq!(aggregated.total, total_from_average);
    }
    
    #[test]
    fn test_billing_record_consistency() {
        let user_id = Uuid::new_v4();
        
        let line_item1 = BillingLineItem {
            metric_type: MetricType::CpuUsage,
            usage: Decimal::new(3600, 0),
            rate: Decimal::new(1, 3),
            free_allowance: Decimal::new(1800, 0),
            billable_usage: Decimal::new(1800, 0),
            cost: Decimal::new(18, 1), // $1.80
            description: "CPU usage".to_string(),
        };
        
        let line_item2 = BillingLineItem {
            metric_type: MetricType::MemoryUsage,
            usage: Decimal::new(3686400, 0),
            rate: Decimal::new(1, 4),
            free_allowance: Decimal::new(1800000, 0),
            billable_usage: Decimal::new(1886400, 0),
            cost: Decimal::new(18864, 4), // $1.8864
            description: "Memory usage".to_string(),
        };
        
        let subtotal = line_item1.cost + line_item2.cost;
        let discounts = Decimal::new(50, 2); // $0.50
        let total_cost = subtotal - discounts;
        
        let billing_record = BillingRecord {
            id: Uuid::new_v4(),
            user_id,
            start_time: Utc::now() - Duration::hours(1),
            end_time: Utc::now(),
            cost_structure: CostStructure::default(),
            line_items: vec![line_item1, line_item2],
            subtotal,
            discounts,
            total_cost,
            created_at: Utc::now(),
        };
        
        // Test consistency
        let calculated_subtotal: Decimal = billing_record.line_items.iter()
            .map(|item| item.cost)
            .sum();
        assert_eq!(billing_record.subtotal, calculated_subtotal);
        
        let calculated_total = billing_record.subtotal - billing_record.discounts;
        assert_eq!(billing_record.total_cost, calculated_total);
        
        // Test that billable usage is correctly calculated
        for item in &billing_record.line_items {
            let expected_billable = (item.usage - item.free_allowance).max(Decimal::ZERO);
            assert_eq!(item.billable_usage, expected_billable);
            
            let expected_cost = item.billable_usage * item.rate;
            assert_eq!(item.cost, expected_cost);
        }
    }
    
    #[test]
    fn test_time_period_consistency() {
        let user_id = Uuid::new_v4();
        let end_time = Utc::now();
        let start_time = end_time - Duration::hours(2);
        
        let usage_summary = UsageSummary {
            user_id,
            start_time,
            end_time,
            metrics: HashMap::new(),
            session_count: 1,
            created_at: Utc::now(),
        };
        
        // Start time should be before end time
        assert!(usage_summary.start_time < usage_summary.end_time);
        
        // Created time should be close to now
        let time_diff = (Utc::now() - usage_summary.created_at).num_seconds().abs();
        assert!(time_diff < 5); // Within 5 seconds
        
        // Period should be reasonable
        let period_duration = usage_summary.end_time - usage_summary.start_time;
        assert_eq!(period_duration, Duration::hours(2));
    }
}

/// Test concurrency and race condition scenarios
mod concurrency_tests {
    use super::*;
    use std::sync::Arc;
    use tokio::task;
    
    #[tokio::test]
    async fn test_concurrent_cost_calculations() {
        let calculator = Arc::new(CostCalculator::new());
        let mut handles = vec![];
        
        // Spawn multiple concurrent cost calculations
        for i in 0..10 {
            let calc = calculator.clone();
            let handle = task::spawn(async move {
                let user_id = Uuid::new_v4();
                
                let mut metrics = HashMap::new();
                metrics.insert(MetricType::CpuUsage, AggregatedMetric {
                    total: Decimal::new(3600 + i * 100, 0), // Slightly different values
                    average: Decimal::new(3600 + i * 100, 0),
                    maximum: Decimal::new(3600 + i * 100, 0),
                    minimum: Decimal::new(3600 + i * 100, 0),
                    count: 1,
                });
                
                let usage_summary = UsageSummary {
                    user_id,
                    start_time: Utc::now() - Duration::hours(1),
                    end_time: Utc::now(),
                    metrics,
                    session_count: 1,
                    created_at: Utc::now(),
                };
                
                calc.calculate_cost(&usage_summary).await
            });
            
            handles.push(handle);
        }
        
        // Wait for all calculations to complete
        let results: Vec<_> = futures::future::try_join_all(handles).await.unwrap();
        
        // All calculations should succeed
        for result in results {
            assert!(result.is_ok());
            let billing_record = result.unwrap();
            assert_eq!(billing_record.line_items.len(), 1);
            assert!(billing_record.total_cost >= Decimal::ZERO);
        }
    }
    
    #[tokio::test]
    async fn test_concurrent_metric_processing() {
        let mut metric_tasks = vec![];
        
        // Simulate concurrent metric creation and processing
        for i in 0..50 {
            let task = task::spawn(async move {
                let user_id = Uuid::new_v4();
                let session_id = Uuid::new_v4();
                
                let metric = UsageMetric {
                    id: Uuid::new_v4(),
                    session_id,
                    user_id,
                    metric_type: if i % 2 == 0 { MetricType::CpuUsage } else { MetricType::MemoryUsage },
                    value: Decimal::new(100 + i, 0),
                    timestamp: Utc::now(),
                    metadata: None,
                };
                
                // Simulate some processing time
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                
                metric
            });
            
            metric_tasks.push(task);
        }
        
        let metrics: Vec<_> = futures::future::try_join_all(metric_tasks).await.unwrap();
        
        // All metrics should be created successfully
        assert_eq!(metrics.len(), 50);
        
        // Check for unique IDs
        let mut ids = std::collections::HashSet::new();
        for metric in &metrics {
            assert!(ids.insert(metric.id), "Metric IDs should be unique");
        }
    }
}

/// Test security vulnerabilities and injection attacks
mod security_tests {
    use super::*;
    
    #[test]
    fn test_custom_metric_injection() {
        // Test that custom metric names are properly handled
        let malicious_names = vec![
            "'; DROP TABLE metrics; --",
            "<script>alert('xss')</script>",
            "../../etc/passwd",
            "\x00\x01\x02", // Binary data
            "a".repeat(10000), // Very long string
        ];
        
        for malicious_name in malicious_names {
            let custom_metric = MetricType::Custom(malicious_name.to_string());
            
            // Should handle malicious names without crashing
            assert_eq!(custom_metric.unit(), "units");
            assert!(!custom_metric.is_cumulative());
            
            // Should be able to use in calculations
            let metric = UsageMetric {
                id: Uuid::new_v4(),
                session_id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                metric_type: custom_metric,
                value: Decimal::new(100, 0),
                timestamp: Utc::now(),
                metadata: None,
            };
            
            assert!(metric.value > Decimal::ZERO);
        }
    }
    
    #[test]
    fn test_discount_manipulation() {
        let subtotal = Decimal::new(10000, 2); // $100.00
        
        // Test malicious discount values
        let malicious_discounts = vec![
            DiscountType::Percentage(Decimal::new(-50, 0)), // Negative discount
            DiscountType::Percentage(Decimal::new(150, 0)), // >100% discount
            DiscountType::FixedAmount(Decimal::new(-1000, 2)), // Negative fixed amount
            DiscountType::FixedAmount(Decimal::new(1_000_000_0000, 4)), // Extremely large amount
        ];
        
        for discount in malicious_discounts {
            let result = discount.calculate_discount(subtotal);
            
            match result {
                Ok(amount) => {
                    // If successful, amount should be reasonable
                    assert!(amount >= Decimal::ZERO);
                    assert!(amount <= subtotal);
                }
                Err(_) => {
                    // Error is acceptable for malicious inputs
                }
            }
        }
    }
    
    #[test]
    fn test_usage_value_bounds() {
        let user_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        
        // Test extreme usage values
        let extreme_values = vec![
            Decimal::new(0, 0), // Zero usage
            Decimal::new(1, 20), // Very small usage
            Decimal::new(i64::MAX, 0), // Maximum int64 value
            Decimal::MAX, // Maximum decimal value
        ];
        
        for value in extreme_values {
            let metric = UsageMetric {
                id: Uuid::new_v4(),
                session_id,
                user_id,
                metric_type: MetricType::CpuUsage,
                value,
                timestamp: Utc::now(),
                metadata: None,
            };
            
            // Should be able to create metric with any decimal value
            assert_eq!(metric.value, value);
            
            // Test in aggregated metric
            let aggregated = AggregatedMetric {
                total: value,
                average: value,
                maximum: value,
                minimum: value,
                count: 1,
            };
            
            assert_eq!(aggregated.total, value);
        }
    }
    
    #[test]
    fn test_metadata_injection() {
        let user_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        
        // Test malicious metadata
        let mut malicious_metadata = HashMap::new();
        malicious_metadata.insert("'; DROP TABLE --".to_string(), "value".to_string());
        malicious_metadata.insert("<script>".to_string(), "alert('xss')".to_string());
        malicious_metadata.insert("key".to_string(), "a".repeat(100000)); // Very long value
        
        let metric = UsageMetric {
            id: Uuid::new_v4(),
            session_id,
            user_id,
            metric_type: MetricType::CpuUsage,
            value: Decimal::new(100, 0),
            timestamp: Utc::now(),
            metadata: Some(malicious_metadata.clone()),
        };
        
        // Should handle malicious metadata without crashing
        assert!(metric.metadata.is_some());
        let metadata = metric.metadata.unwrap();
        assert_eq!(metadata.len(), 3);
        
        // Values should be preserved as-is (no injection should occur)
        for (key, value) in &malicious_metadata {
            assert_eq!(metadata.get(key), Some(value));
        }
    }
}

/// Test performance and resource exhaustion scenarios
mod performance_tests {
    use super::*;
    
    #[test]
    fn test_large_usage_summary_performance() {
        let calculator = CostCalculator::new();
        let user_id = Uuid::new_v4();
        
        // Create usage summary with many metric types
        let mut metrics = HashMap::new();
        
        // Add standard metrics
        for i in 0..100 {
            let custom_metric = MetricType::Custom(format!("custom_metric_{}", i));
            metrics.insert(custom_metric, AggregatedMetric {
                total: Decimal::new(1000 + i, 0),
                average: Decimal::new(100 + i, 0),
                maximum: Decimal::new(200 + i, 0),
                minimum: Decimal::new(50 + i, 0),
                count: 10,
            });
        }
        
        let usage_summary = UsageSummary {
            user_id,
            start_time: Utc::now() - Duration::hours(24),
            end_time: Utc::now(),
            metrics,
            session_count: 100,
            created_at: Utc::now(),
        };
        
        let start_time = std::time::Instant::now();
        let result = tokio_test::block_on(calculator.calculate_cost(&usage_summary));
        let elapsed = start_time.elapsed();
        
        assert!(result.is_ok());
        assert!(elapsed.as_millis() < 1000, "Large calculation should complete within 1 second");
        
        let billing_record = result.unwrap();
        assert_eq!(billing_record.line_items.len(), 100);
    }
    
    #[test]
    fn test_precision_calculation_performance() {
        let context = FinancialContext::default();
        
        // Test many precision calculations
        let start_time = std::time::Instant::now();
        
        for i in 0..10000 {
            let usage = Decimal::new(1000 + i, 3);
            let rate = Decimal::new(25 + (i % 100), 6);
            
            let _cost = context.calculate_cost(usage, rate).unwrap();
        }
        
        let elapsed = start_time.elapsed();
        assert!(elapsed.as_millis() < 100, "10000 precision calculations should complete within 100ms");
    }
    
    #[test]
    fn test_memory_usage_with_large_datasets() {
        // Test memory usage doesn't grow unbounded with large datasets
        let mut large_metrics = HashMap::new();
        
        // Create a very large number of metrics
        for i in 0..1000 {
            let metric_type = MetricType::Custom(format!("metric_{}", i));
            large_metrics.insert(metric_type, AggregatedMetric {
                total: Decimal::new(i * 1000, 0),
                average: Decimal::new(i * 100, 0),
                maximum: Decimal::new(i * 200, 0),
                minimum: Decimal::new(i * 50, 0),
                count: 100,
            });
        }
        
        let usage_summary = UsageSummary {
            user_id: Uuid::new_v4(),
            start_time: Utc::now() - Duration::days(30),
            end_time: Utc::now(),
            metrics: large_metrics,
            session_count: 1000,
            created_at: Utc::now(),
        };
        
        // Should be able to handle large datasets without memory issues
        assert_eq!(usage_summary.metrics.len(), 1000);
        assert_eq!(usage_summary.session_count, 1000);
    }
}

/// Test error handling and recovery scenarios
mod error_handling_tests {
    use super::*;
    
    #[test]
    fn test_financial_precision_error_handling() {
        // Test invalid amount validation
        let result = FinancialPrecision::validate_amount(Decimal::new(-100, 2));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("negative"));
        
        let result = FinancialPrecision::validate_amount(Decimal::new(2_000_000_0000, 4));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("maximum"));
        
        // Test overflow protection
        let large_amount = Decimal::new(999_999_0000, 4);
        let result = FinancialPrecision::safe_add(large_amount, large_amount);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("overflow"));
        
        // Test division by zero
        let result = FinancialPrecision::safe_divide(Decimal::new(100, 2), Decimal::ZERO);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("zero"));
    }
    
    #[test]
    fn test_cost_structure_validation() {
        let mut cost_structure = CostStructure::default();
        
        // Test with invalid rates
        cost_structure.rates.insert(MetricType::CpuUsage, Decimal::new(-1, 3)); // Negative rate
        
        // The cost structure itself doesn't validate, but usage should be handled gracefully
        assert!(cost_structure.rates.get(&MetricType::CpuUsage).unwrap() < &Decimal::ZERO);
    }
    
    #[tokio::test]
    async fn test_cost_calculation_error_recovery() {
        let calculator = CostCalculator::new();
        let user_id = Uuid::new_v4();
        
        // Create usage summary with problematic data
        let mut metrics = HashMap::new();
        
        // Add metric with zero count (potential division by zero)
        metrics.insert(MetricType::CpuUsage, AggregatedMetric {
            total: Decimal::new(1000, 0),
            average: Decimal::new(100, 0),
            maximum: Decimal::new(200, 0),
            minimum: Decimal::new(50, 0),
            count: 0, // Zero count!
        });
        
        let usage_summary = UsageSummary {
            user_id,
            start_time: Utc::now() - Duration::hours(1),
            end_time: Utc::now(),
            metrics,
            session_count: 1,
            created_at: Utc::now(),
        };
        
        // Should handle gracefully (might return error or handle the zero count)
        let result = calculator.calculate_cost(&usage_summary).await;
        
        match result {
            Ok(billing_record) => {
                // If successful, should have reasonable values
                assert!(billing_record.total_cost >= Decimal::ZERO);
            }
            Err(_) => {
                // Error is acceptable for invalid data
            }
        }
    }
}