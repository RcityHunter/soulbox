//! Billing System Module
//! 
//! This module provides comprehensive billing and usage tracking functionality
//! for the SoulBox sandbox platform. It includes real-time metrics collection,
//! usage aggregation, cost calculation, and billing data persistence.

#![allow(unused_imports)]

pub mod models;
pub mod metrics_collector;
pub mod usage_aggregator;
pub mod cost_calculator;
pub mod storage;
pub mod simple_collector;
pub mod simple_aggregator;
pub mod precision;
pub mod error_handling;
pub mod test_config;

#[cfg(test)]
mod tests;

use anyhow::Result;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::sync::Arc;
use uuid::Uuid;

// Re-export main types
pub use models::*;
pub use metrics_collector::MetricsCollector;
pub use usage_aggregator::UsageAggregator;
pub use cost_calculator::CostCalculator;
pub use storage::BillingStorage;
// Simplified implementations
pub use simple_collector::SimpleMetricsCollector;
pub use simple_aggregator::SimpleUsageAggregator;

/// Simplified configuration for the billing system
#[derive(Debug, Clone)]
pub struct BillingConfig {
    /// Batch size for metric processing
    pub batch_size: usize,
    /// Collection interval in seconds
    pub collection_interval: u64,
    /// Storage configuration
    pub storage_config: storage::StorageConfig,
    /// Buffer size for in-memory metrics
    pub buffer_size: usize,
    /// Flush interval in seconds
    pub flush_interval: u64,
    /// Metrics stream name for Redis Streams
    pub metrics_stream: String,
}

impl Default for BillingConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            collection_interval: 30,
            storage_config: storage::StorageConfig::default(),
            buffer_size: 1000,
            flush_interval: 60, // 1 minute
            metrics_stream: "soulbox:metrics".to_string(),
        }
    }
}

/// Main billing service that orchestrates all billing operations
#[derive(Clone)]
pub struct BillingService {
    collector: Arc<MetricsCollector>,
    aggregator: Arc<UsageAggregator>,
    calculator: Arc<CostCalculator>,
    storage: Arc<BillingStorage>,
    config: BillingConfig,
}

impl BillingService {
    /// Create a new billing service instance
    pub async fn new(config: BillingConfig) -> Result<Self> {
        let collector = Arc::new(MetricsCollector::new(&config).await?);
        let aggregator = Arc::new(UsageAggregator::new(&config).await?);
        let calculator = Arc::new(CostCalculator::new());
        let storage = Arc::new(BillingStorage::new(&config.storage_config).await?);

        Ok(Self {
            collector,
            aggregator,
            calculator,
            storage,
            config,
        })
    }

    /// Start the billing service background tasks
    pub async fn start(&self) -> Result<()> {
        // Start metrics collection
        self.collector.start_collection().await?;
        
        // Start usage aggregation
        self.aggregator.start_aggregation().await?;

        tracing::info!("Billing service started successfully");
        Ok(())
    }

    /// Stop the billing service with proper cleanup
    pub async fn stop(&self) -> Result<()> {
        tracing::info!("Stopping billing service...");
        
        // Stop components in reverse order of startup
        if let Err(e) = self.aggregator.stop_aggregation().await {
            tracing::error!("Error stopping aggregator: {}", e);
        }
        
        if let Err(e) = self.collector.stop_collection().await {
            tracing::error!("Error stopping collector: {}", e);
        }
        
        tracing::info!("Billing service stopped");
        Ok(())
    }
    
    /// Get service health status
    pub async fn health_check(&self) -> BillingServiceHealth {
        let collector_stats = self.collector.get_processing_stats().await;
        let billing_metrics = self.collector.get_billing_metrics().await;
        
        BillingServiceHealth {
            collector_running: collector_stats.total_processed > 0,
            metrics_processed: collector_stats.total_processed,
            error_count: collector_stats.error_count,
            buffer_size: collector_stats.buffer_size,
            last_collection: billing_metrics.last_updated,
        }
    }

