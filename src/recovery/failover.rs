//! Failover management for high availability
//! 
//! This module provides automatic failover capabilities to maintain
//! service availability when primary components fail.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Failover configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverConfig {
    /// Enable automatic failover
    pub auto_failover_enabled: bool,
    /// Health check interval for failover monitoring
    pub health_check_interval: Duration,
    /// Threshold for consecutive health check failures
    pub failure_threshold: u32,
    /// Timeout for failover operations
    pub failover_timeout: Duration,
    /// Enable failback to primary when recovered
    pub auto_failback_enabled: bool,
    /// Minimum time before attempting failback
    pub failback_delay: Duration,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            auto_failover_enabled: true,
            health_check_interval: Duration::from_secs(30),
            failure_threshold: 3,
            failover_timeout: Duration::from_secs(60),
            auto_failback_enabled: true,
            failback_delay: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Failover target information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverTarget {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub priority: u32, // Lower numbers have higher priority
    pub capacity: f32, // 0.0 to 1.0, representing capacity percentage
    pub status: TargetStatus,
    pub last_health_check: Option<Instant>,
    pub consecutive_failures: u32,
    pub metadata: HashMap<String, String>,
}

/// Status of a failover target
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TargetStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Offline,
    Maintenance,
}

/// Failover event information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverEvent {
    pub id: uuid::Uuid,
    pub component: String,
    pub event_type: FailoverEventType,
    pub from_target: Option<String>,
    pub to_target: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub duration: Option<Duration>,
    pub success: bool,
    pub error_message: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Types of failover events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FailoverEventType {
    FailoverInitiated,
    FailoverCompleted,
    FailoverFailed,
    FailbackInitiated,
    FailbackCompleted,
    FailbackFailed,
    TargetHealthChanged,
}

/// Failover statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverStats {
    pub total_failovers: u32,
    pub successful_failovers: u32,
    pub failed_failovers: u32,
    pub total_failbacks: u32,
    pub successful_failbacks: u32,
    pub average_failover_time: Duration,
    pub uptime_percentage: f32,
    pub current_active_target: Option<String>,
}

/// Main failover manager
pub struct FailoverManager {
    config: FailoverConfig,
    components: Arc<RwLock<HashMap<String, ComponentFailoverState>>>,
    events: Arc<RwLock<Vec<FailoverEvent>>>,
    stats: Arc<RwLock<HashMap<String, FailoverStats>>>,
}

/// Failover state for a component
#[derive(Debug)]
struct ComponentFailoverState {
    component: String,
    primary_target: String,
    current_active: String,
    targets: HashMap<String, FailoverTarget>,
    last_failover: Option<Instant>,
    in_failover: bool,
}

impl FailoverManager {
    /// Create a new failover manager
    pub async fn new() -> Result<Self> {
        Self::with_config(FailoverConfig::default()).await
    }

    /// Create failover manager with custom configuration
    pub async fn with_config(config: FailoverConfig) -> Result<Self> {
        let manager = Self {
            config: config.clone(),
            components: Arc::new(RwLock::new(HashMap::new())),
            events: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(HashMap::new())),
        };

        // Start health monitoring if auto-failover is enabled
        if config.auto_failover_enabled {
            manager.start_health_monitoring().await;
        }

