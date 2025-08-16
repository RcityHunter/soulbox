//! Exception recovery and resilience system
//! 
//! This module provides comprehensive error recovery mechanisms including
//! circuit breakers, retry strategies, and failover capabilities to ensure
//! system resilience and high availability.

pub mod strategies;
pub mod circuit_breaker;
pub mod retry;
pub mod failover;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};

use crate::error::SoulBoxError;

/// Recovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// Enable automatic recovery
    pub auto_recovery_enabled: bool,
    /// Maximum number of concurrent recovery operations
    pub max_concurrent_recoveries: usize,
    /// Default retry strategy configuration
    pub default_retry_config: retry::RetryConfig,
    /// Default circuit breaker configuration
    pub default_circuit_breaker_config: circuit_breaker::CircuitBreakerConfig,
    /// Recovery timeout duration
    pub recovery_timeout: Duration,
    /// Enable recovery logging
    pub enable_recovery_logging: bool,
    /// Health check interval
    pub health_check_interval: Duration,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            auto_recovery_enabled: true,
            max_concurrent_recoveries: 10,
            default_retry_config: retry::RetryConfig::default(),
            default_circuit_breaker_config: circuit_breaker::CircuitBreakerConfig::default(),
            recovery_timeout: Duration::from_secs(300), // 5 minutes
            enable_recovery_logging: true,
            health_check_interval: Duration::from_secs(30),
        }
    }
}

/// Recovery strategy type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RecoveryStrategy {
    /// Retry with exponential backoff
    ExponentialBackoff,
    /// Circuit breaker pattern
    CircuitBreaker,
    /// Immediate failover to backup
    ImmediateFailover,
    /// Graceful degradation
    GracefulDegradation,
    /// Custom recovery strategy
    Custom(String),
}

/// Recovery action result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryResult {
    pub strategy: RecoveryStrategy,
    pub success: bool,
    pub attempts: u32,
    pub total_duration: Duration,
    pub error_message: Option<String>,
    pub recovery_metadata: HashMap<String, String>,
}

/// Recovery event for logging and monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryEvent {
    pub id: uuid::Uuid,
    pub component: String,
    pub operation: String,
    pub error_type: String,
    pub strategy: RecoveryStrategy,
    pub result: RecoveryResult,
    pub timestamp: DateTime<Utc>,
    pub context: HashMap<String, String>,
}

/// Component health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Recovering,
    Failed,
}

/// Component health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub component: String,
    pub status: HealthStatus,
    pub last_check: DateTime<Utc>,
    pub error_count: u32,
    pub success_count: u32,
    pub recovery_attempts: u32,
    pub metadata: HashMap<String, String>,
}

/// Main recovery manager
pub struct RecoveryManager {
    config: RecoveryConfig,
    circuit_breakers: Arc<RwLock<HashMap<String, circuit_breaker::CircuitBreaker>>>,
    retry_managers: Arc<RwLock<HashMap<String, retry::RetryManager>>>,
    recovery_strategies: Arc<RwLock<HashMap<String, strategies::RecoveryStrategyImpl>>>,
    component_health: Arc<RwLock<HashMap<String, ComponentHealth>>>,
    recovery_events: Arc<RwLock<Vec<RecoveryEvent>>>,
    recovery_semaphore: Arc<Semaphore>,
    failover_manager: failover::FailoverManager,
}

impl RecoveryManager {
    /// Create a new recovery manager
    pub async fn new(config: RecoveryConfig) -> Result<Self> {
        let recovery_semaphore = Arc::new(Semaphore::new(config.max_concurrent_recoveries));
        let failover_manager = failover::FailoverManager::new().await?;

        let manager = Self {
            config: config.clone(),
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            retry_managers: Arc::new(RwLock::new(HashMap::new())),
            recovery_strategies: Arc::new(RwLock::new(HashMap::new())),
            component_health: Arc::new(RwLock::new(HashMap::new())),
            recovery_events: Arc::new(RwLock::new(Vec::new())),
            recovery_semaphore,
            failover_manager,
        };

        // Start health monitoring
        if config.auto_recovery_enabled {
            manager.start_health_monitoring().await;
        }

        Ok(manager)
    }

