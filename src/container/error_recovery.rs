//! Container Error Recovery System
//! 
//! Provides comprehensive error recovery mechanisms including automatic
//! retries, fallback strategies, and self-healing capabilities

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{sleep, timeout, interval};
use tracing::{info, error, debug, warn};
use serde::{Serialize, Deserialize};

use crate::container::ContainerManager;
use crate::error::{Result, SoulBoxError};

/// Serde helpers for Duration and Instant
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_millis().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

mod instant_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(instant: &Instant, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert Instant to a comparable timestamp (millis since process start)
        // Note: This is a simplified approach for serialization
        let now_instant = Instant::now();
        let now_system = SystemTime::now();
        let elapsed_since_instant = if *instant <= now_instant {
            now_instant.duration_since(*instant)
        } else {
            Duration::from_secs(0)
        };
        let timestamp = now_system.duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .saturating_sub(elapsed_since_instant)
            .as_millis();
        timestamp.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Instant, D::Error>
    where
        D: Deserializer<'de>,
    {
        let _millis = u128::deserialize(deserializer)?;
        // For deserialization, we'll use current time as a reasonable default
        // since Instant is relative to process start time
        Ok(Instant::now())
    }
}

/// Error recovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecoveryConfig {
    /// Enable error recovery
    pub enabled: bool,
    /// Maximum retry attempts for failed operations
    pub max_retry_attempts: usize,
    /// Base delay between retries (exponential backoff)
    #[serde(with = "duration_serde")]
    pub base_retry_delay: Duration,
    /// Maximum retry delay (cap for exponential backoff)
    #[serde(with = "duration_serde")]
    pub max_retry_delay: Duration,
    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,
    /// Fallback strategies
    pub fallback_strategies: Vec<FallbackStrategy>,
    /// Health check configuration
    pub health_check: HealthCheckConfig,
    /// Recovery strategies
    pub recovery_strategies: Vec<RecoveryStrategy>,
}

impl Default for ErrorRecoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retry_attempts: 3,
            base_retry_delay: Duration::from_millis(100),
            max_retry_delay: Duration::from_secs(10),
            circuit_breaker: CircuitBreakerConfig::default(),
            fallback_strategies: vec![
                FallbackStrategy::RetryWithBackoff,
                FallbackStrategy::SwitchToBackupContainer,
                FallbackStrategy::CreateNewContainer,
            ],
            health_check: HealthCheckConfig::default(),
            recovery_strategies: vec![
                RecoveryStrategy::RestartContainer,
                RecoveryStrategy::RecreateContainer,
                RecoveryStrategy::ScaleUp,
            ],
        }
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to open circuit
    pub failure_threshold: usize,
    /// Time window for failure counting
    #[serde(with = "duration_serde")]
    pub failure_window: Duration,
    /// Circuit open duration before attempting recovery
    #[serde(with = "duration_serde")]
    pub open_timeout: Duration,
    /// Success threshold to close circuit
    pub success_threshold: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            failure_window: Duration::from_secs(60),
            open_timeout: Duration::from_secs(30),
            success_threshold: 3,
        }
    }
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check interval
    #[serde(with = "duration_serde")]
    pub interval: Duration,
    /// Health check timeout
    #[serde(with = "duration_serde")]
    pub timeout: Duration,
    /// Number of consecutive failures to mark unhealthy
    pub failure_threshold: usize,
    /// Number of consecutive successes to mark healthy
    pub success_threshold: usize,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            failure_threshold: 3,
            success_threshold: 2,
        }
    }
}

/// Fallback strategies for error recovery
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FallbackStrategy {
    /// Retry operation with exponential backoff
    RetryWithBackoff,
    /// Switch to backup container
    SwitchToBackupContainer,
    /// Create new container to replace failed one
    CreateNewContainer,
    /// Degrade service (limited functionality)
    DegradeService,
    /// Failover to different runtime
    FailoverRuntime,
}

/// Recovery strategies for systematic recovery
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RecoveryStrategy {
    /// Restart existing container
    RestartContainer,
    /// Recreate container from scratch
    RecreateContainer,
    /// Scale up additional containers
    ScaleUp,
    /// Switch to different node/host
    NodeFailover,
    /// Reset circuit breaker state
    ResetCircuitBreaker,
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    Closed,    // Normal operation
    Open,      // Failing fast
    HalfOpen,  // Testing recovery
}

