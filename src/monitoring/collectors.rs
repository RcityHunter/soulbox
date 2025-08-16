use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use sysinfo::{System, Cpu, Process};
use crate::error::{Result, SoulBoxError};
use crate::container::manager::ContainerManager;
use crate::session::SessionManager;
use super::metrics::PrometheusMetrics;

/// System resource collector for gathering system-level metrics
pub struct SystemResourceCollector {
    system: System,
    metrics: Arc<PrometheusMetrics>,
    collection_interval: Duration,
    last_collection: Option<Instant>,
}

impl SystemResourceCollector {
    /// Create a new system resource collector
    pub fn new(metrics: Arc<PrometheusMetrics>, collection_interval: Duration) -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        
        Self {
            system,
            metrics,
            collection_interval,
            last_collection: None,
        }
    }

    /// Collect system metrics
    pub async fn collect(&mut self) -> Result<()> {
        let now = Instant::now();
        
        // Check if enough time has passed since last collection
        if let Some(last) = self.last_collection {
            if now.duration_since(last) < self.collection_interval {
                return Ok(());
            }
        }

        // Refresh system information
        self.system.refresh_all();

        // Collect memory metrics
        let total_memory = self.system.total_memory();
        let used_memory = self.system.used_memory();
        let memory_percent = if total_memory > 0 {
            (used_memory as f64 / total_memory as f64) * 100.0
        } else {
            0.0
        };

        // Collect CPU metrics
        let cpu_usage = self.system.global_cpu_info().cpu_usage() as f64;
        
        // Collect load average
        let load_average = self.system.load_average().one;

        // Update metrics
        self.metrics.update_system_metrics(memory_percent, cpu_usage, load_average);

        self.last_collection = Some(now);
        tracing::debug!(
            "Collected system metrics: memory={}%, cpu={}%, load={}",
            memory_percent, cpu_usage, load_average
        );

        Ok(())
    }

    /// Get detailed memory information
    pub fn get_memory_info(&mut self) -> MemoryInfo {
        self.system.refresh_memory();
        
        MemoryInfo {
            total_bytes: self.system.total_memory(),
            used_bytes: self.system.used_memory(),
            available_bytes: self.system.available_memory(),
            free_bytes: self.system.free_memory(),
            total_swap_bytes: self.system.total_swap(),
            used_swap_bytes: self.system.used_swap(),
        }
    }

    /// Get detailed CPU information
    pub fn get_cpu_info(&mut self) -> Vec<CpuInfo> {
        self.system.refresh_cpu();
        
        self.system.cpus().iter().enumerate().map(|(index, cpu)| {
            CpuInfo {
                index,
                name: cpu.name().to_string(),
                brand: cpu.brand().to_string(),
                frequency: cpu.frequency(),
                usage_percent: cpu.cpu_usage() as f64,
            }
        }).collect()
    }

    /// Get process information for SoulBox processes
    pub fn get_process_info(&mut self) -> Vec<ProcessInfo> {
        self.system.refresh_processes();
        
        let current_pid = std::process::id();
        self.system.processes().iter()
            .filter(|(_, process)| {
                let name = process.name();
                name.contains("soulbox") || process.pid().as_u32() == current_pid
            })
            .map(|(pid, process)| {
                ProcessInfo {
                    pid: pid.as_u32(),
                    name: process.name().to_string(),
                    cpu_usage: process.cpu_usage() as f64,
                    memory_usage: process.memory(),
                    virtual_memory: process.virtual_memory(),
                    status: format!("{:?}", process.status()),
                    start_time: process.start_time(),
                }
            })
            .collect()
    }
}

/// Memory information
#[derive(Debug, Clone)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub free_bytes: u64,
    pub total_swap_bytes: u64,
    pub used_swap_bytes: u64,
}

/// CPU information
#[derive(Debug, Clone)]
pub struct CpuInfo {
    pub index: usize,
    pub name: String,
    pub brand: String,
    pub frequency: u64,
    pub usage_percent: f64,
}

