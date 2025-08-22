use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use crate::error::{Result, SoulBoxError};

pub mod metrics;
pub mod collectors;
pub mod alerts;
pub mod dashboard;

#[cfg(test)]
pub mod tests;

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable metrics collection
    pub enabled: bool,
    /// Metrics collection interval in seconds
    pub collection_interval: u64,
    /// Prometheus metrics endpoint path
    pub metrics_path: String,
    /// Enable detailed container metrics
    pub container_metrics: bool,
    /// Enable API latency metrics
    pub api_metrics: bool,
    /// Enable resource usage metrics
    pub resource_metrics: bool,
    /// Maximum number of metrics to retain in memory
    pub max_metrics_history: usize,
    /// Custom labels to add to all metrics
    pub global_labels: HashMap<String, String>,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval: 15, // 15 seconds
            metrics_path: "/metrics".to_string(),
            container_metrics: true,
            api_metrics: true,
            resource_metrics: true,
            max_metrics_history: 1000,
            global_labels: HashMap::new(),
        }
    }
}

/// System health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Component name
    pub component: String,
    /// Health status
    pub status: HealthStatus,
    /// Status message
    pub message: String,
    /// Timestamp of the check
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Performance metrics for various operations
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Container startup times
    pub container_startup_times: Vec<Duration>,
    /// API request latencies
    pub api_latencies: HashMap<String, Vec<Duration>>,
    /// Execution times
    pub execution_times: Vec<Duration>,
    /// Memory usage snapshots
    pub memory_usage: Vec<MemorySnapshot>,
    /// CPU usage snapshots
    pub cpu_usage: Vec<CpuSnapshot>,
}

#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    pub timestamp: Instant,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct CpuSnapshot {
    pub timestamp: Instant,
    pub usage_percent: f64,
    pub load_average: f64,
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            container_startup_times: Vec::new(),
            api_latencies: HashMap::new(),
            execution_times: Vec::new(),
            memory_usage: Vec::new(),
            cpu_usage: Vec::new(),
        }
    }

    /// Record container startup time
    pub fn record_container_startup(&mut self, duration: Duration) {
        self.container_startup_times.push(duration);
        
        // Implement circular buffer behavior
        Self::maintain_circular_buffer(&mut self.container_startup_times, 100);
    }

    /// Record API request latency
    pub fn record_api_latency(&mut self, endpoint: String, duration: Duration) {
        let latencies = self.api_latencies
            .entry(endpoint)
            .or_insert_with(Vec::new);
        latencies.push(duration);
        
        // Maintain circular buffer for each endpoint
        Self::maintain_circular_buffer(latencies, 100);
    }

    /// Record execution time
    pub fn record_execution_time(&mut self, duration: Duration) {
        self.execution_times.push(duration);
        
        // Implement circular buffer behavior
        Self::maintain_circular_buffer(&mut self.execution_times, 100);
    }

    /// Record memory usage
    pub fn record_memory_usage(&mut self, used: u64, available: u64, total: u64) {
        self.memory_usage.push(MemorySnapshot {
            timestamp: Instant::now(),
            used_bytes: used,
            available_bytes: available,
            total_bytes: total,
        });
        
        // Use time-based window management for resource metrics
        maintain_time_window(&mut self.memory_usage, Duration::from_secs(3600)); // 1 hour
    }

    /// Record CPU usage
    pub fn record_cpu_usage(&mut self, usage_percent: f64, load_average: f64) {
        self.cpu_usage.push(CpuSnapshot {
            timestamp: Instant::now(),
            usage_percent,
            load_average,
        });
        
        // Use time-based window management for resource metrics
        maintain_time_window(&mut self.cpu_usage, Duration::from_secs(3600)); // 1 hour
    }

    /// Get average container startup time
    pub fn avg_container_startup_time(&self) -> Option<Duration> {
        if self.container_startup_times.is_empty() {
            return None;
        }

        let total: Duration = self.container_startup_times.iter().sum();
        Some(total / self.container_startup_times.len() as u32)
    }

    /// Get average API latency for an endpoint
    pub fn avg_api_latency(&self, endpoint: &str) -> Option<Duration> {
        let latencies = self.api_latencies.get(endpoint)?;
        if latencies.is_empty() {
            return None;
        }

        let total: Duration = latencies.iter().sum();
        Some(total / latencies.len() as u32)
    }

    /// Get percentile for container startup times
    pub fn container_startup_percentile(&self, percentile: f64) -> Option<Duration> {
        if self.container_startup_times.is_empty() {
            return None;
        }

        let mut sorted = self.container_startup_times.clone();
        sorted.sort();

        let index = (percentile / 100.0 * (sorted.len() - 1) as f64) as usize;
        Some(sorted[index])
    }

    /// Get current memory usage percentage
    pub fn current_memory_usage_percent(&self) -> Option<f64> {
        let latest = self.memory_usage.last()?;
        Some((latest.used_bytes as f64 / latest.total_bytes as f64) * 100.0)
    }

    /// Get current CPU usage
    pub fn current_cpu_usage(&self) -> Option<f64> {
        self.cpu_usage.last().map(|snapshot| snapshot.usage_percent)
    }

    /// Maintain circular buffer with fixed size - efficient implementation
    fn maintain_circular_buffer<T>(buffer: &mut Vec<T>, max_size: usize) {
        if buffer.len() > max_size {
            // More efficient: drain from the front instead of multiple remove(0) calls
            let excess = buffer.len() - max_size;
            buffer.drain(0..excess);
        }
    }

    /// Maintain time-based window for timestamped data
    fn maintain_time_window<T>(buffer: &mut Vec<T>, window_duration: Duration) 
    where 
        T: HasTimestamp
    {
        let cutoff_time = Instant::now() - window_duration;
        buffer.retain(|item| item.timestamp() > cutoff_time);
    }

    /// Clean up old data beyond limits (hybrid approach)
    pub fn cleanup_old_data(&mut self) {
        // Clean up performance metrics with size limits
        Self::maintain_circular_buffer(&mut self.container_startup_times, 100);
        Self::maintain_circular_buffer(&mut self.execution_times, 100);
        
        // Clean up API latencies
        for latencies in self.api_latencies.values_mut() {
            Self::maintain_circular_buffer(latencies, 100);
        }
        
        // Clean up resource metrics with time windows
        let resource_window = Duration::from_secs(3600); // 1 hour
        maintain_time_window(&mut self.memory_usage, resource_window);
        maintain_time_window(&mut self.cpu_usage, resource_window);
        
        // Remove empty endpoint entries
        self.api_latencies.retain(|_, latencies| !latencies.is_empty());
    }
}

