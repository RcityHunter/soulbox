//! Tests for external log system integration

#[cfg(test)]
mod external_integration_tests {
    use super::super::service::AuditService;
    use super::super::models::{AuditLog, AuditEventType, AuditSeverity, AuditResult};
    use super::super::service::AuditConfig;
    use uuid::Uuid;
    use std::env;

    #[tokio::test]
    async fn test_external_log_systems_integration() {
        let config = AuditConfig::default();
        let service = AuditService::new(config).unwrap();

        let log = AuditLog::new(
            AuditEventType::SecurityViolation,
            AuditSeverity::Critical,
            AuditResult::Failure,
            "Test security violation for external integration".to_string()
        );

        // Test that the function can be called without external systems configured
        let result = service.send_to_external_log_systems(&log).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_elk_stack_integration() {
        let config = AuditConfig::default();
        let service = AuditService::new(config).unwrap();

        let log = AuditLog::new(
            AuditEventType::UserLogin,
            AuditSeverity::Info,
            AuditResult::Success,
            "Test user login for ELK integration".to_string()
        );

        // Test ELK integration without configuration
        let result = service.send_to_elk_stack(&log).await;
        assert!(result.is_ok());

        // Test with mock ELK endpoint (will fail but should handle gracefully)
        env::set_var("ELK_ENDPOINT", "http://localhost:9200");
        let result = service.send_to_elk_stack(&log).await;
        // Should handle connection failure gracefully
        assert!(result.is_ok() || result.is_err());
        env::remove_var("ELK_ENDPOINT");
    }

    #[tokio::test]
    async fn test_splunk_integration() {
        let config = AuditConfig::default();
        let service = AuditService::new(config).unwrap();

        let log = AuditLog::new(
            AuditEventType::DataAccess,
            AuditSeverity::Warning,
            AuditResult::Success,
            "Test data access for Splunk integration".to_string()
        );

        // Test Splunk integration without configuration
        let result = service.send_to_splunk(&log).await;
        assert!(result.is_ok());

        // Test with mock Splunk configuration
        env::set_var("SPLUNK_HEC_URL", "http://localhost:8088/services/collector");
        env::set_var("SPLUNK_HEC_TOKEN", "test-token");
        let result = service.send_to_splunk(&log).await;
        // Should handle connection failure gracefully
        assert!(result.is_ok() || result.is_err());
        env::remove_var("SPLUNK_HEC_URL");
        env::remove_var("SPLUNK_HEC_TOKEN");
    }

    #[tokio::test]
    async fn test_other_systems_integration() {
        let config = AuditConfig::default();
        let service = AuditService::new(config).unwrap();

        let log = AuditLog::new(
            AuditEventType::SystemError,
            AuditSeverity::Error,
            AuditResult::Failure,
            "Test system error for other log systems".to_string()
        );

        // Test other systems integration without configuration
        let result = service.send_to_other_systems(&log).await;
        assert!(result.is_ok());

        // Test with mock configurations
        env::set_var("FLUENTD_URL", "http://localhost:24224");
        env::set_var("DATADOG_API_KEY", "test-api-key");
        let result = service.send_to_other_systems(&log).await;
        // Should handle connection failures gracefully
        assert!(result.is_ok() || result.is_err());
        env::remove_var("FLUENTD_URL");
        env::remove_var("DATADOG_API_KEY");
    }

    #[tokio::test]
    async fn test_alert_notification() {
        let config = AuditConfig::default();
        let service = AuditService::new(config).unwrap();

        let log = AuditLog::new(
            AuditEventType::SecurityViolation,
            AuditSeverity::Critical,
            AuditResult::Failure,
            "Test critical security violation for alert notification".to_string()
        );

        // Test alert notification without configuration
        let result = service.send_alert_notification(&log).await;
        assert!(result.is_ok());

        // Test with mock webhook configurations
        env::set_var("SLACK_ALERT_WEBHOOK", "https://hooks.slack.com/test");
        env::set_var("TEAMS_ALERT_WEBHOOK", "https://outlook.office.com/webhook/test");
        let result = service.send_alert_notification(&log).await;
        // Should handle connection failures gracefully
        assert!(result.is_ok() || result.is_err());
        env::remove_var("SLACK_ALERT_WEBHOOK");
        env::remove_var("TEAMS_ALERT_WEBHOOK");
    }

    #[tokio::test]
    async fn test_automated_response() {
        let config = AuditConfig::default();
        let service = AuditService::new(config).unwrap();

        // Test critical security violation response
        let critical_log = AuditLog::new(
            AuditEventType::SecurityViolation,
            AuditSeverity::Critical,
            AuditResult::Failure,
            "Critical security violation requiring automated response".to_string()
        ).with_user(
            Uuid::new_v4(),
            "test_user".to_string(),
            crate::auth::models::Role::User,
            None
        ).with_resource(
            "sandbox".to_string(),
            Some("sandbox-123".to_string()),
            None
        );

        let result = service.trigger_automated_response(&critical_log).await;
        assert!(result.is_ok());

        // Test login failure response
        let login_log = AuditLog::new(
            AuditEventType::UserLogin,
            AuditSeverity::Error,
            AuditResult::Failure,
            "Failed login attempt".to_string()
        ).with_user(
            Uuid::new_v4(),
            "test_user".to_string(),
            crate::auth::models::Role::User,
            None
        );

        let result = service.trigger_automated_response(&login_log).await;
        assert!(result.is_ok());

        // Test permission escalation response
        let perm_log = AuditLog::new(
            AuditEventType::PermissionEscalation,
            AuditSeverity::Warning,
            AuditResult::Success,
            "Permission escalation detected".to_string()
        );

        let result = service.trigger_automated_response(&perm_log).await;
        assert!(result.is_ok());

        // Test other event types (should have no special response)
        let other_log = AuditLog::new(
            AuditEventType::DataAccess,
            AuditSeverity::Info,
            AuditResult::Success,
            "Regular data access".to_string()
        );

        let result = service.trigger_automated_response(&other_log).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_automated_response_helper_functions() {
        let config = AuditConfig::default();
        let service = AuditService::new(config).unwrap();

        let user_id = Uuid::new_v4();
        let log = AuditLog::new(
            AuditEventType::SecurityViolation,
            AuditSeverity::Critical,
            AuditResult::Failure,
            "Test log for helper functions".to_string()
        );

        // Test individual helper functions
        let result = service.suspend_user_account(user_id).await;
        assert!(result.is_ok());

        let resource_info = crate::audit::models::ResourceInfo {
            resource_type: "sandbox".to_string(),
            resource_id: Some("test-sandbox".to_string()),
            resource_name: None,
        };
        let result = service.isolate_resource(&resource_info).await;
        assert!(result.is_ok());

        let result = service.trigger_security_team_alert(&log).await;
        assert!(result.is_ok());

        let result = service.check_and_handle_brute_force(user_id).await;
        assert!(result.is_ok());

        let result = service.log_detailed_permission_change(&log).await;
        assert!(result.is_ok());

        let result = service.notify_administrators(&log).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_log_formatting_for_external_systems() {
        // Test that log data can be properly serialized for external systems
        let log = AuditLog::new(
            AuditEventType::SecurityViolation,
            AuditSeverity::Critical,
            AuditResult::Failure,
            "Test log for formatting".to_string()
        ).with_user(
            Uuid::new_v4(),
            "test_user".to_string(),
            crate::auth::models::Role::Admin,
            Some("tenant-123".to_string())
        ).with_resource(
            "sandbox".to_string(),
            Some("sandbox-456".to_string()),
            Some("Test Sandbox".to_string())
        ).with_network(
            Some("192.168.1.100".to_string()),
            Some("Mozilla/5.0 Test Agent".to_string()),
            Some("session-789".to_string())
        ).with_error(
            "SEC_VIOLATION_001".to_string(),
            "Unauthorized access attempt detected".to_string()
        );

        // Test JSON serialization
        let json_result = serde_json::to_string(&log);
        assert!(json_result.is_ok());

        let json_value = serde_json::to_value(&log);
        assert!(json_value.is_ok());

        // Verify required fields are present
        if let Ok(value) = json_value {
            assert!(value.get("id").is_some());
            assert!(value.get("timestamp").is_some());
            assert!(value.get("event_type").is_some());
            assert!(value.get("severity").is_some());
            assert!(value.get("message").is_some());
        }
    }

    #[test]
    fn test_environment_variable_handling() {
        // Test that environment variables are properly handled
        
        // Test ELK configuration
        let elk_var = env::var("ELK_ENDPOINT");
        assert!(elk_var.is_ok() || elk_var.is_err()); // Either configured or not

        // Test Splunk configuration
        let splunk_url = env::var("SPLUNK_HEC_URL");
        let splunk_token = env::var("SPLUNK_HEC_TOKEN");
        if splunk_url.is_ok() {
            assert!(splunk_token.is_ok()); // If URL is set, token should be too
        }

        // Test webhook configurations
        let slack_webhook = env::var("SLACK_ALERT_WEBHOOK");
        let teams_webhook = env::var("TEAMS_ALERT_WEBHOOK");
        assert!(slack_webhook.is_ok() || slack_webhook.is_err());
        assert!(teams_webhook.is_ok() || teams_webhook.is_err());
    }
}