/// Process information
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub virtual_memory: u64,
    pub status: String,
    pub start_time: u64,
}

/// Container resource collector for gathering container-specific metrics
pub struct ContainerResourceCollector {
    container_manager: Arc<ContainerManager>,
    metrics: Arc<PrometheusMetrics>,
    collection_interval: Duration,
    last_collection: Option<Instant>,
}

impl ContainerResourceCollector {
    /// Create a new container resource collector
    pub fn new(
        container_manager: Arc<ContainerManager>,
        metrics: Arc<PrometheusMetrics>,
        collection_interval: Duration,
    ) -> Self {
        Self {
            container_manager,
            metrics,
            collection_interval,
            last_collection: None,
        }
    }

    /// Collect container metrics
    pub async fn collect(&mut self) -> Result<()> {
        let now = Instant::now();
        
        // Check if enough time has passed since last collection
        if let Some(last) = self.last_collection {
            if now.duration_since(last) < self.collection_interval {
                return Ok(());
            }
        }

        // Get list of running containers
        let containers = self.container_manager.list_containers().await?;
        let running_count = containers.iter().filter(|c| c.state == "running").count() as i64;
        
        self.metrics.set_containers_running(running_count);

        // Collect resource usage for each container
        for container in containers {
            if container.state == "running" {
                match self.container_manager.get_container_stats(&container.id).await {
                    Ok(Some(stats)) => {
                        let runtime = container.labels.get("runtime").cloned().unwrap_or_default();
                        
                        self.metrics.record_container_resources(
                            &container.id,
                            &runtime,
                            stats.memory_usage as f64,
                            stats.cpu_percent,
                        );
                    }
                    Ok(None) => {
                        tracing::debug!("No stats available for container {}", container.id);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get stats for container {}: {}", container.id, e);
                    }
                }
            }
        }

        self.last_collection = Some(now);
        tracing::debug!("Collected container metrics for {} containers", running_count);

        Ok(())
    }
}