        Ok(manager)
    }

    /// Register a component for failover management
    pub async fn register_component(
        &self,
        component: &str,
        primary_target: FailoverTarget,
        backup_targets: Vec<FailoverTarget>,
    ) -> Result<()> {
        let mut components = self.components.write().await;
        
        // Create target map
        let mut targets = HashMap::new();
        targets.insert(primary_target.id.clone(), primary_target.clone());
        
        for target in backup_targets {
            targets.insert(target.id.clone(), target);
        }

        // Create component state
        let state = ComponentFailoverState {
            component: component.to_string(),
            primary_target: primary_target.id.clone(),
            current_active: primary_target.id.clone(),
            targets,
            last_failover: None,
            in_failover: false,
        };

        components.insert(component.to_string(), state);

        // Initialize stats
        {
            let mut stats = self.stats.write().await;
            stats.insert(component.to_string(), FailoverStats {
                total_failovers: 0,
                successful_failovers: 0,
                failed_failovers: 0,
                total_failbacks: 0,
                successful_failbacks: 0,
                average_failover_time: Duration::from_nanos(0),
                uptime_percentage: 100.0,
                current_active_target: Some(primary_target.id),
            });
        }

        tracing::info!(
            component = component,
            primary_target = primary_target.name,
            backup_count = targets.len() - 1,
            "Registered component for failover management"
        );

        Ok(())
    }

    /// Execute failover for a component
    pub async fn execute_failover(&self, component: &str) -> Result<()> {
        let start_time = Instant::now();
        
        tracing::info!(component = component, "Initiating failover");
        
        // Log failover initiation
        self.log_event(FailoverEvent {
            id: uuid::Uuid::new_v4(),
            component: component.to_string(),
            event_type: FailoverEventType::FailoverInitiated,
            from_target: None,
            to_target: None,
            timestamp: chrono::Utc::now(),
            duration: None,
            success: false,
            error_message: None,
            metadata: HashMap::new(),
        }).await;

        let failover_result = self.execute_failover_internal(component).await;
        let duration = start_time.elapsed();

        // Log completion event
        let event = match &failover_result {
            Ok((from_target, to_target)) => FailoverEvent {
                id: uuid::Uuid::new_v4(),
                component: component.to_string(),
                event_type: FailoverEventType::FailoverCompleted,
                from_target: Some(from_target.clone()),
                to_target: Some(to_target.clone()),
                timestamp: chrono::Utc::now(),
                duration: Some(duration),
                success: true,
                error_message: None,
                metadata: HashMap::new(),
            },
            Err(e) => FailoverEvent {
                id: uuid::Uuid::new_v4(),
                component: component.to_string(),
                event_type: FailoverEventType::FailoverFailed,
                from_target: None,
                to_target: None,
                timestamp: chrono::Utc::now(),
                duration: Some(duration),
                success: false,
                error_message: Some(e.to_string()),
                metadata: HashMap::new(),
            },
        };

        self.log_event(event).await;

        // Update statistics
        self.update_failover_stats(component, failover_result.is_ok(), duration).await;

        failover_result.map(|_| ())
    }

    /// Execute failback to primary target
    pub async fn execute_failback(&self, component: &str) -> Result<()> {
        if !self.config.auto_failback_enabled {
            return Err(anyhow::anyhow!("Auto-failback is disabled"));
        }

        let start_time = Instant::now();
        
        tracing::info!(component = component, "Initiating failback to primary");

        // Check if enough time has passed since last failover
        {
            let components = self.components.read().await;
            if let Some(state) = components.get(component) {
                if let Some(last_failover) = state.last_failover {
                    if last_failover.elapsed() < self.config.failback_delay {
                        return Err(anyhow::anyhow!(
                            "Failback attempted too soon after last failover"
                        ));
                    }
                }
            }
        }

        let failback_result = self.execute_failback_internal(component).await;
        let duration = start_time.elapsed();

        // Log event
        let event = match &failback_result {
            Ok((from_target, to_target)) => FailoverEvent {
                id: uuid::Uuid::new_v4(),
                component: component.to_string(),
                event_type: FailoverEventType::FailbackCompleted,
                from_target: Some(from_target.clone()),
                to_target: Some(to_target.clone()),
                timestamp: chrono::Utc::now(),
                duration: Some(duration),
                success: true,
                error_message: None,
                metadata: HashMap::new(),
            },
            Err(e) => FailoverEvent {
                id: uuid::Uuid::new_v4(),
                component: component.to_string(),
                event_type: FailoverEventType::FailbackFailed,
                from_target: None,
                to_target: None,
                timestamp: chrono::Utc::now(),
                duration: Some(duration),
                success: false,
                error_message: Some(e.to_string()),
                metadata: HashMap::new(),
            },
        };

        self.log_event(event).await;

        // Update statistics
        self.update_failback_stats(component, failback_result.is_ok(), duration).await;

        failback_result.map(|_| ())
    }

    /// Get current active target for a component
    pub async fn get_active_target(&self, component: &str) -> Option<FailoverTarget> {
        let components = self.components.read().await;
        components.get(component).and_then(|state| {
            state.targets.get(&state.current_active).cloned()
        })
    }

    /// Get all targets for a component
    pub async fn get_component_targets(&self, component: &str) -> Vec<FailoverTarget> {
        let components = self.components.read().await;
        components.get(component)
            .map(|state| state.targets.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Update target health status
    pub async fn update_target_health(
        &self,
        component: &str,
        target_id: &str,
        status: TargetStatus,
    ) -> Result<()> {
        let mut components = self.components.write().await;
        
        if let Some(state) = components.get_mut(component) {
            if let Some(target) = state.targets.get_mut(target_id) {
                let old_status = target.status.clone();
                target.status = status.clone();
                target.last_health_check = Some(Instant::now());

                // Update failure count
                match status {
                    TargetStatus::Healthy => {
                        target.consecutive_failures = 0;
                    }
                    TargetStatus::Unhealthy | TargetStatus::Offline => {
                        target.consecutive_failures += 1;
                    }
                    _ => {} // No change for degraded or maintenance
                }

                // Log health change event if status changed
                if old_status != status {
                    self.log_event(FailoverEvent {
                        id: uuid::Uuid::new_v4(),
                        component: component.to_string(),
                        event_type: FailoverEventType::TargetHealthChanged,
                        from_target: Some(target_id.to_string()),
                        to_target: None,
                        timestamp: chrono::Utc::now(),
                        duration: None,
                        success: true,
                        error_message: None,
                        metadata: {
                            let mut meta = HashMap::new();
                            meta.insert("old_status".to_string(), format!("{:?}", old_status));
                            meta.insert("new_status".to_string(), format!("{:?}", status));
                            meta
                        },
                    }).await;
                }

                // Trigger failover if current active target is unhealthy
                if target_id == &state.current_active 
                    && matches!(status, TargetStatus::Unhealthy | TargetStatus::Offline)
                    && target.consecutive_failures >= self.config.failure_threshold
                    && !state.in_failover
                {
                    // Trigger automatic failover
                    let component_name = component.to_string();
                    let manager = self.clone_for_background();
                    tokio::spawn(async move {
                        if let Err(e) = manager.execute_failover(&component_name).await {
                            tracing::error!(
                                component = component_name,
                                error = %e,
                                "Automatic failover failed"
                            );
                        }
                    });
                }
            }
        }

        Ok(())
    }

    /// Get failover statistics for a component
    pub async fn get_stats(&self, component: &str) -> Option<FailoverStats> {
        let stats = self.stats.read().await;
        stats.get(component).cloned()
    }

    /// Get recent failover events
    pub async fn get_events(&self, limit: Option<usize>) -> Vec<FailoverEvent> {
        let events = self.events.read().await;
        let limit = limit.unwrap_or(100);
        
        events.iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    // Private implementation methods

    async fn execute_failover_internal(&self, component: &str) -> Result<(String, String)> {
        let mut components = self.components.write().await;
        
        let state = components.get_mut(component)
            .ok_or_else(|| anyhow::anyhow!("Component not registered: {}", component))?;

        if state.in_failover {
            return Err(anyhow::anyhow!("Failover already in progress for component: {}", component));
        }

        state.in_failover = true;
        let from_target = state.current_active.clone();

        // Find best available target
        let best_target = self.find_best_available_target(state)?;
        
        // Perform failover steps
        self.stop_target(&from_target).await?;
        self.start_target(&best_target).await?;
        self.update_routing(component, &from_target, &best_target).await?;

        // Update state
        state.current_active = best_target.clone();
        state.last_failover = Some(Instant::now());
        state.in_failover = false;

        tracing::info!(
            component = component,
            from_target = from_target,
            to_target = best_target,
            "Failover completed successfully"
        );

        Ok((from_target, best_target))
    }

    async fn execute_failback_internal(&self, component: &str) -> Result<(String, String)> {
        let mut components = self.components.write().await;
        
        let state = components.get_mut(component)
            .ok_or_else(|| anyhow::anyhow!("Component not registered: {}", component))?;

        let from_target = state.current_active.clone();
        let to_target = state.primary_target.clone();

        // Check if primary is healthy
        if let Some(primary) = state.targets.get(&to_target) {
            if !matches!(primary.status, TargetStatus::Healthy) {
                return Err(anyhow::anyhow!("Primary target is not healthy for failback"));
            }
        }

        // Perform failback
        self.start_target(&to_target).await?;
        self.update_routing(component, &from_target, &to_target).await?;
        self.stop_target(&from_target).await?;

        // Update state
        state.current_active = to_target.clone();

        Ok((from_target, to_target))
    }

    fn find_best_available_target(&self, state: &ComponentFailoverState) -> Result<String> {
        let available_targets: Vec<_> = state.targets.values()
            .filter(|target| {
                target.id != state.current_active &&
                matches!(target.status, TargetStatus::Healthy | TargetStatus::Degraded)
            })
            .collect();

        if available_targets.is_empty() {
            return Err(anyhow::anyhow!("No available targets for failover"));
        }

        // Select target with highest priority (lowest priority number)
        let best_target = available_targets.iter()
            .min_by_key(|target| target.priority)
            .unwrap();

        Ok(best_target.id.clone())
    }

    async fn stop_target(&self, target_id: &str) -> Result<()> {
        tracing::debug!(target_id = target_id, "Stopping target");
        // Simulate stopping target
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    async fn start_target(&self, target_id: &str) -> Result<()> {
        tracing::debug!(target_id = target_id, "Starting target");
        // Simulate starting target
        tokio::time::sleep(Duration::from_millis(500)).await;
        Ok(())
    }

    async fn update_routing(&self, component: &str, from: &str, to: &str) -> Result<()> {
        tracing::debug!(
            component = component,
            from_target = from,
            to_target = to,
            "Updating routing"
        );
        // Simulate routing update
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(())
    }

    async fn log_event(&self, event: FailoverEvent) {
        let mut events = self.events.write().await;
        events.push(event);
        
        // Keep only recent events
        if events.len() > 1000 {
            events.drain(0..events.len() - 1000);
        }
    }

    async fn update_failover_stats(&self, component: &str, success: bool, duration: Duration) {
        let mut stats = self.stats.write().await;
        if let Some(component_stats) = stats.get_mut(component) {
            component_stats.total_failovers += 1;
            if success {
                component_stats.successful_failovers += 1;
                
                // Update average failover time
                let total_time = component_stats.average_failover_time.as_millis() as f32 * 
                    (component_stats.successful_failovers - 1) as f32 + duration.as_millis() as f32;
                component_stats.average_failover_time = Duration::from_millis(
                    (total_time / component_stats.successful_failovers as f32) as u64
                );
            } else {
                component_stats.failed_failovers += 1;
            }
        }
    }

    async fn update_failback_stats(&self, component: &str, success: bool, _duration: Duration) {
        let mut stats = self.stats.write().await;
        if let Some(component_stats) = stats.get_mut(component) {
            component_stats.total_failbacks += 1;
            if success {
                component_stats.successful_failbacks += 1;
            }
        }
    }

    async fn start_health_monitoring(&self) {
        let components = self.components.clone();
        let health_check_interval = self.config.health_check_interval;
        let manager = self.clone_for_background();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(health_check_interval);
            
            loop {
                interval.tick().await;

                // Perform health checks for all components
                let component_list: Vec<String> = {
                    let components_guard = components.read().await;
                    components_guard.keys().cloned().collect()
                };

                for component in component_list {
                    if let Err(e) = manager.perform_health_checks(&component).await {
                        tracing::error!(
                            component = component,
                            error = %e,
                            "Health check failed"
                        );
                    }
                }
            }
        });
    }

    async fn perform_health_checks(&self, component: &str) -> Result<()> {
        let target_ids: Vec<String> = {
            let components = self.components.read().await;
            components.get(component)
                .map(|state| state.targets.keys().cloned().collect())
                .unwrap_or_default()
        };

        for target_id in target_ids {
            let health_status = self.check_target_health(&target_id).await;
            self.update_target_health(component, &target_id, health_status).await?;
        }

        Ok(())
    }

    async fn check_target_health(&self, _target_id: &str) -> TargetStatus {
        // Simulate health check
        // In a real implementation, this would ping the target or check its metrics
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // Randomly simulate health status for testing
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let health_score: f32 = rng.gen();
        
        if health_score > 0.9 {
            TargetStatus::Healthy
        } else if health_score > 0.7 {
            TargetStatus::Degraded
        } else {
            TargetStatus::Unhealthy
        }
    }

    fn clone_for_background(&self) -> Self {
        Self {
            config: self.config.clone(),
            components: self.components.clone(),
            events: self.events.clone(),
            stats: self.stats.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_failover_manager_creation() {
        let manager = FailoverManager::new().await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_component_registration() {
        let manager = FailoverManager::new().await.unwrap();
        
        let primary = FailoverTarget {
            id: "primary".to_string(),
            name: "Primary Server".to_string(),
            endpoint: "http://primary:8080".to_string(),
            priority: 1,
            capacity: 1.0,
            status: TargetStatus::Healthy,
            last_health_check: None,
            consecutive_failures: 0,
            metadata: HashMap::new(),
        };

        let backup = FailoverTarget {
            id: "backup".to_string(),
            name: "Backup Server".to_string(),
            endpoint: "http://backup:8080".to_string(),
            priority: 2,
            capacity: 0.8,
            status: TargetStatus::Healthy,
            last_health_check: None,
            consecutive_failures: 0,
            metadata: HashMap::new(),
        };

        let result = manager.register_component("test_service", primary, vec![backup]).await;
        assert!(result.is_ok());

        let active = manager.get_active_target("test_service").await;
        assert!(active.is_some());
        assert_eq!(active.unwrap().id, "primary");
    }

    #[tokio::test]
    async fn test_failover_execution() {
        let manager = FailoverManager::new().await.unwrap();
        
        let primary = FailoverTarget {
            id: "primary".to_string(),
            name: "Primary Server".to_string(),
            endpoint: "http://primary:8080".to_string(),
            priority: 1,
            capacity: 1.0,
            status: TargetStatus::Healthy,
            last_health_check: None,
            consecutive_failures: 0,
            metadata: HashMap::new(),
        };

        let backup = FailoverTarget {
            id: "backup".to_string(),
            name: "Backup Server".to_string(),
            endpoint: "http://backup:8080".to_string(),
            priority: 2,
            capacity: 0.8,
            status: TargetStatus::Healthy,
            last_health_check: None,
            consecutive_failures: 0,
            metadata: HashMap::new(),
        };

        manager.register_component("test_service", primary, vec![backup]).await.unwrap();

        // Execute failover
        let result = manager.execute_failover("test_service").await;
        assert!(result.is_ok());

        // Check that active target changed
        let active = manager.get_active_target("test_service").await;
        assert!(active.is_some());
        assert_eq!(active.unwrap().id, "backup");

        // Check statistics
        let stats = manager.get_stats("test_service").await;
        assert!(stats.is_some());
        let stats = stats.unwrap();
        assert_eq!(stats.total_failovers, 1);
        assert_eq!(stats.successful_failovers, 1);
    }
}