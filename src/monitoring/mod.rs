// Comprehensive monitoring and health check system
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, error, debug};

use crate::container::manager::ContainerManager;
use crate::database::SurrealPool;
use crate::error::{Result, SoulBoxError};

/// Health status levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Service is healthy and fully operational
    Healthy,
    /// Service has minor issues but is operational
    Degraded,
    /// Service is not operational
    Unhealthy,
    /// Health status is unknown
    Unknown,
}

impl HealthStatus {
    pub fn as_str(&self) -> &str {
        match self {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Unhealthy => "unhealthy",
            HealthStatus::Unknown => "unknown",
        }
    }
}

/// Component health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    pub message: Option<String>,
    pub last_check: DateTime<Utc>,
    pub response_time_ms: u64,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// System resource metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: u64,
    pub memory_total_mb: u64,
    pub disk_usage_gb: f64,
    pub disk_total_gb: f64,
    pub container_count: usize,
    pub active_sessions: usize,
    pub network_connections: usize,
    pub uptime_seconds: u64,
}

/// Container health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerHealthMetrics {
    pub total_containers: usize,
    pub running_containers: usize,
    pub stopped_containers: usize,
    pub unhealthy_containers: usize,
    pub avg_cpu_usage: f64,
    pub avg_memory_usage_mb: u64,
    pub container_restart_count: u64,
}

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// Interval between health checks
    pub check_interval: Duration,
    /// Timeout for individual health checks
    pub check_timeout: Duration,
    /// Number of consecutive failures before marking unhealthy
    pub failure_threshold: u32,
    /// Number of consecutive successes before marking healthy
    pub success_threshold: u32,
    /// Enable detailed metrics collection
    pub collect_metrics: bool,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(30),
            check_timeout: Duration::from_secs(5),
            failure_threshold: 3,
            success_threshold: 2,
            collect_metrics: true,
        }
    }
}

/// Health monitoring service
pub struct HealthMonitor {
    config: HealthCheckConfig,
    container_manager: Option<Arc<ContainerManager>>,
    database_pool: Option<Arc<SurrealPool>>,
    component_health: Arc<RwLock<HashMap<String, ComponentHealth>>>,
    system_metrics: Arc<RwLock<SystemMetrics>>,
    container_metrics: Arc<RwLock<ContainerHealthMetrics>>,
    failure_counts: Arc<RwLock<HashMap<String, u32>>>,
    success_counts: Arc<RwLock<HashMap<String, u32>>>,
    start_time: Instant,
}

impl HealthMonitor {
    pub fn new(
        config: HealthCheckConfig,
        container_manager: Option<Arc<ContainerManager>>,
        database_pool: Option<Arc<SurrealPool>>,
    ) -> Self {
        Self {
            config,
            container_manager,
            database_pool,
            component_health: Arc::new(RwLock::new(HashMap::new())),
            system_metrics: Arc::new(RwLock::new(SystemMetrics {
                cpu_usage_percent: 0.0,
                memory_usage_mb: 0,
                memory_total_mb: 0,
                disk_usage_gb: 0.0,
                disk_total_gb: 0.0,
                container_count: 0,
                active_sessions: 0,
                network_connections: 0,
                uptime_seconds: 0,
            })),
            container_metrics: Arc::new(RwLock::new(ContainerHealthMetrics {
                total_containers: 0,
                running_containers: 0,
                stopped_containers: 0,
                unhealthy_containers: 0,
                avg_cpu_usage: 0.0,
                avg_memory_usage_mb: 0,
                container_restart_count: 0,
            })),
            failure_counts: Arc::new(RwLock::new(HashMap::new())),
            success_counts: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
        }
    }

    /// Start the health monitoring background task
    pub async fn start(&self) {
        let component_health = self.component_health.clone();
        let system_metrics = self.system_metrics.clone();
        let container_metrics = self.container_metrics.clone();
        let failure_counts = self.failure_counts.clone();
        let success_counts = self.success_counts.clone();
        let config = self.config.clone();
        let container_manager = self.container_manager.clone();
        let database_pool = self.database_pool.clone();
        let start_time = self.start_time;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.check_interval);
            
