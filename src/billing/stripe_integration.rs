//! Stripe payment integration for SoulBox billing
//! 
//! Provides payment processing, subscription management, and invoice generation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::{Result, SoulBoxError};

/// Stripe configuration
#[derive(Debug, Clone)]
pub struct StripeConfig {
    pub api_key: String,
    pub webhook_secret: String,
    pub price_ids: PriceIds,
    pub enable_test_mode: bool,
}

#[derive(Debug, Clone)]
pub struct PriceIds {
    pub starter_monthly: String,
    pub pro_monthly: String,
    pub enterprise_monthly: String,
    pub usage_based_cpu: String,
    pub usage_based_storage: String,
}

/// Subscription plans
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionPlan {
    Free,
    Starter,
    Pro,
    Enterprise,
    Custom,
}

/// Subscription status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Active,
    PastDue,
    Canceled,
    Incomplete,
    IncompleteExpired,
    Trialing,
    Unpaid,
}

/// Customer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Customer {
    pub id: String,
    pub user_id: Uuid,
    pub stripe_customer_id: String,
    pub email: String,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Subscription information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub customer_id: String,
    pub stripe_subscription_id: String,
    pub plan: SubscriptionPlan,
    pub status: SubscriptionStatus,
    pub current_period_start: DateTime<Utc>,
    pub current_period_end: DateTime<Utc>,
    pub cancel_at_period_end: bool,
    pub metadata: HashMap<String, String>,
}

/// Payment method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMethod {
    pub id: String,
    pub customer_id: String,
    pub stripe_payment_method_id: String,
    pub card_last4: Option<String>,
    pub card_brand: Option<String>,
    pub is_default: bool,
}

/// Invoice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub id: String,
    pub stripe_invoice_id: String,
    pub customer_id: String,
    pub subscription_id: Option<String>,
    pub amount_total: i64,
    pub amount_paid: i64,
    pub amount_due: i64,
    pub currency: String,
    pub status: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub pdf_url: Option<String>,
}

/// Usage record for metered billing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub subscription_item_id: String,
    pub quantity: i64,
    pub timestamp: DateTime<Utc>,
    pub action: UsageAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageAction {
    Set,
    Increment,
}

/// Webhook event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub id: String,
    pub event_type: String,
    pub data: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

/// Stripe integration trait
#[async_trait]
pub trait StripeIntegration: Send + Sync {
    /// Create a new customer
    async fn create_customer(&self, email: &str, name: Option<&str>) -> Result<Customer>;
    
    /// Get customer by ID
    async fn get_customer(&self, customer_id: &str) -> Result<Customer>;
    
    /// Create a subscription
    async fn create_subscription(
        &self,
        customer_id: &str,
        plan: SubscriptionPlan,
    ) -> Result<Subscription>;
    
    /// Update subscription
    async fn update_subscription(
        &self,
        subscription_id: &str,
        plan: SubscriptionPlan,
    ) -> Result<Subscription>;
    
    /// Cancel subscription
    async fn cancel_subscription(
        &self,
        subscription_id: &str,
        at_period_end: bool,
    ) -> Result<Subscription>;
    
    /// Add payment method
    async fn add_payment_method(
        &self,
        customer_id: &str,
        payment_method_id: &str,
    ) -> Result<PaymentMethod>;
    
    /// Set default payment method
    async fn set_default_payment_method(
        &self,
        customer_id: &str,
        payment_method_id: &str,
    ) -> Result<()>;
    
    /// Create usage record
    async fn create_usage_record(&self, record: UsageRecord) -> Result<()>;
    
    /// List invoices
    async fn list_invoices(&self, customer_id: &str) -> Result<Vec<Invoice>>;
    
    /// Process webhook
    async fn process_webhook(&self, payload: &str, signature: &str) -> Result<WebhookEvent>;
}

/// Mock Stripe client for development
pub struct MockStripeClient {
    config: StripeConfig,
}

