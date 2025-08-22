//! Tests for alert notification functionality

#[cfg(test)]
mod alert_notification_tests {
    use super::super::alerts::{
        AlertManager, AlertRule, AlertCondition, ComparisonOperator, AlertSeverity,
        NotificationChannel, NotificationChannelType, Alert, AlertStatus
    };
    use chrono::Utc;
    use std::collections::HashMap;
    use std::time::Duration;
    use uuid::Uuid;

    async fn create_test_alert_manager() -> AlertManager {
        AlertManager::new().await.unwrap()
    }

    fn create_test_alert_rule() -> AlertRule {
        AlertRule {
            id: "test_rule".to_string(),
            name: "Test Alert Rule".to_string(),
            description: Some("Test rule for unit testing".to_string()),
            condition: AlertCondition::Threshold {
                metric: "cpu_usage".to_string(),
                operator: ComparisonOperator::GreaterThan,
                threshold: 80.0,
                duration: Duration::from_secs(300),
            },
            severity: AlertSeverity::Warning,
            enabled: true,
            labels: HashMap::new(),
            annotations: HashMap::new(),
            runbook_url: Some("https://runbook.example.com/cpu-high".to_string()),
            suppress_for: None,
        }
    }

    fn create_test_alert() -> Alert {
        Alert {
            id: Uuid::new_v4().to_string(),
            rule_id: "test_rule".to_string(),
            severity: AlertSeverity::Warning,
            status: AlertStatus::Active,
            message: "CPU usage is above 80%".to_string(),
            triggered_at: Utc::now(),
            acknowledged_at: None,
            resolved_at: None,
            labels: {
                let mut labels = HashMap::new();
                labels.insert("instance".to_string(), "server-001".to_string());
                labels.insert("environment".to_string(), "production".to_string());
                labels
            },
            annotations: {
                let mut annotations = HashMap::new();
                annotations.insert("summary".to_string(), "High CPU usage detected".to_string());
                annotations.insert("description".to_string(), "CPU usage has been above 80% for 5 minutes".to_string());
                annotations
            },
            value: Some(85.5),
        }
    }

