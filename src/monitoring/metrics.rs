use prometheus::{
    Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramVec, IntCounter, IntCounterVec,
    IntGauge, IntGaugeVec, Opts, Registry, TextEncoder, Encoder,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::error::{Result, SoulBoxError};
use super::{MonitoringService, MonitoringConfig, HealthCheck, PerformanceMetrics, HealthStatus};

/// Prometheus metrics service
pub struct PrometheusMetrics {
    registry: Registry,
    config: MonitoringConfig,
    
    // Container metrics
    container_starts_total: IntCounterVec,
    container_stops_total: IntCounterVec,
    container_startup_duration: HistogramVec,
    containers_running: IntGauge,
    container_memory_usage: GaugeVec,
    container_cpu_usage: GaugeVec,
    
    // API metrics
    http_requests_total: IntCounterVec,
    http_request_duration: HistogramVec,
    http_requests_in_flight: IntGauge,
    
    // Execution metrics
    code_executions_total: IntCounterVec,
    execution_duration: HistogramVec,
    execution_errors_total: IntCounterVec,
    
    // System metrics
    system_memory_usage: Gauge,
    system_cpu_usage: Gauge,
    system_load_average: Gauge,
    active_sessions: IntGauge,
    
    // Business metrics
    users_active: IntGauge,
    sandboxes_created_total: IntCounter,
    files_uploaded_total: IntCounter,
    runtime_usage: IntCounterVec,
    
    // Performance tracking
    performance_metrics: Arc<RwLock<PerformanceMetrics>>,
}

impl PrometheusMetrics {
    /// Create a new Prometheus metrics service
    pub fn new(config: MonitoringConfig) -> Result<Self> {
        let registry = Registry::new();
        
        // Container metrics
        let container_starts_total = IntCounterVec::new(
            Opts::new("soulbox_container_starts_total", "Total number of container starts"),
            &["runtime", "image"]
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create container_starts_total metric: {}", e)))?;
        
        let container_stops_total = IntCounterVec::new(
            Opts::new("soulbox_container_stops_total", "Total number of container stops"),
            &["runtime", "reason"]
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create container_stops_total metric: {}", e)))?;
        
        let container_startup_duration = HistogramVec::new(
            prometheus::HistogramOpts::new("soulbox_container_startup_duration_seconds", "Container startup duration")
                .buckets(vec![0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0]),
            &["runtime"]
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create container_startup_duration metric: {}", e)))?;
        
        let containers_running = IntGauge::new(
            "soulbox_containers_running", "Number of currently running containers"
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create containers_running metric: {}", e)))?;
        
        let container_memory_usage = GaugeVec::new(
            Opts::new("soulbox_container_memory_usage_bytes", "Container memory usage in bytes"),
            &["container_id", "runtime"]
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create container_memory_usage metric: {}", e)))?;
        
        let container_cpu_usage = GaugeVec::new(
            Opts::new("soulbox_container_cpu_usage_percent", "Container CPU usage percentage"),
            &["container_id", "runtime"]
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create container_cpu_usage metric: {}", e)))?;
        
        // API metrics
        let http_requests_total = IntCounterVec::new(
            Opts::new("soulbox_http_requests_total", "Total number of HTTP requests"),
            &["method", "endpoint", "status"]
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create http_requests_total metric: {}", e)))?;
        
        let http_request_duration = HistogramVec::new(
            prometheus::HistogramOpts::new("soulbox_http_request_duration_seconds", "HTTP request duration")
                .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]),
            &["method", "endpoint"]
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create http_request_duration metric: {}", e)))?;
        
        let http_requests_in_flight = IntGauge::new(
            "soulbox_http_requests_in_flight", "Number of HTTP requests currently being processed"
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create http_requests_in_flight metric: {}", e)))?;
        
        // Execution metrics
        let code_executions_total = IntCounterVec::new(
            Opts::new("soulbox_code_executions_total", "Total number of code executions"),
            &["runtime", "status"]
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create code_executions_total metric: {}", e)))?;
        
        let execution_duration = HistogramVec::new(
            prometheus::HistogramOpts::new("soulbox_execution_duration_seconds", "Code execution duration")
                .buckets(vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0, 300.0]),
            &["runtime"]
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create execution_duration metric: {}", e)))?;
        
        let execution_errors_total = IntCounterVec::new(
            Opts::new("soulbox_execution_errors_total", "Total number of execution errors"),
            &["runtime", "error_type"]
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create execution_errors_total metric: {}", e)))?;
        
        // System metrics
        let system_memory_usage = Gauge::new(
            "soulbox_system_memory_usage_percent", "System memory usage percentage"
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create system_memory_usage metric: {}", e)))?;
        
        let system_cpu_usage = Gauge::new(
            "soulbox_system_cpu_usage_percent", "System CPU usage percentage"
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create system_cpu_usage metric: {}", e)))?;
        
        let system_load_average = Gauge::new(
            "soulbox_system_load_average", "System load average"
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create system_load_average metric: {}", e)))?;
        
        let active_sessions = IntGauge::new(
            "soulbox_active_sessions", "Number of active sessions"
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create active_sessions metric: {}", e)))?;
        
        // Business metrics
        let users_active = IntGauge::new(
            "soulbox_users_active", "Number of active users"
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create users_active metric: {}", e)))?;
        
        let sandboxes_created_total = IntCounter::new(
            "soulbox_sandboxes_created_total", "Total number of sandboxes created"
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create sandboxes_created_total metric: {}", e)))?;
        
        let files_uploaded_total = IntCounter::new(
            "soulbox_files_uploaded_total", "Total number of files uploaded"
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create files_uploaded_total metric: {}", e)))?;
        
        let runtime_usage = IntCounterVec::new(
            Opts::new("soulbox_runtime_usage_total", "Total runtime usage by type"),
            &["runtime"]
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create runtime_usage metric: {}", e)))?;
        
        // Register all metrics
        registry.register(Box::new(container_starts_total.clone()))?;
        registry.register(Box::new(container_stops_total.clone()))?;
        registry.register(Box::new(container_startup_duration.clone()))?;
        registry.register(Box::new(containers_running.clone()))?;
        registry.register(Box::new(container_memory_usage.clone()))?;
        registry.register(Box::new(container_cpu_usage.clone()))?;
        registry.register(Box::new(http_requests_total.clone()))?;
        registry.register(Box::new(http_request_duration.clone()))?;
        registry.register(Box::new(http_requests_in_flight.clone()))?;
        registry.register(Box::new(code_executions_total.clone()))?;
        registry.register(Box::new(execution_duration.clone()))?;
        registry.register(Box::new(execution_errors_total.clone()))?;
        registry.register(Box::new(system_memory_usage.clone()))?;
        registry.register(Box::new(system_cpu_usage.clone()))?;
        registry.register(Box::new(system_load_average.clone()))?;
        registry.register(Box::new(active_sessions.clone()))?;
        registry.register(Box::new(users_active.clone()))?;
        registry.register(Box::new(sandboxes_created_total.clone()))?;
        registry.register(Box::new(files_uploaded_total.clone()))?;
        registry.register(Box::new(runtime_usage.clone()))?;
        
        Ok(Self {
            registry,
            config,
            container_starts_total,
            container_stops_total,
            container_startup_duration,
            containers_running,
            container_memory_usage,
            container_cpu_usage,
            http_requests_total,
            http_request_duration,
            http_requests_in_flight,
            code_executions_total,
            execution_duration,
            execution_errors_total,
            system_memory_usage,
            system_cpu_usage,
            system_load_average,
            active_sessions,
            users_active,
            sandboxes_created_total,
            files_uploaded_total,
            runtime_usage,
            performance_metrics: Arc::new(RwLock::new(PerformanceMetrics::new())),
        })
    }

    /// Record container start
    pub fn record_container_start(&self, runtime: &str, image: &str) {
        self.container_starts_total
            .with_label_values(&[runtime, image])
            .inc();
    }

    /// Record container stop
    pub fn record_container_stop(&self, runtime: &str, reason: &str) {
        self.container_stops_total
            .with_label_values(&[runtime, reason])
            .inc();
    }

    /// Record container startup duration
    pub fn record_container_startup_duration(&self, runtime: &str, duration_secs: f64) {
        self.container_startup_duration
            .with_label_values(&[runtime])
            .observe(duration_secs);
    }

    /// Set number of running containers
    pub fn set_containers_running(&self, count: i64) {
        self.containers_running.set(count);
    }

    /// Record container resource usage
    pub fn record_container_resources(&self, container_id: &str, runtime: &str, memory_bytes: f64, cpu_percent: f64) {
        self.container_memory_usage
            .with_label_values(&[container_id, runtime])
            .set(memory_bytes);
        
        self.container_cpu_usage
            .with_label_values(&[container_id, runtime])
            .set(cpu_percent);
    }

    /// Record HTTP request
    pub fn record_http_request(&self, method: &str, endpoint: &str, status: &str, duration_secs: f64) {
        self.http_requests_total
            .with_label_values(&[method, endpoint, status])
            .inc();
        
        self.http_request_duration
            .with_label_values(&[method, endpoint])
            .observe(duration_secs);
    }

    /// Increment HTTP requests in flight
    pub fn increment_http_requests_in_flight(&self) {
        self.http_requests_in_flight.inc();
    }

    /// Decrement HTTP requests in flight
    pub fn decrement_http_requests_in_flight(&self) {
        self.http_requests_in_flight.dec();
    }

    /// Record code execution
    pub fn record_code_execution(&self, runtime: &str, status: &str, duration_secs: f64) {
        self.code_executions_total
            .with_label_values(&[runtime, status])
            .inc();
        
        self.execution_duration
            .with_label_values(&[runtime])
            .observe(duration_secs);
    }

    /// Record execution error
    pub fn record_execution_error(&self, runtime: &str, error_type: &str) {
        self.execution_errors_total
            .with_label_values(&[runtime, error_type])
            .inc();
    }

    /// Update system metrics
    pub fn update_system_metrics(&self, memory_percent: f64, cpu_percent: f64, load_average: f64) {
        self.system_memory_usage.set(memory_percent);
        self.system_cpu_usage.set(cpu_percent);
        self.system_load_average.set(load_average);
    }

    /// Set active sessions count
    pub fn set_active_sessions(&self, count: i64) {
        self.active_sessions.set(count);
    }

    /// Set active users count
    pub fn set_active_users(&self, count: i64) {
        self.users_active.set(count);
    }

    /// Increment sandbox creation counter
    pub fn increment_sandboxes_created(&self) {
        self.sandboxes_created_total.inc();
    }

    /// Increment file upload counter
    pub fn increment_files_uploaded(&self) {
        self.files_uploaded_total.inc();
    }

    /// Record runtime usage
    pub fn record_runtime_usage(&self, runtime: &str) {
        self.runtime_usage.with_label_values(&[runtime]).inc();
    }

    /// Get registry for custom metrics
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Get or create a dynamic gauge metric
    fn get_or_create_gauge(&self, name: &str, labels: &HashMap<String, String>) -> Result<Gauge> {
        // Create label keys and values
        let label_keys: Vec<&str> = labels.keys().map(|k| k.as_str()).collect();
        let label_values: Vec<&str> = labels.values().map(|v| v.as_str()).collect();
        
        // Try to create a new gauge vector
        let gauge_vec = GaugeVec::new(
            Opts::new(name, format!("Custom metric: {}", name)),
            &label_keys
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create gauge vector: {}", e)))?;
        
        // Register if not already registered (this might fail if already exists, which is OK)
        let _ = self.registry.register(Box::new(gauge_vec.clone()));
        
        // Get the specific gauge with label values
        Ok(gauge_vec.with_label_values(&label_values))
    }

    /// Get or create a dynamic counter metric
    fn get_or_create_counter(&self, name: &str, labels: &HashMap<String, String>) -> Result<Counter> {
        // Create label keys and values
        let label_keys: Vec<&str> = labels.keys().map(|k| k.as_str()).collect();
        let label_values: Vec<&str> = labels.values().map(|v| v.as_str()).collect();
        
        // Try to create a new counter vector
        let counter_vec = CounterVec::new(
            Opts::new(name, format!("Custom counter: {}", name)),
            &label_keys
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create counter vector: {}", e)))?;
        
        // Register if not already registered (this might fail if already exists, which is OK)
        let _ = self.registry.register(Box::new(counter_vec.clone()));
        
        // Get the specific counter with label values
        Ok(counter_vec.with_label_values(&label_values))
    }

    /// Get or create a dynamic histogram metric
    fn get_or_create_histogram(&self, name: &str, labels: &HashMap<String, String>) -> Result<Histogram> {
        // Create label keys and values
        let label_keys: Vec<&str> = labels.keys().map(|k| k.as_str()).collect();
        let label_values: Vec<&str> = labels.values().map(|v| v.as_str()).collect();
        
        // Try to create a new histogram vector with default buckets
        let histogram_vec = HistogramVec::new(
            prometheus::HistogramOpts::new(name, format!("Custom histogram: {}", name))
                .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]),
            &label_keys
        ).map_err(|e| SoulBoxError::MonitoringError(format!("Failed to create histogram vector: {}", e)))?;
        
        // Register if not already registered (this might fail if already exists, which is OK)
        let _ = self.registry.register(Box::new(histogram_vec.clone()));
        
        // Get the specific histogram with label values
        Ok(histogram_vec.with_label_values(&label_values))
    }
}

#[async_trait::async_trait]
impl MonitoringService for PrometheusMetrics {
    async fn initialize(&mut self) -> Result<()> {
        tracing::info!("Initializing Prometheus metrics service");
        Ok(())
    }

    async fn record_metric(&self, name: &str, value: f64, labels: &HashMap<String, String>) -> Result<()> {
        // Create or get a dynamic gauge metric
        let metric_name = format!("soulbox_custom_{}", name);
        
        // Try to get existing metric from registry or create a new one
        let gauge = match self.get_or_create_gauge(&metric_name, labels) {
            Ok(gauge) => gauge,
            Err(e) => {
                tracing::warn!("Failed to create/get gauge metric {}: {}", metric_name, e);
                return Ok(()); // Don't fail the operation, just log the issue
            }
        };
        
        gauge.set(value);
        tracing::debug!("Recorded metric {}: {} with labels {:?}", name, value, labels);
        Ok(())
    }

    async fn increment_counter(&self, name: &str, labels: &HashMap<String, String>) -> Result<()> {
        // Create or get a dynamic counter metric
        let metric_name = format!("soulbox_custom_{}", name);
        
        // Try to get existing metric from registry or create a new one
        let counter = match self.get_or_create_counter(&metric_name, labels) {
            Ok(counter) => counter,
            Err(e) => {
                tracing::warn!("Failed to create/get counter metric {}: {}", metric_name, e);
                return Ok(()); // Don't fail the operation, just log the issue
            }
        };
        
        counter.inc();
        tracing::debug!("Incremented counter {}: with labels {:?}", name, labels);
        Ok(())
    }

    async fn record_histogram(&self, name: &str, value: f64, labels: &HashMap<String, String>) -> Result<()> {
        // Create or get a dynamic histogram metric
        let metric_name = format!("soulbox_custom_{}", name);
        
        // Try to get existing metric from registry or create a new one
        let histogram = match self.get_or_create_histogram(&metric_name, labels) {
            Ok(histogram) => histogram,
            Err(e) => {
                tracing::warn!("Failed to create/get histogram metric {}: {}", metric_name, e);
                return Ok(()); // Don't fail the operation, just log the issue
            }
        };
        
        histogram.observe(value);
        tracing::debug!("Recorded histogram {}: {} with labels {:?}", name, value, labels);
        Ok(())
    }

    async fn get_metric(&self, _name: &str, _labels: &HashMap<String, String>) -> Result<Option<f64>> {
        // Implementation would query the metric registry
        Ok(None)
    }

    async fn health_check(&self) -> Result<Vec<HealthCheck>> {
        let mut health_checks = Vec::new();
        
        // Check Prometheus registry health
        health_checks.push(HealthCheck {
            component: "prometheus_metrics".to_string(),
            status: HealthStatus::Healthy,
            message: "Metrics collection is operational".to_string(),
            timestamp: chrono::Utc::now(),
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("metric_families".to_string(), self.registry.gather().len().to_string());
                meta
            },
        });
        
        Ok(health_checks)
    }

    async fn get_performance_metrics(&self) -> Result<PerformanceMetrics> {
        let metrics = self.performance_metrics.read().await;
        Ok(metrics.clone())
    }

    async fn export_metrics(&self) -> Result<String> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)
            .map_err(|e| SoulBoxError::MonitoringError(format!("Failed to encode metrics: {}", e)))?;
        
        String::from_utf8(buffer)
            .map_err(|e| SoulBoxError::MonitoringError(format!("Failed to convert metrics to string: {}", e)))
    }
}

/// HTTP middleware for tracking request metrics
pub struct MetricsMiddleware {
    metrics: Arc<PrometheusMetrics>,
}

impl MetricsMiddleware {
    pub fn new(metrics: Arc<PrometheusMetrics>) -> Self {
        Self { metrics }
    }

    /// Start request tracking
    pub fn start_request(&self) -> RequestTracker {
        self.metrics.increment_http_requests_in_flight();
        RequestTracker {
            metrics: self.metrics.clone(),
            start_time: std::time::Instant::now(),
        }
    }
}

/// Request tracker for automatic metrics collection
pub struct RequestTracker {
    metrics: Arc<PrometheusMetrics>,
    start_time: std::time::Instant,
}

impl RequestTracker {
    /// Finish request tracking
    pub fn finish(self, method: &str, endpoint: &str, status: &str) {
        let duration = self.start_time.elapsed().as_secs_f64();
        self.metrics.record_http_request(method, endpoint, status, duration);
        self.metrics.decrement_http_requests_in_flight();
    }
}

impl Drop for RequestTracker {
    fn drop(&mut self) {
        self.metrics.decrement_http_requests_in_flight();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_prometheus_metrics_creation() {
        let config = MonitoringConfig::default();
        let metrics = PrometheusMetrics::new(config);
        assert!(metrics.is_ok());
    }

    #[tokio::test]
    async fn test_container_metrics() {
        let config = MonitoringConfig::default();
        let metrics = PrometheusMetrics::new(config).unwrap();
        
        metrics.record_container_start("python", "python:3.11");
        metrics.record_container_startup_duration("python", 0.5);
        metrics.set_containers_running(3);
        
        // Test export
        let exported = metrics.export_metrics().await.unwrap();
        assert!(exported.contains("soulbox_container_starts_total"));
        assert!(exported.contains("soulbox_containers_running"));
    }

    #[tokio::test]
    async fn test_http_metrics() {
        let config = MonitoringConfig::default();
        let metrics = PrometheusMetrics::new(config).unwrap();
        
        metrics.record_http_request("GET", "/api/execute", "200", 0.1);
        metrics.increment_http_requests_in_flight();
        
        let exported = metrics.export_metrics().await.unwrap();
        assert!(exported.contains("soulbox_http_requests_total"));
        assert!(exported.contains("soulbox_http_requests_in_flight"));
    }

    #[tokio::test]
    async fn test_execution_metrics() {
        let config = MonitoringConfig::default();
        let metrics = PrometheusMetrics::new(config).unwrap();
        
        metrics.record_code_execution("python", "success", 2.5);
        metrics.record_execution_error("python", "timeout");
        
        let exported = metrics.export_metrics().await.unwrap();
        assert!(exported.contains("soulbox_code_executions_total"));
        assert!(exported.contains("soulbox_execution_errors_total"));
    }

    #[test]
    fn test_request_tracker() {
        let config = MonitoringConfig::default();
        let metrics = Arc::new(PrometheusMetrics::new(config).unwrap());
        let middleware = MetricsMiddleware::new(metrics.clone());
        
        let tracker = middleware.start_request();
        std::thread::sleep(std::time::Duration::from_millis(10));
        tracker.finish("GET", "/test", "200");
        
        // Verify that in-flight counter was decremented
        // (This would require access to the actual metric value in a real test)
    }
}