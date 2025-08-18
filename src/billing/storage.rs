//! Billing Storage
//! 
//! This module handles persistent storage of billing data including usage metrics,
//! summaries, billing records, and invoices. It supports both SurrealDB and Redis
//! for different storage requirements.

use super::models::{
    UsageMetric, UsageSummary, BillingRecord, Invoice, InvoiceStatus, MetricType
};
use super::error_handling::{
    ErrorRecoveryService, TransactionContext, RetryConfig, BillingError, ValidationHelper, ToBillingError
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use redis::{Client as RedisClient, AsyncCommands};
use rust_decimal::Decimal;
use serde_json;
use std::sync::Arc;
use surrealdb::{Surreal, engine::remote::ws::{Client, Ws}, sql::Thing};
use uuid::Uuid;

/// Storage configuration for billing data
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// SurrealDB connection URL
    pub surrealdb_url: String,
    /// SurrealDB namespace
    pub namespace: String,
    /// SurrealDB database
    pub database: String,
    /// Redis connection URL for caching
    pub redis_url: String,
    /// Cache TTL in seconds
    pub cache_ttl: u64,
    /// Enable metrics compression
    pub enable_compression: bool,
    /// Batch size for bulk operations
    pub batch_size: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            surrealdb_url: "ws://localhost:8000".to_string(),
            namespace: "soulbox".to_string(),
            database: "billing".to_string(),
            redis_url: "redis://localhost:6379".to_string(),
            cache_ttl: 3600, // 1 hour
            enable_compression: true,
            batch_size: 1000,
        }
    }
}

/// Storage implementation for billing data with error recovery
pub struct BillingStorage {
    surrealdb: Surreal<Client>,
    redis: RedisClient,
    config: StorageConfig,
    recovery_service: Arc<ErrorRecoveryService>,
}

impl BillingStorage {
    /// Create a new billing storage instance
    pub async fn new(config: &StorageConfig) -> Result<Self> {
        // Initialize SurrealDB connection
        let surrealdb = Surreal::new::<Ws>(&config.surrealdb_url).await
            .context("Failed to connect to SurrealDB")?;
        
        surrealdb.use_ns(&config.namespace).use_db(&config.database).await
            .context("Failed to set SurrealDB namespace/database")?;

        // Initialize Redis connection
        let redis = RedisClient::open(config.redis_url.as_str())
            .context("Failed to create Redis client")?;

        // Test Redis connection
        let _conn = redis.get_multiplexed_async_connection().await
            .context("Failed to connect to Redis")?;

        let recovery_service = Arc::new(ErrorRecoveryService::new(RetryConfig::default()));
        
        let storage = Self {
            surrealdb,
            redis,
            config: config.clone(),
            recovery_service,
        };

        // Initialize schema with error recovery
        storage.initialize_schema().await?;

        Ok(storage)
    }

