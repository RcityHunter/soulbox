//! Billing Data Models
//! 
//! This module defines the core data structures used throughout the billing system.
//! All monetary values use rust_decimal::Decimal for precise financial calculations.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Types of metrics that can be collected for billing
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricType {
    /// CPU usage in CPU-seconds
    CpuUsage,
    /// Memory usage in MB-seconds
    MemoryUsage,
    /// Network ingress traffic in bytes
    NetworkIngress,
    /// Network egress traffic in bytes
    NetworkEgress,
    /// Storage usage in GB-seconds
    StorageUsage,
    /// Execution time in seconds
    ExecutionTime,
    /// Number of API requests
    ApiRequests,
    /// Custom metric types
    Custom(String),
}

impl MetricType {
    /// Get the unit of measurement for this metric type
    pub fn unit(&self) -> &'static str {
        match self {
            MetricType::CpuUsage => "cpu-seconds",
            MetricType::MemoryUsage => "mb-seconds",
            MetricType::NetworkIngress | MetricType::NetworkEgress => "bytes",
            MetricType::StorageUsage => "gb-seconds",
            MetricType::ExecutionTime => "seconds",
            MetricType::ApiRequests => "requests",
            MetricType::Custom(_) => "units",
        }
    }

    /// Check if this metric type is cumulative (accumulates over time)
    pub fn is_cumulative(&self) -> bool {
        matches!(self, 
            MetricType::CpuUsage | 
            MetricType::MemoryUsage | 
            MetricType::StorageUsage |
            MetricType::ExecutionTime
        )
    }
}

/// A single usage metric collected from a sandbox session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetric {
    /// Unique identifier for this metric
    pub id: Uuid,
    /// Session this metric belongs to
    pub session_id: Uuid,
    /// User who owns the session
    pub user_id: Uuid,
    /// Type of metric
    pub metric_type: MetricType,
    /// Metric value (using Decimal for precision)
    pub value: Decimal,
    /// When this metric was recorded
    pub timestamp: DateTime<Utc>,
    /// Additional metadata for the metric
    pub metadata: Option<HashMap<String, String>>,
}

/// Aggregated usage summary for a user over a time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    /// User this summary belongs to
    pub user_id: Uuid,
    /// Start of the aggregation period
    pub start_time: DateTime<Utc>,
    /// End of the aggregation period
    pub end_time: DateTime<Utc>,
    /// Aggregated metrics by type
    pub metrics: HashMap<MetricType, AggregatedMetric>,
    /// Total sessions included in this summary
    pub session_count: u64,
    /// When this summary was created
    pub created_at: DateTime<Utc>,
}

/// Aggregated metric data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedMetric {
    /// Total value across all sessions
    pub total: Decimal,
    /// Average value per session
    pub average: Decimal,
    /// Maximum value in a single session
    pub maximum: Decimal,
    /// Minimum value in a single session
    pub minimum: Decimal,
    /// Number of data points aggregated
    pub count: u64,
}

/// Pricing tier for different service levels
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PricingTier {
    /// Free tier with limited usage
    Free,
    /// Basic paid tier
    Basic,
    /// Professional tier
    Professional,
    /// Enterprise tier with custom pricing
    Enterprise,
}

/// Cost structure for different metric types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostStructure {
    /// Cost per unit for each metric type
    pub rates: HashMap<MetricType, Decimal>,
    /// Pricing tier this structure applies to
    pub tier: PricingTier,
    /// Currency code (e.g., "USD")
    pub currency: String,
    /// Free allowances for each metric type
    pub free_allowances: HashMap<MetricType, Decimal>,
}

impl Default for CostStructure {
    fn default() -> Self {
        let mut rates = HashMap::new();
        let mut free_allowances = HashMap::new();

        // Default rates in USD
        rates.insert(MetricType::CpuUsage, Decimal::new(1, 3)); // $0.001 per cpu-second
        rates.insert(MetricType::MemoryUsage, Decimal::new(1, 4)); // $0.0001 per mb-second
        rates.insert(MetricType::NetworkIngress, Decimal::new(1, 7)); // $0.0000001 per byte
        rates.insert(MetricType::NetworkEgress, Decimal::new(1, 6)); // $0.000001 per byte
        rates.insert(MetricType::StorageUsage, Decimal::new(1, 5)); // $0.00001 per gb-second
        rates.insert(MetricType::ExecutionTime, Decimal::new(5, 3)); // $0.005 per second
        rates.insert(MetricType::ApiRequests, Decimal::new(1, 4)); // $0.0001 per request

        // Default free allowances for Basic tier
        free_allowances.insert(MetricType::CpuUsage, Decimal::new(3600, 0)); // 1 hour
        free_allowances.insert(MetricType::MemoryUsage, Decimal::new(3600000, 0)); // 1GB for 1 hour
        free_allowances.insert(MetricType::NetworkIngress, Decimal::new(1000000000, 0)); // 1GB
        free_allowances.insert(MetricType::NetworkEgress, Decimal::new(1000000000, 0)); // 1GB
        free_allowances.insert(MetricType::StorageUsage, Decimal::new(3600, 0)); // 1GB for 1 hour
        free_allowances.insert(MetricType::ExecutionTime, Decimal::new(3600, 0)); // 1 hour
        free_allowances.insert(MetricType::ApiRequests, Decimal::new(10000, 0)); // 10k requests

        Self {
            rates,
            tier: PricingTier::Basic,
            currency: "USD".to_string(),
            free_allowances,
        }
    }
}