/// Maintain time-based window for timestamped data
fn maintain_time_window<T>(buffer: &mut Vec<T>, window_duration: Duration) 
where 
    T: HasTimestamp
{
    let cutoff_time = Instant::now() - window_duration;
    buffer.retain(|item| item.timestamp() > cutoff_time);
}

/// Trait for items that have a timestamp
trait HasTimestamp {
    fn timestamp(&self) -> Instant;
}

impl HasTimestamp for MemorySnapshot {
    fn timestamp(&self) -> Instant {
        self.timestamp
    }
}

impl HasTimestamp for CpuSnapshot {
    fn timestamp(&self) -> Instant {
        self.timestamp
    }
}

/// High-performance circular buffer with fixed capacity
/// Uses VecDeque for O(1) operations at both ends
pub struct CircularBuffer<T> {
    buffer: VecDeque<T>,
    capacity: usize,
}

impl<T> CircularBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Add item to buffer, removing oldest if at capacity
    pub fn push(&mut self, item: T) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(item);
    }

    /// Get reference to all items
    pub fn items(&self) -> &VecDeque<T> {
        &self.buffer
    }

    /// Get the most recent item
    pub fn last(&self) -> Option<&T> {
        self.buffer.back()
    }

    /// Get buffer length
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clear all items
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Get capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// Alert levels for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Alert name
    pub name: String,
    /// Alert level
    pub level: AlertLevel,
    /// Metric threshold
    pub threshold: f64,
    /// Comparison operator (gt, lt, eq)
    pub operator: String,
    /// Alert message template
    pub message: String,
    /// Whether the alert is enabled
    pub enabled: bool,
    /// Cooldown period between alerts in seconds
    pub cooldown: u64,
}

/// Alert instance
#[derive(Debug, Clone)]
pub struct Alert {
    pub config: AlertConfig,
    pub current_value: f64,
    pub triggered_at: Option<Instant>,
    pub last_sent: Option<Instant>,
}

impl Alert {
    pub fn new(config: AlertConfig) -> Self {
        Self {
            config,
            current_value: 0.0,
            triggered_at: None,
            last_sent: None,
        }
    }

    /// Check if alert should be triggered
    pub fn should_trigger(&self, value: f64) -> bool {
        if !self.config.enabled {
            return false;
        }

        match self.config.operator.as_str() {
            "gt" => value > self.config.threshold,
            "lt" => value < self.config.threshold,
            "eq" => (value - self.config.threshold).abs() < f64::EPSILON,
            "gte" => value >= self.config.threshold,
            "lte" => value <= self.config.threshold,
            _ => false,
        }
    }