impl MockStripeClient {
    pub fn new(config: StripeConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl StripeIntegration for MockStripeClient {
    async fn create_customer(&self, email: &str, name: Option<&str>) -> Result<Customer> {
        Ok(Customer {
            id: Uuid::new_v4().to_string(),
            user_id: Uuid::new_v4(),
            stripe_customer_id: format!("cus_{}", Uuid::new_v4().to_string().replace("-", "")[..14].to_string()),
            email: email.to_string(),
            name: name.map(|n| n.to_string()),
            created_at: Utc::now(),
        })
    }
    
    async fn get_customer(&self, customer_id: &str) -> Result<Customer> {
        Ok(Customer {
            id: customer_id.to_string(),
            user_id: Uuid::new_v4(),
            stripe_customer_id: customer_id.to_string(),
            email: "user@example.com".to_string(),
            name: Some("Test User".to_string()),
            created_at: Utc::now(),
        })
    }
    
    async fn create_subscription(
        &self,
        customer_id: &str,
        plan: SubscriptionPlan,
    ) -> Result<Subscription> {
        Ok(Subscription {
            id: Uuid::new_v4().to_string(),
            customer_id: customer_id.to_string(),
            stripe_subscription_id: format!("sub_{}", Uuid::new_v4().to_string().replace("-", "")[..14].to_string()),
            plan,
            status: SubscriptionStatus::Active,
            current_period_start: Utc::now(),
            current_period_end: Utc::now() + chrono::Duration::days(30),
            cancel_at_period_end: false,
            metadata: HashMap::new(),
        })
    }
    
    async fn update_subscription(
        &self,
        subscription_id: &str,
        plan: SubscriptionPlan,
    ) -> Result<Subscription> {
        Ok(Subscription {
            id: subscription_id.to_string(),
            customer_id: "cus_test".to_string(),
            stripe_subscription_id: subscription_id.to_string(),
            plan,
            status: SubscriptionStatus::Active,
            current_period_start: Utc::now(),
            current_period_end: Utc::now() + chrono::Duration::days(30),
            cancel_at_period_end: false,
            metadata: HashMap::new(),
        })
    }
    
    async fn cancel_subscription(
        &self,
        subscription_id: &str,
        at_period_end: bool,
    ) -> Result<Subscription> {
        Ok(Subscription {
            id: subscription_id.to_string(),
            customer_id: "cus_test".to_string(),
            stripe_subscription_id: subscription_id.to_string(),
            plan: SubscriptionPlan::Free,
            status: if at_period_end {
                SubscriptionStatus::Active
            } else {
                SubscriptionStatus::Canceled
            },
            current_period_start: Utc::now(),
            current_period_end: Utc::now() + chrono::Duration::days(30),
            cancel_at_period_end: at_period_end,
            metadata: HashMap::new(),
        })
    }
    
    async fn add_payment_method(
        &self,
        customer_id: &str,
        payment_method_id: &str,
    ) -> Result<PaymentMethod> {
        Ok(PaymentMethod {
            id: Uuid::new_v4().to_string(),
            customer_id: customer_id.to_string(),
            stripe_payment_method_id: payment_method_id.to_string(),
            card_last4: Some("4242".to_string()),
            card_brand: Some("Visa".to_string()),
            is_default: true,
        })
    }
    
    async fn set_default_payment_method(
        &self,
        _customer_id: &str,
        _payment_method_id: &str,
    ) -> Result<()> {
        Ok(())
    }
    
    async fn create_usage_record(&self, _record: UsageRecord) -> Result<()> {
        Ok(())
    }
    
    async fn list_invoices(&self, customer_id: &str) -> Result<Vec<Invoice>> {
        Ok(vec![
            Invoice {
                id: Uuid::new_v4().to_string(),
                stripe_invoice_id: format!("in_{}", Uuid::new_v4().to_string().replace("-", "")[..14].to_string()),
                customer_id: customer_id.to_string(),
                subscription_id: Some("sub_test".to_string()),
                amount_total: 4900,
                amount_paid: 4900,
                amount_due: 0,
                currency: "usd".to_string(),
                status: "paid".to_string(),
                period_start: Utc::now() - chrono::Duration::days(30),
                period_end: Utc::now(),
                pdf_url: Some("https://example.com/invoice.pdf".to_string()),
            }
        ])
    }
    
    async fn process_webhook(&self, _payload: &str, signature: &str) -> Result<WebhookEvent> {
        // Verify signature in production
        if !self.config.enable_test_mode && signature != self.config.webhook_secret {
            return Err(SoulBoxError::Unauthorized {
                message: "Invalid webhook signature".to_string(),
                user_id: None,
                security_context: None,
            });
        }
        
        Ok(WebhookEvent {
            id: Uuid::new_v4().to_string(),
            event_type: "invoice.payment_succeeded".to_string(),
            data: serde_json::json!({
                "object": "event",
                "type": "invoice.payment_succeeded"
            }),
            timestamp: Utc::now(),
        })
    }
}

/// Webhook processor
pub struct WebhookProcessor {
    stripe_client: Box<dyn StripeIntegration>,
}

impl WebhookProcessor {
    pub fn new(stripe_client: Box<dyn StripeIntegration>) -> Self {
        Self { stripe_client }
    }
    
    /// Process webhook event
    pub async fn process_event(&self, event: WebhookEvent) -> Result<()> {
        match event.event_type.as_str() {
            "customer.subscription.created" => self.handle_subscription_created(event).await,
            "customer.subscription.updated" => self.handle_subscription_updated(event).await,
            "customer.subscription.deleted" => self.handle_subscription_deleted(event).await,
            "invoice.payment_succeeded" => self.handle_payment_succeeded(event).await,
            "invoice.payment_failed" => self.handle_payment_failed(event).await,
            _ => {
                tracing::debug!("Unhandled webhook event type: {}", event.event_type);
                Ok(())
            }
        }
    }
    
    async fn handle_subscription_created(&self, event: WebhookEvent) -> Result<()> {
        tracing::info!("Subscription created: {:?}", event.data);
        // Update database with new subscription
        Ok(())
    }
    
    async fn handle_subscription_updated(&self, event: WebhookEvent) -> Result<()> {
        tracing::info!("Subscription updated: {:?}", event.data);
        // Update subscription in database
        Ok(())
    }
    
    async fn handle_subscription_deleted(&self, event: WebhookEvent) -> Result<()> {
        tracing::info!("Subscription deleted: {:?}", event.data);
        // Mark subscription as canceled in database
        Ok(())
    }
    
    async fn handle_payment_succeeded(&self, event: WebhookEvent) -> Result<()> {
        tracing::info!("Payment succeeded: {:?}", event.data);
        // Update payment status in database
        Ok(())
    }
    
    async fn handle_payment_failed(&self, event: WebhookEvent) -> Result<()> {
        tracing::warn!("Payment failed: {:?}", event.data);
        // Handle payment failure (notify user, retry, etc.)
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_mock_stripe_client() {
        let config = StripeConfig {
            api_key: "sk_test_123".to_string(),
            webhook_secret: "whsec_test".to_string(),
            price_ids: PriceIds {
                starter_monthly: "price_starter".to_string(),
                pro_monthly: "price_pro".to_string(),
                enterprise_monthly: "price_enterprise".to_string(),
                usage_based_cpu: "price_cpu".to_string(),
                usage_based_storage: "price_storage".to_string(),
            },
            enable_test_mode: true,
        };
        
        let client = MockStripeClient::new(config);
        
        // Test customer creation
        let customer = client.create_customer("test@example.com", Some("Test User")).await.unwrap();
        assert_eq!(customer.email, "test@example.com");
        
        // Test subscription creation
        let subscription = client.create_subscription(&customer.stripe_customer_id, SubscriptionPlan::Pro).await.unwrap();
        assert_eq!(subscription.status, SubscriptionStatus::Active);
    }
}