            loop {
                interval.tick().await;
                
                // Perform all health checks
                Self::perform_health_checks(
                    &component_health,
                    &system_metrics,
                    &container_metrics,
                    &failure_counts,
                    &success_counts,
                    &config,
                    container_manager.as_ref(),
                    database_pool.as_ref(),
                    start_time,
                ).await;
            }
        });
        
        info!("Health monitoring service started");
    }

    /// Perform all health checks
    async fn perform_health_checks(
        component_health: &Arc<RwLock<HashMap<String, ComponentHealth>>>,
        system_metrics: &Arc<RwLock<SystemMetrics>>,
        container_metrics: &Arc<RwLock<ContainerHealthMetrics>>,
        failure_counts: &Arc<RwLock<HashMap<String, u32>>>,
        success_counts: &Arc<RwLock<HashMap<String, u32>>>,
        config: &HealthCheckConfig,
        container_manager: Option<&Arc<ContainerManager>>,
        database_pool: Option<&Arc<SurrealPool>>,
        start_time: Instant,
    ) {
        debug!("Performing health checks");
        
        // Check database health
        if let Some(db) = database_pool {
            let result = Self::check_database_health(db, config.check_timeout).await;
            Self::update_component_health(
                component_health,
                failure_counts,
                success_counts,
                "database",
                result,
                config,
            ).await;
        }
        
        // Check container runtime health
        if let Some(manager) = container_manager {
            let result = Self::check_container_runtime_health(manager, config.check_timeout).await;
            Self::update_component_health(
                component_health,
                failure_counts,
                success_counts,
                "container_runtime",
                result,
                config,
            ).await;
            
            // Collect container metrics
            if config.collect_metrics {
                Self::collect_container_metrics(manager, container_metrics).await;
            }
        }
        
        // Collect system metrics
        if config.collect_metrics {
            Self::collect_system_metrics(system_metrics, start_time).await;
        }
    }

    /// Check database health
    async fn check_database_health(
        database_pool: &Arc<SurrealPool>,
        timeout: Duration,
    ) -> Result<ComponentHealth> {
        let start = Instant::now();
        
        let result = tokio::time::timeout(timeout, async {
            // Try to get a connection and execute a simple query
            match database_pool.get_connection().await {
                Ok(conn) => {
                    // Simple ping query
                    match conn.query::<Vec<i32>>("SELECT 1").await {
                        Ok(_) => Ok(()),
                        Err(e) => Err(SoulBoxError::Database(format!("Query failed: {}", e)))
                    }
                }
                Err(e) => Err(SoulBoxError::Database(format!("Connection failed: {}", e)))
            }
        }).await;
        
        let response_time_ms = start.elapsed().as_millis() as u64;
        
        match result {
            Ok(Ok(_)) => Ok(ComponentHealth {
                name: "database".to_string(),
                status: HealthStatus::Healthy,
                message: Some("Database connection successful".to_string()),
                last_check: Utc::now(),
                response_time_ms,
                metadata: HashMap::new(),
            }),
            Ok(Err(e)) => Ok(ComponentHealth {
                name: "database".to_string(),
                status: HealthStatus::Unhealthy,
                message: Some(format!("Database check failed: {}", e)),
                last_check: Utc::now(),
                response_time_ms,
                metadata: HashMap::new(),
            }),
            Err(_) => Ok(ComponentHealth {
                name: "database".to_string(),
                status: HealthStatus::Unhealthy,
                message: Some("Database check timed out".to_string()),
                last_check: Utc::now(),
                response_time_ms,
                metadata: HashMap::new(),
            }),
        }
    }

    /// Check container runtime health
    async fn check_container_runtime_health(
        container_manager: &Arc<ContainerManager>,
        timeout: Duration,
    ) -> Result<ComponentHealth> {
        let start = Instant::now();
        
        let result = tokio::time::timeout(timeout, async {
            // List containers to verify Docker is responsive
            container_manager.list_containers().await?;
            Ok::<(), SoulBoxError>(())
        }).await;
        
        let response_time_ms = start.elapsed().as_millis() as u64;
        
        match result {
            Ok(Ok(_)) => Ok(ComponentHealth {
                name: "container_runtime".to_string(),
                status: HealthStatus::Healthy,
                message: Some("Container runtime is responsive".to_string()),
                last_check: Utc::now(),
                response_time_ms,
                metadata: HashMap::new(),
            }),
            Ok(Err(e)) => Ok(ComponentHealth {
                name: "container_runtime".to_string(),
                status: HealthStatus::Unhealthy,
                message: Some(format!("Container runtime check failed: {}", e)),
                last_check: Utc::now(),
                response_time_ms,
                metadata: HashMap::new(),
            }),
            Err(_) => Ok(ComponentHealth {
                name: "container_runtime".to_string(),
                status: HealthStatus::Unhealthy,
                message: Some("Container runtime check timed out".to_string()),
                last_check: Utc::now(),
                response_time_ms,
                metadata: HashMap::new(),
            }),
        }
    }

    /// Collect container metrics
    async fn collect_container_metrics(
        container_manager: &Arc<ContainerManager>,
        metrics: &Arc<RwLock<ContainerHealthMetrics>>,
    ) {
        match container_manager.list_containers().await {
            Ok(containers) => {
                let total = containers.len();
                let running = containers.iter().filter(|c| c.status == "running").count();
                let stopped = containers.iter().filter(|c| c.status == "stopped" || c.status == "exited").count();
                
                // Calculate average resource usage
                let mut total_cpu = 0.0;
                let mut total_memory = 0;
                let mut count = 0;
                
                for container in &containers {
                    if container.status == "running" {
                        // In a real implementation, would get actual stats
                        // For now, use placeholder values
                        total_cpu += 10.0; // Placeholder
                        total_memory += 100; // Placeholder
                        count += 1;
                    }
                }
                
                let avg_cpu = if count > 0 { total_cpu / count as f64 } else { 0.0 };
                let avg_memory = if count > 0 { total_memory / count } else { 0 };
                
                let mut metrics_guard = metrics.write().await;
                metrics_guard.total_containers = total;
                metrics_guard.running_containers = running;
                metrics_guard.stopped_containers = stopped;
                metrics_guard.avg_cpu_usage = avg_cpu;
                metrics_guard.avg_memory_usage_mb = avg_memory;
            }
            Err(e) => {
                error!("Failed to collect container metrics: {}", e);
            }
        }
    }

    /// Collect system metrics
    async fn collect_system_metrics(
        metrics: &Arc<RwLock<SystemMetrics>>,
        start_time: Instant,
    ) {
        let mut metrics_guard = metrics.write().await;
        
        // Update uptime
        metrics_guard.uptime_seconds = start_time.elapsed().as_secs();
        
        // In a real implementation, would use sysinfo crate to get actual metrics
        // For now, use placeholder values
        metrics_guard.cpu_usage_percent = 25.0;
        metrics_guard.memory_usage_mb = 1024;
        metrics_guard.memory_total_mb = 8192;
        metrics_guard.disk_usage_gb = 10.0;
        metrics_guard.disk_total_gb = 100.0;
        metrics_guard.network_connections = 42;
    }

    /// Update component health status
    async fn update_component_health(
        component_health: &Arc<RwLock<HashMap<String, ComponentHealth>>>,
        failure_counts: &Arc<RwLock<HashMap<String, u32>>>,
        success_counts: &Arc<RwLock<HashMap<String, u32>>>,
        component_name: &str,
        health: Result<ComponentHealth>,
        config: &HealthCheckConfig,
    ) {
        match health {
            Ok(health) => {
                let is_healthy = health.status == HealthStatus::Healthy;
                
                // Update failure/success counts
                let mut failures = failure_counts.write().await;
                let mut successes = success_counts.write().await;
                
                if is_healthy {
                    *successes.entry(component_name.to_string()).or_insert(0) += 1;
                    failures.remove(component_name);
                } else {
                    *failures.entry(component_name.to_string()).or_insert(0) += 1;
                    successes.remove(component_name);
                }
                
                // Determine final status based on thresholds
                let final_status = if is_healthy {
                    if *successes.get(component_name).unwrap_or(&0) >= config.success_threshold {
                        HealthStatus::Healthy
                    } else {
                        HealthStatus::Degraded
                    }
                } else {
                    if *failures.get(component_name).unwrap_or(&0) >= config.failure_threshold {
                        HealthStatus::Unhealthy
                    } else {
                        HealthStatus::Degraded
                    }
                };
                
                // Update component health
                let mut health_guard = component_health.write().await;
                health_guard.insert(component_name.to_string(), ComponentHealth {
                    status: final_status,
                    ..health
                });
            }
            Err(e) => {
                error!("Health check failed for {}: {}", component_name, e);
                
                let mut health_guard = component_health.write().await;
                health_guard.insert(component_name.to_string(), ComponentHealth {
                    name: component_name.to_string(),
                    status: HealthStatus::Unknown,
                    message: Some(format!("Check failed: {}", e)),
                    last_check: Utc::now(),
                    response_time_ms: 0,
                    metadata: HashMap::new(),
                });
            }
        }
    }

    /// Get overall system health status
    pub async fn get_system_health(&self) -> HealthStatus {
        let health_guard = self.component_health.read().await;
        
        if health_guard.is_empty() {
            return HealthStatus::Unknown;
        }
        
        let mut has_unhealthy = false;
        let mut has_degraded = false;
        
        for (_, component) in health_guard.iter() {
            match component.status {
                HealthStatus::Unhealthy => has_unhealthy = true,
                HealthStatus::Degraded => has_degraded = true,
                _ => {}
            }
        }
        
        if has_unhealthy {
            HealthStatus::Unhealthy
        } else if has_degraded {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    /// Get component health details
    pub async fn get_component_health(&self) -> HashMap<String, ComponentHealth> {
        self.component_health.read().await.clone()
    }

    /// Get system metrics
    pub async fn get_system_metrics(&self) -> SystemMetrics {
        self.system_metrics.read().await.clone()
    }

    /// Get container metrics
    pub async fn get_container_metrics(&self) -> ContainerHealthMetrics {
        self.container_metrics.read().await.clone()
    }

    /// Get detailed health report
    pub async fn get_health_report(&self) -> serde_json::Value {
        let system_health = self.get_system_health().await;
        let component_health = self.get_component_health().await;
        let system_metrics = self.get_system_metrics().await;
        let container_metrics = self.get_container_metrics().await;
        
        serde_json::json!({
            "status": system_health.as_str(),
            "timestamp": Utc::now().to_rfc3339(),
            "uptime_seconds": self.start_time.elapsed().as_secs(),
            "components": component_health,
            "system_metrics": system_metrics,
            "container_metrics": container_metrics,
        })
    }
}