    /// Register a component for recovery management
    pub async fn register_component(
        &self,
        component: &str,
        strategy: RecoveryStrategy,
    ) -> Result<()> {
        // Create circuit breaker for component
        {
            let mut circuit_breakers = self.circuit_breakers.write().await;
            circuit_breakers.insert(
                component.to_string(),
                circuit_breaker::CircuitBreaker::new(
                    component,
                    self.config.default_circuit_breaker_config.clone(),
                ),
            );
        }

        // Create retry manager for component
        {
            let mut retry_managers = self.retry_managers.write().await;
            retry_managers.insert(
                component.to_string(),
                retry::RetryManager::new(self.config.default_retry_config.clone()),
            );
        }

        // Initialize component health
        {
            let mut component_health = self.component_health.write().await;
            component_health.insert(
                component.to_string(),
                ComponentHealth {
                    component: component.to_string(),
                    status: HealthStatus::Healthy,
                    last_check: Utc::now(),
                    error_count: 0,
                    success_count: 0,
                    recovery_attempts: 0,
                    metadata: HashMap::new(),
                },
            );
        }

        // Register recovery strategy
        {
            let mut recovery_strategies = self.recovery_strategies.write().await;
            recovery_strategies.insert(
                component.to_string(),
                strategies::RecoveryStrategyImpl::new(strategy, component),
            );
        }

        tracing::info!(
            component = component,
            strategy = ?strategy,
            "Registered component for recovery management"
        );

        Ok(())
    }

    /// Execute an operation with recovery protection
    pub async fn execute_with_recovery<T, F, E>(
        &self,
        component: &str,
        operation_name: &str,
        operation: F,
    ) -> Result<T>
    where
        F: Fn() -> std::result::Result<T, E> + Send + Sync + Clone + 'static,
        E: std::error::Error + Send + Sync + 'static,
        T: Send + 'static,
    {
        let _permit = self.recovery_semaphore.acquire().await
            .context("Failed to acquire recovery semaphore")?;

        let start_time = Instant::now();

        // Check circuit breaker first
        if let Some(circuit_breaker) = self.get_circuit_breaker(component).await {
            if !circuit_breaker.can_execute().await {
                return Err(anyhow::anyhow!("Circuit breaker is open for component: {}", component));
            }
        }

        // Try to execute with retry mechanism
        let retry_result = if let Some(retry_manager) = self.get_retry_manager(component).await {
            retry_manager.execute_with_retry(operation_name, operation).await
        } else {
            operation().map_err(|e| anyhow::anyhow!("Operation failed: {}", e))
        };

        match &retry_result {
            Ok(_) => {
                self.record_success(component).await;
                if let Some(circuit_breaker) = self.get_circuit_breaker(component).await {
                    circuit_breaker.record_success().await;
                }
            }
            Err(e) => {
                self.record_failure(component, e).await;
                if let Some(circuit_breaker) = self.get_circuit_breaker(component).await {
                    circuit_breaker.record_failure().await;
                }

                // Attempt recovery if configured
                if self.config.auto_recovery_enabled {
                    let recovery_result = self.attempt_recovery(component, operation_name, e).await;
                    self.log_recovery_event(component, operation_name, recovery_result).await;
                }
            }
        }

        retry_result
    }

    /// Get component health status
    pub async fn get_component_health(&self, component: &str) -> Option<ComponentHealth> {
        let component_health = self.component_health.read().await;
        component_health.get(component).cloned()
    }

    /// Get all component health statuses
    pub async fn get_all_component_health(&self) -> HashMap<String, ComponentHealth> {
        let component_health = self.component_health.read().await;
        component_health.clone()
    }

    /// Force recovery for a component
    pub async fn force_recovery(&self, component: &str) -> Result<RecoveryResult> {
        tracing::info!(component = component, "Forcing recovery for component");

        let strategy = {
            let strategies = self.recovery_strategies.read().await;
            strategies.get(component)
                .map(|s| s.get_strategy())
                .unwrap_or(RecoveryStrategy::ExponentialBackoff)
        };

        let start_time = Instant::now();
        let recovery_result = self.execute_recovery_strategy(component, &strategy).await;

        let result = RecoveryResult {
            strategy: strategy.clone(),
            success: recovery_result.is_ok(),
            attempts: 1,
            total_duration: start_time.elapsed(),
            error_message: recovery_result.err().map(|e| e.to_string()),
            recovery_metadata: HashMap::new(),
        };

        // Log recovery event
        let event = RecoveryEvent {
            id: uuid::Uuid::new_v4(),
            component: component.to_string(),
            operation: "force_recovery".to_string(),
            error_type: "manual_recovery".to_string(),
            strategy,
            result: result.clone(),
            timestamp: Utc::now(),
            context: HashMap::new(),
        };

        {
            let mut events = self.recovery_events.write().await;
            events.push(event);
            
            // Keep only recent events
            if events.len() > 1000 {
                events.drain(0..events.len() - 1000);
            }
        }

        Ok(result)
    }