    #[tokio::test]
    async fn test_email_notification_preparation() {
        let alert_manager = create_test_alert_manager().await;
        let alert = create_test_alert();
        let rule = create_test_alert_rule();
        
        let mut email_config = HashMap::new();
        email_config.insert("to".to_string(), "admin@soulbox.dev".to_string());
        email_config.insert("smtp_server".to_string(), "smtp.example.com".to_string());
        
        let email_channel = NotificationChannel {
            id: "email_channel".to_string(),
            name: "Admin Email".to_string(),
            channel_type: NotificationChannelType::Email,
            config: email_config,
            enabled: true,
        };
        
        // Test email notification (this will just log the preparation)
        let result = alert_manager.send_email_notification(&alert, &rule, &email_channel).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_webhook_notification_preparation() {
        let alert_manager = create_test_alert_manager().await;
        let alert = create_test_alert();
        let rule = create_test_alert_rule();
        
        let mut webhook_config = HashMap::new();
        webhook_config.insert("url".to_string(), "https://webhook.example.com/alerts".to_string());
        
        let webhook_channel = NotificationChannel {
            id: "webhook_channel".to_string(),
            name: "Generic Webhook".to_string(),
            channel_type: NotificationChannelType::Webhook,
            config: webhook_config,
            enabled: true,
        };
        
        let result = alert_manager.send_webhook_notification(&alert, &rule, &webhook_channel).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_slack_notification_preparation() {
        let alert_manager = create_test_alert_manager().await;
        let alert = create_test_alert();
        let rule = create_test_alert_rule();
        
        let mut slack_config = HashMap::new();
        slack_config.insert("webhook_url".to_string(), "https://hooks.slack.com/services/TEST/URL".to_string());
        
        let slack_channel = NotificationChannel {
            id: "slack_channel".to_string(),
            name: "Operations Slack".to_string(),
            channel_type: NotificationChannelType::Slack,
            config: slack_config,
            enabled: true,
        };
        
        let result = alert_manager.send_slack_notification(&alert, &rule, &slack_channel).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pagerduty_notification_preparation() {
        let alert_manager = create_test_alert_manager().await;
        let alert = create_test_alert();
        let rule = create_test_alert_rule();
        
        let mut pagerduty_config = HashMap::new();
        pagerduty_config.insert("integration_key".to_string(), "TEST_INTEGRATION_KEY".to_string());
        
        let pagerduty_channel = NotificationChannel {
            id: "pagerduty_channel".to_string(),
            name: "On-Call PagerDuty".to_string(),
            channel_type: NotificationChannelType::PagerDuty,
            config: pagerduty_config,
            enabled: true,
        };
        
        let result = alert_manager.send_pagerduty_notification(&alert, &rule, &pagerduty_channel).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_teams_notification_preparation() {
        let alert_manager = create_test_alert_manager().await;
        let alert = create_test_alert();
        let rule = create_test_alert_rule();
        
        let mut teams_config = HashMap::new();
        teams_config.insert("webhook_url".to_string(), "https://outlook.office.com/webhook/TEST".to_string());
        
        let teams_channel = NotificationChannel {
            id: "teams_channel".to_string(),
            name: "DevOps Teams".to_string(),
            channel_type: NotificationChannelType::Teams,
            config: teams_config,
            enabled: true,
        };
        
        let result = alert_manager.send_teams_notification(&alert, &rule, &teams_channel).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_discord_notification_preparation() {
        let alert_manager = create_test_alert_manager().await;
        let alert = create_test_alert();
        let rule = create_test_alert_rule();
        
        let mut discord_config = HashMap::new();
        discord_config.insert("webhook_url".to_string(), "https://discord.com/api/webhooks/TEST".to_string());
        
        let discord_channel = NotificationChannel {
            id: "discord_channel".to_string(),
            name: "SoulBox Discord".to_string(),
            channel_type: NotificationChannelType::Discord,
            config: discord_config,
            enabled: true,
        };
        
        let result = alert_manager.send_discord_notification(&alert, &rule, &discord_channel).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_notification_with_missing_config() {
        let alert_manager = create_test_alert_manager().await;
        let alert = create_test_alert();
        let rule = create_test_alert_rule();
        
        // Email channel without required 'to' config
        let email_channel = NotificationChannel {
            id: "invalid_email".to_string(),
            name: "Invalid Email".to_string(),
            channel_type: NotificationChannelType::Email,
            config: HashMap::new(), // Missing 'to' field
            enabled: true,
        };
        
        let result = alert_manager.send_email_notification(&alert, &rule, &email_channel).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Email 'to' address not configured"));
    }

    #[tokio::test]
    async fn test_alert_severity_display() {
        assert_eq!(AlertSeverity::Info.display(), "INFO");
        assert_eq!(AlertSeverity::Warning.display(), "WARNING");
        assert_eq!(AlertSeverity::Critical.display(), "CRITICAL");
        assert_eq!(AlertSeverity::Emergency.display(), "EMERGENCY");
    }

    #[tokio::test]
    async fn test_alert_display_method() {
        let alert = create_test_alert();
        assert_eq!(alert.severity_display(), "WARNING");
    }

    #[tokio::test]
    async fn test_format_alert_message_email() {
        let alert_manager = create_test_alert_manager().await;
        let alert = create_test_alert();
        let rule = create_test_alert_rule();
        
        let email_message = alert_manager.format_alert_message(&alert, &rule, "email").await;
        
        assert!(email_message.contains("Test Alert Rule"));
        assert!(email_message.contains("WARNING"));
        assert!(email_message.contains("CPU usage is above 80%"));
        assert!(email_message.contains("instance: server-001"));
        assert!(email_message.contains("summary: High CPU usage detected"));
    }

    #[tokio::test]
    async fn test_format_alert_message_default() {
        let alert_manager = create_test_alert_manager().await;
        let alert = create_test_alert();
        let rule = create_test_alert_rule();
        
        let simple_message = alert_manager.format_alert_message(&alert, &rule, "simple").await;
        
        assert_eq!(simple_message, "Test Alert Rule: CPU usage is above 80%");
    }

    #[tokio::test]
    async fn test_send_alert_notifications_integration() {
        let alert_manager = create_test_alert_manager().await;
        let alert = create_test_alert();
        let rule = create_test_alert_rule();
        
        // Add multiple notification channels
        let mut channels = HashMap::new();
        
        // Add email channel
        let mut email_config = HashMap::new();
        email_config.insert("to".to_string(), "admin@soulbox.dev".to_string());
        let email_channel = NotificationChannel {
            id: "email".to_string(),
            name: "Admin Email".to_string(),
            channel_type: NotificationChannelType::Email,
            config: email_config,
            enabled: true,
        };
        channels.insert("email".to_string(), email_channel);
        
        // Add disabled webhook channel (should be skipped)
        let mut webhook_config = HashMap::new();
        webhook_config.insert("url".to_string(), "https://webhook.example.com".to_string());
        let webhook_channel = NotificationChannel {
            id: "webhook".to_string(),
            name: "Disabled Webhook".to_string(),
            channel_type: NotificationChannelType::Webhook,
            config: webhook_config,
            enabled: false, // Disabled
        };
        channels.insert("webhook".to_string(), webhook_channel);
        
        // Replace notification channels
        {
            let mut notification_channels = alert_manager.notification_channels.write().await;
            *notification_channels = channels;
        }
        
        // Send notifications - this should only send to enabled channels
        alert_manager.send_alert_notifications(&alert, &rule).await;
        
        // If we reach here without panicking, the test passes
        // (since we can't easily verify actual notification sending in unit tests)
    }

    #[tokio::test]
    async fn test_different_alert_severities() {
        let alert_manager = create_test_alert_manager().await;
        let rule = create_test_alert_rule();
        
        let severities = vec![
            AlertSeverity::Info,
            AlertSeverity::Warning,
            AlertSeverity::Critical,
            AlertSeverity::Emergency,
        ];
        
        for severity in severities {
            let mut alert = create_test_alert();
            alert.severity = severity.clone();
            
            let mut slack_config = HashMap::new();
            slack_config.insert("webhook_url".to_string(), "https://hooks.slack.com/test".to_string());
            let slack_channel = NotificationChannel {
                id: "slack".to_string(),
                name: "Test Slack".to_string(),
                channel_type: NotificationChannelType::Slack,
                config: slack_config,
                enabled: true,
            };
            
            // Test that each severity level works
            let result = alert_manager.send_slack_notification(&alert, &rule, &slack_channel).await;
            assert!(result.is_ok(), "Failed for severity: {:?}", severity);
        }
    }

    #[tokio::test]
    async fn test_alert_with_different_statuses() {
        let alert_manager = create_test_alert_manager().await;
        let rule = create_test_alert_rule();
        
        let statuses = vec![
            AlertStatus::Active,
            AlertStatus::Acknowledged,
            AlertStatus::Resolved,
            AlertStatus::Suppressed,
        ];
        
        for status in statuses {
            let mut alert = create_test_alert();
            alert.status = status.clone();
            
            let mut pagerduty_config = HashMap::new();
            pagerduty_config.insert("integration_key".to_string(), "TEST_KEY".to_string());
            let pagerduty_channel = NotificationChannel {
                id: "pagerduty".to_string(),
                name: "Test PagerDuty".to_string(),
                channel_type: NotificationChannelType::PagerDuty,
                config: pagerduty_config,
                enabled: true,
            };
            
            // Test that each status works
            let result = alert_manager.send_pagerduty_notification(&alert, &rule, &pagerduty_channel).await;
            assert!(result.is_ok(), "Failed for status: {:?}", status);
        }
    }
}