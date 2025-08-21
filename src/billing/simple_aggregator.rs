//! Simplified Usage Aggregator
//! 
//! This module implements a simplified usage aggregation system that works
//! directly with SurrealDB without Redis complexity.

use super::models::{UsageMetric, UsageSummary, AggregatedMetric, MetricType};
use super::storage::BillingStorage;
use super::BillingConfig;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc, Duration, DurationRound, Datelike, Timelike};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::Mutex;
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

/// Simplified usage aggregator that works directly with database
pub struct SimpleUsageAggregator {
    storage: Arc<BillingStorage>,
    config: BillingConfig,
    running: Arc<AtomicBool>,
    processed_count: Arc<AtomicU64>,
    last_aggregation: Arc<Mutex<DateTime<Utc>>>,
}

impl SimpleUsageAggregator {
    /// Create a new simplified usage aggregator
    pub async fn new(config: &BillingConfig, storage: Arc<BillingStorage>) -> Result<Self> {
        Ok(Self {
            storage,
            config: config.clone(),
            running: Arc::new(AtomicBool::new(false)),
            processed_count: Arc::new(AtomicU64::new(0)),
            last_aggregation: Arc::new(Mutex::new(Utc::now())),
        })
    }

    /// Start aggregation processing
    pub async fn start_aggregation(&self) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            warn!("Usage aggregation is already running");
            return Ok(());
        }

        info!("Starting simplified usage aggregation");
        self.start_periodic_aggregation().await?;
        info!("Simplified usage aggregation started successfully");
        Ok(())
    }

    /// Stop aggregation processing
    pub async fn stop_aggregation(&self) -> Result<()> {
        info!("Stopping simplified usage aggregation");
        self.running.store(false, Ordering::SeqCst);
        
        // Run final aggregation
        self.run_aggregation().await?;
        
        info!("Simplified usage aggregation stopped");
        Ok(())
    }

    /// Get usage summary for a user within a time period
    pub async fn get_usage_summary(
        &self,
        user_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<UsageSummary> {
        // Try to get from stored summaries first
        let summaries = self.storage.get_user_summaries(user_id, start_time, end_time).await?;
        
        if !summaries.is_empty() {
            // Merge multiple summaries if needed
            return Ok(self.merge_summaries(summaries));
        }

        // If no stored summaries, aggregate from raw metrics
        self.aggregate_metrics_for_period(user_id, start_time, end_time).await
    }

    /// Start periodic aggregation task
    async fn start_periodic_aggregation(&self) -> Result<()> {
        let storage = self.storage.clone();
        let running = self.running.clone();
        let processed_count = self.processed_count.clone();
        let last_aggregation = self.last_aggregation.clone();
        let interval_secs = self.config.collection_interval;

        tokio::spawn(async move {
            let mut interval = interval(TokioDuration::from_secs(interval_secs));
            
            while running.load(Ordering::Relaxed) {
                interval.tick().await;
                
                match Self::run_periodic_aggregation(&storage, &processed_count, &last_aggregation).await {
                    Ok(count) => {
                        if count > 0 {
                            debug!("Processed {} aggregations", count);
                        }
                    }
                    Err(e) => {
                        error!("Error in periodic aggregation: {}", e);
                    }
                }
            }
        });

        Ok(())
    }

    /// Run periodic aggregation
    async fn run_periodic_aggregation(
        storage: &Arc<BillingStorage>,
        processed_count: &Arc<AtomicU64>,
        last_aggregation: &Arc<Mutex<DateTime<Utc>>>,
    ) -> Result<u64> {
        let now = Utc::now();
        let mut last_agg = last_aggregation.lock().await;
        let since = *last_agg;
        *last_agg = now;
        drop(last_agg);

        // Aggregate hourly data for recent periods
        let mut count = 0;
        let mut current_hour = AggregationPeriod::Hourly.round_timestamp(since);
        let end_hour = AggregationPeriod::Hourly.round_timestamp(now);

        while current_hour < end_hour {
            let next_hour = current_hour + AggregationPeriod::Hourly.duration();
            
            // Find all users with metrics in this period
            if let Err(e) = Self::aggregate_hour(storage, current_hour, next_hour).await {
                error!("Failed to aggregate hour {}: {}", current_hour, e);
            } else {
                count += 1;
            }
            
            current_hour = next_hour;
        }

        processed_count.fetch_add(count, Ordering::Relaxed);
        Ok(count)
    }

    /// Aggregate metrics for a specific hour
    async fn aggregate_hour(
        storage: &Arc<BillingStorage>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<()> {
        // This is a simplified approach - in production you might want to
        // query for unique user IDs first, then aggregate each user separately
        
        // For now, we'll skip the automatic aggregation and rely on on-demand aggregation
        // when get_usage_summary is called
        
        debug!("Skipping automatic aggregation for period {} to {}", start_time, end_time);
        Ok(())
    }

    /// Run aggregation now
    async fn run_aggregation(&self) -> Result<()> {
        let mut last_agg = self.last_aggregation.lock().await;
        *last_agg = Utc::now();
        Ok(())
    }

    /// Aggregate raw metrics for a specific period
    async fn aggregate_metrics_for_period(
        &self,
        user_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<UsageSummary> {
        // Get raw metrics from storage
        let metrics = self.storage.get_user_metrics(user_id, start_time, end_time, None).await?;
        
        if metrics.is_empty() {
            return Ok(UsageSummary {
                user_id,
                start_time,
                end_time,
                metrics: HashMap::new(),
                session_count: 0,
                created_at: Utc::now(),
            });
        }

        // Group metrics by type and calculate aggregations
        let mut metric_groups: HashMap<MetricType, Vec<Decimal>> = HashMap::new();
        let mut session_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

        for metric in &metrics {
            session_ids.insert(metric.session_id);
            metric_groups
                .entry(metric.metric_type.clone())
                .or_insert_with(Vec::new)
                .push(metric.value);
        }

        // Calculate aggregated metrics
        let mut aggregated_metrics = HashMap::new();
        for (metric_type, values) in metric_groups {
            if !values.is_empty() {
                let total = values.iter().sum::<Decimal>();
                let count = values.len() as u64;
                let average = if count > 0 {
                    total / Decimal::from(count)
                } else {
                    Decimal::ZERO
                };
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

        let summary = UsageSummary {
            user_id,
            start_time,
            end_time,
            metrics: aggregated_metrics,
            session_count: session_ids.len() as u64,
            created_at: Utc::now(),
        };

        // Store the summary for future use
        if let Err(e) = self.storage.store_summary(&summary).await {
            warn!("Failed to store usage summary: {}", e);
        }

        Ok(summary)
    }

    /// Merge multiple usage summaries into one
    fn merge_summaries(&self, summaries: Vec<UsageSummary>) -> UsageSummary {
        if summaries.is_empty() {
            return UsageSummary {
                user_id: Uuid::new_v4(),
                start_time: Utc::now(),
                end_time: Utc::now(),
                metrics: HashMap::new(),
                session_count: 0,
                created_at: Utc::now(),
            };
        }

        if summaries.len() == 1 {
            return summaries.into_iter().next().unwrap();
        }

        let first = &summaries[0];
        let mut merged_metrics: HashMap<MetricType, AggregatedMetric> = HashMap::new();
        let mut total_sessions = 0;
        let mut start_time = first.start_time;
        let mut end_time = first.end_time;

        for summary in &summaries {
            total_sessions += summary.session_count;
            start_time = start_time.min(summary.start_time);
            end_time = end_time.max(summary.end_time);

            for (metric_type, metric) in &summary.metrics {
                match merged_metrics.get_mut(metric_type) {
                    Some(existing) => {
                        existing.total += metric.total;
                        existing.count += metric.count;
                        existing.average = if existing.count > 0 {
                            existing.total / Decimal::from(existing.count)
                        } else {
                            Decimal::ZERO
                        };
                        existing.maximum = existing.maximum.max(metric.maximum);
                        existing.minimum = existing.minimum.min(metric.minimum);
                    }
                    None => {
                        merged_metrics.insert(metric_type.clone(), metric.clone());
                    }
                }
            }
        }

        UsageSummary {
            user_id: first.user_id,
            start_time,
            end_time,
            metrics: merged_metrics,
            session_count: total_sessions,
            created_at: Utc::now(),
        }
    }

    /// Get aggregation statistics
    pub async fn get_aggregation_stats(&self) -> SimpleAggregationStats {
        let last_agg = self.last_aggregation.lock().await;
        SimpleAggregationStats {
            processed_count: self.processed_count.load(Ordering::Relaxed),
            last_aggregation: *last_agg,
            is_running: self.running.load(Ordering::Relaxed),
        }
    }
}

impl Drop for SimpleUsageAggregator {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        tracing::info!("SimpleUsageAggregator dropped - ensure stop_aggregation() was called");
    }
}

/// Simple aggregation statistics
#[derive(Debug, Clone)]
pub struct SimpleAggregationStats {
    pub processed_count: u64,
    pub last_aggregation: DateTime<Utc>,
    pub is_running: bool,
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
}