    /// Check if alert is in cooldown period
    pub fn in_cooldown(&self) -> bool {
        if let Some(last_sent) = self.last_sent {
            let cooldown_duration = Duration::from_secs(self.config.cooldown);
            last_sent.elapsed() < cooldown_duration
        } else {
            false
        }
    }

    /// Update alert state
    pub fn update(&mut self, value: f64) -> bool {
        self.current_value = value;
        
        if self.should_trigger(value) {
            if self.triggered_at.is_none() {
                self.triggered_at = Some(Instant::now());
            }
            
            if !self.in_cooldown() {
                self.last_sent = Some(Instant::now());
                return true;
            }
        } else {
            self.triggered_at = None;
        }
        
        false
    }
}

/// Monitoring service trait
#[async_trait::async_trait]
pub trait MonitoringService: Send + Sync {
    /// Initialize the monitoring service
    async fn initialize(&mut self) -> Result<()>;
    
    /// Record a metric value
    async fn record_metric(&self, name: &str, value: f64, labels: &HashMap<String, String>) -> Result<()>;
    
    /// Record a counter increment
    async fn increment_counter(&self, name: &str, labels: &HashMap<String, String>) -> Result<()>;
    
    /// Record a histogram value
    async fn record_histogram(&self, name: &str, value: f64, labels: &HashMap<String, String>) -> Result<()>;
    
    /// Get current metric value
    async fn get_metric(&self, name: &str, labels: &HashMap<String, String>) -> Result<Option<f64>>;
    
    /// Perform health check
    async fn health_check(&self) -> Result<Vec<HealthCheck>>;
    
    /// Get performance metrics
    async fn get_performance_metrics(&self) -> Result<PerformanceMetrics>;
    
    /// Export metrics in Prometheus format
    async fn export_metrics(&self) -> Result<String>;
}

/// Timer for measuring duration
pub struct Timer {
    start: Instant,
    name: String,
    labels: HashMap<String, String>,
}

impl Timer {
    pub fn new(name: String, labels: HashMap<String, String>) -> Self {
        Self {
            start: Instant::now(),
            name,
            labels,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    pub fn stop(self) -> (String, Duration, HashMap<String, String>) {
        let elapsed = self.elapsed();
        (self.name, elapsed, self.labels)
    }
}

/// Macro for timing operations
#[macro_export]
macro_rules! time_operation {
    ($name:expr, $operation:expr) => {{
        let start = std::time::Instant::now();
        let result = $operation;
        let duration = start.elapsed();
        tracing::debug!("Operation '{}' took {:?}", $name, duration);
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_metrics() {
        let mut metrics = PerformanceMetrics::new();
        
        // Record some startup times
        metrics.record_container_startup(Duration::from_millis(200));
        metrics.record_container_startup(Duration::from_millis(150));
        metrics.record_container_startup(Duration::from_millis(300));
        
        let avg = metrics.avg_container_startup_time().unwrap();
        assert_eq!(avg, Duration::from_millis(216)); // (200+150+300)/3 = 216.67 ≈ 216
        
        let p50 = metrics.container_startup_percentile(50.0).unwrap();
        assert_eq!(p50, Duration::from_millis(200));
    }

    #[test]
    fn test_alert_triggering() {
        let config = AlertConfig {
            name: "high_cpu".to_string(),
            level: AlertLevel::Warning,
            threshold: 80.0,
            operator: "gt".to_string(),
            message: "CPU usage is high".to_string(),
            enabled: true,
            cooldown: 60,
        };
        
        let mut alert = Alert::new(config);
        
        // Should not trigger below threshold
        assert!(!alert.update(70.0));
        
        // Should trigger above threshold
        assert!(alert.update(85.0));
        
        // Should not trigger again in cooldown
        assert!(!alert.update(90.0));
    }

    #[test]
    fn test_timer() {
        let timer = Timer::new("test_operation".to_string(), HashMap::new());
        std::thread::sleep(Duration::from_millis(10));
        let (name, duration, _) = timer.stop();
        
        assert_eq!(name, "test_operation");
        assert!(duration >= Duration::from_millis(10));
    }

    #[test]
    fn test_health_status() {
        let health_check = HealthCheck {
            component: "database".to_string(),
            status: HealthStatus::Healthy,
            message: "Connection OK".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        };
        
        assert!(matches!(health_check.status, HealthStatus::Healthy));
    }
}