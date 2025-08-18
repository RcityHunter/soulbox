//! Metrics dashboard backend
//! 
//! This module provides the backend API for the monitoring dashboard,
//! including data aggregation, real-time updates, and visualization support.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};

use super::{PerformanceMetrics, HealthCheck, HealthStatus};
use super::alerts::{Alert, AlertSeverity, AlertStats};

/// Dashboard configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    /// Data retention period
    pub data_retention: Duration,
    /// Refresh interval for real-time updates
    pub refresh_interval: Duration,
    /// Maximum number of data points per metric
    pub max_data_points: usize,
    /// Enable real-time streaming
    pub enable_streaming: bool,
    /// Default time range for queries
    pub default_time_range: Duration,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            data_retention: Duration::from_secs(86400 * 7), // 7 days
            refresh_interval: Duration::from_secs(30),
            max_data_points: 1000,
            enable_streaming: true,
            default_time_range: Duration::from_secs(3600), // 1 hour
        }
    }
}

/// Time series data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub labels: HashMap<String, String>,
}

/// Time series data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeries {
    pub metric: String,
    pub data_points: Vec<DataPoint>,
    pub aggregation: Option<AggregationType>,
}

/// Aggregation types for time series data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregationType {
    Mean,
    Max,
    Min,
    Sum,
    Count,
    Percentile(f64),
}

/// Dashboard widget configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetConfig {
    pub id: String,
    pub title: String,
    pub widget_type: WidgetType,
    pub metrics: Vec<String>,
    pub time_range: Duration,
    pub refresh_interval: Option<Duration>,
    pub thresholds: Option<WidgetThresholds>,
    pub display_options: HashMap<String, String>,
}

/// Widget types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WidgetType {
    LineChart,
    AreaChart,
    BarChart,
    Gauge,
    SingleStat,
    Table,
    Heatmap,
    AlertList,
    HealthStatus,
}

/// Widget threshold configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetThresholds {
    pub warning: Option<f64>,
    pub critical: Option<f64>,
    pub colors: HashMap<String, String>,
}

/// Dashboard layout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardLayout {
    pub id: String,
    pub name: String,
    pub description: String,
    pub widgets: Vec<WidgetConfig>,
    pub auto_refresh: bool,
    pub time_range: Duration,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Real-time update message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeUpdate {
    pub update_type: UpdateType,
    pub data: UpdateData,
    pub timestamp: DateTime<Utc>,
}

/// Types of real-time updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdateType {
    MetricUpdate,
    AlertTriggered,
    AlertResolved,
    HealthStatusChanged,
    SystemEvent,
}

/// Update data payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdateData {
    Metric {
        name: String,
        value: f64,
        labels: HashMap<String, String>,
    },
    Alert {
        alert: Alert,
    },
    HealthStatus {
        component: String,
        status: HealthStatus,
        details: HashMap<String, String>,
    },
    Event {
        message: String,
        severity: String,
        details: HashMap<String, String>,
    },
}

/// Dashboard query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardQuery {
    pub metrics: Vec<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub time_range: Option<Duration>,
    pub aggregation: Option<AggregationType>,
    pub step: Option<Duration>,
    pub labels: Option<HashMap<String, String>>,
}

/// Dashboard response data
#[derive(Debug, Clone)]
pub struct DashboardResponse {
    pub time_series: Vec<TimeSeries>,
    pub alerts: Vec<Alert>,
    pub health_checks: Vec<HealthCheck>,
    pub performance_metrics: PerformanceMetrics,
    pub alert_stats: AlertStats,
    pub system_info: SystemInfo,
}

/// System information for dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub uptime: Duration,
    pub version: String,
    pub total_containers: u32,
    pub active_sessions: u32,
    pub total_executions: u64,
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_usage: f64,
}

/// Main dashboard backend
pub struct DashboardBackend {
    config: DashboardConfig,
    metrics_store: Arc<RwLock<HashMap<String, VecDeque<DataPoint>>>>,
    alert_manager: Option<Arc<super::alerts::AlertManager>>,
    layouts: Arc<RwLock<HashMap<String, DashboardLayout>>>,
    realtime_tx: broadcast::Sender<RealtimeUpdate>,
    system_start_time: DateTime<Utc>,
}