/// Session metrics collector for gathering session-related metrics
pub struct SessionMetricsCollector {
    session_manager: Arc<dyn SessionManager>,
    metrics: Arc<PrometheusMetrics>,
    collection_interval: Duration,
    last_collection: Option<Instant>,
    active_users_cache: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl SessionMetricsCollector {
    /// Create a new session metrics collector
    pub fn new(
        session_manager: Arc<dyn SessionManager>,
        metrics: Arc<PrometheusMetrics>,
        collection_interval: Duration,
    ) -> Self {
        Self {
            session_manager,
            metrics,
            collection_interval,
            last_collection: None,
            active_users_cache: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }

    /// Collect session metrics
    pub async fn collect(&mut self) -> Result<()> {
        let now = Instant::now();
        
        // Check if enough time has passed since last collection
        if let Some(last) = self.last_collection {
            if now.duration_since(last) < self.collection_interval {
                return Ok(());
            }
        }

        // Clean up expired sessions first
        let _cleaned = self.session_manager.cleanup_expired_sessions().await?;

        // Count active sessions (this is a simplified approach)
        // In practice, you'd want a more efficient way to count sessions
        let mut active_users = std::collections::HashSet::new();
        let mut total_sessions = 0;

        // This would need to be implemented based on your session manager's capabilities
        // For now, we'll use cached data or default values
        {
            let cache = self.active_users_cache.read().await;
            active_users.extend(cache.iter().cloned());
        }

        self.metrics.set_active_sessions(total_sessions);
        self.metrics.set_active_users(active_users.len() as i64);

        self.last_collection = Some(now);
        tracing::debug!(
            "Collected session metrics: {} sessions, {} users",
            total_sessions, active_users.len()
        );

        Ok(())
    }

    /// Update active users cache
    pub async fn update_active_users(&self, user_ids: Vec<String>) {
        let mut cache = self.active_users_cache.write().await;
        cache.clear();
        cache.extend(user_ids);
    }

    /// Add active user
    pub async fn add_active_user(&self, user_id: String) {
        let mut cache = self.active_users_cache.write().await;
        cache.insert(user_id);
    }

    /// Remove active user
    pub async fn remove_active_user(&self, user_id: &str) {
        let mut cache = self.active_users_cache.write().await;
        cache.remove(user_id);
    }
}

/// Composite metrics collector that orchestrates all metric collection
pub struct MetricsCollectionService {
    system_collector: SystemResourceCollector,
    container_collector: ContainerResourceCollector,
    session_collector: SessionMetricsCollector,
    collection_interval: Duration,
    running: Arc<RwLock<bool>>,
}

impl MetricsCollectionService {
    /// Create a new metrics collection service
    pub fn new(
        container_manager: Arc<ContainerManager>,
        session_manager: Arc<dyn SessionManager>,
        metrics: Arc<PrometheusMetrics>,
        collection_interval: Duration,
    ) -> Self {
        let system_collector = SystemResourceCollector::new(metrics.clone(), collection_interval);
        let container_collector = ContainerResourceCollector::new(
            container_manager,
            metrics.clone(),
            collection_interval,
        );
        let session_collector = SessionMetricsCollector::new(
            session_manager,
            metrics.clone(),
            collection_interval,
        );

        Self {
            system_collector,
            container_collector,
            session_collector,
            collection_interval,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start the metrics collection background task
    pub async fn start(&mut self) -> Result<()> {
        {
            let mut running = self.running.write().await;
            if *running {
                return Err(SoulBoxError::MonitoringError("Metrics collection already running".to_string()));
            }
            *running = true;
        }

        tracing::info!("Starting metrics collection service with interval {:?}", self.collection_interval);
        
        // Clone what we need for the background task
        let running = self.running.clone();
        let collection_interval = self.collection_interval;
        
        // Move the collectors into the background task
        let mut system_collector = std::mem::replace(
            &mut self.system_collector,
            SystemResourceCollector::new(
                self.system_collector.metrics.clone(),
                collection_interval
            )
        );
        let mut container_collector = std::mem::replace(
            &mut self.container_collector,
            ContainerResourceCollector::new(
                self.container_collector.container_manager.clone(),
                self.container_collector.metrics.clone(),
                collection_interval
            )
        );
        let mut session_collector = std::mem::replace(
            &mut self.session_collector,
            SessionMetricsCollector::new(
                self.session_collector.session_manager.clone(),
                self.session_collector.metrics.clone(),
                collection_interval
            )
        );

        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(collection_interval);
            
            while {
                let r = running.read().await;
                *r
            } {
                interval.tick().await;
                
                tracing::debug!("Collecting metrics...");
                
                // Collect system metrics
                if let Err(e) = system_collector.collect().await {
                    tracing::error!("Failed to collect system metrics: {}", e);
                }
                
                // Collect container metrics
                if let Err(e) = container_collector.collect().await {
                    tracing::error!("Failed to collect container metrics: {}", e);
                }
                
                // Collect session metrics
                if let Err(e) = session_collector.collect().await {
                    tracing::error!("Failed to collect session metrics: {}", e);
                }
            }
            
            tracing::info!("Metrics collection service stopped");
        });

        Ok(())
    }

    /// Stop the metrics collection service
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.write().await;
        *running = false;
        tracing::info!("Stopping metrics collection service");
        Ok(())
    }

    /// Check if the service is running
    pub async fn is_running(&self) -> bool {
        let running = self.running.read().await;
        *running
    }

    /// Perform a one-time collection of all metrics
    pub async fn collect_once(&mut self) -> Result<()> {
        tracing::debug!("Performing one-time metrics collection");
        
        // Collect system metrics
        if let Err(e) = self.system_collector.collect().await {
            tracing::error!("Failed to collect system metrics: {}", e);
        }

        // Collect container metrics
        if let Err(e) = self.container_collector.collect().await {
            tracing::error!("Failed to collect container metrics: {}", e);
        }

        // Collect session metrics
        if let Err(e) = self.session_collector.collect().await {
            tracing::error!("Failed to collect session metrics: {}", e);
        }

        Ok(())
    }

    /// Get reference to session collector for external updates
    pub fn session_collector(&self) -> &SessionMetricsCollector {
        &self.session_collector
    }
}

/// Custom metric for tracking application-specific metrics
pub struct CustomMetric {
    pub name: String,
    pub value: f64,
    pub labels: std::collections::HashMap<String, String>,
    pub timestamp: Instant,
    pub metric_type: CustomMetricType,
}

#[derive(Debug, Clone)]
pub enum CustomMetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
}

