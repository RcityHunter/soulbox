//! Cost Calculator
//! 
//! This module calculates costs based on usage summaries and pricing structures.
//! It handles different pricing tiers, free allowances, and discount calculations.

use super::models::{
    UsageSummary, BillingRecord, BillingLineItem, CostStructure, PricingTier, MetricType
};
use super::precision::{FinancialPrecision, FinancialContext, ApplyPrecision};
use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use uuid::Uuid;

/// Discount types that can be applied to billing
#[derive(Debug, Clone)]
pub enum DiscountType {
    /// Percentage discount (0.0 to 100.0)
    Percentage(Decimal),
    /// Fixed amount discount
    FixedAmount(Decimal),
    /// Volume-based discount (threshold, percentage)
    Volume(Decimal, Decimal),
    /// First-time user discount
    FirstTime(Decimal),
    /// Promotional discount with expiry
    Promotional {
        percentage: Decimal,
        expires_at: DateTime<Utc>,
    },
}

impl DiscountType {
    /// Calculate discount amount for a given subtotal with proper precision
    pub fn calculate_discount(&self, subtotal: Decimal) -> Result<Decimal, String> {
        let context = FinancialContext::default();
        
        match self {
            DiscountType::Percentage(percentage) => {
                context.apply_discount(subtotal, *percentage)
            }
            DiscountType::FixedAmount(amount) => {
                let validated_amount = FinancialPrecision::validate_amount(*amount)?;
                Ok(validated_amount.min(subtotal))
            }
            DiscountType::Volume(threshold, percentage) => {
                if subtotal >= *threshold {
                    context.apply_discount(subtotal, *percentage)
                } else {
                    Ok(FinancialPrecision::zero_amount())
                }
            }
            DiscountType::FirstTime(percentage) => {
                context.apply_discount(subtotal, *percentage)
            }
            DiscountType::Promotional { percentage, expires_at } => {
                if Utc::now() <= *expires_at {
                    context.apply_discount(subtotal, *percentage)
                } else {
                    Ok(FinancialPrecision::zero_amount())
                }
            }
        }
    }
}

/// User billing context for cost calculations
#[derive(Debug, Clone)]
pub struct UserBillingContext {
    pub user_id: Uuid,
    pub pricing_tier: PricingTier,
    pub is_first_billing: bool,
    pub total_previous_usage: Decimal,
    pub applicable_discounts: Vec<DiscountType>,
    pub custom_rates: Option<HashMap<MetricType, Decimal>>,
}

/// Cost calculation engine with precision handling
pub struct CostCalculator {
    default_cost_structure: CostStructure,
    tier_structures: HashMap<PricingTier, CostStructure>,
    financial_context: FinancialContext,
}

