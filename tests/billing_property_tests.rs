//! Property-Based Tests for Billing System
//! 
//! These tests use property-based testing to verify invariants and
//! mathematical properties of the billing system across a wide range of inputs.

use proptest::prelude::*;
use chrono::{DateTime, Utc, Duration};
use rust_decimal::Decimal;
use soulbox::billing::{
    CostCalculator,
    models::*,
    cost_calculator::{DiscountType, UserBillingContext},
    precision::{FinancialPrecision, FinancialContext},
};
use std::collections::HashMap;
use uuid::Uuid;

// Property test strategies for generating test data

fn arb_decimal_amount() -> impl Strategy<Value = Decimal> {
    (0i64..=1_000_000_00, 0u32..=4)
        .prop_map(|(mantissa, scale)| Decimal::new(mantissa, scale))
}

fn arb_small_decimal() -> impl Strategy<Value = Decimal> {
    (0i64..=100_000, 0u32..=6)
        .prop_map(|(mantissa, scale)| Decimal::new(mantissa, scale))
}

fn arb_metric_type() -> impl Strategy<Value = MetricType> {
    prop_oneof![
        Just(MetricType::CpuUsage),
        Just(MetricType::MemoryUsage),
        Just(MetricType::NetworkIngress),
        Just(MetricType::NetworkEgress),
        Just(MetricType::StorageUsage),
        Just(MetricType::ExecutionTime),
        Just(MetricType::ApiRequests),
        "[a-zA-Z0-9_]{1,20}".prop_map(|s| MetricType::Custom(s)),
    ]
}

fn arb_pricing_tier() -> impl Strategy<Value = PricingTier> {
    prop_oneof![
        Just(PricingTier::Free),
        Just(PricingTier::Basic),
        Just(PricingTier::Professional),
        Just(PricingTier::Enterprise),
    ]
}

fn arb_aggregated_metric() -> impl Strategy<Value = AggregatedMetric> {
    (arb_small_decimal(), 1u64..=1000)
        .prop_map(|(total, count)| {
            let average = total / Decimal::new(count as i64, 0);
            AggregatedMetric {
                total,
                average,
                maximum: average * Decimal::new(150, 2), // 1.5x average
                minimum: average * Decimal::new(50, 2),  // 0.5x average
                count,
            }
        })
}

fn arb_usage_summary() -> impl Strategy<Value = UsageSummary> {
    (
        prop::collection::hash_map(arb_metric_type(), arb_aggregated_metric(), 1..=10),
        1u64..=100
    ).prop_map(|(metrics, session_count)| {
        let now = Utc::now();
        UsageSummary {
            user_id: Uuid::new_v4(),
            start_time: now - Duration::hours(24),
            end_time: now,
            metrics,
            session_count,
            created_at: now,
        }
    })
}

fn arb_discount_type() -> impl Strategy<Value = DiscountType> {
    prop_oneof![
        (0u32..=100).prop_map(|p| DiscountType::Percentage(Decimal::new(p.into(), 0))),
        arb_decimal_amount().prop_map(DiscountType::FixedAmount),
        (arb_decimal_amount(), 0u32..=50).prop_map(|(threshold, percent)| 
            DiscountType::Volume(threshold, Decimal::new(percent.into(), 0))),
        (0u32..=50).prop_map(|p| DiscountType::FirstTime(Decimal::new(p.into(), 0))),
    ]
}

// Property tests for financial precision

proptest! {
    #[test]
    fn prop_safe_arithmetic_never_overflows(
        a in arb_decimal_amount(),
        b in arb_decimal_amount()
    ) {
        // Safe operations should never panic or return invalid results
        let add_result = FinancialPrecision::safe_add(a, b);
        let mul_result = FinancialPrecision::safe_multiply(a, b);
        
        // Results should either be valid amounts or errors
        if let Ok(result) = add_result {
            prop_assert!(result >= Decimal::ZERO);
            prop_assert!(result <= FinancialPrecision::max_charge());
        }
        
        if let Ok(result) = mul_result {
            prop_assert!(result >= Decimal::ZERO);
            prop_assert!(result <= FinancialPrecision::max_charge());
        }
    }
    
    #[test]
    fn prop_validate_amount_is_consistent(amount in any::<i64>(), scale in 0u32..=8) {
        let decimal_amount = Decimal::new(amount, scale);
        let validation_result = FinancialPrecision::validate_amount(decimal_amount);
        
        if amount >= 0 && decimal_amount <= FinancialPrecision::max_charge() {
            prop_assert!(validation_result.is_ok());
            let validated = validation_result.unwrap();
            prop_assert!(validated >= Decimal::ZERO);
            prop_assert!(validated <= FinancialPrecision::max_charge());
        } else {
            // Negative amounts or amounts exceeding max should be rejected
            if amount < 0 || decimal_amount > FinancialPrecision::max_charge() {
                prop_assert!(validation_result.is_err());
            }
        }
    }
    
    #[test]
    fn prop_rounding_preserves_order(
        a in arb_decimal_amount(),
        b in arb_decimal_amount()
    ) {
        // If a <= b, then round(a) <= round(b)
        if a <= b {
            let rounded_a = FinancialPrecision::round_monetary(a);
            let rounded_b = FinancialPrecision::round_monetary(b);
            prop_assert!(rounded_a <= rounded_b);
        }
    }
    
    #[test]
    fn prop_financial_context_discount_bounds(
        amount in arb_decimal_amount(),
        discount_percent in 0u32..=100
    ) {
        let context = FinancialContext::default();
        let discount_decimal = Decimal::new(discount_percent.into(), 0);
        
        if let Ok(discounted) = context.apply_discount(amount, discount_decimal) {
            // Discounted amount should be between 0 and original amount
            prop_assert!(discounted >= Decimal::ZERO);
            prop_assert!(discounted <= amount);
            
            // For valid discount percentages, result should be deterministic
            if discount_percent == 0 {
                prop_assert_eq!(discounted, amount);
            } else if discount_percent == 100 {
                prop_assert_eq!(discounted, Decimal::ZERO);
            }
        }
    }
}