/// Registry for custom metrics
pub struct CustomMetricRegistry {
    metrics: Arc<RwLock<Vec<CustomMetric>>>,
    max_metrics: usize,
}

impl CustomMetricRegistry {
    /// Create a new custom metric registry
    pub fn new(max_metrics: usize) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Vec::new())),
            max_metrics,
        }
    }

    /// Record a custom metric
    pub async fn record_metric(&self, metric: CustomMetric) -> Result<()> {
        let mut metrics = self.metrics.write().await;
        
        metrics.push(metric);
        
        // Keep only recent metrics
        if metrics.len() > self.max_metrics {
            metrics.remove(0);
        }
        
        Ok(())
    }

    /// Get all metrics
    pub async fn get_metrics(&self) -> Vec<CustomMetric> {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }

    /// Get metrics by name
    pub async fn get_metrics_by_name(&self, name: &str) -> Vec<CustomMetric> {
        let metrics = self.metrics.read().await;
        metrics.iter().filter(|m| m.name == name).cloned().collect()
    }

    /// Clear old metrics
    pub async fn cleanup_old_metrics(&self, max_age: Duration) -> Result<usize> {
        let mut metrics = self.metrics.write().await;
        let cutoff = Instant::now() - max_age;
        
        let initial_len = metrics.len();
        metrics.retain(|m| m.timestamp > cutoff);
        let removed = initial_len - metrics.len();
        
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_system_collector() {
        let config = crate::monitoring::MonitoringConfig::default();
        let metrics = Arc::new(crate::monitoring::metrics::PrometheusMetrics::new(config).unwrap());
        let mut collector = SystemResourceCollector::new(metrics, Duration::from_secs(1));
        
        let result = collector.collect().await;
        assert!(result.is_ok());
        
        let memory_info = collector.get_memory_info();
        assert!(memory_info.total_bytes > 0);
        
        let cpu_info = collector.get_cpu_info();
        assert!(!cpu_info.is_empty());
    }

    #[tokio::test]
    async fn test_custom_metric_registry() {
        let registry = CustomMetricRegistry::new(100);
        
        let metric = CustomMetric {
            name: "test_metric".to_string(),
            value: 42.0,
            labels: std::collections::HashMap::new(),
            timestamp: Instant::now(),
            metric_type: CustomMetricType::Gauge,
        };
        
        registry.record_metric(metric).await.unwrap();
        
        let metrics = registry.get_metrics_by_name("test_metric").await;
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].value, 42.0);
    }

    #[tokio::test]
    async fn test_custom_metric_cleanup() {
        let registry = CustomMetricRegistry::new(100);
        
        // Add an old metric
        let old_metric = CustomMetric {
            name: "old_metric".to_string(),
            value: 1.0,
            labels: std::collections::HashMap::new(),
            timestamp: Instant::now() - Duration::from_secs(3600), // 1 hour ago
            metric_type: CustomMetricType::Counter,
        };
        
        registry.record_metric(old_metric).await.unwrap();
        
        // Cleanup metrics older than 30 minutes
        let removed = registry.cleanup_old_metrics(Duration::from_secs(1800)).await.unwrap();
        assert_eq!(removed, 1);
        
        let metrics = registry.get_metrics().await;
        assert!(metrics.is_empty());
    }
}