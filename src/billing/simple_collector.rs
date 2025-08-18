//! Simplified Metrics Collector
//! 
//! This module implements a simplified metrics collection system that writes
//! directly to SurrealDB without the complexity of Redis Streams.

use super::models::{UsageMetric, MetricType, BillingMetrics};
use super::storage::BillingStorage;
use super::error_handling::{ErrorRecoveryService, RetryConfig};
use super::BillingConfig;
use anyhow::{Context, Result};
use chrono::Utc;
use rust_decimal::Decimal;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{Mutex, mpsc};
use tokio::time::{interval, Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Simplified metrics buffer that writes directly to database
#[derive(Debug)]
pub struct SimpleMetricBuffer {
    metrics: VecDeque<UsageMetric>,
    last_flush: Instant,
    capacity: usize,
}

impl SimpleMetricBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            metrics: VecDeque::with_capacity(capacity),
            last_flush: Instant::now(),
            capacity,
        }
    }

    fn add_metric(&mut self, metric: UsageMetric) -> bool {
        if self.metrics.len() >= self.capacity {
            // Drop oldest metric if buffer is full
            self.metrics.pop_front();
            warn!("Metrics buffer full, dropping oldest metric");
        }
        
        self.metrics.push_back(metric);
        self.should_flush()
    }

    fn should_flush(&self) -> bool {
        self.metrics.len() >= self.capacity / 2 || 
        self.last_flush.elapsed() >= Duration::from_secs(30)
    }

    fn flush(&mut self) -> Vec<UsageMetric> {
        let metrics = self.metrics.drain(..).collect();
        self.last_flush = Instant::now();
        metrics
    }

    fn len(&self) -> usize {
        self.metrics.len()
    }
}

/// Simplified metrics collector that writes directly to SurrealDB with error recovery
pub struct SimpleMetricsCollector {
    storage: Arc<BillingStorage>,
    config: BillingConfig,
    running: Arc<AtomicBool>,
    metrics_count: Arc<AtomicU64>,
    buffer: Arc<Mutex<SimpleMetricBuffer>>,
    metrics_tx: mpsc::UnboundedSender<UsageMetric>,
    real_time_metrics: Arc<Mutex<BillingMetrics>>,
    error_count: Arc<AtomicU64>,
    recovery_service: Arc<ErrorRecoveryService>,
}

impl SimpleMetricsCollector {
    /// Create a new simplified metrics collector
    pub async fn new(config: &BillingConfig, storage: Arc<BillingStorage>) -> Result<Self> {
        let (metrics_tx, metrics_rx) = mpsc::unbounded_channel();

        let real_time_metrics = BillingMetrics {
            active_sessions: 0,
            current_hourly_rate: Decimal::ZERO,
            period_cost: Decimal::ZERO,
            collection_rate: 0.0,
            last_updated: Utc::now(),
        };

        let recovery_service = Arc::new(ErrorRecoveryService::new(RetryConfig::default()));
        
        let collector = Self {
            storage,
            config: config.clone(),
            running: Arc::new(AtomicBool::new(false)),
            metrics_count: Arc::new(AtomicU64::new(0)),
            buffer: Arc::new(Mutex::new(SimpleMetricBuffer::new(config.buffer_size))),
            metrics_tx,
            real_time_metrics: Arc::new(Mutex::new(real_time_metrics)),
            error_count: Arc::new(AtomicU64::new(0)),
            recovery_service,
        };

        // Start the background processor
        collector.start_processor(metrics_rx).await?;

        Ok(collector)
    }