/// A billing record representing charges for a usage period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingRecord {
    /// Unique identifier for this billing record
    pub id: Uuid,
    /// User this record belongs to
    pub user_id: Uuid,
    /// Start of the billing period
    pub start_time: DateTime<Utc>,
    /// End of the billing period
    pub end_time: DateTime<Utc>,
    /// Cost structure used for calculations
    pub cost_structure: CostStructure,
    /// Line items breaking down costs by metric type
    pub line_items: Vec<BillingLineItem>,
    /// Total cost before any discounts
    pub subtotal: Decimal,
    /// Total discounts applied
    pub discounts: Decimal,
    /// Total cost after discounts
    pub total_cost: Decimal,
    /// When this record was created
    pub created_at: DateTime<Utc>,
}

/// Individual line item in a billing record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingLineItem {
    /// Metric type this line item represents
    pub metric_type: MetricType,
    /// Total usage for this metric
    pub usage: Decimal,
    /// Rate applied per unit
    pub rate: Decimal,
    /// Free allowance applied
    pub free_allowance: Decimal,
    /// Billable usage after free allowance
    pub billable_usage: Decimal,
    /// Total cost for this line item
    pub cost: Decimal,
    /// Description of the charge
    pub description: String,
}

/// Status of an invoice
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvoiceStatus {
    /// Invoice is in draft state
    Draft,
    /// Invoice has been sent to customer
    Sent,
    /// Invoice has been paid
    Paid,
    /// Invoice is overdue
    Overdue,
    /// Invoice has been cancelled
    Cancelled,
}

/// An invoice generated from billing records
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    /// Unique identifier for this invoice
    pub id: Uuid,
    /// User this invoice belongs to
    pub user_id: Uuid,
    /// Billing record this invoice is based on
    pub billing_record_id: Uuid,
    /// Start of the invoice period
    pub start_time: DateTime<Utc>,
    /// End of the invoice period
    pub end_time: DateTime<Utc>,
    /// Total amount due
    pub total_amount: Decimal,
    /// Current status of the invoice
    pub status: InvoiceStatus,
    /// When this invoice was created
    pub created_at: DateTime<Utc>,
    /// When this invoice was last updated
    pub updated_at: DateTime<Utc>,
    /// Line items for detailed breakdown
    pub line_items: Vec<InvoiceLineItem>,
}

/// Line item in an invoice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceLineItem {
    /// Description of the service
    pub description: String,
    /// Quantity of service used
    pub quantity: Decimal,
    /// Unit of measurement
    pub unit: String,
    /// Rate per unit
    pub rate: Decimal,
    /// Total amount for this line item
    pub amount: Decimal,
}

/// Real-time billing metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingMetrics {
    /// Total active sessions being billed
    pub active_sessions: u64,
    /// Current cost rate per hour
    pub current_hourly_rate: Decimal,
    /// Total cost accumulated in current period
    pub period_cost: Decimal,
    /// Metrics collection rate (metrics per second)
    pub collection_rate: f64,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_type_units() {
        assert_eq!(MetricType::CpuUsage.unit(), "cpu-seconds");
        assert_eq!(MetricType::MemoryUsage.unit(), "mb-seconds");
        assert_eq!(MetricType::NetworkIngress.unit(), "bytes");
        assert_eq!(MetricType::ApiRequests.unit(), "requests");
    }

    #[test]
    fn test_metric_type_cumulative() {
        assert!(MetricType::CpuUsage.is_cumulative());
        assert!(MetricType::MemoryUsage.is_cumulative());
        assert!(!MetricType::NetworkIngress.is_cumulative());
        assert!(!MetricType::ApiRequests.is_cumulative());
    }

    #[test]
    fn test_cost_structure_default() {
        let cost_structure = CostStructure::default();
        assert_eq!(cost_structure.currency, "USD");
        assert_eq!(cost_structure.tier, PricingTier::Basic);
        assert!(cost_structure.rates.contains_key(&MetricType::CpuUsage));
        assert!(cost_structure.free_allowances.contains_key(&MetricType::CpuUsage));
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
    fn test_invoice_status() {
        let status = InvoiceStatus::Draft;
        assert_eq!(status, InvoiceStatus::Draft);
        assert_ne!(status, InvoiceStatus::Paid);
    }
}