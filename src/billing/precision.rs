//! Financial Precision Constants and Utilities
//! 
//! This module defines standard precision constants and utilities for financial
//! calculations to ensure consistency across the billing system.

use rust_decimal::Decimal;
use std::str::FromStr;

/// Standard financial precision constants
pub struct FinancialPrecision;

impl FinancialPrecision {
    /// Standard monetary precision (4 decimal places for sub-cent precision)
    pub const MONETARY_SCALE: u32 = 4;
    
    /// CPU usage precision (6 decimal places for micro-second precision)
    pub const CPU_SCALE: u32 = 6;
    
    /// Memory usage precision (2 decimal places for KB precision in MB)
    pub const MEMORY_SCALE: u32 = 2;
    
    /// Network usage precision (0 decimal places for byte precision)
    pub const NETWORK_SCALE: u32 = 0;
    
    /// Storage usage precision (3 decimal places for MB precision in GB)
    pub const STORAGE_SCALE: u32 = 3;
    
    /// Time precision (3 decimal places for millisecond precision)
    pub const TIME_SCALE: u32 = 3;
    
    /// Request count precision (0 decimal places for exact counting)
    pub const COUNT_SCALE: u32 = 0;

    /// Minimum chargeable amount (0.0001 USD)
    pub fn min_charge() -> Decimal {
        Decimal::new(1, Self::MONETARY_SCALE)
    }

    /// Maximum single transaction amount (1 million USD)
    pub fn max_charge() -> Decimal {
        Decimal::new(1_000_000_0000, Self::MONETARY_SCALE)
    }

    /// Zero amount with proper precision
    pub fn zero_amount() -> Decimal {
        Decimal::new(0, Self::MONETARY_SCALE)
    }

    /// Round amount to monetary precision
    pub fn round_monetary(amount: Decimal) -> Decimal {
        amount.round_dp(Self::MONETARY_SCALE)
    }

    /// Round CPU usage to standard precision
    pub fn round_cpu(usage: Decimal) -> Decimal {
        usage.round_dp(Self::CPU_SCALE)
    }

    /// Round memory usage to standard precision
    pub fn round_memory(usage: Decimal) -> Decimal {
        usage.round_dp(Self::MEMORY_SCALE)
    }

    /// Round network usage to standard precision
    pub fn round_network(usage: Decimal) -> Decimal {
        usage.round_dp(Self::NETWORK_SCALE)
    }

    /// Round storage usage to standard precision
    pub fn round_storage(usage: Decimal) -> Decimal {
        usage.round_dp(Self::STORAGE_SCALE)
    }

    /// Round time to standard precision
    pub fn round_time(time: Decimal) -> Decimal {
        time.round_dp(Self::TIME_SCALE)
    }

    /// Round count to standard precision
    pub fn round_count(count: Decimal) -> Decimal {
        count.round_dp(Self::COUNT_SCALE)
    }

    /// Validate that an amount is within acceptable bounds
    pub fn validate_amount(amount: Decimal) -> Result<Decimal, String> {
        if amount < Decimal::ZERO {
            return Err("Amount cannot be negative".to_string());
        }
        
        if amount > Self::max_charge() {
            return Err("Amount exceeds maximum allowed".to_string());
        }
        
        Ok(Self::round_monetary(amount))
    }

    /// Safe addition with overflow check
    pub fn safe_add(a: Decimal, b: Decimal) -> Result<Decimal, String> {
        match a.checked_add(b) {
            Some(result) => {
                if result > Self::max_charge() {
                    Err("Addition would exceed maximum allowed amount".to_string())
                } else {
                    Ok(Self::round_monetary(result))
                }
            }
            None => Err("Arithmetic overflow in addition".to_string()),
        }
    }

    /// Safe multiplication with overflow check
    pub fn safe_multiply(a: Decimal, b: Decimal) -> Result<Decimal, String> {
        match a.checked_mul(b) {
            Some(result) => {
                if result > Self::max_charge() {
                    Err("Multiplication would exceed maximum allowed amount".to_string())
                } else {
                    Ok(Self::round_monetary(result))
                }
            }
            None => Err("Arithmetic overflow in multiplication".to_string()),
        }
    }

    /// Safe division with zero check
    pub fn safe_divide(a: Decimal, b: Decimal) -> Result<Decimal, String> {
        if b == Decimal::ZERO {
            return Err("Division by zero".to_string());
        }
        
        match a.checked_div(b) {
            Some(result) => Ok(Self::round_monetary(result)),
            None => Err("Arithmetic overflow in division".to_string()),
        }
    }

    /// Format amount for display with currency
    pub fn format_currency(amount: Decimal, currency: &str) -> String {
        format!("{} {:.4}", currency, amount)
    }

    /// Parse currency amount from string
    pub fn parse_currency(amount_str: &str) -> Result<Decimal, String> {
        match Decimal::from_str(amount_str) {
            Ok(amount) => Self::validate_amount(amount),
            Err(e) => Err(format!("Failed to parse amount: {}", e)),
        }
    }
}

/// Trait for applying standard precision to different metric types
pub trait ApplyPrecision {
    fn apply_precision(&self) -> Decimal;
}