impl CostCalculator {
    /// Create a new cost calculator with default pricing
    pub fn new() -> Self {
        let mut tier_structures = HashMap::new();
        
        // Free tier
        let mut free_rates = HashMap::new();
        free_rates.insert(MetricType::CpuUsage, Decimal::ZERO);
        free_rates.insert(MetricType::MemoryUsage, Decimal::ZERO);
        free_rates.insert(MetricType::NetworkIngress, Decimal::ZERO);
        free_rates.insert(MetricType::NetworkEgress, Decimal::ZERO);
        free_rates.insert(MetricType::StorageUsage, Decimal::ZERO);
        free_rates.insert(MetricType::ExecutionTime, Decimal::ZERO);
        free_rates.insert(MetricType::ApiRequests, Decimal::ZERO);

        let mut free_allowances = HashMap::new();
        free_allowances.insert(MetricType::CpuUsage, Decimal::new(1800, 0)); // 30 minutes
        free_allowances.insert(MetricType::MemoryUsage, Decimal::new(1800000, 0)); // 500MB for 30 minutes
        free_allowances.insert(MetricType::NetworkIngress, Decimal::new(100000000, 0)); // 100MB
        free_allowances.insert(MetricType::NetworkEgress, Decimal::new(100000000, 0)); // 100MB
        free_allowances.insert(MetricType::StorageUsage, Decimal::new(1800, 0)); // 500MB for 30 minutes
        free_allowances.insert(MetricType::ExecutionTime, Decimal::new(1800, 0)); // 30 minutes
        free_allowances.insert(MetricType::ApiRequests, Decimal::new(1000, 0)); // 1k requests

        tier_structures.insert(PricingTier::Free, CostStructure {
            rates: free_rates,
            tier: PricingTier::Free,
            currency: "USD".to_string(),
            free_allowances,
        });

        // Basic tier (default)
        tier_structures.insert(PricingTier::Basic, CostStructure::default());

        // Professional tier - reduced rates
        let mut pro_rates = HashMap::new();
        pro_rates.insert(MetricType::CpuUsage, Decimal::new(8, 4)); // $0.0008 per cpu-second
        pro_rates.insert(MetricType::MemoryUsage, Decimal::new(8, 5)); // $0.00008 per mb-second
        pro_rates.insert(MetricType::NetworkIngress, Decimal::new(8, 8)); // $0.00000008 per byte
        pro_rates.insert(MetricType::NetworkEgress, Decimal::new(8, 7)); // $0.0000008 per byte
        pro_rates.insert(MetricType::StorageUsage, Decimal::new(8, 6)); // $0.000008 per gb-second
        pro_rates.insert(MetricType::ExecutionTime, Decimal::new(4, 3)); // $0.004 per second
        pro_rates.insert(MetricType::ApiRequests, Decimal::new(8, 5)); // $0.00008 per request

        let mut pro_allowances = HashMap::new();
        pro_allowances.insert(MetricType::CpuUsage, Decimal::new(7200, 0)); // 2 hours
        pro_allowances.insert(MetricType::MemoryUsage, Decimal::new(7200000, 0)); // 2GB for 1 hour
        pro_allowances.insert(MetricType::NetworkIngress, Decimal::new(5000000000_i64, 0)); // 5GB
        pro_allowances.insert(MetricType::NetworkEgress, Decimal::new(5000000000_i64, 0)); // 5GB
        pro_allowances.insert(MetricType::StorageUsage, Decimal::new(7200, 0)); // 2GB for 1 hour
        pro_allowances.insert(MetricType::ExecutionTime, Decimal::new(7200, 0)); // 2 hours
        pro_allowances.insert(MetricType::ApiRequests, Decimal::new(50000, 0)); // 50k requests

        tier_structures.insert(PricingTier::Professional, CostStructure {
            rates: pro_rates,
            tier: PricingTier::Professional,
            currency: "USD".to_string(),
            free_allowances: pro_allowances,
        });

        // Enterprise tier - custom pricing (placeholder)
        tier_structures.insert(PricingTier::Enterprise, CostStructure {
            rates: HashMap::new(), // Will be set per customer
            tier: PricingTier::Enterprise,
            currency: "USD".to_string(),
            free_allowances: HashMap::new(),
        });

        Self {
            default_cost_structure: CostStructure::default(),
            tier_structures,
            financial_context: FinancialContext::default(),
        }
    }

    /// Calculate cost for a usage summary
    pub async fn calculate_cost(&self, usage: &UsageSummary) -> Result<BillingRecord> {
        let cost_structure = &self.default_cost_structure;
        self.calculate_cost_with_structure(usage, cost_structure, None).await
    }

    /// Calculate cost with specific billing context
    pub async fn calculate_cost_with_context(
        &self,
        usage: &UsageSummary,
        context: &UserBillingContext,
    ) -> Result<BillingRecord> {
        let cost_structure = context.custom_rates.as_ref()
            .map(|rates| self.create_custom_cost_structure(rates, &context.pricing_tier))
            .unwrap_or_else(|| {
                self.tier_structures.get(&context.pricing_tier)
                    .cloned()
                    .unwrap_or_else(|| self.default_cost_structure.clone())
            });

        self.calculate_cost_with_structure(usage, &cost_structure, Some(context)).await
    }

    /// Calculate cost with a specific cost structure
    async fn calculate_cost_with_structure(
        &self,
        usage: &UsageSummary,
        cost_structure: &CostStructure,
        context: Option<&UserBillingContext>,
    ) -> Result<BillingRecord> {
        let mut line_items = Vec::new();
        let mut subtotal = Decimal::ZERO;

        // Calculate cost for each metric type
        for (metric_type, aggregated_metric) in &usage.metrics {
            let rate = cost_structure.rates.get(metric_type)
                .copied()
                .unwrap_or(Decimal::ZERO);

            let free_allowance = cost_structure.free_allowances.get(metric_type)
                .copied()
                .unwrap_or(Decimal::ZERO);

            let total_usage = (metric_type.clone(), aggregated_metric.total).apply_precision();
            let validated_rate = FinancialPrecision::validate_amount(rate)
                .map_err(|e| anyhow::anyhow!("Invalid rate for {}: {}", self.metric_type_display_name(metric_type), e))?;
            let validated_allowance = FinancialPrecision::validate_amount(free_allowance)
                .map_err(|e| anyhow::anyhow!("Invalid allowance for {}: {}", self.metric_type_display_name(metric_type), e))?;
            
            let billable_usage = (total_usage - validated_allowance).max(FinancialPrecision::zero_amount());
            let cost = self.financial_context.calculate_cost(billable_usage, validated_rate)
                .map_err(|e| anyhow::anyhow!("Cost calculation error for {}: {}", self.metric_type_display_name(metric_type), e))?;

            let description = format!(
                "{} usage: {} {} (Rate: ${}/{})",
                self.metric_type_display_name(metric_type),
                total_usage,
                metric_type.unit(),
                rate,
                metric_type.unit()
            );

            let line_item = BillingLineItem {
                metric_type: metric_type.clone(),
                usage: total_usage,
                rate: validated_rate,
                free_allowance: validated_allowance,
                billable_usage,
                cost,
                description,
            };

            line_items.push(line_item);
            subtotal = FinancialPrecision::safe_add(subtotal, cost)
                .map_err(|e| anyhow::anyhow!("Subtotal calculation error: {}", e))?;
        }

        // Apply discounts with proper precision
        let total_discounts = if let Some(context) = context {
            self.calculate_total_discounts(&context.applicable_discounts, subtotal)
                .map_err(|e| anyhow::anyhow!("Discount calculation error: {}", e))?
        } else {
            FinancialPrecision::zero_amount()
        };

        let total_cost = FinancialPrecision::safe_add(subtotal, -total_discounts)
            .map_err(|e| anyhow::anyhow!("Total cost calculation error: {}", e))?
            .max(FinancialPrecision::zero_amount());

        Ok(BillingRecord {
            id: Uuid::new_v4(),
            user_id: usage.user_id,
            start_time: usage.start_time,
            end_time: usage.end_time,
            cost_structure: cost_structure.clone(),
            line_items,
            subtotal,
            discounts: total_discounts,
            total_cost,
            created_at: Utc::now(),
        })
    }