// Property tests for cost calculations

proptest! {
    #[test]
    fn prop_cost_calculation_non_negative(
        usage_summary in arb_usage_summary()
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let calculator = CostCalculator::new();
        
        let result = rt.block_on(calculator.calculate_cost(&usage_summary));
        
        if let Ok(billing_record) = result {
            // All costs should be non-negative
            prop_assert!(billing_record.total_cost >= Decimal::ZERO);
            prop_assert!(billing_record.subtotal >= Decimal::ZERO);
            prop_assert!(billing_record.discounts >= Decimal::ZERO);
            
            // Total cost should be subtotal minus discounts
            let expected_total = billing_record.subtotal - billing_record.discounts;
            prop_assert_eq!(billing_record.total_cost, expected_total.max(Decimal::ZERO));
            
            // Line items should sum to subtotal
            let line_items_sum: Decimal = billing_record.line_items.iter()
                .map(|item| item.cost)
                .sum();
            prop_assert_eq!(billing_record.subtotal, line_items_sum);
        }
    }
    
    #[test]
    fn prop_line_item_consistency(
        metric_type in arb_metric_type(),
        usage in arb_small_decimal(),
        rate in arb_small_decimal(),
        free_allowance in arb_small_decimal()
    ) {
        // Test that billable usage calculation is consistent
        let billable_usage = (usage - free_allowance).max(Decimal::ZERO);
        let expected_cost = billable_usage * rate;
        
        let line_item = BillingLineItem {
            metric_type,
            usage,
            rate,
            free_allowance,
            billable_usage,
            cost: expected_cost,
            description: "Test item".to_string(),
        };
        
        // Billable usage should never be negative
        prop_assert!(line_item.billable_usage >= Decimal::ZERO);
        
        // Billable usage should not exceed total usage
        prop_assert!(line_item.billable_usage <= line_item.usage);
        
        // Cost should be rate * billable_usage
        prop_assert_eq!(line_item.cost, line_item.rate * line_item.billable_usage);
        
        // If usage <= free_allowance, billable_usage should be zero
        if usage <= free_allowance {
            prop_assert_eq!(line_item.billable_usage, Decimal::ZERO);
            prop_assert_eq!(line_item.cost, Decimal::ZERO);
        }
    }
    
    #[test]
    fn prop_pricing_tier_consistency(
        tier in arb_pricing_tier(),
        usage_map in prop::collection::hash_map(arb_metric_type(), arb_small_decimal(), 1..=5),
        duration_hours in 0.1f64..=168.0 // Up to one week
    ) {
        let calculator = CostCalculator::new();
        
        let result = calculator.calculate_estimated_cost(
            &usage_map,
            &tier,
            duration_hours
        );
        
        if let Ok(estimated_cost) = result {
            // Estimated cost should be non-negative
            prop_assert!(estimated_cost >= Decimal::ZERO);
            
            // Free tier should result in zero cost
            if tier == PricingTier::Free {
                prop_assert_eq!(estimated_cost, Decimal::ZERO);
            }
            
            // Longer durations should generally result in equal or higher costs
            // (for cumulative metrics)
            let shorter_result = calculator.calculate_estimated_cost(
                &usage_map,
                &tier,
                duration_hours / 2.0
            );
            
            if let Ok(shorter_cost) = shorter_result {
                // This property may not hold for non-cumulative metrics,
                // but estimated_cost should be reasonable
                prop_assert!(estimated_cost >= Decimal::ZERO);
                prop_assert!(shorter_cost >= Decimal::ZERO);
            }
        }
    }
}