    /// Get recent recovery events
    pub async fn get_recovery_events(&self, limit: Option<usize>) -> Vec<RecoveryEvent> {
        let events = self.recovery_events.read().await;
        let limit = limit.unwrap_or(100);
        
        events.iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Update recovery configuration for a component
    pub async fn update_component_config(
        &self,
        component: &str,
        retry_config: Option<retry::RetryConfig>,
        circuit_breaker_config: Option<circuit_breaker::CircuitBreakerConfig>,
    ) -> Result<()> {
        if let Some(config) = retry_config {
            let mut retry_managers = self.retry_managers.write().await;
            retry_managers.insert(
                component.to_string(),
                retry::RetryManager::new(config),
            );
        }

        if let Some(config) = circuit_breaker_config {
            let mut circuit_breakers = self.circuit_breakers.write().await;
            circuit_breakers.insert(
                component.to_string(),
                circuit_breaker::CircuitBreaker::new(component, config),
            );
        }

        tracing::info!(
            component = component,
            "Updated recovery configuration for component"
        );

        Ok(())
    }

    /// Get recovery statistics
    pub async fn get_recovery_statistics(&self) -> RecoveryStatistics {
        let events = self.recovery_events.read().await;
        let component_health = self.component_health.read().await;

        let total_recoveries = events.len();
        let successful_recoveries = events.iter()
            .filter(|e| e.result.success)
            .count();

        let recovery_success_rate = if total_recoveries > 0 {
            successful_recoveries as f32 / total_recoveries as f32
        } else {
            0.0
        };

        let components_healthy = component_health.values()
            .filter(|h| h.status == HealthStatus::Healthy)
            .count();

        let components_unhealthy = component_health.values()
            .filter(|h| h.status == HealthStatus::Unhealthy || h.status == HealthStatus::Failed)
            .count();

        RecoveryStatistics {
            total_recoveries,
            successful_recoveries,
            recovery_success_rate,
            components_healthy,
            components_unhealthy,
            total_components: component_health.len(),
        }
    }

    // Private helper methods

    async fn get_circuit_breaker(&self, component: &str) -> Option<circuit_breaker::CircuitBreaker> {
        let circuit_breakers = self.circuit_breakers.read().await;
        circuit_breakers.get(component).cloned()
    }

    async fn get_retry_manager(&self, component: &str) -> Option<retry::RetryManager> {
        let retry_managers = self.retry_managers.read().await;
        retry_managers.get(component).cloned()
    }

    async fn record_success(&self, component: &str) {
        let mut component_health = self.component_health.write().await;
        if let Some(health) = component_health.get_mut(component) {
            health.success_count += 1;
            health.last_check = Utc::now();
            
            // Update status based on recent performance
            if health.status != HealthStatus::Healthy && health.error_count == 0 {
                health.status = HealthStatus::Healthy;
                tracing::info!(component = component, "Component recovered to healthy state");
            }
        }
    }

    async fn record_failure(&self, component: &str, error: &anyhow::Error) {
        let mut component_health = self.component_health.write().await;
        if let Some(health) = component_health.get_mut(component) {
            health.error_count += 1;
            health.last_check = Utc::now();
            
            // Update status based on error rate
            let total_operations = health.success_count + health.error_count;
            let error_rate = health.error_count as f32 / total_operations as f32;
            
            if error_rate > 0.5 {
                health.status = HealthStatus::Unhealthy;
            } else if error_rate > 0.2 {
                health.status = HealthStatus::Degraded;
            }

            tracing::warn!(
                component = component,
                error = %error,
                error_count = health.error_count,
                "Recorded failure for component"
            );
        }
    }

    async fn attempt_recovery(
        &self,
        component: &str,
        operation_name: &str,
        error: &anyhow::Error,
    ) -> RecoveryResult {
        let strategy = {
            let strategies = self.recovery_strategies.read().await;
            strategies.get(component)
                .map(|s| s.get_strategy())
                .unwrap_or(RecoveryStrategy::ExponentialBackoff)
        };

        let start_time = Instant::now();
        let recovery_result = self.execute_recovery_strategy(component, &strategy).await;

        // Update recovery attempts
        {
            let mut component_health = self.component_health.write().await;
            if let Some(health) = component_health.get_mut(component) {
                health.recovery_attempts += 1;
                if recovery_result.is_ok() {
                    health.status = HealthStatus::Recovering;
                }
            }
        }

        RecoveryResult {
            strategy: strategy.clone(),
            success: recovery_result.is_ok(),
            attempts: 1,
            total_duration: start_time.elapsed(),
            error_message: recovery_result.err().map(|e| e.to_string()),
            recovery_metadata: HashMap::new(),
        }
    }

    async fn execute_recovery_strategy(
        &self,
        component: &str,
        strategy: &RecoveryStrategy,
    ) -> Result<()> {
        match strategy {
            RecoveryStrategy::ExponentialBackoff => {
                // Reset retry counters
                if let Some(retry_manager) = self.get_retry_manager(component).await {
                    retry_manager.reset().await;
                }
                Ok(())
            }
            RecoveryStrategy::CircuitBreaker => {
                // Reset circuit breaker
                if let Some(circuit_breaker) = self.get_circuit_breaker(component).await {
                    circuit_breaker.reset().await;
                }
                Ok(())
            }
            RecoveryStrategy::ImmediateFailover => {
                self.failover_manager.execute_failover(component).await
            }
            RecoveryStrategy::GracefulDegradation => {
                // Implement graceful degradation logic
                tracing::info!(component = component, "Executing graceful degradation");
                Ok(())
            }
            RecoveryStrategy::Custom(strategy_name) => {
                tracing::info!(
                    component = component,
                    strategy = strategy_name,
                    "Executing custom recovery strategy"
                );
                Ok(())
            }
        }
    }

    async fn log_recovery_event(
        &self,
        component: &str,
        operation_name: &str,
        result: RecoveryResult,
    ) {
        if !self.config.enable_recovery_logging {
            return;
        }

        let event = RecoveryEvent {
            id: uuid::Uuid::new_v4(),
            component: component.to_string(),
            operation: operation_name.to_string(),
            error_type: "operation_failure".to_string(),
            strategy: result.strategy.clone(),
            result,
            timestamp: Utc::now(),
            context: HashMap::new(),
        };

        {
            let mut events = self.recovery_events.write().await;
            events.push(event);
            
            // Keep only recent events
            if events.len() > 1000 {
                events.drain(0..events.len() - 1000);
            }
        }
    }

    async fn start_health_monitoring(&self) {
        let component_health = self.component_health.clone();
        let health_check_interval = self.config.health_check_interval;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(health_check_interval);
            
            loop {
                interval.tick().await;

                // Perform health checks
                let mut health_guard = component_health.write().await;
                for (component, health) in health_guard.iter_mut() {
                    // Simple health check based on recent activity
                    let time_since_check = Utc::now() - health.last_check;
                    
                    if time_since_check > chrono::Duration::minutes(5) {
                        // Component hasn't been active - mark as degraded
                        if health.status == HealthStatus::Healthy {
                            health.status = HealthStatus::Degraded;
                            tracing::warn!(
                                component = component,
                                "Component marked as degraded due to inactivity"
                            );
                        }
                    }
                }
            }
        });
    }
}