    /// Calculate estimated cost for ongoing usage
    pub fn calculate_estimated_cost(
        &self,
        current_usage: &HashMap<MetricType, Decimal>,
        pricing_tier: &PricingTier,
        duration_hours: f64,
    ) -> Result<Decimal> {
        let cost_structure = self.tier_structures.get(pricing_tier)
            .unwrap_or(&self.default_cost_structure);

        let mut estimated_cost = Decimal::ZERO;

        for (metric_type, current_value) in current_usage {
            let rate = cost_structure.rates.get(metric_type)
                .copied()
                .unwrap_or(Decimal::ZERO);

            let free_allowance = cost_structure.free_allowances.get(metric_type)
                .copied()
                .unwrap_or(Decimal::ZERO);

            // Project usage for the duration
            let projected_usage = if metric_type.is_cumulative() {
                *current_value * Decimal::from_f64_retain(duration_hours).unwrap_or(Decimal::ONE)
            } else {
                *current_value
            };

            let billable_usage = (projected_usage - free_allowance).max(Decimal::ZERO);
            estimated_cost += billable_usage * rate;
        }

        Ok(estimated_cost)
    }

    /// Get pricing information for a tier
    pub fn get_pricing_info(&self, tier: &PricingTier) -> Option<&CostStructure> {
        self.tier_structures.get(tier)
    }

    /// Create custom cost structure from custom rates
    fn create_custom_cost_structure(
        &self,
        custom_rates: &HashMap<MetricType, Decimal>,
        tier: &PricingTier,
    ) -> CostStructure {
        let base_structure = self.tier_structures.get(tier)
            .unwrap_or(&self.default_cost_structure);

        let mut rates = base_structure.rates.clone();
        for (metric_type, rate) in custom_rates {
            rates.insert(metric_type.clone(), *rate);
        }

        CostStructure {
            rates,
            tier: tier.clone(),
            currency: base_structure.currency.clone(),
            free_allowances: base_structure.free_allowances.clone(),
        }
    }

    /// Calculate total discounts from a list of discount types with precision
    fn calculate_total_discounts(&self, discounts: &[DiscountType], subtotal: Decimal) -> Result<Decimal, String> {
        let mut total_discount = FinancialPrecision::zero_amount();
        let mut remaining_amount = subtotal;

        for discount in discounts {
            let discount_amount = discount.calculate_discount(remaining_amount)?;
            total_discount = FinancialPrecision::safe_add(total_discount, discount_amount)?;
            remaining_amount = FinancialPrecision::safe_add(remaining_amount, -discount_amount)?;
            
            // Ensure we don't have negative remaining amount
            if remaining_amount <= FinancialPrecision::zero_amount() {
                break;
            }
        }

        Ok(total_discount.min(subtotal)) // Total discount can't exceed subtotal
    }

    /// Get display name for metric type
    fn metric_type_display_name(&self, metric_type: &MetricType) -> &'static str {
        match metric_type {
            MetricType::CpuUsage => "CPU",
            MetricType::MemoryUsage => "Memory",
            MetricType::NetworkIngress => "Network Ingress",
            MetricType::NetworkEgress => "Network Egress",
            MetricType::StorageUsage => "Storage",
            MetricType::ExecutionTime => "Execution Time",
            MetricType::ApiRequests => "API Requests",
            MetricType::Custom(_) => "Custom Metric",
        }
    }
}