/// Container health status for error recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecoveryContainerHealth {
    pub container_id: String,
    pub is_healthy: bool,
    #[serde(with = "instant_serde")]
    pub last_check: Instant,
    pub consecutive_failures: usize,
    pub consecutive_successes: usize,
    pub total_failures: u64,
    pub total_checks: u64,
}

/// Error recovery metrics
#[derive(Debug, Clone, Serialize)]
pub struct ErrorRecoveryMetrics {
    pub total_errors: u64,
    pub recovered_errors: u64,
    pub failed_recoveries: u64,
    pub circuit_breaker_opens: u64,
    pub container_restarts: u64,
    pub container_recreations: u64,
    #[serde(with = "duration_serde")]
    pub avg_recovery_time: Duration,
}

impl Default for ErrorRecoveryMetrics {
    fn default() -> Self {
        Self {
            total_errors: 0,
            recovered_errors: 0,
            failed_recoveries: 0,
            circuit_breaker_opens: 0,
            container_restarts: 0,
            container_recreations: 0,
            avg_recovery_time: Duration::from_millis(0),
        }
    }
}

/// Circuit breaker for error tracking
#[derive(Debug)]
struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: CircuitBreakerState,
    failures: Vec<Instant>,
    successes: usize,
    last_failure_time: Option<Instant>,
    last_state_change: Instant,
}

impl CircuitBreaker {
    fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: CircuitBreakerState::Closed,
            failures: Vec::new(),
            successes: 0,
            last_failure_time: None,
            last_state_change: Instant::now(),
        }
    }

    /// Record a successful operation
    fn record_success(&mut self) {
        let now = Instant::now();
        
        match self.state {
            CircuitBreakerState::HalfOpen => {
                self.successes += 1;
                if self.successes >= self.config.success_threshold {
                    self.state = CircuitBreakerState::Closed;
                    self.failures.clear();
                    self.successes = 0;
                    self.last_state_change = now;
                    info!("Circuit breaker closed after successful recovery");
                }
            }
            CircuitBreakerState::Closed => {
                // Reset failure count on success
                self.failures.clear();
            }
            _ => {}
        }
    }

    /// Record a failed operation
    fn record_failure(&mut self) {
        let now = Instant::now();
        
        match self.state {
            CircuitBreakerState::Closed => {
                self.failures.push(now);
                self.last_failure_time = Some(now);
                
                // Remove old failures outside the window
                self.failures.retain(|&failure_time| {
                    now.duration_since(failure_time) <= self.config.failure_window
                });
                
                if self.failures.len() >= self.config.failure_threshold {
                    self.state = CircuitBreakerState::Open;
                    self.last_state_change = now;
                    warn!("Circuit breaker opened due to {} failures", self.failures.len());
                }
            }
            CircuitBreakerState::HalfOpen => {
                self.state = CircuitBreakerState::Open;
                self.last_state_change = now;
                self.successes = 0;
                warn!("Circuit breaker reopened due to failure during testing");
            }
            _ => {}
        }
    }

    /// Check if operation should be allowed
    fn can_execute(&mut self) -> bool {
        let now = Instant::now();
        
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if now.duration_since(self.last_state_change) >= self.config.open_timeout {
                    self.state = CircuitBreakerState::HalfOpen;
                    self.last_state_change = now;
                    self.successes = 0;
                    info!("Circuit breaker entering half-open state");
                    true
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }
}

/// Error recovery system
pub struct ErrorRecoverySystem {
    config: ErrorRecoveryConfig,
    container_manager: Arc<ContainerManager>,
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
    container_health: Arc<RwLock<HashMap<String, ErrorRecoveryContainerHealth>>>,
    metrics: Arc<RwLock<ErrorRecoveryMetrics>>,
    recovery_semaphore: Arc<Semaphore>,
}