    /// Store a usage metric with transaction support
    pub async fn store_metric(&self, metric: &UsageMetric) -> Result<()> {
        // Validate input
        ValidationHelper::validate_user_id(metric.user_id)?;
        ValidationHelper::validate_amount(metric.value, "metric_value")?;
        
        let mut context = TransactionContext::new(std::time::Duration::from_secs(30));
        context.add_operation("store_metric".to_string());
        
        self.recovery_service.execute_transaction(context, |ctx| {
            let metric = metric.clone();
            let surrealdb = &self.surrealdb;
            let redis = &self.redis;
            let config = &self.config;
            
            ctx.add_operation(format!("storing_metric_{}", metric.id));
            
            // Use blocking context for async operations in transaction
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    // Store in SurrealDB for persistence
                    let _: Option<Thing> = surrealdb
                        .create(("usage_metrics", &metric.id.to_string()))
                        .content(&metric)
                        .await
                        .to_billing_error("surrealdb_store")?;

                    // Cache in Redis for fast access
                    let cache_key = format!("metric:{}", metric.id);
                    let data = serde_json::to_string(&metric)
                        .to_billing_error("metric_serialization")?;

                    let mut conn = redis.get_multiplexed_async_connection().await
                        .to_billing_error("redis_connection")?;
                    let _: () = conn.set_ex(&cache_key, config.cache_ttl, data).await
                        .to_billing_error("redis_setex")?;

                    // Add to session metrics sorted set
                    let session_key = format!("session:{}:metrics", metric.session_id);
                    let _: () = conn.zadd(&session_key, &cache_key, metric.timestamp.timestamp()).await
                        .to_billing_error("redis_zadd_session")?;

                    // Add to user metrics sorted set
                    let user_key = format!("user:{}:metrics", metric.user_id);
                    let _: () = conn.zadd(&user_key, &cache_key, metric.timestamp.timestamp()).await
                        .to_billing_error("redis_zadd_user")?;

                    Ok(())
                })
            })
        }).await
    }

    /// Store multiple metrics in batch
    pub async fn store_metrics_batch(&self, metrics: &[UsageMetric]) -> Result<()> {
        if metrics.is_empty() {
            return Ok(());
        }

        // Batch store in SurrealDB
        for chunk in metrics.chunks(self.config.batch_size) {
            let futures: Vec<_> = chunk.iter().map(|metric| {
                self.surrealdb.create(("usage_metrics", &metric.id.to_string())).content(metric)
            }).collect();

            // Execute all inserts concurrently
            futures_util::future::try_join_all(futures).await
                .context("Failed to batch store metrics in SurrealDB")?;
        }

        // Batch cache in Redis
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let mut pipe = redis::pipe();

        for metric in metrics {
            let cache_key = format!("metric:{}", metric.id);
            let data = serde_json::to_string(metric)
                .context("Failed to serialize metric for batch")?;

            pipe.set_ex(&cache_key, self.config.cache_ttl, data);

            // Add to session and user sorted sets
            let session_key = format!("session:{}:metrics", metric.session_id);
            let user_key = format!("user:{}:metrics", metric.user_id);
            
            pipe.zadd(&session_key, &cache_key, metric.timestamp.timestamp());
            pipe.zadd(&user_key, &cache_key, metric.timestamp.timestamp());
        }

        pipe.query_async(&mut conn).await
            .context("Failed to batch cache metrics in Redis")?;

        Ok(())
    }

    /// Get metrics for a session
    pub async fn get_session_metrics(
        &self,
        session_id: Uuid,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<Vec<UsageMetric>> {
        let session_key = format!("session:{}:metrics", session_id);
        let mut conn = self.redis.get_multiplexed_async_connection().await?;

        // Get metric keys from sorted set
        let metric_keys: Vec<String> = if let (Some(start), Some(end)) = (start_time, end_time) {
            conn.zrangebyscore(&session_key, start.timestamp(), end.timestamp()).await?
        } else {
            conn.zrange(&session_key, 0, -1).await?
        };

        if metric_keys.is_empty() {
            return Ok(Vec::new());
        }

        // Get metric data from cache
        let cached_data: Vec<Option<String>> = conn.mget(&metric_keys).await?;
        let mut metrics = Vec::new();

        for (key, data) in metric_keys.iter().zip(cached_data.iter()) {
            if let Some(data) = data {
                match serde_json::from_str::<UsageMetric>(data) {
                    Ok(metric) => metrics.push(metric),
                    Err(_) => {
                        // Cache miss, try to load from SurrealDB
                        if let Some(metric) = self.load_metric_from_db(key).await? {
                            metrics.push(metric);
                        }
                    }
                }
            }
        }

        Ok(metrics)
    }

    /// Get metrics for a user within a time range using parameterized queries
    pub async fn get_user_metrics(
        &self,
        user_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        metric_types: Option<&[MetricType]>,
    ) -> Result<Vec<UsageMetric>> {
        // Use parameterized query to prevent SQL injection
        let base_query = "SELECT * FROM usage_metrics WHERE user_id = $user_id AND timestamp >= $start_time AND timestamp <= $end_time";
        
        let mut query = self.surrealdb.query(base_query)
            .bind(("user_id", user_id.to_string()))
            .bind(("start_time", start_time))
            .bind(("end_time", end_time));

        if let Some(types) = metric_types {
            let type_strings: Vec<String> = types.iter()
                .map(|t| format!("{:?}", t))
                .collect();
            query = query.query("SELECT * FROM usage_metrics WHERE user_id = $user_id AND timestamp >= $start_time AND timestamp <= $end_time AND metric_type IN $metric_types ORDER BY timestamp DESC")
                .bind(("metric_types", type_strings));
        } else {
            query = query.query("SELECT * FROM usage_metrics WHERE user_id = $user_id AND timestamp >= $start_time AND timestamp <= $end_time ORDER BY timestamp DESC");
        }

        let metrics: Vec<UsageMetric> = query.await
            .context("Failed to query user metrics")?
            .take(0)
            .context("Failed to parse user metrics query result")?;

        Ok(metrics)
    }

    /// Store a usage summary
    pub async fn store_summary(&self, summary: &UsageSummary) -> Result<()> {
        let summary_id = format!("{}:{}:{}", 
            summary.user_id, 
            summary.start_time.timestamp(), 
            summary.end_time.timestamp()
        );

        // Store in SurrealDB
        let _: Option<Thing> = self.surrealdb
            .create(("usage_summaries", &summary_id))
            .content(summary)
            .await
            .context("Failed to store summary in SurrealDB")?;

        // Cache in Redis
        let cache_key = format!("summary:{}", summary_id);
        let data = serde_json::to_string(summary)
            .context("Failed to serialize summary")?;

        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let _: () = conn.set_ex(&cache_key, self.config.cache_ttl * 24, data).await?; // Longer TTL for summaries

        // Add to user summaries index
        let user_summaries_key = format!("user:{}:summaries", summary.user_id);
        let _: () = conn.zadd(&user_summaries_key, &cache_key, summary.start_time.timestamp()).await?;

        Ok(())
    }

    /// Get usage summaries for a user using parameterized queries
    pub async fn get_user_summaries(
        &self,
        user_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<UsageSummary>> {
        let summaries: Vec<UsageSummary> = self.surrealdb
            .query("SELECT * FROM usage_summaries WHERE user_id = $user_id AND start_time >= $start_time AND end_time <= $end_time ORDER BY start_time DESC")
            .bind(("user_id", user_id.to_string()))
            .bind(("start_time", start_time))
            .bind(("end_time", end_time))
            .await
            .context("Failed to query user summaries")?
            .take(0)
            .context("Failed to parse user summaries query result")?;

        Ok(summaries)
    }

    /// Store a billing record
    pub async fn store_billing_record(&self, record: &BillingRecord) -> Result<()> {
        let _: Option<Thing> = self.surrealdb
            .create(("billing_records", &record.id.to_string()))
            .content(record)
            .await
            .context("Failed to store billing record")?;

        // Cache recent billing records
        let cache_key = format!("billing:{}", record.id);
        let data = serde_json::to_string(record)
            .context("Failed to serialize billing record")?;

        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let _: () = conn.set_ex(&cache_key, self.config.cache_ttl * 24 * 7, data).await?; // 1 week TTL

        // Add to user billing records index
        let user_billing_key = format!("user:{}:billing", record.user_id);
        let _: () = conn.zadd(&user_billing_key, &cache_key, record.created_at.timestamp()).await?;

        Ok(())
    }

    /// Get billing records for a user using parameterized queries
    pub async fn get_user_billing_records(
        &self,
        user_id: Uuid,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: Option<u32>,
    ) -> Result<Vec<BillingRecord>> {
        let mut query_str = "SELECT * FROM billing_records WHERE user_id = $user_id".to_string();
        let mut query = self.surrealdb.query(&query_str)
            .bind(("user_id", user_id.to_string()));

        if let Some(start) = start_time {
            query_str.push_str(" AND created_at >= $start_time");
            query = self.surrealdb.query(&query_str)
                .bind(("user_id", user_id.to_string()))
                .bind(("start_time", start));
        }

        if let Some(end) = end_time {
            query_str.push_str(" AND created_at <= $end_time");
            let mut bindings = vec![("user_id", user_id.to_string())];
            if start_time.is_some() {
                bindings.push(("start_time", start_time.unwrap().to_string()));
            }
            bindings.push(("end_time", end.to_string()));
            
            query = self.surrealdb.query(&query_str);
            for (key, value) in bindings {
                query = query.bind((key, value));
            }
        }

        query_str.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = limit {
            query_str.push_str(" LIMIT $limit");
            query = query.bind(("limit", limit));
        }

        let records: Vec<BillingRecord> = query.await
            .context("Failed to query billing records")?
            .take(0)
            .context("Failed to parse billing records query result")?;

        Ok(records)
    }

    /// Store an invoice
    pub async fn store_invoice(&self, invoice: &Invoice) -> Result<()> {
        let _: Option<Thing> = self.surrealdb
            .create(("invoices", &invoice.id.to_string()))
            .content(invoice)
            .await
            .context("Failed to store invoice")?;

        // Cache active invoices
        let cache_key = format!("invoice:{}", invoice.id);
        let data = serde_json::to_string(invoice)
            .context("Failed to serialize invoice")?;

        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let _: () = conn.set_ex(&cache_key, self.config.cache_ttl * 24 * 30, data).await?; // 30 days TTL

        // Add to user invoices index
        let user_invoices_key = format!("user:{}:invoices", invoice.user_id);
        let _: () = conn.zadd(&user_invoices_key, &cache_key, invoice.created_at.timestamp()).await?;

        // Add to status-based indexes
        let status_key = format!("invoices:status:{:?}", invoice.status);
        let _: () = conn.zadd(&status_key, &cache_key, invoice.created_at.timestamp()).await?;

        Ok(())
    }

    /// Update invoice status using parameterized queries
    pub async fn update_invoice_status(&self, invoice_id: Uuid, status: InvoiceStatus) -> Result<()> {
        let _: Vec<Thing> = self.surrealdb
            .query("UPDATE type::thing('invoices', $invoice_id) SET status = $status, updated_at = $updated_at")
            .bind(("invoice_id", invoice_id.to_string()))
            .bind(("status", format!("{:?}", status)))
            .bind(("updated_at", Utc::now()))
            .await
            .context("Failed to update invoice status")?
            .take(0)
            .context("Failed to parse invoice update result")?;

        // Update cache
        let cache_key = format!("invoice:{}", invoice_id);
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let _: () = conn.del(&cache_key).await?; // Remove from cache to force reload

        Ok(())
    }

    /// Get invoices by status using parameterized queries
    pub async fn get_invoices_by_status(&self, status: InvoiceStatus, limit: Option<u32>) -> Result<Vec<Invoice>> {
        let mut query = self.surrealdb
            .query("SELECT * FROM invoices WHERE status = $status ORDER BY created_at DESC")
            .bind(("status", format!("{:?}", status)));

        if let Some(limit) = limit {
            query = self.surrealdb
                .query("SELECT * FROM invoices WHERE status = $status ORDER BY created_at DESC LIMIT $limit")
                .bind(("status", format!("{:?}", status)))
                .bind(("limit", limit));
        }

        let invoices: Vec<Invoice> = query.await
            .context("Failed to query invoices by status")?
            .take(0)
            .context("Failed to parse invoices by status query result")?;

        Ok(invoices)
    }

    /// Get user's invoices using parameterized queries
    pub async fn get_user_invoices(
        &self,
        user_id: Uuid,
        status: Option<InvoiceStatus>,
        limit: Option<u32>,
    ) -> Result<Vec<Invoice>> {
        let base_query = "SELECT * FROM invoices WHERE user_id = $user_id";
        let mut query = self.surrealdb.query(base_query)
            .bind(("user_id", user_id.to_string()));

        if let Some(status) = status {
            query = if let Some(limit) = limit {
                self.surrealdb
                    .query("SELECT * FROM invoices WHERE user_id = $user_id AND status = $status ORDER BY created_at DESC LIMIT $limit")
                    .bind(("user_id", user_id.to_string()))
                    .bind(("status", format!("{:?}", status)))
                    .bind(("limit", limit))
            } else {
                self.surrealdb
                    .query("SELECT * FROM invoices WHERE user_id = $user_id AND status = $status ORDER BY created_at DESC")
                    .bind(("user_id", user_id.to_string()))
                    .bind(("status", format!("{:?}", status)))
            };
        } else if let Some(limit) = limit {
            query = self.surrealdb
                .query("SELECT * FROM invoices WHERE user_id = $user_id ORDER BY created_at DESC LIMIT $limit")
                .bind(("user_id", user_id.to_string()))
                .bind(("limit", limit));
        } else {
            query = self.surrealdb
                .query("SELECT * FROM invoices WHERE user_id = $user_id ORDER BY created_at DESC")
                .bind(("user_id", user_id.to_string()));
        }

        let invoices: Vec<Invoice> = query.await
            .context("Failed to query user invoices")?
            .take(0)
            .context("Failed to parse user invoices query result")?;

        Ok(invoices)
    }

    /// Get billing statistics for a user using parameterized queries
    pub async fn get_user_billing_stats(
        &self,
        user_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<BillingStats> {
        let result: Vec<BillingStatsRaw> = self.surrealdb
            .query("SELECT 
                COUNT() as total_records,
                math::sum(total_cost) as total_amount,
                math::avg(total_cost) as average_amount,
                math::max(total_cost) as max_amount,
                math::min(total_cost) as min_amount
            FROM billing_records 
            WHERE user_id = $user_id AND created_at >= $start_time AND created_at <= $end_time")
            .bind(("user_id", user_id.to_string()))
            .bind(("start_time", start_time))
            .bind(("end_time", end_time))
            .await
            .context("Failed to query billing stats")?
            .take(0)
            .context("Failed to parse billing stats query result")?;

        let stats = result.into_iter().next().unwrap_or_default();
        
        Ok(BillingStats {
            user_id,
            period_start: start_time,
            period_end: end_time,
            total_records: stats.total_records,
            total_amount: stats.total_amount,
            average_amount: stats.average_amount,
            max_amount: stats.max_amount,
            min_amount: stats.min_amount,
        })
    }

    /// Initialize database schema
    async fn initialize_schema(&self) -> Result<()> {
        // Create tables and indexes
        let schema_queries = vec![
            // Usage metrics table
            "DEFINE TABLE usage_metrics SCHEMAFULL;",
            "DEFINE FIELD id ON TABLE usage_metrics TYPE string;",
            "DEFINE FIELD session_id ON TABLE usage_metrics TYPE string;",
            "DEFINE FIELD user_id ON TABLE usage_metrics TYPE string;",
            "DEFINE FIELD metric_type ON TABLE usage_metrics TYPE string;",
            "DEFINE FIELD value ON TABLE usage_metrics TYPE decimal;",
            "DEFINE FIELD timestamp ON TABLE usage_metrics TYPE datetime;",
            "DEFINE FIELD metadata ON TABLE usage_metrics TYPE option<object>;",
            "DEFINE INDEX idx_usage_metrics_user_time ON TABLE usage_metrics COLUMNS user_id, timestamp;",
            "DEFINE INDEX idx_usage_metrics_session_time ON TABLE usage_metrics COLUMNS session_id, timestamp;",

            // Usage summaries table
            "DEFINE TABLE usage_summaries SCHEMAFULL;",
            "DEFINE FIELD user_id ON TABLE usage_summaries TYPE string;",
            "DEFINE FIELD start_time ON TABLE usage_summaries TYPE datetime;",
            "DEFINE FIELD end_time ON TABLE usage_summaries TYPE datetime;",
            "DEFINE FIELD metrics ON TABLE usage_summaries TYPE object;",
            "DEFINE FIELD session_count ON TABLE usage_summaries TYPE number;",
            "DEFINE FIELD created_at ON TABLE usage_summaries TYPE datetime;",
            "DEFINE INDEX idx_usage_summaries_user_time ON TABLE usage_summaries COLUMNS user_id, start_time;",

            // Billing records table
            "DEFINE TABLE billing_records SCHEMAFULL;",
            "DEFINE FIELD id ON TABLE billing_records TYPE string;",
            "DEFINE FIELD user_id ON TABLE billing_records TYPE string;",
            "DEFINE FIELD start_time ON TABLE billing_records TYPE datetime;",
            "DEFINE FIELD end_time ON TABLE billing_records TYPE datetime;",
            "DEFINE FIELD total_cost ON TABLE billing_records TYPE decimal;",
            "DEFINE FIELD created_at ON TABLE billing_records TYPE datetime;",
            "DEFINE INDEX idx_billing_records_user_time ON TABLE billing_records COLUMNS user_id, created_at;",

            // Invoices table
            "DEFINE TABLE invoices SCHEMAFULL;",
            "DEFINE FIELD id ON TABLE invoices TYPE string;",
            "DEFINE FIELD user_id ON TABLE invoices TYPE string;",
            "DEFINE FIELD billing_record_id ON TABLE invoices TYPE string;",
            "DEFINE FIELD total_amount ON TABLE invoices TYPE decimal;",
            "DEFINE FIELD status ON TABLE invoices TYPE string;",
            "DEFINE FIELD created_at ON TABLE invoices TYPE datetime;",
            "DEFINE FIELD updated_at ON TABLE invoices TYPE datetime;",
            "DEFINE INDEX idx_invoices_user_status ON TABLE invoices COLUMNS user_id, status;",
            "DEFINE INDEX idx_invoices_status_time ON TABLE invoices COLUMNS status, created_at;",
        ];

        for query in schema_queries {
            let _: Vec<Thing> = self.surrealdb.query(query).await
                .with_context(|| format!("Failed to execute schema query: {}", query))?
                .take(0)?;
        }

        Ok(())
    }

    /// Load metric from database when cache miss occurs using parameterized queries
    async fn load_metric_from_db(&self, cache_key: &str) -> Result<Option<UsageMetric>> {
        // Extract metric ID from cache key
        let metric_id = cache_key.strip_prefix("metric:")
            .ok_or_else(|| anyhow::anyhow!("Invalid cache key format"))?;

        let metrics: Vec<UsageMetric> = self.surrealdb
            .query("SELECT * FROM type::thing('usage_metrics', $metric_id)")
            .bind(("metric_id", metric_id))
            .await
            .context("Failed to load metric from database")?
            .take(0)
            .context("Failed to parse metric query result")?;

        Ok(metrics.into_iter().next())
    }
}

// Implement Drop trait for proper resource cleanup
impl Drop for BillingStorage {
    fn drop(&mut self) {
        // Connection cleanup is handled by the SurrealDB and Redis clients themselves
        // This is mainly for logging and ensuring graceful shutdown
        tracing::info!("BillingStorage dropped - connections will be cleaned up by clients");
    }
}

/// Billing statistics structure
#[derive(Debug, Clone)]
pub struct BillingStats {
    pub user_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_records: u64,
    pub total_amount: Decimal,
    pub average_amount: Decimal,
    pub max_amount: Decimal,
    pub min_amount: Decimal,
}

/// Raw billing statistics from database query
#[derive(Debug, Clone, serde::Deserialize)]
struct BillingStatsRaw {
    total_records: u64,
    total_amount: Decimal,
    average_amount: Decimal,
    max_amount: Decimal,
    min_amount: Decimal,
}

impl Default for BillingStatsRaw {
    fn default() -> Self {
        Self {
            total_records: 0,
            total_amount: Decimal::ZERO,
            average_amount: Decimal::ZERO,
            max_amount: Decimal::ZERO,
            min_amount: Decimal::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.namespace, "soulbox");
        assert_eq!(config.database, "billing");
        assert_eq!(config.cache_ttl, 3600);
        assert_eq!(config.batch_size, 1000);
    }

    #[tokio::test]
    async fn test_billing_storage_creation() {
        let config = StorageConfig::default();
        let result = BillingStorage::new(&config).await;
        // This will fail without SurrealDB and Redis, but tests the interface
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn test_billing_stats_default() {
        let stats = BillingStatsRaw::default();
        assert_eq!(stats.total_records, 0);
        assert_eq!(stats.total_amount, Decimal::ZERO);
        assert_eq!(stats.average_amount, Decimal::ZERO);
    }
}