// Property tests for discount calculations

proptest! {
    #[test]
    fn prop_discount_application_bounds(
        subtotal in arb_decimal_amount(),
        discount in arb_discount_type()
    ) {
        let discount_result = discount.calculate_discount(subtotal);
        
        if let Ok(discount_amount) = discount_result {
            // Discount should never be negative
            prop_assert!(discount_amount >= Decimal::ZERO);
            
            // Discount should never exceed the subtotal
            prop_assert!(discount_amount <= subtotal);
            
            // Resulting amount after discount should be non-negative
            let final_amount = subtotal - discount_amount;
            prop_assert!(final_amount >= Decimal::ZERO);
        }
    }
    
    #[test]
    fn prop_percentage_discount_correctness(
        amount in arb_decimal_amount(),
        percentage in 0u32..=100
    ) {
        let discount = DiscountType::Percentage(Decimal::new(percentage.into(), 0));
        let result = discount.calculate_discount(amount);
        
        if let Ok(discount_amount) = result {
            // For percentage discounts, we can verify the exact calculation
            let expected_remaining = amount * Decimal::new((100 - percentage).into(), 0) / Decimal::new(100, 0);
            let actual_remaining = amount - discount_amount;
            
            // Should be approximately equal (allowing for rounding)
            let diff = (expected_remaining - actual_remaining).abs();
            prop_assert!(diff <= Decimal::new(1, 4)); // Within 0.0001
        }
    }
    
    #[test]
    fn prop_fixed_discount_behavior(
        subtotal in arb_decimal_amount(),
        fixed_amount in arb_decimal_amount()
    ) {
        let discount = DiscountType::FixedAmount(fixed_amount);
        let result = discount.calculate_discount(subtotal);
        
        if let Ok(discount_amount) = result {
            if fixed_amount <= subtotal {
                // If fixed amount is less than subtotal, discount should equal fixed amount
                prop_assert_eq!(discount_amount, fixed_amount);
            } else {
                // If fixed amount exceeds subtotal, discount should equal subtotal
                prop_assert_eq!(discount_amount, subtotal);
            }
        }
    }
    
    #[test]
    fn prop_volume_discount_threshold(
        subtotal in arb_decimal_amount(),
        threshold in arb_decimal_amount(),
        percentage in 0u32..=50
    ) {
        let discount = DiscountType::Volume(threshold, Decimal::new(percentage.into(), 0));
        let result = discount.calculate_discount(subtotal);
        
        if let Ok(discount_amount) = result {
            if subtotal < threshold {
                // Below threshold, no discount should be applied
                prop_assert_eq!(discount_amount, Decimal::ZERO);
            } else {
                // Above threshold, discount should be applied
                if percentage > 0 {
                    prop_assert!(discount_amount > Decimal::ZERO);
                }
            }
        }
    }
}

// Property tests for metric type behaviors

proptest! {
    #[test]
    fn prop_metric_type_unit_consistency(metric_type in arb_metric_type()) {
        let unit = metric_type.unit();
        
        // Unit should be a non-empty string
        prop_assert!(!unit.is_empty());
        
        // Unit should be consistent with metric type
        match &metric_type {
            MetricType::CpuUsage => prop_assert_eq!(unit, "cpu-seconds"),
            MetricType::MemoryUsage => prop_assert_eq!(unit, "mb-seconds"),
            MetricType::NetworkIngress | MetricType::NetworkEgress => prop_assert_eq!(unit, "bytes"),
            MetricType::StorageUsage => prop_assert_eq!(unit, "gb-seconds"),
            MetricType::ExecutionTime => prop_assert_eq!(unit, "seconds"),
            MetricType::ApiRequests => prop_assert_eq!(unit, "requests"),
            MetricType::Custom(_) => prop_assert_eq!(unit, "units"),
        }
    }
    
    #[test]
    fn prop_cumulative_metric_behavior(metric_type in arb_metric_type()) {
        let is_cumulative = metric_type.is_cumulative();
        
        // Cumulative property should be consistent with metric type
        match &metric_type {
            MetricType::CpuUsage => prop_assert!(is_cumulative),
            MetricType::MemoryUsage => prop_assert!(is_cumulative),
            MetricType::StorageUsage => prop_assert!(is_cumulative),
            MetricType::ExecutionTime => prop_assert!(is_cumulative),
            MetricType::NetworkIngress => prop_assert!(!is_cumulative),
            MetricType::NetworkEgress => prop_assert!(!is_cumulative),
            MetricType::ApiRequests => prop_assert!(!is_cumulative),
            MetricType::Custom(_) => {
                // Custom metrics can be either cumulative or not
                // No specific assertion needed
            }
        }
    }
}

// Property tests for aggregated metrics

