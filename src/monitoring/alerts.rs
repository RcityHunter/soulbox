//! Alert definitions and triggers
//! 
//! This module provides comprehensive alerting capabilities including
//! threshold-based alerts, anomaly detection, and notification management.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

use super::{PerformanceMetrics, HealthStatus};

/// Alert severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

/// Alert status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertStatus {
    Active,
    Acknowledged,
    Resolved,
    Suppressed,
}

/// Alert condition types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertCondition {
    /// Threshold-based alert (metric > threshold)
    Threshold {
        metric: String,
        operator: ComparisonOperator,
        threshold: f64,
        duration: Duration,
    },
    /// Rate of change alert
    RateOfChange {
        metric: String,
        rate_threshold: f64,
        time_window: Duration,
    },
    /// Anomaly detection alert
    AnomalyDetection {
        metric: String,
        sensitivity: f64,
        baseline_window: Duration,
    },
    /// Health check failure
    HealthCheckFailure {
        component: String,
        consecutive_failures: u32,
    },
    /// Custom condition
    Custom {
        name: String,
        expression: String,
    },
}

/// Comparison operators for threshold alerts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComparisonOperator {
    GreaterThan,
    LessThan,
    Equal,
    NotEqual,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

/// Alert rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub condition: AlertCondition,
    pub severity: AlertSeverity,
    pub enabled: bool,
    pub notification_channels: Vec<String>,
    pub labels: HashMap<String, String>,
    pub runbook_url: Option<String>,
    pub suppress_for: Option<Duration>,
}

/// Active alert instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub severity: AlertSeverity,
    pub status: AlertStatus,
    pub triggered_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub message: String,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
    pub value: Option<f64>,
}

/// Notification channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub id: String,
    pub name: String,
    pub channel_type: NotificationChannelType,
    pub config: HashMap<String, String>,
    pub enabled: bool,
}

/// Types of notification channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationChannelType {
    Email,
    Slack,
    Webhook,
    PagerDuty,
    Teams,
    Discord,
}

/// Alert statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertStats {
    pub total_alerts: u64,
    pub active_alerts: u64,
    pub critical_alerts: u64,
    pub alerts_by_severity: HashMap<AlertSeverity, u64>,
    pub mean_time_to_acknowledge: Duration,
    pub mean_time_to_resolve: Duration,
    pub alert_rate_per_hour: f64,
}

/// Main alert manager
pub struct AlertManager {
    rules: Arc<RwLock<HashMap<String, AlertRule>>>,
    active_alerts: Arc<RwLock<HashMap<String, Alert>>>,
    alert_history: Arc<RwLock<VecDeque<Alert>>>,
    notification_channels: Arc<RwLock<HashMap<String, NotificationChannel>>>,
    alert_tx: mpsc::UnboundedSender<AlertEvent>,
    metric_buffer: Arc<RwLock<HashMap<String, VecDeque<MetricSample>>>>,
    stats: Arc<RwLock<AlertStats>>,
}

/// Alert events for processing
#[derive(Debug)]
enum AlertEvent {
    MetricReceived { metric: String, value: f64, timestamp: DateTime<Utc> },
    HealthCheckFailed { component: String },
    RuleUpdated { rule: AlertRule },
    AlertAcknowledged { alert_id: String },
    AlertResolved { alert_id: String },
}

/// Metric sample for analysis
#[derive(Debug, Clone)]
struct MetricSample {
    value: f64,
    timestamp: DateTime<Utc>,
}

impl AlertManager {
    /// Create a new alert manager
    pub async fn new() -> Result<Self> {
        let (alert_tx, alert_rx) = mpsc::unbounded_channel();

        let manager = Self {
            rules: Arc::new(RwLock::new(HashMap::new())),
            active_alerts: Arc::new(RwLock::new(HashMap::new())),
            alert_history: Arc::new(RwLock::new(VecDeque::new())),
            notification_channels: Arc::new(RwLock::new(HashMap::new())),
            alert_tx,
            metric_buffer: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(AlertStats::default())),
        };

