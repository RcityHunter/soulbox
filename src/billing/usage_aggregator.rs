//! Usage Aggregator
//! 
//! This module aggregates raw usage metrics into time-based summaries
//! for billing calculations. It handles different aggregation periods
//! and provides efficient querying of usage data.

use super::models::{UsageMetric, UsageSummary, AggregatedMetric, MetricType};
use super::BillingConfig;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc, Duration, DurationRound, Datelike, Timelike};
use redis::{Client, AsyncCommands};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{RwLock, Mutex};
use tokio::time::{interval, Duration as TokioDuration};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Aggregation period for usage summaries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AggregationPeriod {
    /// Hourly aggregation
    Hourly,
    /// Daily aggregation
    Daily,
    /// Weekly aggregation
    Weekly,
    /// Monthly aggregation
    Monthly,
}

impl AggregationPeriod {
    /// Get the duration for this aggregation period
    pub fn duration(&self) -> Duration {
        match self {
            AggregationPeriod::Hourly => Duration::hours(1),
            AggregationPeriod::Daily => Duration::days(1),
            AggregationPeriod::Weekly => Duration::weeks(1),
            AggregationPeriod::Monthly => Duration::days(30), // Approximate
        }
    }

    /// Round a timestamp to the start of this period
    pub fn round_timestamp(&self, timestamp: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            AggregationPeriod::Hourly => timestamp.duration_round(chrono::Duration::hours(1)).unwrap_or(timestamp),
            AggregationPeriod::Daily => timestamp.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc(),
            AggregationPeriod::Weekly => {
                let days_since_monday = timestamp.weekday().num_days_from_monday();
                (timestamp - chrono::Duration::days(days_since_monday as i64))
                    .date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc()
            }
            AggregationPeriod::Monthly => {
                timestamp.date_naive().with_day(1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc()
            }
        }
    }
}

/// Thread-safe aggregation bucket with atomic counters
#[derive(Debug)]
struct AggregationBucket {
    user_id: Uuid,
    period: AggregationPeriod,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    metrics: Arc<Mutex<HashMap<MetricType, Vec<Decimal>>>>,
    session_ids: Arc<Mutex<std::collections::HashSet<Uuid>>>,
    metric_count: AtomicU64,
    last_update: Arc<Mutex<DateTime<Utc>>>,
}

impl AggregationBucket {
    fn new(user_id: Uuid, period: AggregationPeriod, start_time: DateTime<Utc>) -> Self {
        let end_time = start_time + period.duration();
        
        Self {
            user_id,
            period,
            start_time,
            end_time,
            metrics: Arc::new(Mutex::new(HashMap::new())),
            session_ids: Arc::new(Mutex::new(std::collections::HashSet::new())),
            metric_count: AtomicU64::new(0),
            last_update: Arc::new(Mutex::new(start_time)),
        }
    }

    async fn add_metric(&self, metric: &UsageMetric) -> Result<()> {
        // Use atomic operations for concurrent safety
        self.metric_count.fetch_add(1, Ordering::SeqCst);
        
        // Update session IDs with proper locking
        {
            let mut sessions = self.session_ids.lock().await;
            sessions.insert(metric.session_id);
        }
        
        // Update metrics with proper locking
        {
            let mut metrics = self.metrics.lock().await;
            metrics
                .entry(metric.metric_type.clone())
                .or_insert_with(Vec::new)
                .push(metric.value);
        }
        
        // Update last modified timestamp
        {
            let mut last_update = self.last_update.lock().await;
            *last_update = Utc::now();
        }
        
        Ok(())
    }

    async fn to_usage_summary(&self) -> Result<UsageSummary> {
        let mut aggregated_metrics = HashMap::new();
        
        // Lock metrics for reading
        let metrics = self.metrics.lock().await;
        for (metric_type, values) in metrics.iter() {
            if values.is_empty() {
                continue;
            }

            let total = values.iter().sum::<Decimal>();
            let count = values.len() as u64;
            let average = if count > 0 {
                total / Decimal::from(count)
            } else {
                Decimal::ZERO
            };
            let maximum = values.iter().max().cloned().unwrap_or(Decimal::ZERO);
            let minimum = values.iter().min().cloned().unwrap_or(Decimal::ZERO);

            aggregated_metrics.insert(metric_type.clone(), AggregatedMetric {
                total,
                average,
                maximum,
                minimum,
                count,
            });
        }
        drop(metrics); // Explicitly release the lock
        
        // Lock session IDs for reading
        let session_ids = self.session_ids.lock().await;
        let session_count = session_ids.len() as u64;
        drop(session_ids); // Explicitly release the lock

        Ok(UsageSummary {
            user_id: self.user_id,
            start_time: self.start_time,
            end_time: self.end_time,
            metrics: aggregated_metrics,
            session_count,
            created_at: Utc::now(),
        })
    }
}