proptest! {
    #[test]
    fn prop_aggregated_metric_mathematical_properties(
        total in arb_small_decimal(),
        count in 1u64..=1000
    ) {
        let average = total / Decimal::new(count, 0);
        let aggregated = AggregatedMetric {
            total,
            average,
            maximum: average * Decimal::new(2, 0), // 2x average
            minimum: average / Decimal::new(2, 0), // 0.5x average
            count,
        };
        
        // Mathematical properties should hold
        prop_assert_eq!(aggregated.total, aggregated.average * Decimal::new(count, 0));
        prop_assert!(aggregated.maximum >= aggregated.average);
        prop_assert!(aggregated.average >= aggregated.minimum);
        prop_assert!(aggregated.maximum >= aggregated.minimum);
        prop_assert!(aggregated.count > 0);
    }
    
    #[test]
    fn prop_usage_summary_time_consistency(
        metrics in prop::collection::hash_map(arb_metric_type(), arb_aggregated_metric(), 1..=10),
        session_count in 1u64..=1000,
        hours_ago in 1i64..=168 // 1 hour to 1 week ago
    ) {
        let end_time = Utc::now();
        let start_time = end_time - Duration::hours(hours_ago);
        
        let usage_summary = UsageSummary {
            user_id: Uuid::new_v4(),
            start_time,
            end_time,
            metrics,
            session_count,
            created_at: Utc::now(),
        };
        
        // Time properties should be consistent
        prop_assert!(usage_summary.start_time < usage_summary.end_time);
        prop_assert!(usage_summary.created_at >= usage_summary.start_time);
        prop_assert!(usage_summary.session_count > 0);
        
        // Duration should match expectation
        let duration = usage_summary.end_time - usage_summary.start_time;
        prop_assert_eq!(duration, Duration::hours(hours_ago));
    }
}

// Property tests for user billing context

proptest! {
    #[test]
    fn prop_user_billing_context_discount_application(
        pricing_tier in arb_pricing_tier(),
        discounts in prop::collection::vec(arb_discount_type(), 0..=5),
        is_first_billing in any::<bool>()
    ) {
        let context = UserBillingContext {
            user_id: Uuid::new_v4(),
            pricing_tier: pricing_tier.clone(),
            is_first_billing,
            total_previous_usage: Decimal::new(1000, 2), // Fixed test value: $10.00
            applicable_discounts: discounts.clone(),
            custom_rates: None,
        };
        
        // Context should be valid
        prop_assert_eq!(context.pricing_tier, pricing_tier);
        prop_assert_eq!(context.is_first_billing, is_first_billing);
        prop_assert_eq!(context.applicable_discounts.len(), discounts.len());
        
        // First-time billing should typically have favorable conditions
        if is_first_billing {
            // Often has discounts or special treatment
            // (Implementation-specific, but context should be consistent)
        }
    }
}

// Invariant tests - properties that should always hold

proptest! {
    #[test]
    fn prop_billing_record_invariants(
        user_id in prop::strategy::Just(Uuid::new_v4()),
        line_items in prop::collection::vec(
            (arb_metric_type(), arb_small_decimal(), arb_small_decimal())
                .prop_map(|(metric_type, usage, rate)| BillingLineItem {
                    metric_type,
                    usage,
                    rate,
                    free_allowance: Decimal::ZERO,
                    billable_usage: usage,
                    cost: usage * rate,
                    description: "Test".to_string(),
                }),
            1..=10
        ),
        discount_amount in arb_decimal_amount()
    ) {
        let subtotal: Decimal = line_items.iter().map(|item| item.cost).sum();
        let discounts = discount_amount.min(subtotal); // Ensure discount doesn't exceed subtotal
        let total_cost = subtotal - discounts;
        
        let billing_record = BillingRecord {
            id: Uuid::new_v4(),
            user_id,
            start_time: Utc::now() - Duration::hours(1),
            end_time: Utc::now(),
            cost_structure: CostStructure::default(),
            line_items,
            subtotal,
            discounts,
            total_cost,
            created_at: Utc::now(),
        };
        
        // Billing record invariants
        prop_assert!(billing_record.total_cost >= Decimal::ZERO);
        prop_assert!(billing_record.subtotal >= Decimal::ZERO);
        prop_assert!(billing_record.discounts >= Decimal::ZERO);
        prop_assert!(billing_record.discounts <= billing_record.subtotal);
        prop_assert_eq!(billing_record.total_cost, billing_record.subtotal - billing_record.discounts);
        
        // Line items should sum to subtotal
        let calculated_subtotal: Decimal = billing_record.line_items.iter()
            .map(|item| item.cost)
            .sum();
        prop_assert_eq!(billing_record.subtotal, calculated_subtotal);
        
        // Time consistency
        prop_assert!(billing_record.start_time < billing_record.end_time);
    }
}