impl DashboardBackend {
    /// Create a new dashboard backend
    pub fn new() -> Result<Self> {
        Self::with_config(DashboardConfig::default())
    }

    /// Create dashboard backend with custom configuration
    pub fn with_config(config: DashboardConfig) -> Result<Self> {
        let (realtime_tx, _) = broadcast::channel(1000);

        let backend = Self {
            config,
            metrics_store: Arc::new(RwLock::new(HashMap::new())),
            alert_manager: None,
            layouts: Arc::new(RwLock::new(HashMap::new())),
            realtime_tx,
            system_start_time: Utc::now(),
        };

        // Load default layouts
        tokio::spawn({
            let backend_clone = backend.clone_for_background();
            async move {
                if let Err(e) = backend_clone.load_default_layouts().await {
                    tracing::error!("Failed to load default layouts: {}", e);
                }
            }
        });

        // Start cleanup task
        backend.start_cleanup_task();

        Ok(backend)
    }

    /// Set alert manager reference
    pub fn set_alert_manager(&mut self, alert_manager: Arc<super::alerts::AlertManager>) {
        self.alert_manager = Some(alert_manager);
    }

    /// Submit metric data
    pub async fn submit_metric(
        &self,
        metric: &str,
        value: f64,
        labels: Option<HashMap<String, String>>,
    ) -> Result<()> {
        let data_point = DataPoint {
            timestamp: Utc::now(),
            value,
            labels: labels.unwrap_or_default(),
        };

        // Store in metrics store
        {
            let mut store = self.metrics_store.write().await;
            let series = store.entry(metric.to_string()).or_insert_with(VecDeque::new);
            series.push_back(data_point.clone());

            // Limit data points
            while series.len() > self.config.max_data_points {
                series.pop_front();
            }
        }

        // Send real-time update
        if self.config.enable_streaming {
            let update = RealtimeUpdate {
                update_type: UpdateType::MetricUpdate,
                data: UpdateData::Metric {
                    name: metric.to_string(),
                    value,
                    labels: data_point.labels,
                },
                timestamp: data_point.timestamp,
            };

            let _ = self.realtime_tx.send(update);
        }

        Ok(())
    }

    /// Query metrics data
    pub async fn query_metrics(&self, query: DashboardQuery) -> Result<Vec<TimeSeries>> {
        let store = self.metrics_store.read().await;
        let mut result = Vec::new();

        // Determine time range
        let (start_time, end_time) = self.calculate_time_range(&query);

        for metric in &query.metrics {
            if let Some(data_points) = store.get(metric) {
                let filtered_points: Vec<DataPoint> = data_points
                    .iter()
                    .filter(|point| {
                        point.timestamp >= start_time && point.timestamp <= end_time
                    })
                    .filter(|point| {
                        if let Some(ref labels) = query.labels {
                            labels.iter().all(|(key, value)| {
                                point.labels.get(key).map(|v| v == value).unwrap_or(false)
                            })
                        } else {
                            true
                        }
                    })
                    .cloned()
                    .collect();

                let aggregated_points = if let Some(ref aggregation) = query.aggregation {
                    self.aggregate_data_points(filtered_points, aggregation, query.step)?
                } else {
                    filtered_points
                };

                result.push(TimeSeries {
                    metric: metric.clone(),
                    data_points: aggregated_points,
                    aggregation: query.aggregation.clone(),
                });
            }
        }

        Ok(result)
    }