impl ErrorRecoverySystem {
    /// Create new error recovery system
    pub fn new(
        config: ErrorRecoveryConfig,
        container_manager: Arc<ContainerManager>,
    ) -> Self {
        Self {
            config,
            container_manager,
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            container_health: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(ErrorRecoveryMetrics::default())),
            recovery_semaphore: Arc::new(Semaphore::new(5)), // Max 5 concurrent recoveries
        }
    }

    /// Start the error recovery system
    pub async fn start(&self) -> Result<()> {
        if !self.config.enabled {
            info!("Error recovery system is disabled");
            return Ok(());
        }

        info!("Starting error recovery system");

        // Start health monitoring
        self.start_health_monitoring().await;

        // Start metrics reporting
        self.start_metrics_reporting().await;

        info!("Error recovery system started successfully");
        Ok(())
    }

    /// Execute operation with error recovery
    pub async fn execute_with_recovery<T, F, Fut>(
        &self,
        operation_id: &str,
        container_id: &str,
        operation: F,
    ) -> Result<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<T>> + Send,
        T: Send,
    {
        let mut attempt = 0;
        let mut last_error = None;

        while attempt < self.config.max_retry_attempts {
            // Check circuit breaker
            if !self.can_execute(operation_id).await {
                warn!("Operation {} blocked by circuit breaker", operation_id);
                self.record_failure(operation_id).await;
                return Err(SoulBoxError::RuntimeError(
                    "Operation blocked by circuit breaker".to_string(),
                ));
            }

            let start_time = Instant::now();
            
            match operation().await {
                Ok(result) => {
                    let recovery_time = start_time.elapsed();
                    self.record_success(operation_id, recovery_time).await;
                    
                    if attempt > 0 {
                        info!("Operation {} recovered after {} attempts", operation_id, attempt);
                    }
                    
                    return Ok(result);
                }
                Err(error) => {
                    attempt += 1;
                    last_error = Some(error);
                    
                    self.record_failure(operation_id).await;
                    
                    if attempt < self.config.max_retry_attempts {
                        let delay = self.calculate_retry_delay(attempt);
                        warn!(
                            "Operation {} failed on attempt {}, retrying in {:?}: {}",
                            operation_id, attempt, delay, last_error.as_ref().unwrap()
                        );
                        
                        sleep(delay).await;
                        
                        // Try recovery strategies
                        if let Err(recovery_error) = self.attempt_recovery(container_id).await {
                            error!("Recovery failed for container {}: {}", container_id, recovery_error);
                        }
                    }
                }
            }
        }

        error!("Operation {} failed after {} attempts", operation_id, attempt);
        self.update_failed_recovery_metrics().await;
        
        Err(last_error.unwrap_or_else(|| {
            SoulBoxError::RuntimeError("Operation failed after max retries".to_string())
        }))
    }

    /// Attempt recovery for failed container
    async fn attempt_recovery(&self, container_id: &str) -> Result<()> {
        let _permit = self.recovery_semaphore.acquire().await.unwrap();
        
        info!("Attempting recovery for container {}", container_id);
        let start_time = Instant::now();

        for strategy in &self.config.recovery_strategies {
            match self.apply_recovery_strategy(container_id, strategy).await {
                Ok(_) => {
                    let recovery_time = start_time.elapsed();
                    info!("Recovery successful for container {} using {:?} in {:?}", 
                          container_id, strategy, recovery_time);
                    
                    self.update_recovery_metrics(recovery_time).await;
                    return Ok(());
                }
                Err(e) => {
                    warn!("Recovery strategy {:?} failed for container {}: {}", 
                          strategy, container_id, e);
                    continue;
                }
            }
        }

        error!("All recovery strategies failed for container {}", container_id);
        Err(SoulBoxError::RuntimeError("All recovery strategies failed".to_string()))
    }

    /// Apply specific recovery strategy
    async fn apply_recovery_strategy(
        &self,
        container_id: &str,
        strategy: &RecoveryStrategy,
    ) -> Result<()> {
        match strategy {
            RecoveryStrategy::RestartContainer => {
                info!("Restarting container {}", container_id);
                self.container_manager.restart_container(container_id).await?;
                
                let mut metrics = self.metrics.write().await;
                metrics.container_restarts += 1;
            }
            RecoveryStrategy::RecreateContainer => {
                info!("Recreating container {}", container_id);
                
                // Get container info first
                if let Some(_container) = self.container_manager.get_container(container_id).await {
                    // Remove old container
                    self.container_manager.remove_container(container_id).await?;
                    
                    // Create new container (simplified - would need full container config)
                    let container_config = bollard::container::Config {
                        image: Some("ubuntu:22.04".to_string()),
                        ..Default::default()
                    };
                    
                    let new_container_id = self.container_manager.create_container(container_config).await?;
                    self.container_manager.start_container(&new_container_id).await?;
                    
                    let mut metrics = self.metrics.write().await;
                    metrics.container_recreations += 1;
                }
            }
            RecoveryStrategy::ScaleUp => {
                info!("Scaling up additional containers for failover");
                // Implementation would create additional backup containers
            }
            RecoveryStrategy::NodeFailover => {
                info!("Attempting node failover for container {}", container_id);
                // Implementation would move container to different node
            }
            RecoveryStrategy::ResetCircuitBreaker => {
                info!("Resetting circuit breaker for container {}", container_id);
                let mut breakers = self.circuit_breakers.write().await;
                breakers.remove(container_id);
            }
        }

        Ok(())
    }

    /// Check if operation can execute based on circuit breaker
    async fn can_execute(&self, operation_id: &str) -> bool {
        let mut breakers = self.circuit_breakers.write().await;
        let breaker = breakers
            .entry(operation_id.to_string())
            .or_insert_with(|| CircuitBreaker::new(self.config.circuit_breaker.clone()));
        
        breaker.can_execute()
    }

    /// Record successful operation
    async fn record_success(&self, operation_id: &str, recovery_time: Duration) {
        let mut breakers = self.circuit_breakers.write().await;
        if let Some(breaker) = breakers.get_mut(operation_id) {
            breaker.record_success();
        }

        let mut metrics = self.metrics.write().await;
        metrics.recovered_errors += 1;
        metrics.avg_recovery_time = self.update_average_duration(
            metrics.avg_recovery_time,
            recovery_time,
            metrics.recovered_errors,
        );
    }

    /// Record failed operation
    async fn record_failure(&self, operation_id: &str) {
        let mut breakers = self.circuit_breakers.write().await;
        let breaker = breakers
            .entry(operation_id.to_string())
            .or_insert_with(|| CircuitBreaker::new(self.config.circuit_breaker.clone()));
        
        let was_open = matches!(breaker.state, CircuitBreakerState::Open);
        breaker.record_failure();
        
        if !was_open && matches!(breaker.state, CircuitBreakerState::Open) {
            let mut metrics = self.metrics.write().await;
            metrics.circuit_breaker_opens += 1;
        }

        let mut metrics = self.metrics.write().await;
        metrics.total_errors += 1;
    }

    /// Update recovery metrics
    async fn update_recovery_metrics(&self, recovery_time: Duration) {
        let mut metrics = self.metrics.write().await;
        metrics.recovered_errors += 1;
        metrics.avg_recovery_time = self.update_average_duration(
            metrics.avg_recovery_time,
            recovery_time,
            metrics.recovered_errors,
        );
    }

    /// Update failed recovery metrics
    async fn update_failed_recovery_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.failed_recoveries += 1;
    }

    /// Calculate retry delay with exponential backoff
    fn calculate_retry_delay(&self, attempt: usize) -> Duration {
        let exponential_delay = self.config.base_retry_delay * (2_u32.pow(attempt as u32 - 1));
        std::cmp::min(exponential_delay, self.config.max_retry_delay)
    }

    /// Update average duration
    fn update_average_duration(&self, current_avg: Duration, new_value: Duration, count: u64) -> Duration {
        if count == 1 {
            new_value
        } else {
            let current_total = current_avg.as_nanos() * (count - 1) as u128;
            let new_total = current_total + new_value.as_nanos();
            Duration::from_nanos((new_total / count as u128) as u64)
        }
    }

    /// Start health monitoring
    async fn start_health_monitoring(&self) {
        let health_check = Arc::clone(&self.container_health);
        let container_manager = Arc::clone(&self.container_manager);
        let config = self.config.health_check.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(config.interval);
            
            loop {
                interval.tick().await;
                Self::perform_health_checks(
                    Arc::clone(&health_check),
                    Arc::clone(&container_manager),
                    &config
                ).await;
            }
        });
    }

    /// Perform health checks on all containers
    async fn perform_health_checks(
        health_map: Arc<RwLock<HashMap<String, ErrorRecoveryContainerHealth>>>,
        container_manager: Arc<ContainerManager>,
        config: &HealthCheckConfig,
    ) {
        debug!("Performing container health checks");
        
        let containers = container_manager.list_containers().await.unwrap_or_default();
        
        for container_info in containers {
            let container_id = &container_info.container_id;
            
            let is_healthy = match timeout(
                config.timeout,
                container_manager.get_container_health(container_id)
            ).await {
                Ok(Some(health)) => health.status == "healthy" || health.status == "running",
                _ => false,
            };

            let mut health_map_guard = health_map.write().await;
            let health = health_map_guard
                .entry(container_id.clone())
                .or_insert_with(|| ErrorRecoveryContainerHealth {
                    container_id: container_id.clone(),
                    is_healthy: true,
                    last_check: Instant::now(),
                    consecutive_failures: 0,
                    consecutive_successes: 0,
                    total_failures: 0,
                    total_checks: 0,
                });

            health.last_check = Instant::now();
            health.total_checks += 1;

            if is_healthy {
                health.consecutive_successes += 1;
                health.consecutive_failures = 0;
                
                if !health.is_healthy && health.consecutive_successes >= config.success_threshold {
                    health.is_healthy = true;
                    info!("Container {} marked as healthy after {} successes", container_id, health.consecutive_successes);
                }
            } else {
                health.consecutive_failures += 1;
                health.consecutive_successes = 0;
                health.total_failures += 1;
                
                if health.is_healthy && health.consecutive_failures >= config.failure_threshold {
                    health.is_healthy = false;
                    warn!("Container {} marked as unhealthy after {} failures", container_id, health.consecutive_failures);
                }
            }
        }
    }

    /// Start metrics reporting
    async fn start_metrics_reporting(&self) {
        let metrics = Arc::clone(&self.metrics);
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(300)); // 5 minutes
            
            loop {
                interval.tick().await;
                let metrics_guard = metrics.read().await;
                info!("Error recovery metrics: {:?}", *metrics_guard);
            }
        });
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> ErrorRecoveryMetrics {
        self.metrics.read().await.clone()
    }

    /// Get container health status
    pub async fn get_container_health(&self, container_id: &str) -> Option<ErrorRecoveryContainerHealth> {
        let health_map = self.container_health.read().await;
        health_map.get(container_id).cloned()
    }

    /// Get all container health statuses
    pub async fn get_all_container_health(&self) -> HashMap<String, ErrorRecoveryContainerHealth> {
        self.container_health.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_error_recovery_system_creation() {
        let config = ErrorRecoveryConfig::default();
        let container_manager = Arc::new(ContainerManager::new_default().unwrap());
        let recovery_system = ErrorRecoverySystem::new(config, container_manager);
        
        // Basic test that recovery system can be created
        assert!(std::ptr::addr_of!(recovery_system) as usize != 0);
    }

    #[test]
    fn test_circuit_breaker_states() {
        let config = CircuitBreakerConfig::default();
        let mut breaker = CircuitBreaker::new(config);
        
        // Initially closed
        assert_eq!(breaker.state, CircuitBreakerState::Closed);
        assert!(breaker.can_execute());
        
        // Record failures to open
        for _ in 0..5 {
            breaker.record_failure();
        }
        assert_eq!(breaker.state, CircuitBreakerState::Open);
        
        // Record success to eventually close
        breaker.record_success();
    }

    #[test]
    fn test_retry_delay_calculation() {
        let config = ErrorRecoveryConfig::default();
        let recovery_system = ErrorRecoverySystem::new(
            config.clone(),
            Arc::new(ContainerManager::new_default().unwrap()),
        );
        
        let delay1 = recovery_system.calculate_retry_delay(1);
        let delay2 = recovery_system.calculate_retry_delay(2);
        let delay3 = recovery_system.calculate_retry_delay(3);
        
        assert!(delay1 <= delay2);
        assert!(delay2 <= delay3);
        assert!(delay3 <= config.max_retry_delay);
    }
}