impl ApplyPrecision for (super::MetricType, Decimal) {
    fn apply_precision(&self) -> Decimal {
        use super::MetricType;
        
        match &self.0 {
            MetricType::CpuUsage => FinancialPrecision::round_cpu(self.1),
            MetricType::MemoryUsage => FinancialPrecision::round_memory(self.1),
            MetricType::NetworkIngress | MetricType::NetworkEgress => FinancialPrecision::round_network(self.1),
            MetricType::StorageUsage => FinancialPrecision::round_storage(self.1),
            MetricType::ExecutionTime => FinancialPrecision::round_time(self.1),
            MetricType::ApiRequests => FinancialPrecision::round_count(self.1),
            MetricType::Custom(_) => FinancialPrecision::round_monetary(self.1), // Default to monetary precision
        }
    }
}

/// Financial calculation context with error handling
#[derive(Debug, Clone)]
pub struct FinancialContext {
    pub currency: String,
    pub enforce_limits: bool,
}

impl Default for FinancialContext {
    fn default() -> Self {
        Self {
            currency: "USD".to_string(),
            enforce_limits: true,
        }
    }
}

impl FinancialContext {
    pub fn new(currency: String) -> Self {
        Self {
            currency,
            enforce_limits: true,
        }
    }

    /// Calculate cost with proper precision and validation
    pub fn calculate_cost(&self, usage: Decimal, rate: Decimal) -> Result<Decimal, String> {
        let raw_cost = FinancialPrecision::safe_multiply(usage, rate)?;
        
        if self.enforce_limits {
            FinancialPrecision::validate_amount(raw_cost)
        } else {
            Ok(FinancialPrecision::round_monetary(raw_cost))
        }
    }

    /// Sum multiple costs with overflow protection
    pub fn sum_costs(&self, costs: &[Decimal]) -> Result<Decimal, String> {
        let mut total = FinancialPrecision::zero_amount();
        
        for cost in costs {
            total = FinancialPrecision::safe_add(total, *cost)?;
        }
        
        Ok(total)
    }

    /// Apply discount to amount
    pub fn apply_discount(&self, amount: Decimal, discount_percent: Decimal) -> Result<Decimal, String> {
        if discount_percent < Decimal::ZERO || discount_percent > Decimal::new(100, 0) {
            return Err("Discount percent must be between 0 and 100".to_string());
        }

        let discount_multiplier = Decimal::new(100, 0) - discount_percent;
        let discounted = FinancialPrecision::safe_multiply(amount, discount_multiplier)?;
        FinancialPrecision::safe_divide(discounted, Decimal::new(100, 0))
    }

    /// Format amount for this context
    pub fn format_amount(&self, amount: Decimal) -> String {
        FinancialPrecision::format_currency(amount, &self.currency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_precision_constants() {
        assert_eq!(FinancialPrecision::MONETARY_SCALE, 4);
        assert_eq!(FinancialPrecision::CPU_SCALE, 6);
        assert_eq!(FinancialPrecision::MEMORY_SCALE, 2);
    }

    #[test]
    fn test_min_max_amounts() {
        let min = FinancialPrecision::min_charge();
        let max = FinancialPrecision::max_charge();
        
        assert_eq!(min, Decimal::new(1, 4));
        assert_eq!(max, Decimal::new(1_000_000_0000, 4));
    }

    #[test]
    fn test_amount_validation() {
        // Valid amount
        let valid = Decimal::new(1000, 2); // 10.00
        assert!(FinancialPrecision::validate_amount(valid).is_ok());
        
        // Negative amount
        let negative = Decimal::new(-100, 2);
        assert!(FinancialPrecision::validate_amount(negative).is_err());
        
        // Too large amount
        let too_large = Decimal::new(2_000_000_0000, 4);
        assert!(FinancialPrecision::validate_amount(too_large).is_err());
    }

    #[test]
    fn test_safe_arithmetic() {
        let a = Decimal::new(1000, 2); // 10.00
        let b = Decimal::new(2000, 2); // 20.00
        
        // Safe addition
        let sum = FinancialPrecision::safe_add(a, b).unwrap();
        assert_eq!(sum, Decimal::new(30_0000, 4)); // 30.0000
        
        // Safe multiplication
        let product = FinancialPrecision::safe_multiply(a, b).unwrap();
        assert_eq!(product, Decimal::new(200_0000, 4)); // 200.0000
        
        // Safe division
        let quotient = FinancialPrecision::safe_divide(b, a).unwrap();
        assert_eq!(quotient, Decimal::new(2_0000, 4)); // 2.0000
    }

    #[test]
    fn test_financial_context() {
        let context = FinancialContext::new("EUR".to_string());
        
        let usage = Decimal::new(100, 2); // 1.00
        let rate = Decimal::new(50, 3); // 0.050
        
        let cost = context.calculate_cost(usage, rate).unwrap();
        assert_eq!(cost, Decimal::new(500, 4)); // 0.0500
        
        let formatted = context.format_amount(cost);
        assert_eq!(formatted, "EUR 0.0500");
    }

    #[test]
    fn test_apply_discount() {
        let context = FinancialContext::default();
        let amount = Decimal::new(100_0000, 4); // 100.0000
        let discount = Decimal::new(10, 0); // 10%
        
        let discounted = context.apply_discount(amount, discount).unwrap();
        assert_eq!(discounted, Decimal::new(90_0000, 4)); // 90.0000
    }
}