    /// Get dashboard data for a specific layout
    pub async fn get_dashboard_data(&self, layout_id: &str) -> Result<DashboardResponse> {
        let layout = {
            let layouts = self.layouts.read().await;
            layouts.get(layout_id).cloned()
        };

        let layout = layout.ok_or_else(|| anyhow::anyhow!("Layout not found: {}", layout_id))?;

        // Collect all metrics from widgets
        let mut all_metrics = Vec::new();
        for widget in &layout.widgets {
            all_metrics.extend(widget.metrics.clone());
        }

        // Query metrics
        let query = DashboardQuery {
            metrics: all_metrics,
            time_range: Some(layout.time_range),
            start_time: None,
            end_time: None,
            aggregation: None,
            step: None,
            labels: None,
        };

        let time_series = self.query_metrics(query).await?;

        // Get alerts
        let alerts = if let Some(ref alert_manager) = self.alert_manager {
            alert_manager.get_active_alerts().await
        } else {
            Vec::new()
        };

        // Get alert stats
        let alert_stats = if let Some(ref alert_manager) = self.alert_manager {
            alert_manager.get_stats().await
        } else {
            AlertStats::default()
        };

        // Create mock data for other fields
        let health_checks = vec![
            HealthCheck {
                component: "container_manager".to_string(),
                status: HealthStatus::Healthy,
                message: "All containers running normally".to_string(),
                timestamp: Utc::now(),
                metadata: HashMap::new(),
            },
            HealthCheck {
                component: "database".to_string(),
                status: HealthStatus::Healthy,
                message: "Database connections healthy".to_string(),
                timestamp: Utc::now(),
                metadata: HashMap::new(),
            },
        ];

        let performance_metrics = PerformanceMetrics::new();

        let system_info = SystemInfo {
            uptime: Utc::now().signed_duration_since(self.system_start_time).to_std()
                .unwrap_or(Duration::from_secs(0)),
            version: "1.0.0".to_string(),
            total_containers: 0, // Would be populated from container manager
            active_sessions: 0,  // Would be populated from session manager
            total_executions: 0, // Would be populated from execution stats
            cpu_usage: 0.0,      // Would be populated from system metrics
            memory_usage: 0.0,   // Would be populated from system metrics
            disk_usage: 0.0,     // Would be populated from system metrics
        };

        Ok(DashboardResponse {
            time_series,
            alerts,
            health_checks,
            performance_metrics,
            alert_stats,
            system_info,
        })
    }

    /// Subscribe to real-time updates
    pub fn subscribe_realtime(&self) -> broadcast::Receiver<RealtimeUpdate> {
        self.realtime_tx.subscribe()
    }

    /// Create or update dashboard layout
    pub async fn save_layout(&self, layout: DashboardLayout) -> Result<()> {
        let mut layouts = self.layouts.write().await;
        layouts.insert(layout.id.clone(), layout.clone());

        tracing::info!(
            layout_id = layout.id,
            layout_name = layout.name,
            "Saved dashboard layout"
        );

        Ok(())
    }

    /// Get all available layouts
    pub async fn get_layouts(&self) -> Vec<DashboardLayout> {
        let layouts = self.layouts.read().await;
        layouts.values().cloned().collect()
    }

    /// Delete a layout
    pub async fn delete_layout(&self, layout_id: &str) -> Result<()> {
        let mut layouts = self.layouts.write().await;
        layouts.remove(layout_id);

        tracing::info!(layout_id = layout_id, "Deleted dashboard layout");
        Ok(())
    }

    /// Send real-time alert update
    pub async fn send_alert_update(&self, alert: Alert, is_resolved: bool) {
        if !self.config.enable_streaming {
            return;
        }

        let update = RealtimeUpdate {
            update_type: if is_resolved {
                UpdateType::AlertResolved
            } else {
                UpdateType::AlertTriggered
            },
            data: UpdateData::Alert { alert },
            timestamp: Utc::now(),
        };

        let _ = self.realtime_tx.send(update);
    }

    /// Send real-time health status update
    pub async fn send_health_status_update(
        &self,
        component: &str,
        status: HealthStatus,
        details: HashMap<String, String>,
    ) {
        if !self.config.enable_streaming {
            return;
        }

        let update = RealtimeUpdate {
            update_type: UpdateType::HealthStatusChanged,
            data: UpdateData::HealthStatus {
                component: component.to_string(),
                status,
                details,
            },
            timestamp: Utc::now(),
        };

        let _ = self.realtime_tx.send(update);
    }

    /// Calculate time range for query
    fn calculate_time_range(&self, query: &DashboardQuery) -> (DateTime<Utc>, DateTime<Utc>) {
        let end_time = query.end_time.unwrap_or_else(Utc::now);
        
        let start_time = if let Some(start) = query.start_time {
            start
        } else if let Some(range) = query.time_range {
            end_time - ChronoDuration::from_std(range).unwrap_or_default()
        } else {
            end_time - ChronoDuration::from_std(self.config.default_time_range).unwrap_or_default()
        };

        (start_time, end_time)
    }