    /// Record a usage metric for a session
    pub async fn record_usage(
        &self, 
        session_id: Uuid, 
        user_id: Uuid, 
        metric_type: MetricType,
        value: Decimal,
        timestamp: Option<DateTime<Utc>>
    ) -> Result<()> {
        let metric = UsageMetric {
            id: Uuid::new_v4(),
            session_id,
            user_id,
            metric_type,
            value,
            timestamp: timestamp.unwrap_or_else(Utc::now),
            metadata: None,
        };

        self.collector.collect_metric(metric).await?;
        Ok(())
    }

    /// Get real-time usage for a session
    pub async fn get_realtime_usage(&self, session_id: Uuid) -> Result<Vec<UsageMetric>> {
        self.storage.get_session_metrics(session_id, None, None).await
    }

    /// Get aggregated usage for a user within a time period
    pub async fn get_usage_summary(
        &self, 
        user_id: Uuid, 
        start_time: DateTime<Utc>, 
        end_time: DateTime<Utc>
    ) -> Result<UsageSummary> {
        self.aggregator.get_usage_summary(user_id, start_time, end_time).await
    }

    /// Calculate cost for usage
    pub async fn calculate_cost(&self, usage: &UsageSummary) -> Result<BillingRecord> {
        self.calculator.calculate_cost(usage).await
    }

    /// Generate invoice for a user
    pub async fn generate_invoice(
        &self, 
        user_id: Uuid, 
        start_time: DateTime<Utc>, 
        end_time: DateTime<Utc>
    ) -> Result<Invoice> {
        let usage_summary = self.get_usage_summary(user_id, start_time, end_time).await?;
        let billing_record = self.calculate_cost(&usage_summary).await?;
        
        let invoice = Invoice {
            id: Uuid::new_v4(),
            user_id,
            billing_record_id: billing_record.id,
            start_time,
            end_time,
            total_amount: billing_record.total_cost,
            status: InvoiceStatus::Draft,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            line_items: vec![], // Will be populated from billing record
        };

        self.storage.store_invoice(&invoice).await?;
        Ok(invoice)
    }
}

// Implement Drop trait for proper resource cleanup
impl Drop for BillingService {
    fn drop(&mut self) {
        tracing::info!("BillingService dropped - ensure stop() was called for proper cleanup");
    }
}

/// Health status of the billing service
#[derive(Debug, Clone)]
pub struct BillingServiceHealth {
    pub collector_running: bool,
    pub metrics_processed: u64,
    pub error_count: u64,
    pub buffer_size: u64,
    pub last_collection: DateTime<Utc>,
}

/// Simplified billing service that uses direct database operations
#[derive(Clone)]
pub struct SimpleBillingService {
    collector: Arc<SimpleMetricsCollector>,
    aggregator: Arc<SimpleUsageAggregator>,
    calculator: Arc<CostCalculator>,
    storage: Arc<BillingStorage>,
    config: BillingConfig,
}

impl SimpleBillingService {
    /// Create a new simplified billing service instance
    pub async fn new(config: BillingConfig) -> Result<Self> {
        let storage = Arc::new(BillingStorage::new(&config.storage_config).await?);
        let collector = Arc::new(SimpleMetricsCollector::new(&config, storage.clone()).await?);
        let aggregator = Arc::new(SimpleUsageAggregator::new(&config, storage.clone()).await?);
        let calculator = Arc::new(CostCalculator::new());

        Ok(Self {
            collector,
            aggregator,
            calculator,
            storage,
            config,
        })
    }

    /// Start the simplified billing service
    pub async fn start(&self) -> Result<()> {
        // Start metrics collection
        self.collector.start_collection().await?;
        
        // Start usage aggregation
        self.aggregator.start_aggregation().await?;

        tracing::info!("Simplified billing service started successfully");
        Ok(())
    }

