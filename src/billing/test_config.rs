//! Test Configuration and Integration
//! 
//! This module provides test configurations and integration tests for the billing system.

#[cfg(test)]
mod tests {
    use super::super::*;
    use chrono::Utc;
    use rust_decimal::Decimal;
    use uuid::Uuid;

    #[test]
    fn test_billing_config_creation() {
        let config = BillingConfig::default();
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.collection_interval, 30);
        assert_eq!(config.buffer_size, 1000);
        assert_eq!(config.flush_interval, 60);
    }

    #[test]
    fn test_simple_billing_service_creation() {
        // This test just verifies the types compile correctly
        let config = BillingConfig::default();
        
        // We can't actually test async creation without a running database,
        // but we can verify the config is valid
        assert!(config.batch_size > 0);
        assert!(config.collection_interval > 0);
    }

    #[test]
    fn test_usage_metric_creation() {
        let metric = UsageMetric {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            metric_type: MetricType::CpuUsage,
            value: Decimal::new(100, 0),
            timestamp: Utc::now(),
            metadata: None,
        };

        assert_eq!(metric.metric_type, MetricType::CpuUsage);
        assert_eq!(metric.value, Decimal::new(100, 0));
    }

    #[test]
    fn test_financial_precision() {
        use crate::billing::precision::FinancialPrecision;
        
        let amount = Decimal::new(123456, 4); // 12.3456
        let rounded = FinancialPrecision::round_monetary(amount);
        assert_eq!(rounded, amount); // Should remain the same within precision
        
        let large_amount = Decimal::new(123456789, 6); // 123.456789
        let rounded_large = FinancialPrecision::round_monetary(large_amount);
        assert_eq!(rounded_large.scale(), 4); // Should be rounded to 4 decimal places
    }
}