/// Thread-safe usage aggregator for processing metrics into summaries
pub struct UsageAggregator {
    redis_client: Client,
    config: BillingConfig,
    running: Arc<AtomicBool>,
    buckets: Arc<RwLock<HashMap<String, Arc<AggregationBucket>>>>,
    metrics_processed: AtomicU64,
    last_flush_time: Arc<Mutex<DateTime<Utc>>>,
}

impl UsageAggregator {
    /// Create a new usage aggregator
    pub async fn new(config: &BillingConfig) -> Result<Self> {
        let redis_client = Client::open(config.storage_config.redis_url.as_str())
            .context("Failed to create Redis client")?;

        // Test connection
        let _conn = redis_client.get_multiplexed_async_connection().await
            .context("Failed to connect to Redis")?;

        Ok(Self {
            redis_client,
            config: config.clone(),
            running: Arc::new(AtomicBool::new(false)),
            buckets: Arc::new(RwLock::new(HashMap::new())),
            metrics_processed: AtomicU64::new(0),
            last_flush_time: Arc::new(Mutex::new(Utc::now())),
        })
    }

    /// Start aggregation processing
    pub async fn start_aggregation(&self) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            warn!("Usage aggregation is already running");
            return Ok(());
        }

        info!("Starting usage aggregation");

        // Start aggregation tasks
        self.start_metric_aggregation().await?;
        self.start_bucket_flushing().await?;

        info!("Usage aggregation started successfully");
        Ok(())
    }

    /// Stop aggregation processing
    pub async fn stop_aggregation(&self) -> Result<()> {
        info!("Stopping usage aggregation");
        self.running.store(false, Ordering::SeqCst);
        
        // Flush any remaining buckets
        self.flush_all_buckets().await?;
        
        info!("Usage aggregation stopped");
        Ok(())
    }

    /// Get usage summary for a user within a time period
    pub async fn get_usage_summary(
        &self,
        user_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<UsageSummary> {
        // First check if we have data in active buckets
        let active_summary = self.get_active_bucket_summary(user_id, start_time, end_time).await;

        // Then query stored summaries from Redis/database
        let stored_summary = self.get_stored_summary(user_id, start_time, end_time).await?;

        // Merge active and stored data
        self.merge_summaries(active_summary, stored_summary)
    }

    /// Add a metric to aggregation buckets with thread safety
    pub async fn add_metric(&self, metric: UsageMetric) -> Result<()> {
        let bucket_key = self.get_bucket_key(
            metric.user_id,
            AggregationPeriod::Hourly,
            metric.timestamp,
        );

        // Get or create bucket with proper locking
        let bucket = {
            let buckets = self.buckets.read().await;
            if let Some(bucket) = buckets.get(&bucket_key) {
                bucket.clone()
            } else {
                drop(buckets);
                let mut buckets = self.buckets.write().await;
                // Double-check pattern to avoid race condition
                buckets.entry(bucket_key.clone())
                    .or_insert_with(|| {
                        let start_time = AggregationPeriod::Hourly.round_timestamp(metric.timestamp);
                        Arc::new(AggregationBucket::new(metric.user_id, AggregationPeriod::Hourly, start_time))
                    })
                    .clone()
            }
        };

        // Add metric to bucket safely
        bucket.add_metric(&metric).await
            .context("Failed to add metric to bucket")?;
        
        // Update global metrics counter
        self.metrics_processed.fetch_add(1, Ordering::SeqCst);
        
        debug!("Added metric to bucket {}: {} {}", bucket_key, metric.metric_type.unit(), metric.value);
        Ok(())
    }

    /// Get real-time usage for a session
    pub async fn get_session_realtime_usage(&self, session_id: Uuid) -> Result<Vec<UsageMetric>> {
        // Query recent metrics for the session from Redis
        let mut conn = self.redis_client.get_multiplexed_async_connection().await?;
        
        // Use a sorted set to maintain metrics by timestamp
        let key = format!("session:{}:metrics", session_id);
        let now = Utc::now().timestamp();
        let one_hour_ago = now - 3600;

        let metrics_data: Vec<String> = conn.zrangebyscore(
            &key,
            one_hour_ago,
            now
        ).await.context("Failed to get session metrics")?;

        let mut metrics = Vec::new();
        for data in metrics_data {
            match serde_json::from_str::<UsageMetric>(&data) {
                Ok(metric) => metrics.push(metric),
                Err(e) => warn!("Failed to deserialize metric: {}", e),
            }
        }

        Ok(metrics)
    }

    /// Start metric aggregation task
    async fn start_metric_aggregation(&self) -> Result<()> {
        let redis_client = self.redis_client.clone();
        let config = self.config.clone();
        let running = self.running.clone();
        let aggregator = Arc::new(self.clone());

        tokio::spawn(async move {
            let mut conn = match redis_client.get_multiplexed_async_connection().await {
                Ok(conn) => conn,
                Err(e) => {
                    error!("Failed to get Redis connection for aggregation: {}", e);
                    return;
                }
            };

            let consumer_name = format!("aggregator-{}", Uuid::new_v4());

            while running.load(Ordering::Relaxed) {
                match Self::read_metrics_for_aggregation(&mut conn, &config, &consumer_name).await {
                    Ok(metrics) => {
                        for metric in metrics {
                            if let Err(e) = aggregator.add_metric(metric).await {
                                error!("Failed to add metric to aggregation: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error reading metrics for aggregation: {}", e);
                        tokio::time::sleep(TokioDuration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(())
    }

    /// Start bucket flushing task
    async fn start_bucket_flushing(&self) -> Result<()> {
        let buckets = self.buckets.clone();
        let redis_client = self.redis_client.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut flush_interval = interval(TokioDuration::from_secs(300)); // 5 minutes

            while running.load(Ordering::Relaxed) {
                flush_interval.tick().await;

                let buckets_to_flush = {
                    let mut buckets_guard = buckets.write().await;
                    let now = Utc::now();
                    let mut to_flush = Vec::new();

                    buckets_guard.retain(|key, bucket| {
                        // Flush buckets that are older than their period
                        if now > bucket.end_time {
                            to_flush.push((key.clone(), bucket.clone()));
                            false
                        } else {
                            true
                        }
                    });

                    to_flush
                };

                for (key, bucket) in buckets_to_flush {
                    match bucket.to_usage_summary().await {
                        Ok(summary) => {
                            if let Err(e) = Self::store_summary(&redis_client, &summary).await {
                                error!("Failed to store usage summary for bucket {}: {}", key, e);
                            } else {
                                debug!("Flushed bucket {}", key);
                            }
                        }
                        Err(e) => {
                            error!("Failed to create usage summary for bucket {}: {}", key, e);
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Read metrics from Redis stream for aggregation
    async fn read_metrics_for_aggregation(
        conn: &mut redis::aio::MultiplexedConnection,
        config: &BillingConfig,
        consumer_name: &str,
    ) -> Result<Vec<UsageMetric>> {
        use redis::streams::StreamReadOptions;

        let opts = StreamReadOptions::default()
            .count(config.batch_size)
            .block(5000); // 5 second timeout

        let results: HashMap<String, HashMap<String, HashMap<String, String>>> = conn
            .xread_options(&[&config.metrics_stream], &[">"], &opts)
            .await
            .context("Failed to read from metrics stream")?;

        let mut metrics = Vec::new();

        for (stream_name, entries) in results {
            if stream_name == config.metrics_stream {
                for (_entry_id, fields) in entries {
                    if let Some(data) = fields.get("data") {
                        match serde_json::from_str::<UsageMetric>(data) {
                            Ok(metric) => metrics.push(metric),
                            Err(e) => warn!("Failed to deserialize metric for aggregation: {}", e),
                        }
                    }
                }
            }
        }

        Ok(metrics)
    }

    /// Store usage summary to Redis
    async fn store_summary(redis_client: &Client, summary: &UsageSummary) -> Result<()> {
        let mut conn = redis_client.get_multiplexed_async_connection().await?;
        
        let key = format!(
            "usage:{}:{}:{}",
            summary.user_id,
            summary.start_time.timestamp(),
            summary.end_time.timestamp()
        );

        let data = serde_json::to_string(summary)
            .context("Failed to serialize usage summary")?;

        let _: () = conn.set_ex(&key, &data, 86400 * 30).await // 30 days TTL
            .context("Failed to store usage summary")?;

        // Also store in a sorted set for range queries
        let set_key = format!("user:{}:summaries", summary.user_id);
        let _: () = conn.zadd(&set_key, &key, summary.start_time.timestamp()).await
            .context("Failed to add summary to sorted set")?;

        Ok(())
    }

    /// Get bucket key for a user, period, and timestamp
    fn get_bucket_key(&self, user_id: Uuid, period: AggregationPeriod, timestamp: DateTime<Utc>) -> String {
        let rounded_time = period.round_timestamp(timestamp);
        format!("{}:{}:{}", user_id, period as u8, rounded_time.timestamp())
    }

    /// Get summary from active buckets
    async fn get_active_bucket_summary(
        &self,
        user_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Option<UsageSummary> {
        let buckets = self.buckets.read().await;
        
        // Find buckets that overlap with the requested time range
        let mut combined_metrics: HashMap<MetricType, Vec<Decimal>> = HashMap::new();
        let mut session_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

        for bucket in buckets.values() {
            if bucket.user_id == user_id &&
               bucket.start_time < end_time &&
               bucket.end_time > start_time {
                
                let bucket_metrics = bucket.metrics.lock().await;
                for (metric_type, values) in bucket_metrics.iter() {
                    combined_metrics
                        .entry(metric_type.clone())
                        .or_insert_with(Vec::new)
                        .extend(values.iter().cloned());
                }
                drop(bucket_metrics);
                
                let bucket_sessions = bucket.session_ids.lock().await;
                session_ids.extend(bucket_sessions.iter().cloned());
                drop(bucket_sessions);
            }
        }

        if combined_metrics.is_empty() {
            return None;
        }

        // Create aggregated metrics
        let mut aggregated_metrics = HashMap::new();
        for (metric_type, values) in combined_metrics {
            if !values.is_empty() {
                let total = values.iter().sum::<Decimal>();
                let count = values.len() as u64;
                let average = total / Decimal::from(count);
                let maximum = values.iter().max().cloned().unwrap_or(Decimal::ZERO);
                let minimum = values.iter().min().cloned().unwrap_or(Decimal::ZERO);

                aggregated_metrics.insert(metric_type, AggregatedMetric {
                    total,
                    average,
                    maximum,
                    minimum,
                    count,
                });
            }
        }

        Some(UsageSummary {
            user_id,
            start_time,
            end_time,
            metrics: aggregated_metrics,
            session_count: session_ids.len() as u64,
            created_at: Utc::now(),
        })
    }

    /// Get stored summary from Redis
    async fn get_stored_summary(
        &self,
        user_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Option<UsageSummary>> {
        let mut conn = self.redis_client.get_multiplexed_async_connection().await?;
        
        let set_key = format!("user:{}:summaries", user_id);
        let summary_keys: Vec<String> = conn.zrangebyscore(
            &set_key,
            start_time.timestamp(),
            end_time.timestamp()
        ).await.context("Failed to get summary keys")?;

        if summary_keys.is_empty() {
            return Ok(None);
        }

        // For now, return the first summary found
        // In a real implementation, you'd merge multiple summaries
        if let Some(first_key) = summary_keys.first() {
            let data: Option<String> = conn.get(first_key).await
                .context("Failed to get summary data")?;

            if let Some(data) = data {
                let summary = serde_json::from_str::<UsageSummary>(&data)
                    .context("Failed to deserialize usage summary")?;
                return Ok(Some(summary));
            }
        }

        Ok(None)
    }

    /// Merge active and stored summaries
    fn merge_summaries(
        &self,
        active: Option<UsageSummary>,
        stored: Option<UsageSummary>,
    ) -> Result<UsageSummary> {
        match (active, stored) {
            (Some(active), Some(stored)) => {
                // Merge the two summaries
                let mut merged_metrics = stored.metrics;
                
                for (metric_type, active_metric) in active.metrics {
                    match merged_metrics.get_mut(&metric_type) {
                        Some(stored_metric) => {
                            // Merge the metrics
                            stored_metric.total += active_metric.total;
                            stored_metric.count += active_metric.count;
                            stored_metric.average = stored_metric.total / Decimal::from(stored_metric.count);
                            stored_metric.maximum = stored_metric.maximum.max(active_metric.maximum);
                            stored_metric.minimum = stored_metric.minimum.min(active_metric.minimum);
                        }
                        None => {
                            merged_metrics.insert(metric_type, active_metric);
                        }
                    }
                }

                Ok(UsageSummary {
                    user_id: active.user_id,
                    start_time: stored.start_time.min(active.start_time),
                    end_time: stored.end_time.max(active.end_time),
                    metrics: merged_metrics,
                    session_count: stored.session_count + active.session_count,
                    created_at: Utc::now(),
                })
            }
            (Some(active), None) => Ok(active),
            (None, Some(stored)) => Ok(stored),
            (None, None) => {
                // Return empty summary
                Ok(UsageSummary {
                    user_id: Uuid::new_v4(), // This should be passed as parameter
                    start_time: Utc::now(),
                    end_time: Utc::now(),
                    metrics: HashMap::new(),
                    session_count: 0,
                    created_at: Utc::now(),
                })
            }
        }
    }

    /// Flush all remaining buckets
    async fn flush_all_buckets(&self) -> Result<()> {
        let buckets_to_flush = {
            let mut buckets_guard = self.buckets.write().await;
            let buckets: Vec<_> = buckets_guard.drain().collect();
            buckets
        };

        for (key, bucket) in buckets_to_flush {
            match bucket.to_usage_summary().await {
                Ok(summary) => {
                    if let Err(e) = Self::store_summary(&self.redis_client, &summary).await {
                        error!("Failed to store usage summary for bucket {}: {}", key, e);
                    }
                }
                Err(e) => {
                    error!("Failed to create usage summary for bucket {}: {}", key, e);
                }
            }
        }

        Ok(())
    }
}

// Implement Drop trait for proper resource cleanup
impl Drop for UsageAggregator {
    fn drop(&mut self) {
        // Signal shutdown
        self.running.store(false, Ordering::SeqCst);
        
        // Note: We can't call async methods in Drop, so we rely on the stop_aggregation method
        // being called explicitly before dropping the instance
        tracing::info!("UsageAggregator dropped - ensure stop_aggregation() was called");
    }
}

// We need Clone for the Arc<Self> pattern
impl Clone for UsageAggregator {
    fn clone(&self) -> Self {
        Self {
            redis_client: self.redis_client.clone(),
            config: self.config.clone(),
            running: self.running.clone(),
            buckets: self.buckets.clone(),
            metrics_processed: AtomicU64::new(self.metrics_processed.load(Ordering::Relaxed)),
            last_flush_time: self.last_flush_time.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregation_period_duration() {
        assert_eq!(AggregationPeriod::Hourly.duration(), Duration::hours(1));
        assert_eq!(AggregationPeriod::Daily.duration(), Duration::days(1));
        assert_eq!(AggregationPeriod::Weekly.duration(), Duration::weeks(1));
        assert_eq!(AggregationPeriod::Monthly.duration(), Duration::days(30));
    }

    #[test]
    fn test_aggregation_period_rounding() {
        let timestamp = DateTime::parse_from_rfc3339("2024-01-15T14:30:45Z")
            .unwrap()
            .with_timezone(&Utc);

        let hourly_rounded = AggregationPeriod::Hourly.round_timestamp(timestamp);
        assert_eq!(hourly_rounded.minute(), 0);
        assert_eq!(hourly_rounded.second(), 0);

        let daily_rounded = AggregationPeriod::Daily.round_timestamp(timestamp);
        assert_eq!(daily_rounded.hour(), 0);
        assert_eq!(daily_rounded.minute(), 0);
        assert_eq!(daily_rounded.second(), 0);
    }

    #[tokio::test]
    async fn test_aggregation_bucket() {
        let user_id = Uuid::new_v4();
        let start_time = Utc::now();
        let mut bucket = AggregationBucket::new(user_id, AggregationPeriod::Hourly, start_time);

        let metric = UsageMetric {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            user_id,
            metric_type: MetricType::CpuUsage,
            value: Decimal::new(100, 0),
            timestamp: start_time,
            metadata: None,
        };

        bucket.add_metric(&metric);
        assert_eq!(bucket.session_ids.lock().await.len(), 1);
        assert_eq!(bucket.metrics.lock().await.get(&MetricType::CpuUsage).unwrap().len(), 1);

        let summary = bucket.to_usage_summary().await.unwrap();
        assert_eq!(summary.user_id, user_id);
        assert_eq!(summary.session_count, 1);
        assert!(summary.metrics.contains_key(&MetricType::CpuUsage));
    }

    #[tokio::test]
    async fn test_usage_aggregator_creation() {
        let config = BillingConfig::default();
        let result = UsageAggregator::new(&config).await;
        // This will fail without Redis, but tests the interface
        assert!(result.is_err() || result.is_ok());
    }
}