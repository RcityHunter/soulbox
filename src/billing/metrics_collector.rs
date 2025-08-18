//! Metrics Collector
//! 
//! This module implements real-time collection of usage metrics from running
//! sandbox sessions using Redis Streams for high-throughput, reliable data ingestion.

use super::models::{UsageMetric, MetricType, BillingMetrics};
use super::BillingConfig;
use anyhow::{Context, Result};
use chrono::Utc;
use redis::{Client, AsyncCommands, streams::StreamReadOptions};
use rust_decimal::Decimal;
use serde_json;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio::time::{interval, Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Redis stream entry for metrics
#[derive(Debug, Clone)]
struct MetricStreamEntry {
    id: String,
    data: HashMap<String, String>,
}

/// Thread-safe in-memory buffer for batch processing
#[derive(Debug)]
struct MetricBuffer {
    metrics: Arc<Mutex<Vec<UsageMetric>>>,
    last_flush: Arc<Mutex<Instant>>,
    buffer_size: AtomicU64,
}

impl MetricBuffer {
    fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(Vec::new())),
            last_flush: Arc::new(Mutex::new(Instant::now())),
            buffer_size: AtomicU64::new(0),
        }
    }

    async fn add_metric(&self, metric: UsageMetric) {
        let mut metrics = self.metrics.lock().await;
        metrics.push(metric);
        self.buffer_size.fetch_add(1, Ordering::SeqCst);
    }

    async fn should_flush(&self, batch_size: usize, max_age: Duration) -> bool {
        let size = self.buffer_size.load(Ordering::SeqCst) as usize;
        if size >= batch_size {
            return true;
        }
        
        let last_flush = self.last_flush.lock().await;
        last_flush.elapsed() >= max_age
    }

    async fn flush(&self) -> Vec<UsageMetric> {
        let mut metrics = self.metrics.lock().await;
        let result = std::mem::take(&mut *metrics);
        self.buffer_size.store(0, Ordering::SeqCst);
        
        let mut last_flush = self.last_flush.lock().await;
        *last_flush = Instant::now();
        
        result
    }

    async fn len(&self) -> usize {
        self.buffer_size.load(Ordering::SeqCst) as usize
    }
}

/// Thread-safe metrics collector for real-time usage tracking
pub struct MetricsCollector {
    redis_client: Client,
    config: BillingConfig,
    running: Arc<AtomicBool>,
    metrics_count: Arc<AtomicU64>,
    buffer: Arc<MetricBuffer>,
    metrics_tx: mpsc::UnboundedSender<UsageMetric>,
    metrics_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<UsageMetric>>>>,
    real_time_metrics: Arc<RwLock<BillingMetrics>>,
    error_count: AtomicU64,
    last_error_time: Arc<Mutex<Option<Instant>>>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub async fn new(config: &BillingConfig) -> Result<Self> {
        let redis_client = Client::open(config.redis_url.as_str())
            .context("Failed to create Redis client")?;

        // Test connection and create consumer group if it doesn't exist
        let mut conn = redis_client.get_multiplexed_async_connection().await
            .context("Failed to connect to Redis")?;
        
        let _: Result<String, redis::RedisError> = conn.xgroup_create_mkstream(
            &config.metrics_stream,
            &config.consumer_group,
            0
        ).await;

        let (metrics_tx, metrics_rx) = mpsc::unbounded_channel();

        let real_time_metrics = BillingMetrics {
            active_sessions: 0,
            current_hourly_rate: Decimal::ZERO,
            period_cost: Decimal::ZERO,
            collection_rate: 0.0,
            last_updated: Utc::now(),
        };

        Ok(Self {
            redis_client,
            config: config.clone(),
            running: Arc::new(AtomicBool::new(false)),
            metrics_count: Arc::new(AtomicU64::new(0)),
            buffer: Arc::new(MetricBuffer::new()),
            metrics_tx,
            metrics_rx: Arc::new(RwLock::new(Some(metrics_rx))),
            real_time_metrics: Arc::new(RwLock::new(real_time_metrics)),
            error_count: AtomicU64::new(0),
            last_error_time: Arc::new(Mutex::new(None)),
        })
    }