        // Start alert processing
        manager.start_alert_processor(alert_rx).await;

        // Load default alert rules
        manager.load_default_rules().await?;

        Ok(manager)
    }

    /// Add an alert rule
    pub async fn add_rule(&self, rule: AlertRule) -> Result<()> {
        {
            let mut rules = self.rules.write().await;
            rules.insert(rule.id.clone(), rule.clone());
        }

        // Notify processor about rule update
        self.alert_tx.send(AlertEvent::RuleUpdated { rule })
            .context("Failed to send rule update event")?;

        tracing::info!(rule_id = rule.id, rule_name = rule.name, "Added alert rule");
        Ok(())
    }

    /// Remove an alert rule
    pub async fn remove_rule(&self, rule_id: &str) -> Result<()> {
        {
            let mut rules = self.rules.write().await;
            rules.remove(rule_id);
        }

        tracing::info!(rule_id = rule_id, "Removed alert rule");
        Ok(())
    }

    /// Get all alert rules
    pub async fn get_rules(&self) -> Vec<AlertRule> {
        let rules = self.rules.read().await;
        rules.values().cloned().collect()
    }

    /// Get active alerts
    pub async fn get_active_alerts(&self) -> Vec<Alert> {
        let alerts = self.active_alerts.read().await;
        alerts.values().cloned().collect()
    }

    /// Get alert history
    pub async fn get_alert_history(&self, limit: Option<usize>) -> Vec<Alert> {
        let history = self.alert_history.read().await;
        let limit = limit.unwrap_or(100);
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Acknowledge an alert
    pub async fn acknowledge_alert(&self, alert_id: &str) -> Result<()> {
        self.alert_tx.send(AlertEvent::AlertAcknowledged { 
            alert_id: alert_id.to_string() 
        }).context("Failed to send alert acknowledgment")?;

        Ok(())
    }

    /// Resolve an alert
    pub async fn resolve_alert(&self, alert_id: &str) -> Result<()> {
        self.alert_tx.send(AlertEvent::AlertResolved { 
            alert_id: alert_id.to_string() 
        }).context("Failed to send alert resolution")?;

        Ok(())
    }

    /// Submit a metric value for monitoring
    pub async fn submit_metric(&self, metric: &str, value: f64) -> Result<()> {
        self.alert_tx.send(AlertEvent::MetricReceived {
            metric: metric.to_string(),
            value,
            timestamp: Utc::now(),
        }).context("Failed to send metric event")?;

        Ok(())
    }

    /// Report health check failure
    pub async fn report_health_check_failure(&self, component: &str) -> Result<()> {
        self.alert_tx.send(AlertEvent::HealthCheckFailed {
            component: component.to_string(),
        }).context("Failed to send health check failure event")?;

        Ok(())
    }

    /// Add notification channel
    pub async fn add_notification_channel(&self, channel: NotificationChannel) -> Result<()> {
        let mut channels = self.notification_channels.write().await;
        channels.insert(channel.id.clone(), channel.clone());

        tracing::info!(
            channel_id = channel.id,
            channel_name = channel.name,
            channel_type = ?channel.channel_type,
            "Added notification channel"
        );

        Ok(())
    }

    /// Get alert statistics
    pub async fn get_stats(&self) -> AlertStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Load default alert rules
    async fn load_default_rules(&self) -> Result<()> {
        let default_rules = vec![
            AlertRule {
                id: "high_cpu_usage".to_string(),
                name: "High CPU Usage".to_string(),
                description: "CPU usage is above 80% for more than 5 minutes".to_string(),
                condition: AlertCondition::Threshold {
                    metric: "cpu_usage_percent".to_string(),
                    operator: ComparisonOperator::GreaterThan,
                    threshold: 80.0,
                    duration: Duration::from_secs(300),
                },
                severity: AlertSeverity::Warning,
                enabled: true,
                notification_channels: vec!["default".to_string()],
                labels: HashMap::new(),
                runbook_url: Some("https://docs.soulbox.dev/runbooks/high-cpu".to_string()),
                suppress_for: Some(Duration::from_secs(600)),
            },
            AlertRule {
                id: "high_memory_usage".to_string(),
                name: "High Memory Usage".to_string(),
                description: "Memory usage is above 85% for more than 3 minutes".to_string(),
                condition: AlertCondition::Threshold {
                    metric: "memory_usage_percent".to_string(),
                    operator: ComparisonOperator::GreaterThan,
                    threshold: 85.0,
                    duration: Duration::from_secs(180),
                },
                severity: AlertSeverity::Critical,
                enabled: true,
                notification_channels: vec!["default".to_string()],
                labels: HashMap::new(),
                runbook_url: Some("https://docs.soulbox.dev/runbooks/high-memory".to_string()),
                suppress_for: Some(Duration::from_secs(300)),
            },
            AlertRule {
                id: "container_startup_slow".to_string(),
                name: "Slow Container Startup".to_string(),
                description: "Container startup time is above 10 seconds".to_string(),
                condition: AlertCondition::Threshold {
                    metric: "container_startup_duration_seconds".to_string(),
                    operator: ComparisonOperator::GreaterThan,
                    threshold: 10.0,
                    duration: Duration::from_secs(0),
                },
                severity: AlertSeverity::Warning,
                enabled: true,
                notification_channels: vec!["default".to_string()],
                labels: HashMap::new(),
                runbook_url: None,
                suppress_for: Some(Duration::from_secs(60)),
            },
            AlertRule {
                id: "error_rate_high".to_string(),
                name: "High Error Rate".to_string(),
                description: "Error rate is above 5% over the last 5 minutes".to_string(),
                condition: AlertCondition::RateOfChange {
                    metric: "error_rate_percent".to_string(),
                    rate_threshold: 5.0,
                    time_window: Duration::from_secs(300),
                },
                severity: AlertSeverity::Critical,
                enabled: true,
                notification_channels: vec!["default".to_string()],
                labels: HashMap::new(),
                runbook_url: Some("https://docs.soulbox.dev/runbooks/high-error-rate".to_string()),
                suppress_for: Some(Duration::from_secs(180)),
            },
        ];

        for rule in default_rules {
            self.add_rule(rule).await?;
        }

        Ok(())
    }

    /// Start alert processing loop
    async fn start_alert_processor(&self, mut alert_rx: mpsc::UnboundedReceiver<AlertEvent>) {
        let rules = self.rules.clone();
        let active_alerts = self.active_alerts.clone();
        let alert_history = self.alert_history.clone();
        let notification_channels = self.notification_channels.clone();
        let metric_buffer = self.metric_buffer.clone();
        let stats = self.stats.clone();

        tokio::spawn(async move {
            while let Some(event) = alert_rx.recv().await {
                match event {
                    AlertEvent::MetricReceived { metric, value, timestamp } => {
                        Self::process_metric_event(
                            &rules,
                            &active_alerts,
                            &alert_history,
                            &metric_buffer,
                            &stats,
                            metric,
                            value,
                            timestamp,
                        ).await;
                    }
                    AlertEvent::HealthCheckFailed { component } => {
                        Self::process_health_check_failure(
                            &rules,
                            &active_alerts,
                            &alert_history,
                            &stats,
                            component,
                        ).await;
                    }
                    AlertEvent::RuleUpdated { rule: _ } => {
                        // Rule already updated in the main structure
                        tracing::debug!("Alert rule updated");
                    }
                    AlertEvent::AlertAcknowledged { alert_id } => {
                        Self::acknowledge_alert_internal(&active_alerts, &alert_id).await;
                    }
                    AlertEvent::AlertResolved { alert_id } => {
                        Self::resolve_alert_internal(
                            &active_alerts,
                            &alert_history,
                            &alert_id,
                        ).await;
                    }
                }
            }
        });
    }

    /// Process metric event and check for threshold violations
    async fn process_metric_event(
        rules: &Arc<RwLock<HashMap<String, AlertRule>>>,
        active_alerts: &Arc<RwLock<HashMap<String, Alert>>>,
        alert_history: &Arc<RwLock<VecDeque<Alert>>>,
        metric_buffer: &Arc<RwLock<HashMap<String, VecDeque<MetricSample>>>>,
        stats: &Arc<RwLock<AlertStats>>,
        metric: String,
        value: f64,
        timestamp: DateTime<Utc>,
    ) {
        // Store metric sample
        {
            let mut buffer = metric_buffer.write().await;
            let samples = buffer.entry(metric.clone()).or_insert_with(VecDeque::new);
            samples.push_back(MetricSample { value, timestamp });
            
            // Keep only last 1000 samples
            if samples.len() > 1000 {
                samples.pop_front();
            }
        }

        // Check rules
        let rules_guard = rules.read().await;
        for rule in rules_guard.values() {
            if !rule.enabled {
                continue;
            }

            let should_trigger = match &rule.condition {
                AlertCondition::Threshold { metric: rule_metric, operator, threshold, duration } => {
                    if rule_metric == &metric {
                        Self::check_threshold_condition(&metric_buffer, &metric, operator, *threshold, *duration).await
                    } else {
                        false
                    }
                }
                AlertCondition::RateOfChange { metric: rule_metric, rate_threshold, time_window } => {
                    if rule_metric == &metric {
                        Self::check_rate_of_change_condition(&metric_buffer, &metric, *rate_threshold, *time_window).await
                    } else {
                        false
                    }
                }
                AlertCondition::AnomalyDetection { metric: rule_metric, sensitivity, baseline_window } => {
                    if rule_metric == &metric {
                        Self::check_anomaly_condition(&metric_buffer, &metric, *sensitivity, *baseline_window).await
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if should_trigger {
                Self::trigger_alert(
                    active_alerts,
                    alert_history,
                    stats,
                    rule,
                    Some(value),
                ).await;
            }
        }
    }

    /// Check threshold condition
    async fn check_threshold_condition(
        metric_buffer: &Arc<RwLock<HashMap<String, VecDeque<MetricSample>>>>,
        metric: &str,
        operator: &ComparisonOperator,
        threshold: f64,
        duration: Duration,
    ) -> bool {
        let buffer = metric_buffer.read().await;
        if let Some(samples) = buffer.get(metric) {
            let cutoff_time = Utc::now() - ChronoDuration::from_std(duration).unwrap_or_default();
            
            let recent_samples: Vec<_> = samples
                .iter()
                .filter(|sample| sample.timestamp >= cutoff_time)
                .collect();

            if recent_samples.is_empty() {
                return false;
            }

            // Check if all recent samples violate the threshold
            recent_samples.iter().all(|sample| {
                match operator {
                    ComparisonOperator::GreaterThan => sample.value > threshold,
                    ComparisonOperator::LessThan => sample.value < threshold,
                    ComparisonOperator::Equal => (sample.value - threshold).abs() < 0.001,
                    ComparisonOperator::NotEqual => (sample.value - threshold).abs() >= 0.001,
                    ComparisonOperator::GreaterThanOrEqual => sample.value >= threshold,
                    ComparisonOperator::LessThanOrEqual => sample.value <= threshold,
                }
            })
        } else {
            false
        }
    }

    /// Check rate of change condition
    async fn check_rate_of_change_condition(
        metric_buffer: &Arc<RwLock<HashMap<String, VecDeque<MetricSample>>>>,
        metric: &str,
        rate_threshold: f64,
        time_window: Duration,
    ) -> bool {
        let buffer = metric_buffer.read().await;
        if let Some(samples) = buffer.get(metric) {
            let cutoff_time = Utc::now() - ChronoDuration::from_std(time_window).unwrap_or_default();
            
            let recent_samples: Vec<_> = samples
                .iter()
                .filter(|sample| sample.timestamp >= cutoff_time)
                .collect();

            if recent_samples.len() < 2 {
                return false;
            }

            // Calculate rate of change
            let first_value = recent_samples.first().unwrap().value;
            let last_value = recent_samples.last().unwrap().value;
            let rate_of_change = ((last_value - first_value) / first_value) * 100.0;

            rate_of_change.abs() > rate_threshold
        } else {
            false
        }
    }

    /// Check anomaly detection condition (simplified implementation)
    async fn check_anomaly_condition(
        metric_buffer: &Arc<RwLock<HashMap<String, VecDeque<MetricSample>>>>,
        metric: &str,
        sensitivity: f64,
        baseline_window: Duration,
    ) -> bool {
        let buffer = metric_buffer.read().await;
        if let Some(samples) = buffer.get(metric) {
            let cutoff_time = Utc::now() - ChronoDuration::from_std(baseline_window).unwrap_or_default();
            
            let baseline_samples: Vec<_> = samples
                .iter()
                .filter(|sample| sample.timestamp >= cutoff_time)
                .collect();

            if baseline_samples.len() < 10 {
                return false;
            }

            // Calculate mean and standard deviation
            let mean = baseline_samples.iter().map(|s| s.value).sum::<f64>() / baseline_samples.len() as f64;
            let variance = baseline_samples.iter()
                .map(|s| (s.value - mean).powi(2))
                .sum::<f64>() / baseline_samples.len() as f64;
            let std_dev = variance.sqrt();

            // Check if latest value is anomalous
            if let Some(latest) = baseline_samples.last() {
                let z_score = (latest.value - mean) / std_dev;
                z_score.abs() > (3.0 - sensitivity) // Adjust threshold based on sensitivity
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Process health check failure
    async fn process_health_check_failure(
        rules: &Arc<RwLock<HashMap<String, AlertRule>>>,
        active_alerts: &Arc<RwLock<HashMap<String, Alert>>>,
        alert_history: &Arc<RwLock<VecDeque<Alert>>>,
        stats: &Arc<RwLock<AlertStats>>,
        component: String,
    ) {
        let rules_guard = rules.read().await;
        for rule in rules_guard.values() {
            if let AlertCondition::HealthCheckFailure { component: rule_component, consecutive_failures: _ } = &rule.condition {
                if rule_component == &component {
                    Self::trigger_alert(
                        active_alerts,
                        alert_history,
                        stats,
                        rule,
                        None,
                    ).await;
                }
            }
        }
    }

    /// Trigger an alert
    async fn trigger_alert(
        active_alerts: &Arc<RwLock<HashMap<String, Alert>>>,
        alert_history: &Arc<RwLock<VecDeque<Alert>>>,
        stats: &Arc<RwLock<AlertStats>>,
        rule: &AlertRule,
        value: Option<f64>,
    ) {
        let alert_id = format!("{}_{}", rule.id, Utc::now().timestamp());
        
        // Check if alert is already active
        {
            let active = active_alerts.read().await;
            if active.values().any(|a| a.rule_id == rule.id && a.status == AlertStatus::Active) {
                return; // Alert already active
            }
        }

        let alert = Alert {
            id: alert_id.clone(),
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            severity: rule.severity.clone(),
            status: AlertStatus::Active,
            triggered_at: Utc::now(),
            acknowledged_at: None,
            resolved_at: None,
            message: format!("Alert triggered: {}", rule.description),
            labels: rule.labels.clone(),
            annotations: HashMap::new(),
            value,
        };

        // Add to active alerts
        {
            let mut active = active_alerts.write().await;
            active.insert(alert_id.clone(), alert.clone());
        }

        // Add to history
        {
            let mut history = alert_history.write().await;
            history.push_back(alert.clone());
            
            // Keep only last 10000 alerts in history
            if history.len() > 10000 {
                history.pop_front();
            }
        }

        // Update statistics
        {
            let mut stats_guard = stats.write().await;
            stats_guard.total_alerts += 1;
            stats_guard.active_alerts += 1;
            if alert.severity == AlertSeverity::Critical {
                stats_guard.critical_alerts += 1;
            }
            *stats_guard.alerts_by_severity.entry(alert.severity.clone()).or_insert(0) += 1;
        }

        tracing::warn!(
            alert_id = alert_id,
            rule_id = rule.id,
            severity = ?alert.severity,
            "Alert triggered"
        );

        // TODO: Send notifications
    }

    /// Acknowledge alert internally
    async fn acknowledge_alert_internal(
        active_alerts: &Arc<RwLock<HashMap<String, Alert>>>,
        alert_id: &str,
    ) {
        let mut active = active_alerts.write().await;
        if let Some(alert) = active.get_mut(alert_id) {
            alert.status = AlertStatus::Acknowledged;
            alert.acknowledged_at = Some(Utc::now());
            
            tracing::info!(alert_id = alert_id, "Alert acknowledged");
        }
    }

    /// Resolve alert internally
    async fn resolve_alert_internal(
        active_alerts: &Arc<RwLock<HashMap<String, Alert>>>,
        alert_history: &Arc<RwLock<VecDeque<Alert>>>,
        alert_id: &str,
    ) {
        let mut alert_to_resolve = None;
        
        {
            let mut active = active_alerts.write().await;
            if let Some(mut alert) = active.remove(alert_id) {
                alert.status = AlertStatus::Resolved;
                alert.resolved_at = Some(Utc::now());
                alert_to_resolve = Some(alert);
            }
        }

        if let Some(resolved_alert) = alert_to_resolve {
            // Update in history
            {
                let mut history = alert_history.write().await;
                if let Some(pos) = history.iter().position(|a| a.id == alert_id) {
                    history[pos] = resolved_alert;
                }
            }

            tracing::info!(alert_id = alert_id, "Alert resolved");
        }
    }
}

impl Default for AlertStats {
    fn default() -> Self {
        Self {
            total_alerts: 0,
            active_alerts: 0,
            critical_alerts: 0,
            alerts_by_severity: HashMap::new(),
            mean_time_to_acknowledge: Duration::from_secs(0),
            mean_time_to_resolve: Duration::from_secs(0),
            alert_rate_per_hour: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_alert_manager_creation() {
        let manager = AlertManager::new().await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_add_alert_rule() {
        let manager = AlertManager::new().await.unwrap();
        
        let rule = AlertRule {
            id: "test_rule".to_string(),
            name: "Test Rule".to_string(),
            description: "Test description".to_string(),
            condition: AlertCondition::Threshold {
                metric: "test_metric".to_string(),
                operator: ComparisonOperator::GreaterThan,
                threshold: 100.0,
                duration: Duration::from_secs(60),
            },
            severity: AlertSeverity::Warning,
            enabled: true,
            notification_channels: vec!["default".to_string()],
            labels: HashMap::new(),
            runbook_url: None,
            suppress_for: None,
        };

        let result = manager.add_rule(rule).await;
        assert!(result.is_ok());

        let rules = manager.get_rules().await;
        assert!(rules.iter().any(|r| r.id == "test_rule"));
    }

    #[tokio::test]
    async fn test_metric_submission() {
        let manager = AlertManager::new().await.unwrap();
        
        let result = manager.submit_metric("cpu_usage_percent", 85.0).await;
        assert!(result.is_ok());
        
        // Give some time for processing
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_default_rules_loaded() {
        let manager = AlertManager::new().await.unwrap();
        
        let rules = manager.get_rules().await;
        assert!(!rules.is_empty());
        
        // Should have some default rules
        assert!(rules.iter().any(|r| r.id == "high_cpu_usage"));
        assert!(rules.iter().any(|r| r.id == "high_memory_usage"));
    }
}