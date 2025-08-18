//! Error Handling and Recovery
//! 
//! This module provides comprehensive error handling, transaction support,
//! and recovery mechanisms for the billing system.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};
use tracing::{error, warn, info, debug};

/// Error types specific to billing operations
#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    #[error("Financial calculation error: {message}")]
    FinancialCalculation { message: String },
    
    #[error("Storage operation failed: {operation} - {source}")]
    Storage { operation: String, source: anyhow::Error },
    
    #[error("Validation failed: {field} - {message}")]
    Validation { field: String, message: String },
    
    #[error("Concurrency conflict: {operation}")]
    Concurrency { operation: String },
    
    #[error("Rate limit exceeded: {limit} requests per {window:?}")]
    RateLimit { limit: u64, window: Duration },
    
    #[error("Transaction failed: {transaction_id} - {reason}")]
    Transaction { transaction_id: String, reason: String },
    
    #[error("Recovery failed after {attempts} attempts: {last_error}")]
    Recovery { attempts: u32, last_error: String },
}

/// Transaction context for billing operations
#[derive(Debug, Clone)]
pub struct TransactionContext {
    pub id: String,
    pub started_at: DateTime<Utc>,
    pub timeout: Duration,
    pub isolation_level: IsolationLevel,
    pub operations: Vec<String>,
}

impl TransactionContext {
    pub fn new(timeout: Duration) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            started_at: Utc::now(),
            timeout,
            isolation_level: IsolationLevel::ReadCommitted,
            operations: Vec::new(),
        }
    }

    pub fn add_operation(&mut self, operation: String) {
        self.operations.push(operation);
    }

    pub fn is_expired(&self) -> bool {
        let elapsed = Utc::now() - self.started_at;
        elapsed.to_std().unwrap_or(Duration::from_secs(0)) > self.timeout
    }
}

/// Database isolation levels
#[derive(Debug, Clone, Copy)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

/// Retry configuration for operations
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
    pub retryable_errors: Vec<String>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            retryable_errors: vec![
                "connection timeout".to_string(),
                "temporary failure".to_string(),
                "network error".to_string(),
                "database busy".to_string(),
            ],
        }
    }
}

impl RetryConfig {
    pub fn is_retryable(&self, error: &str) -> bool {
        self.retryable_errors.iter().any(|pattern| error.contains(pattern))
    }

    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let delay = self.base_delay.as_millis() as f64 * self.backoff_multiplier.powi(attempt as i32);
        Duration::from_millis(delay.min(self.max_delay.as_millis() as f64) as u64)
    }
}

/// Circuit breaker for protecting against cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    failure_threshold: u64,
    recovery_timeout: Duration,
    state: Arc<Mutex<CircuitBreakerState>>,
    failure_count: AtomicU64,
    last_failure_time: Arc<Mutex<Option<Instant>>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CircuitBreakerState {
    Closed,  // Normal operation
    Open,    // Failing, reject requests
    HalfOpen, // Testing if service recovered
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u64, recovery_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            recovery_timeout,
            state: Arc::new(Mutex::new(CircuitBreakerState::Closed)),
            failure_count: AtomicU64::new(0),
            last_failure_time: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn call<F, T, E>(&self, operation: F) -> Result<T, BillingError>
    where
        F: FnOnce() -> Result<T, E>,
        E: Into<anyhow::Error>,
    {
        // Check if circuit is open
        if self.is_open().await {
            return Err(BillingError::Recovery {
                attempts: self.failure_count.load(Ordering::Relaxed) as u32,
                last_error: "Circuit breaker is open".to_string(),
            });
        }

        // Execute operation
        match operation() {
            Ok(result) => {
                self.on_success().await;
                Ok(result)
            }
            Err(error) => {
                self.on_failure().await;
                Err(BillingError::Storage {
                    operation: "circuit_breaker_protected".to_string(),
                    source: error.into(),
                })
            }
        }
    }

    async fn is_open(&self) -> bool {
        let state = self.state.lock().await;
        match *state {
            CircuitBreakerState::Open => {
                // Check if we should transition to half-open
                if let Some(last_failure) = *self.last_failure_time.lock().await {
                    if last_failure.elapsed() > self.recovery_timeout {
                        drop(state);
                        let mut state = self.state.lock().await;
                        *state = CircuitBreakerState::HalfOpen;
                        return false;
                    }
                }
                true
            }
            CircuitBreakerState::HalfOpen => false,
            CircuitBreakerState::Closed => false,
        }
    }

    async fn on_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        let mut state = self.state.lock().await;
        *state = CircuitBreakerState::Closed;
    }

    async fn on_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        let mut last_failure = self.last_failure_time.lock().await;
        *last_failure = Some(Instant::now());

        if failures >= self.failure_threshold {
            let mut state = self.state.lock().await;
            *state = CircuitBreakerState::Open;
            warn!("Circuit breaker opened after {} failures", failures);
        }
    }
}