    /// Aggregate data points based on aggregation type
    fn aggregate_data_points(
        &self,
        data_points: Vec<DataPoint>,
        aggregation: &AggregationType,
        step: Option<Duration>,
    ) -> Result<Vec<DataPoint>> {
        if data_points.is_empty() {
            return Ok(data_points);
        }

        let step_duration = step.unwrap_or(Duration::from_secs(60));
        let step_chrono = ChronoDuration::from_std(step_duration)?;

        let mut aggregated = Vec::new();
        let start_time = data_points.first().unwrap().timestamp;
        let end_time = data_points.last().unwrap().timestamp;

        let mut current_time = start_time;
        while current_time <= end_time {
            let next_time = current_time + step_chrono;
            
            let bucket_points: Vec<_> = data_points
                .iter()
                .filter(|point| point.timestamp >= current_time && point.timestamp < next_time)
                .collect();

            if !bucket_points.is_empty() {
                let aggregated_value = match aggregation {
                    AggregationType::Mean => {
                        bucket_points.iter().map(|p| p.value).sum::<f64>() / bucket_points.len() as f64
                    }
                    AggregationType::Max => {
                        bucket_points.iter().map(|p| p.value).fold(f64::NEG_INFINITY, f64::max)
                    }
                    AggregationType::Min => {
                        bucket_points.iter().map(|p| p.value).fold(f64::INFINITY, f64::min)
                    }
                    AggregationType::Sum => {
                        bucket_points.iter().map(|p| p.value).sum()
                    }
                    AggregationType::Count => {
                        bucket_points.len() as f64
                    }
                    AggregationType::Percentile(p) => {
                        let mut values: Vec<f64> = bucket_points.iter().map(|point| point.value).collect();
                        values.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        let index = ((*p / 100.0) * (values.len() - 1) as f64).round() as usize;
                        values.get(index).copied().unwrap_or(0.0)
                    }
                };

                aggregated.push(DataPoint {
                    timestamp: current_time,
                    value: aggregated_value,
                    labels: HashMap::new(),
                });
            }

            current_time = next_time;
        }

        Ok(aggregated)
    }

    /// Load default dashboard layouts
    async fn load_default_layouts(&self) -> Result<()> {
        let overview_layout = DashboardLayout {
            id: "overview".to_string(),
            name: "System Overview".to_string(),
            description: "High-level system metrics and status".to_string(),
            widgets: vec![
                WidgetConfig {
                    id: "cpu_usage".to_string(),
                    title: "CPU Usage".to_string(),
                    widget_type: WidgetType::Gauge,
                    metrics: vec!["cpu_usage_percent".to_string()],
                    time_range: Duration::from_secs(300),
                    refresh_interval: Some(Duration::from_secs(30)),
                    thresholds: Some(WidgetThresholds {
                        warning: Some(70.0),
                        critical: Some(90.0),
                        colors: {
                            let mut colors = HashMap::new();
                            colors.insert("normal".to_string(), "#00ff00".to_string());
                            colors.insert("warning".to_string(), "#ffaa00".to_string());
                            colors.insert("critical".to_string(), "#ff0000".to_string());
                            colors
                        },
                    }),
                    display_options: HashMap::new(),
                },
                WidgetConfig {
                    id: "memory_usage".to_string(),
                    title: "Memory Usage".to_string(),
                    widget_type: WidgetType::Gauge,
                    metrics: vec!["memory_usage_percent".to_string()],
                    time_range: Duration::from_secs(300),
                    refresh_interval: Some(Duration::from_secs(30)),
                    thresholds: Some(WidgetThresholds {
                        warning: Some(80.0),
                        critical: Some(95.0),
                        colors: {
                            let mut colors = HashMap::new();
                            colors.insert("normal".to_string(), "#00ff00".to_string());
                            colors.insert("warning".to_string(), "#ffaa00".to_string());
                            colors.insert("critical".to_string(), "#ff0000".to_string());
                            colors
                        },
                    }),
                    display_options: HashMap::new(),
                },
                WidgetConfig {
                    id: "active_alerts".to_string(),
                    title: "Active Alerts".to_string(),
                    widget_type: WidgetType::AlertList,
                    metrics: vec![],
                    time_range: Duration::from_secs(3600),
                    refresh_interval: Some(Duration::from_secs(60)),
                    thresholds: None,
                    display_options: HashMap::new(),
                },
            ],
            auto_refresh: true,
            time_range: Duration::from_secs(3600),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let performance_layout = DashboardLayout {
            id: "performance".to_string(),
            name: "Performance Monitoring".to_string(),
            description: "Detailed performance metrics and trends".to_string(),
            widgets: vec![
                WidgetConfig {
                    id: "container_startup_times".to_string(),
                    title: "Container Startup Times".to_string(),
                    widget_type: WidgetType::LineChart,
                    metrics: vec!["container_startup_duration_seconds".to_string()],
                    time_range: Duration::from_secs(7200),
                    refresh_interval: Some(Duration::from_secs(60)),
                    thresholds: None,
                    display_options: HashMap::new(),
                },
                WidgetConfig {
                    id: "execution_metrics".to_string(),
                    title: "Code Execution Metrics".to_string(),
                    widget_type: WidgetType::AreaChart,
                    metrics: vec![
                        "execution_duration_seconds".to_string(),
                        "executions_per_minute".to_string(),
                    ],
                    time_range: Duration::from_secs(3600),
                    refresh_interval: Some(Duration::from_secs(30)),
                    thresholds: None,
                    display_options: HashMap::new(),
                },
            ],
            auto_refresh: true,
            time_range: Duration::from_secs(7200),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        self.save_layout(overview_layout).await?;
        self.save_layout(performance_layout).await?;

        Ok(())
    }

    /// Start cleanup task for old data
    fn start_cleanup_task(&self) {
        let metrics_store = self.metrics_store.clone();
        let retention_period = self.config.data_retention;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Run hourly
            
            loop {
                interval.tick().await;

                let cutoff_time = Utc::now() - ChronoDuration::from_std(retention_period).unwrap_or_default();
                let mut store = metrics_store.write().await;

                for (metric_name, data_points) in store.iter_mut() {
                    let original_len = data_points.len();
                    data_points.retain(|point| point.timestamp >= cutoff_time);
                    let removed = original_len - data_points.len();

                    if removed > 0 {
                        tracing::debug!(
                            metric = metric_name,
                            removed = removed,
                            "Cleaned up old metric data"
                        );
                    }
                }
            }
        });
    }