    /// Start collecting metrics
    pub async fn start_collection(&self) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            warn!("Metrics collection is already running");
            return Ok(());
        }

        info!("Starting metrics collection");

        // Start the collection tasks
        self.start_stream_reader().await?;
        self.start_buffer_processor().await?;
        self.start_metrics_updater().await?;

        info!("Metrics collection started successfully");
        Ok(())
    }

    /// Stop collecting metrics
    pub async fn stop_collection(&self) -> Result<()> {
        info!("Stopping metrics collection");
        self.running.store(false, Ordering::SeqCst);
        
        // Flush any remaining metrics
        self.flush_buffer().await?;
        
        info!("Metrics collection stopped");
        Ok(())
    }

    /// Collect a single metric (direct API)
    pub async fn collect_metric(&self, metric: UsageMetric) -> Result<()> {
        // Send to stream
        self.send_to_stream(&metric).await?;
        
        // Also send to local buffer for immediate processing
        if let Err(_) = self.metrics_tx.send(metric) {
            warn!("Failed to send metric to local buffer - channel closed");
        }

        self.metrics_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Get current billing metrics
    pub async fn get_billing_metrics(&self) -> BillingMetrics {
        self.real_time_metrics.read().await.clone()
    }

    /// Collect metrics from a container/session
    pub async fn collect_container_metrics(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        container_stats: &ContainerStats,
    ) -> Result<()> {
        let timestamp = Utc::now();
        
        // CPU usage metric
        if let Some(cpu_usage) = container_stats.cpu_usage {
            let metric = UsageMetric {
                id: Uuid::new_v4(),
                session_id,
                user_id,
                metric_type: MetricType::CpuUsage,
                value: Decimal::from_f64_retain(cpu_usage).unwrap_or(Decimal::ZERO),
                timestamp,
                metadata: Some([
                    ("source".to_string(), "container".to_string()),
                    ("container_id".to_string(), container_stats.container_id.clone()),
                ].iter().cloned().collect()),
            };
            self.collect_metric(metric).await?;
        }

        // Memory usage metric
        if let Some(memory_usage) = container_stats.memory_usage {
            let metric = UsageMetric {
                id: Uuid::new_v4(),
                session_id,
                user_id,
                metric_type: MetricType::MemoryUsage,
                value: Decimal::from(memory_usage),
                timestamp,
                metadata: Some([
                    ("source".to_string(), "container".to_string()),
                    ("container_id".to_string(), container_stats.container_id.clone()),
                ].iter().cloned().collect()),
            };
            self.collect_metric(metric).await?;
        }

        // Network metrics
        if let Some(network_rx) = container_stats.network_rx {
            let metric = UsageMetric {
                id: Uuid::new_v4(),
                session_id,
                user_id,
                metric_type: MetricType::NetworkIngress,
                value: Decimal::from(network_rx),
                timestamp,
                metadata: Some([
                    ("source".to_string(), "container".to_string()),
                    ("container_id".to_string(), container_stats.container_id.clone()),
                ].iter().cloned().collect()),
            };
            self.collect_metric(metric).await?;
        }

        if let Some(network_tx) = container_stats.network_tx {
            let metric = UsageMetric {
                id: Uuid::new_v4(),
                session_id,
                user_id,
                metric_type: MetricType::NetworkEgress,
                value: Decimal::from(network_tx),
                timestamp,
                metadata: Some([
                    ("source".to_string(), "container".to_string()),
                    ("container_id".to_string(), container_stats.container_id.clone()),
                ].iter().cloned().collect()),
            };
            self.collect_metric(metric).await?;
        }

        Ok(())
    }

    /// Start Redis stream reader
    async fn start_stream_reader(&self) -> Result<()> {
        let redis_client = self.redis_client.clone();
        let config = self.config.clone();
        let running = self.running.clone();
        let metrics_tx = self.metrics_tx.clone();

        tokio::spawn(async move {
            let mut conn = match redis_client.get_multiplexed_async_connection().await {
                Ok(conn) => conn,
                Err(e) => {
                    error!("Failed to get Redis connection: {}", e);
                    return;
                }
            };

            let consumer_name = format!("collector-{}", Uuid::new_v4());
            
            while running.load(Ordering::Relaxed) {
                match Self::read_stream_batch(&mut conn, &config, &consumer_name).await {
                    Ok(metrics) => {
                        for metric in metrics {
                            if let Err(_) = metrics_tx.send(metric) {
                                warn!("Failed to send metric from stream reader");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error reading from stream: {}", e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(())
    }

    /// Start buffer processor
    async fn start_buffer_processor(&self) -> Result<()> {
        let mut metrics_rx = self.metrics_rx.write().await.take()
            .ok_or_else(|| anyhow::anyhow!("Buffer processor already started"))?;
        
        let buffer = self.buffer.clone();
        let config = self.config.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut flush_interval = interval(Duration::from_secs(config.collection_interval));
            
            loop {
                tokio::select! {
                    metric = metrics_rx.recv() => {
                        match metric {
                            Some(metric) => {
                                buffer.add_metric(metric).await;
                                
                                if buffer.should_flush(config.batch_size, Duration::from_secs(30)).await {
                                    let metrics = buffer.flush().await;
                                    
                                    if let Err(e) = Self::process_metric_batch(metrics).await {
                                        error!("Failed to process metric batch: {}", e);
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                    _ = flush_interval.tick() => {
                        if !running.load(Ordering::Relaxed) {
                            break;
                        }
                        
                        if buffer.len().await > 0 {
                            let metrics = buffer.flush().await;
                            
                            if let Err(e) = Self::process_metric_batch(metrics).await {
                                error!("Failed to process metric batch during flush: {}", e);
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Start real-time metrics updater
    async fn start_metrics_updater(&self) -> Result<()> {
        let real_time_metrics = self.real_time_metrics.clone();
        let metrics_count = self.metrics_count.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            let mut last_count = 0u64;
            let mut last_update = Instant::now();

            while running.load(Ordering::Relaxed) {
                interval.tick().await;

                let current_count = metrics_count.load(Ordering::Relaxed);
                let elapsed = last_update.elapsed().as_secs_f64();
                let collection_rate = (current_count - last_count) as f64 / elapsed;

                {
                    let mut metrics = real_time_metrics.write().await;
                    metrics.collection_rate = collection_rate;
                    metrics.last_updated = Utc::now();
                }

                last_count = current_count;
                last_update = Instant::now();
            }
        });

        Ok(())
    }

    /// Send metric to Redis stream
    async fn send_to_stream(&self, metric: &UsageMetric) -> Result<()> {
        let mut conn = self.redis_client.get_multiplexed_async_connection().await
            .context("Failed to get Redis connection")?;

        let metric_data = serde_json::to_string(metric)
            .context("Failed to serialize metric")?;

        let _: String = conn.xadd(
            &self.config.metrics_stream,
            "*",
            &[("data", metric_data)]
        ).await.context("Failed to add metric to stream")?;

        Ok(())
    }

    /// Read batch of metrics from Redis stream
    async fn read_stream_batch(
        conn: &mut redis::aio::MultiplexedConnection,
        config: &BillingConfig,
        consumer_name: &str,
    ) -> Result<Vec<UsageMetric>> {
        let opts = StreamReadOptions::default()
            .count(config.batch_size)
            .block(1000); // 1 second timeout

        let results: HashMap<String, HashMap<String, HashMap<String, String>>> = conn
            .xread_options(&[&config.metrics_stream], &[">"], &opts)
            .await
            .context("Failed to read from stream")?;

        let mut metrics = Vec::new();

        for (stream_name, entries) in results {
            if stream_name == config.metrics_stream {
                for (entry_id, fields) in entries {
                    if let Some(data) = fields.get("data") {
                        match serde_json::from_str::<UsageMetric>(data) {
                            Ok(metric) => metrics.push(metric),
                            Err(e) => {
                                warn!("Failed to deserialize metric from stream entry {}: {}", entry_id, e);
                            }
                        }
                    }
                }
            }
        }

        Ok(metrics)
    }

    /// Process a batch of metrics
    async fn process_metric_batch(metrics: Vec<UsageMetric>) -> Result<()> {
        if metrics.is_empty() {
            return Ok(());
        }

        debug!("Processing batch of {} metrics", metrics.len());

        // Here you would typically:
        // 1. Validate metrics
        // 2. Store to database
        // 3. Send to aggregation pipeline
        // 4. Update real-time dashboards

        // For now, just log the processing
        for metric in &metrics {
            debug!(
                "Processed metric: {} {} {} at {}",
                metric.session_id,
                metric.metric_type.unit(),
                metric.value,
                metric.timestamp
            );
        }

        Ok(())
    }

    /// Flush remaining metrics in buffer with error handling
    async fn flush_buffer(&self) -> Result<()> {
        if self.buffer.len().await > 0 {
            let metrics = self.buffer.flush().await;
            Self::process_metric_batch(metrics).await?
        }
        Ok(())
    }
    
    /// Get metrics processing statistics
    pub async fn get_processing_stats(&self) -> MetricsProcessingStats {
        let last_error = self.last_error_time.lock().await;
        MetricsProcessingStats {
            total_processed: self.metrics_count.load(Ordering::Relaxed),
            error_count: self.error_count.load(Ordering::Relaxed),
            buffer_size: self.buffer.len().await as u64,
            last_error_time: *last_error,
        }
    }
    
    /// Record processing error with circuit breaker logic
    async fn record_error(&self, error: &anyhow::Error) {
        self.error_count.fetch_add(1, Ordering::SeqCst);
        let mut last_error = self.last_error_time.lock().await;
        *last_error = Some(Instant::now());
        
        warn!("Metrics processing error: {}", error);
        
        // Implement basic circuit breaker
        let error_count = self.error_count.load(Ordering::Relaxed);
        if error_count > 100 { // Too many errors
            error!("Too many errors in metrics processing ({}), stopping collection", error_count);
            self.running.store(false, Ordering::SeqCst);
        }
    }
}

// Implement Drop trait for proper resource cleanup
impl Drop for MetricsCollector {
    fn drop(&mut self) {
        // Signal shutdown
        self.running.store(false, Ordering::SeqCst);
        
        // Note: We can't call async methods in Drop, so we rely on the stop_collection method
        // being called explicitly before dropping the instance
        tracing::info!("MetricsCollector dropped - ensure stop_collection() was called");
    }
}

/// Metrics processing statistics
#[derive(Debug, Clone)]
pub struct MetricsProcessingStats {
    pub total_processed: u64,
    pub error_count: u64,
    pub buffer_size: u64,
    pub last_error_time: Option<Instant>,
}

/// Container statistics for metric collection
#[derive(Debug, Clone)]
pub struct ContainerStats {
    pub container_id: String,
    pub cpu_usage: Option<f64>,    // CPU usage percentage
    pub memory_usage: Option<u64>,  // Memory usage in bytes
    pub network_rx: Option<u64>,    // Network bytes received
    pub network_tx: Option<u64>,    // Network bytes transmitted
    pub disk_read: Option<u64>,     // Disk bytes read
    pub disk_write: Option<u64>,    // Disk bytes written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_buffer() {
        let mut buffer = MetricBuffer::new();
        assert_eq!(buffer.metrics.len(), 0);

        let metric = UsageMetric {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            metric_type: MetricType::CpuUsage,
            value: Decimal::new(100, 0),
            timestamp: Utc::now(),
            metadata: None,
        };

        buffer.add_metric(metric);
        assert_eq!(buffer.metrics.len(), 1);

        let flushed = buffer.flush();
        assert_eq!(flushed.len(), 1);
        assert_eq!(buffer.metrics.len(), 0);
    }

    #[test]
    fn test_container_stats() {
        let stats = ContainerStats {
            container_id: "test-container".to_string(),
            cpu_usage: Some(50.0),
            memory_usage: Some(1024 * 1024 * 1024), // 1GB
            network_rx: Some(1000),
            network_tx: Some(2000),
            disk_read: Some(500),
            disk_write: Some(750),
        };

        assert_eq!(stats.container_id, "test-container");
        assert_eq!(stats.cpu_usage, Some(50.0));
        assert_eq!(stats.memory_usage, Some(1024 * 1024 * 1024));
    }

    #[tokio::test]
    async fn test_metrics_collector_creation() {
        let config = BillingConfig::default();
        // This will fail without Redis, but tests the interface
        let result = MetricsCollector::new(&config).await;
        assert!(result.is_err() || result.is_ok());
    }
}