impl Default for CostCalculator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::models::AggregatedMetric;

    #[test]
    fn test_discount_calculation() {
        let subtotal = Decimal::new(1000, 2); // $10.00

        // Percentage discount
        let percentage_discount = DiscountType::Percentage(Decimal::new(10, 0)); // 10%
        assert_eq!(percentage_discount.calculate_discount(subtotal), Decimal::new(100, 2)); // $1.00

        // Fixed amount discount
        let fixed_discount = DiscountType::FixedAmount(Decimal::new(200, 2)); // $2.00
        assert_eq!(fixed_discount.calculate_discount(subtotal), Decimal::new(200, 2)); // $2.00

        // Volume discount (threshold met)
        let volume_discount = DiscountType::Volume(Decimal::new(500, 2), Decimal::new(15, 0)); // $5.00 threshold, 15%
        assert_eq!(volume_discount.calculate_discount(subtotal), Decimal::new(150, 2)); // $1.50

        // Volume discount (threshold not met)
        let volume_discount_not_met = DiscountType::Volume(Decimal::new(1500, 2), Decimal::new(15, 0)); // $15.00 threshold, 15%
        assert_eq!(volume_discount_not_met.calculate_discount(subtotal), Decimal::ZERO);
    }

    #[test]
    fn test_cost_calculator_creation() {
        let calculator = CostCalculator::new();
        
        // Check that all tiers are available
        assert!(calculator.tier_structures.contains_key(&PricingTier::Free));
        assert!(calculator.tier_structures.contains_key(&PricingTier::Basic));
        assert!(calculator.tier_structures.contains_key(&PricingTier::Professional));
        assert!(calculator.tier_structures.contains_key(&PricingTier::Enterprise));
    }

    #[tokio::test]
    async fn test_basic_cost_calculation() {
        let calculator = CostCalculator::new();
        
        // Create a simple usage summary
        let mut metrics = HashMap::new();
        metrics.insert(MetricType::CpuUsage, AggregatedMetric {
            total: Decimal::new(3600, 0), // 1 hour
            average: Decimal::new(3600, 0),
            maximum: Decimal::new(3600, 0),
            minimum: Decimal::new(3600, 0),
            count: 1,
        });

        let usage_summary = UsageSummary {
            user_id: Uuid::new_v4(),
            start_time: Utc::now() - chrono::Duration::hours(1),
            end_time: Utc::now(),
            metrics,
            session_count: 1,
            created_at: Utc::now(),
        };

        let billing_record = calculator.calculate_cost(&usage_summary).await.unwrap();
        
        // Should have one line item for CPU usage
        assert_eq!(billing_record.line_items.len(), 1);
        assert_eq!(billing_record.line_items[0].metric_type, MetricType::CpuUsage);
        
        // With default rates and free allowance, should have some cost
        assert!(billing_record.total_cost >= Decimal::ZERO);
    }

    #[test]
    fn test_estimated_cost_calculation() {
        let calculator = CostCalculator::new();
        
        let mut current_usage = HashMap::new();
        current_usage.insert(MetricType::CpuUsage, Decimal::new(50, 0)); // 50 cpu-seconds
        current_usage.insert(MetricType::MemoryUsage, Decimal::new(1000, 0)); // 1000 mb-seconds
        
        let estimated = calculator.calculate_estimated_cost(
            &current_usage,
            &PricingTier::Basic,
            2.0 // 2 hours
        ).unwrap();
        
        assert!(estimated >= Decimal::ZERO);
    }

    #[test]
    fn test_total_discount_calculation() {
        let calculator = CostCalculator::new();
        let subtotal = Decimal::new(2000, 2); // $20.00
        
        let discounts = vec![
            DiscountType::Percentage(Decimal::new(10, 0)), // 10%
            DiscountType::FixedAmount(Decimal::new(200, 2)), // $2.00
        ];
        
        let total_discount = calculator.calculate_total_discounts(&discounts, subtotal);
        
        // 10% of $20.00 = $2.00, then $2.00 fixed = $4.00 total
        assert_eq!(total_discount, Decimal::new(400, 2));
    }

    #[test]
    fn test_promotional_discount_expiry() {
        let expired_promo = DiscountType::Promotional {
            percentage: Decimal::new(50, 0), // 50%
            expires_at: Utc::now() - chrono::Duration::days(1), // Expired yesterday
        };
        
        let subtotal = Decimal::new(1000, 2); // $10.00
        assert_eq!(expired_promo.calculate_discount(subtotal), Decimal::ZERO);
        
        let active_promo = DiscountType::Promotional {
            percentage: Decimal::new(50, 0), // 50%
            expires_at: Utc::now() + chrono::Duration::days(1), // Expires tomorrow
        };
        
        assert_eq!(active_promo.calculate_discount(subtotal), Decimal::new(500, 2)); // $5.00
    }
}