    /// Stop the simplified billing service
    pub async fn stop(&self) -> Result<()> {
        tracing::info!("Stopping simplified billing service...");
        
        // Stop components in reverse order of startup
        if let Err(e) = self.aggregator.stop_aggregation().await {
            tracing::error!("Error stopping aggregator: {}", e);
        }
        
        if let Err(e) = self.collector.stop_collection().await {
            tracing::error!("Error stopping collector: {}", e);
        }
        
        tracing::info!("Simplified billing service stopped");
        Ok(())
    }

    /// Record a usage metric for a session
    pub async fn record_usage(
        &self, 
        session_id: Uuid, 
        user_id: Uuid, 
        metric_type: MetricType,
        value: Decimal,
        timestamp: Option<DateTime<Utc>>
    ) -> Result<()> {
        let metric = UsageMetric {
            id: Uuid::new_v4(),
            session_id,
            user_id,
            metric_type,
            value,
            timestamp: timestamp.unwrap_or_else(Utc::now),
            metadata: None,
        };

        self.collector.collect_metric(metric).await?;
        Ok(())
    }

    /// Get real-time usage for a session
    pub async fn get_realtime_usage(&self, session_id: Uuid) -> Result<Vec<UsageMetric>> {
        self.storage.get_session_metrics(session_id, None, None).await
    }

    /// Get aggregated usage for a user within a time period
    pub async fn get_usage_summary(
        &self, 
        user_id: Uuid, 
        start_time: DateTime<Utc>, 
        end_time: DateTime<Utc>
    ) -> Result<UsageSummary> {
        self.aggregator.get_usage_summary(user_id, start_time, end_time).await
    }

    /// Calculate cost for usage
    pub async fn calculate_cost(&self, usage: &UsageSummary) -> Result<BillingRecord> {
        self.calculator.calculate_cost(usage).await
    }

    /// Generate invoice for a user
    pub async fn generate_invoice(
        &self, 
        user_id: Uuid, 
        start_time: DateTime<Utc>, 
        end_time: DateTime<Utc>
    ) -> Result<Invoice> {
        let usage_summary = self.get_usage_summary(user_id, start_time, end_time).await?;
        let billing_record = self.calculate_cost(&usage_summary).await?;
        
        let invoice = Invoice {
            id: Uuid::new_v4(),
            user_id,
            billing_record_id: billing_record.id,
            start_time,
            end_time,
            total_amount: billing_record.total_cost,
            status: InvoiceStatus::Draft,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            line_items: vec![], // Will be populated from billing record
        };

        self.storage.store_invoice(&invoice).await?;
        Ok(invoice)
    }

    /// Get service health status
    pub async fn health_check(&self) -> SimpleBillingServiceHealth {
        let collector_stats = self.collector.get_processing_stats().await;
        let aggregator_stats = self.aggregator.get_aggregation_stats().await;
        let billing_metrics = self.collector.get_billing_metrics().await;
        
        SimpleBillingServiceHealth {
            collector_running: collector_stats.is_running,
            aggregator_running: aggregator_stats.is_running,
            metrics_processed: collector_stats.total_processed,
            aggregations_processed: aggregator_stats.processed_count,
            error_count: collector_stats.error_count,
            buffer_size: collector_stats.buffer_size,
            last_collection: billing_metrics.last_updated,
            last_aggregation: aggregator_stats.last_aggregation,
        }
    }
}

impl Drop for SimpleBillingService {
    fn drop(&mut self) {
        tracing::info!("SimpleBillingService dropped - ensure stop() was called for proper cleanup");
    }
}

/// Health status of the simplified billing service
#[derive(Debug, Clone)]
pub struct SimpleBillingServiceHealth {
    pub collector_running: bool,
    pub aggregator_running: bool,
    pub metrics_processed: u64,
    pub aggregations_processed: u64,
    pub error_count: u64,
    pub buffer_size: u64,
    pub last_collection: DateTime<Utc>,
    pub last_aggregation: DateTime<Utc>,
}