/// Recovery statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStatistics {
    pub total_recoveries: usize,
    pub successful_recoveries: usize,
    pub recovery_success_rate: f32,
    pub components_healthy: usize,
    pub components_unhealthy: usize,
    pub total_components: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_recovery_manager_creation() {
        let config = RecoveryConfig::default();
        let manager = RecoveryManager::new(config).await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_component_registration() {
        let config = RecoveryConfig::default();
        let manager = RecoveryManager::new(config).await.unwrap();
        
        let result = manager.register_component("test_component", RecoveryStrategy::ExponentialBackoff).await;
        assert!(result.is_ok());

        let health = manager.get_component_health("test_component").await;
        assert!(health.is_some());
        assert_eq!(health.unwrap().status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_recovery_statistics() {
        let config = RecoveryConfig::default();
        let manager = RecoveryManager::new(config).await.unwrap();
        
        manager.register_component("comp1", RecoveryStrategy::CircuitBreaker).await.unwrap();
        manager.register_component("comp2", RecoveryStrategy::ExponentialBackoff).await.unwrap();

        let stats = manager.get_recovery_statistics().await;
        assert_eq!(stats.total_components, 2);
        assert_eq!(stats.components_healthy, 2);
        assert_eq!(stats.components_unhealthy, 0);
    }
}