    /// Clone for background tasks
    fn clone_for_background(&self) -> Self {
        Self {
            config: self.config.clone(),
            metrics_store: self.metrics_store.clone(),
            alert_manager: self.alert_manager.clone(),
            layouts: self.layouts.clone(),
            realtime_tx: self.realtime_tx.clone(),
            system_start_time: self.system_start_time,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dashboard_backend_creation() {
        let backend = DashboardBackend::new();
        assert!(backend.is_ok());
    }

    #[tokio::test]
    async fn test_metric_submission() {
        let backend = DashboardBackend::new().unwrap();
        
        let mut labels = HashMap::new();
        labels.insert("container".to_string(), "test".to_string());
        
        let result = backend.submit_metric("cpu_usage", 75.0, Some(labels)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_metric_query() {
        let backend = DashboardBackend::new().unwrap();
        
        // Submit some test data
        backend.submit_metric("test_metric", 10.0, None).await.unwrap();
        backend.submit_metric("test_metric", 20.0, None).await.unwrap();
        backend.submit_metric("test_metric", 30.0, None).await.unwrap();

        let query = DashboardQuery {
            metrics: vec!["test_metric".to_string()],
            time_range: Some(Duration::from_secs(3600)),
            start_time: None,
            end_time: None,
            aggregation: None,
            step: None,
            labels: None,
        };

        let result = backend.query_metrics(query).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].metric, "test_metric");
        assert_eq!(result[0].data_points.len(), 3);
    }

    #[tokio::test]
    async fn test_layout_management() {
        let backend = DashboardBackend::new().unwrap();
        
        let layout = DashboardLayout {
            id: "test_layout".to_string(),
            name: "Test Layout".to_string(),
            description: "Test description".to_string(),
            widgets: vec![],
            auto_refresh: true,
            time_range: Duration::from_secs(3600),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Save layout
        backend.save_layout(layout.clone()).await.unwrap();

        // Get layouts
        let layouts = backend.get_layouts().await;
        assert!(layouts.iter().any(|l| l.id == "test_layout"));

        // Delete layout
        backend.delete_layout("test_layout").await.unwrap();

        let layouts = backend.get_layouts().await;
        assert!(!layouts.iter().any(|l| l.id == "test_layout"));
    }
}