/// Error recovery coordinator
pub struct ErrorRecoveryService {
    retry_config: RetryConfig,
    circuit_breaker: CircuitBreaker,
    error_count: AtomicU64,
    recovery_count: AtomicU64,
}

impl ErrorRecoveryService {
    pub fn new(retry_config: RetryConfig) -> Self {
        Self {
            retry_config,
            circuit_breaker: CircuitBreaker::new(5, Duration::from_secs(60)),
            error_count: AtomicU64::new(0),
            recovery_count: AtomicU64::new(0),
        }
    }

    /// Execute operation with retry and circuit breaker protection
    pub async fn execute_with_recovery<F, T, E>(
        &self,
        operation_name: &str,
        operation: F,
    ) -> Result<T>
    where
        F: Fn() -> Result<T, E> + Send + Sync,
        E: Into<anyhow::Error> + std::fmt::Display,
        T: Send,
    {
        let mut last_error = None;
        
        for attempt in 0..self.retry_config.max_attempts {
            match self.circuit_breaker.call(&operation).await {
                Ok(result) => {
                    if attempt > 0 {
                        self.recovery_count.fetch_add(1, Ordering::Relaxed);
                        info!("Operation {} recovered after {} attempts", operation_name, attempt + 1);
                    }
                    return Ok(result);
                }
                Err(error) => {
                    self.error_count.fetch_add(1, Ordering::Relaxed);
                    last_error = Some(error);
                    
                    if attempt < self.retry_config.max_attempts - 1 {
                        let delay = self.retry_config.calculate_delay(attempt);
                        warn!(
                            "Operation {} failed on attempt {}, retrying in {:?}: {}",
                            operation_name, attempt + 1, delay, last_error.as_ref().unwrap()
                        );
                        sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap().into())
    }

    /// Execute operation within a transaction context
    pub async fn execute_transaction<F, T>(
        &self,
        mut context: TransactionContext,
        operation: F,
    ) -> Result<T>
    where
        F: FnOnce(&mut TransactionContext) -> Result<T> + Send,
        T: Send,
    {
        info!("Starting transaction {}", context.id);
        
        if context.is_expired() {
            return Err(BillingError::Transaction {
                transaction_id: context.id,
                reason: "Transaction timeout before start".to_string(),
            }.into());
        }

        let start_time = Instant::now();
        let result = operation(&mut context);
        let duration = start_time.elapsed();

        match result {
            Ok(value) => {
                info!("Transaction {} completed successfully in {:?}", context.id, duration);
                Ok(value)
            }
            Err(error) => {
                error!("Transaction {} failed in {:?}: {}", context.id, duration, error);
                Err(BillingError::Transaction {
                    transaction_id: context.id,
                    reason: error.to_string(),
                }.into())
            }
        }
    }

    /// Get error statistics
    pub fn get_error_stats(&self) -> ErrorStats {
        ErrorStats {
            total_errors: self.error_count.load(Ordering::Relaxed),
            total_recoveries: self.recovery_count.load(Ordering::Relaxed),
        }
    }
}

/// Error statistics
#[derive(Debug, Clone)]
pub struct ErrorStats {
    pub total_errors: u64,
    pub total_recoveries: u64,
}

/// Helper trait for converting errors to billing errors
pub trait ToBillingError<T> {
    fn to_billing_error(self, operation: &str) -> Result<T, BillingError>;
}

impl<T, E> ToBillingError<T> for Result<T, E>
where
    E: Into<anyhow::Error>,
{
    fn to_billing_error(self, operation: &str) -> Result<T, BillingError> {
        self.map_err(|e| BillingError::Storage {
            operation: operation.to_string(),
            source: e.into(),
        })
    }
}

/// Validation helpers
pub struct ValidationHelper;

impl ValidationHelper {
    pub fn validate_amount(amount: rust_decimal::Decimal, field: &str) -> Result<rust_decimal::Decimal, BillingError> {
        if amount < rust_decimal::Decimal::ZERO {
            return Err(BillingError::Validation {
                field: field.to_string(),
                message: "Amount cannot be negative".to_string(),
            });
        }
        
        if amount > rust_decimal::Decimal::new(1_000_000_0000, 4) {
            return Err(BillingError::Validation {
                field: field.to_string(),
                message: "Amount exceeds maximum allowed".to_string(),
            });
        }
        
        Ok(amount)
    }

    pub fn validate_user_id(user_id: uuid::Uuid) -> Result<uuid::Uuid, BillingError> {
        if user_id.is_nil() {
            return Err(BillingError::Validation {
                field: "user_id".to_string(),
                message: "User ID cannot be nil".to_string(),
            });
        }
        Ok(user_id)
    }

    pub fn validate_time_range(
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<(DateTime<Utc>, DateTime<Utc>), BillingError> {
        if start_time >= end_time {
            return Err(BillingError::Validation {
                field: "time_range".to_string(),
                message: "Start time must be before end time".to_string(),
            });
        }

        let max_range = chrono::Duration::days(365); // 1 year max
        if end_time - start_time > max_range {
            return Err(BillingError::Validation {
                field: "time_range".to_string(),
                message: "Time range cannot exceed 1 year".to_string(),
            });
        }

        Ok((start_time, end_time))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[test]
    fn test_retry_config() {
        let config = RetryConfig::default();
        
        assert!(config.is_retryable("connection timeout"));
        assert!(config.is_retryable("network error"));
        assert!(!config.is_retryable("invalid input"));
        
        let delay1 = config.calculate_delay(0);
        let delay2 = config.calculate_delay(1);
        assert!(delay2 > delay1);
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let breaker = CircuitBreaker::new(2, Duration::from_millis(100));
        
        // First failure
        let result1 = breaker.call(|| -> Result<(), &str> { Err("test error") }).await;
        assert!(result1.is_err());
        
        // Second failure should open circuit
        let result2 = breaker.call(|| -> Result<(), &str> { Err("test error") }).await;
        assert!(result2.is_err());
        
        // Third call should be rejected due to open circuit
        let result3 = breaker.call(|| -> Result<(), &str> { Ok(()) }).await;
        assert!(result3.is_err());
    }

    #[tokio::test]
    async fn test_error_recovery_service() {
        let service = ErrorRecoveryService::new(RetryConfig::default());
        
        let mut attempt_count = 0;
        let result = service.execute_with_recovery("test_operation", || {
            attempt_count += 1;
            if attempt_count < 3 {
                Err("temporary failure")
            } else {
                Ok("success")
            }
        }).await;
        
        assert!(result.is_ok());
        assert_eq!(attempt_count, 3);
    }

    #[test]
    fn test_validation_helper() {
        use rust_decimal::Decimal;
        
        // Valid amount
        let valid_amount = Decimal::new(1000, 2);
        assert!(ValidationHelper::validate_amount(valid_amount, "test").is_ok());
        
        // Negative amount
        let negative_amount = Decimal::new(-100, 2);
        assert!(ValidationHelper::validate_amount(negative_amount, "test").is_err());
        
        // Valid user ID
        let valid_user_id = uuid::Uuid::new_v4();
        assert!(ValidationHelper::validate_user_id(valid_user_id).is_ok());
        
        // Nil user ID
        let nil_user_id = uuid::Uuid::nil();
        assert!(ValidationHelper::validate_user_id(nil_user_id).is_err());
    }

    #[test]
    fn test_transaction_context() {
        let mut context = TransactionContext::new(Duration::from_secs(60));
        
        assert!(!context.is_expired());
        context.add_operation("test_operation".to_string());
        assert_eq!(context.operations.len(), 1);
    }
}