    /// Start collecting metrics
    pub async fn start_collection(&self) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            warn!("Metrics collection is already running");
            return Ok(());
        }

        info!("Starting simplified metrics collection");
        self.start_flush_timer().await?;
        info!("Simplified metrics collection started successfully");
        Ok(())
    }

    /// Stop collecting metrics
    pub async fn stop_collection(&self) -> Result<()> {
        info!("Stopping simplified metrics collection");
        self.running.store(false, Ordering::SeqCst);
        
        // Flush any remaining metrics
        self.flush_buffer().await?;
        
        info!("Simplified metrics collection stopped");
        Ok(())
    }

    /// Collect a single metric (direct API)
    pub async fn collect_metric(&self, metric: UsageMetric) -> Result<()> {
        if let Err(_) = self.metrics_tx.send(metric) {
            warn!("Failed to send metric to buffer - channel closed");
            return Err(anyhow::anyhow!("Metrics channel closed"));
        }

        self.metrics_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Get current billing metrics
    pub async fn get_billing_metrics(&self) -> BillingMetrics {
        self.real_time_metrics.lock().await.clone()
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

    /// Start background processor
    async fn start_processor(&self, mut metrics_rx: mpsc::UnboundedReceiver<UsageMetric>) -> Result<()> {
        let buffer = self.buffer.clone();
        let storage = self.storage.clone();
        let running = self.running.clone();
        let error_count = self.error_count.clone();

        tokio::spawn(async move {
            while running.load(Ordering::Relaxed) {
                match metrics_rx.recv().await {
                    Some(metric) => {
                        let mut buffer_guard = buffer.lock().await;
                        let should_flush = buffer_guard.add_metric(metric);
                        
                        if should_flush {
                            let metrics = buffer_guard.flush();
                            drop(buffer_guard);
                            
                            let storage_clone = storage.clone();
                            let metrics_clone = metrics.clone();
                            match storage_clone.store_metrics_batch(&metrics_clone).await {
                                Ok(_) => {
                                    debug!("Stored batch of {} metrics", metrics.len());
                                }
                                Err(e) => {
                                    error!("Failed to store metric batch: {}", e);
                                    error_count.fetch_add(1, Ordering::Relaxed);
                                    
                                    // Try to re-add failed metrics to buffer for retry
                                    let mut buffer_guard = buffer.lock().await;
                                    for metric in metrics {
                                        buffer_guard.add_metric(metric);
                                    }
                                }
                            }
                        }
                    }
                    None => break,
                }
            }
        });

        Ok(())
    }

    /// Start periodic flush timer
    async fn start_flush_timer(&self) -> Result<()> {
        let buffer = self.buffer.clone();
        let storage = self.storage.clone();
        let running = self.running.clone();
        let flush_interval = self.config.flush_interval;

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(flush_interval));
            
            while running.load(Ordering::Relaxed) {
                interval.tick().await;
                
                let mut buffer_guard = buffer.lock().await;
                if buffer_guard.len() > 0 {
                    let metrics = buffer_guard.flush();
                    drop(buffer_guard);
                    
                    if let Err(e) = storage.store_metrics_batch(&metrics).await {
                        error!("Failed to flush metrics batch: {}", e);
                    } else {
                        debug!("Flushed batch of {} metrics", metrics.len());
                    }
                }
            }
        });

        Ok(())
    }

    /// Flush remaining metrics in buffer
    async fn flush_buffer(&self) -> Result<()> {
        let mut buffer_guard = self.buffer.lock().await;
        if buffer_guard.len() > 0 {
            let metrics = buffer_guard.flush();
            drop(buffer_guard);
            self.storage.store_metrics_batch(&metrics).await?;
        }
        Ok(())
    }

    /// Get processing statistics
    pub async fn get_processing_stats(&self) -> SimpleMetricsProcessingStats {
        let buffer_guard = self.buffer.lock().await;
        let buffer_size = buffer_guard.len();
        drop(buffer_guard);

        SimpleMetricsProcessingStats {
            total_processed: self.metrics_count.load(Ordering::Relaxed),
            error_count: self.error_count.load(Ordering::Relaxed),
            buffer_size: buffer_size as u64,
            is_running: self.running.load(Ordering::Relaxed),
        }
    }
}

impl Drop for SimpleMetricsCollector {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        tracing::info!("SimpleMetricsCollector dropped - ensure stop_collection() was called");
    }
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

/// Simplified metrics processing statistics
#[derive(Debug, Clone)]
pub struct SimpleMetricsProcessingStats {
    pub total_processed: u64,
    pub error_count: u64,
    pub buffer_size: u64,
    pub is_running: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_metric_buffer() {
        let mut buffer = SimpleMetricBuffer::new(10);
        assert_eq!(buffer.len(), 0);

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
        assert_eq!(buffer.len(), 1);

        let flushed = buffer.flush();
        assert_eq!(flushed.len(), 1);
        assert_eq!(buffer.len(), 0);
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
}