//! Retry mechanism with various strategies
//! 
//! This module provides configurable retry mechanisms including
//! exponential backoff, linear backoff, and custom retry policies.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Base delay between retries
    pub base_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Retry strategy to use
    pub strategy: RetryStrategy,
    /// Backoff multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Jitter factor to add randomness (0.0 to 1.0)
    pub jitter_factor: f64,
    /// Timeout for individual retry attempts
    pub attempt_timeout: Option<Duration>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            strategy: RetryStrategy::ExponentialBackoff,
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
            attempt_timeout: Some(Duration::from_secs(10)),
        }
    }
}

/// Retry strategies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RetryStrategy {
    /// Fixed delay between retries
    FixedDelay,
    /// Linear increase in delay
    LinearBackoff,
    /// Exponential increase in delay
    ExponentialBackoff,
    /// Custom retry intervals
    CustomIntervals(Vec<Duration>),
}

/// Retry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryStats {
    pub total_attempts: u32,
    pub successful_attempts: u32,
    pub failed_attempts: u32,
    pub total_retry_time: Duration,
    pub average_attempts_per_operation: f32,
    pub success_rate: f32,
}

/// Retry manager implementation
#[derive(Debug, Clone)]
pub struct RetryManager {
    config: RetryConfig,
    stats: std::sync::Arc<tokio::sync::RwLock<RetryStats>>,
}

impl RetryManager {
    /// Create a new retry manager
    pub fn new(config: RetryConfig) -> Self {
        Self {
            config,
            stats: std::sync::Arc::new(tokio::sync::RwLock::new(RetryStats {
                total_attempts: 0,
                successful_attempts: 0,
                failed_attempts: 0,
                total_retry_time: Duration::from_nanos(0),
                average_attempts_per_operation: 0.0,
                success_rate: 0.0,
            })),
        }
    }

    /// Execute an operation with retry logic
    pub async fn execute_with_retry<T, F, E>(
        &self,
        operation_name: &str,
        operation: F,
    ) -> Result<T>
    where
        F: Fn() -> std::result::Result<T, E> + Send + Sync + Clone,
        E: std::error::Error + Send + Sync + 'static,
    {
        let start_time = Instant::now();
        let mut attempt = 1;
        let mut last_error: Option<E> = None;

        loop {
            tracing::debug!(
                operation = operation_name,
                attempt = attempt,
                max_attempts = self.config.max_attempts,
                "Executing retry attempt"
            );

            // Execute the operation
            let attempt_start = Instant::now();
            let result = if let Some(timeout) = self.config.attempt_timeout {
                // Execute with timeout
                tokio::time::timeout(timeout, async { operation() }).await
                    .map_err(|_| anyhow::anyhow!("Operation timed out after {:?}", timeout))
                    .and_then(|r| r.map_err(|e| anyhow::anyhow!("Operation failed: {}", e)))
            } else {
                // Execute without timeout
                operation().map_err(|e| anyhow::anyhow!("Operation failed: {}", e))
            };

            let attempt_duration = attempt_start.elapsed();

            match result {
                Ok(value) => {
                    // Success - update stats and return
                    self.update_stats_success(attempt, start_time.elapsed()).await;
                    
                    tracing::info!(
                        operation = operation_name,
                        attempt = attempt,
                        duration_ms = attempt_duration.as_millis(),
                        "Operation succeeded"
                    );
                    
                    return Ok(value);
                }
                Err(e) => {
                    // Store error message before downcast
                    let error_msg = e.to_string();
                    
                    // Store the error for potential return
                    if let Ok(original_error) = e.downcast::<E>() {
                        last_error = Some(original_error);
                    }

                    // Check if we should retry
                    if attempt >= self.config.max_attempts {
                        // No more attempts - update stats and return error
                        self.update_stats_failure(attempt, start_time.elapsed()).await;
                        
                        tracing::error!(
                            operation = operation_name,
                            total_attempts = attempt,
                            total_duration_ms = start_time.elapsed().as_millis(),
                            "Operation failed after all retry attempts"
                        );
                        
                        return Err(anyhow::anyhow!(
                            "Operation failed after {} attempts. Last error: {}",
                            attempt,
                            last_error.as_ref().map(|e| e.to_string()).unwrap_or_else(|| "Unknown error".to_string())
                        ));
                    }

                    // Calculate delay before next attempt
                    let delay = self.calculate_delay(attempt);
                    
                    tracing::warn!(
                        operation = operation_name,
                        attempt = attempt,
                        delay_ms = delay.as_millis(),
                        error = error_msg,
                        "Operation failed, retrying after delay"
                    );

                    // Wait before next attempt
                    sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }

    /// Execute an operation with custom retry condition
    pub async fn execute_with_custom_retry<T, F, E, C>(
        &self,
        operation_name: &str,
        operation: F,
        should_retry: C,
    ) -> Result<T>
    where
        F: Fn() -> std::result::Result<T, E> + Send + Sync + Clone,
        E: std::error::Error + Send + Sync + 'static + Clone,
        C: Fn(&E) -> bool + Send + Sync,
    {
        let start_time = Instant::now();
        let mut attempt = 1;
        let mut last_error: Option<E> = None;

        loop {
            let result = operation().map_err(|e| anyhow::anyhow!("Operation failed: {}", e));

            match result {
                Ok(value) => {
                    self.update_stats_success(attempt, start_time.elapsed()).await;
                    return Ok(value);
                }
                Err(e) => {
                    if let Ok(original_error) = e.downcast::<E>() {
                        // Check if we should retry this specific error
                        if !should_retry(&original_error) {
                            tracing::info!(
                                operation = operation_name,
                                attempt = attempt,
                                error = %original_error,
                                "Error is not retryable, failing immediately"
                            );
                            
                            self.update_stats_failure(attempt, start_time.elapsed()).await;
                            return Err(anyhow::anyhow!("Non-retryable error: {}", original_error));
                        }
                        
                        last_error = Some(original_error);
                    }

                    if attempt >= self.config.max_attempts {
                        self.update_stats_failure(attempt, start_time.elapsed()).await;
                        return Err(anyhow::anyhow!(
                            "Operation failed after {} attempts. Last error: {}",
                            attempt,
                            last_error.as_ref().map(|e| e.to_string()).unwrap_or_else(|| "Unknown error".to_string())
                        ));
                    }

                    let delay = self.calculate_delay(attempt);
                    sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }

    /// Get retry statistics
    pub async fn get_stats(&self) -> RetryStats {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Reset retry statistics
    pub async fn reset(&self) {
        let mut stats = self.stats.write().await;
        *stats = RetryStats {
            total_attempts: 0,
            successful_attempts: 0,
            failed_attempts: 0,
            total_retry_time: Duration::from_nanos(0),
            average_attempts_per_operation: 0.0,
            success_rate: 0.0,
        };
    }

    /// Calculate delay for next retry attempt
    fn calculate_delay(&self, attempt: u32) -> Duration {
        let base_delay = match &self.config.strategy {
            RetryStrategy::FixedDelay => self.config.base_delay,
            
            RetryStrategy::LinearBackoff => {
                let multiplier = attempt as f64;
                Duration::from_millis(
                    (self.config.base_delay.as_millis() as f64 * multiplier) as u64
                )
            }
            
            RetryStrategy::ExponentialBackoff => {
                let multiplier = self.config.backoff_multiplier.powi((attempt - 1) as i32);
                Duration::from_millis(
                    (self.config.base_delay.as_millis() as f64 * multiplier) as u64
                )
            }
            
            RetryStrategy::CustomIntervals(intervals) => {
                let index = (attempt - 1) as usize;
                if index < intervals.len() {
                    intervals[index]
                } else {
                    // Use last interval if we exceed the list
                    intervals.last().copied().unwrap_or(self.config.base_delay)
                }
            }
        };

        // Apply jitter
        let jitter = if self.config.jitter_factor > 0.0 {
            let jitter_amount = base_delay.as_millis() as f64 * self.config.jitter_factor;
            let random_jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_amount;
            Duration::from_millis(random_jitter.abs() as u64)
        } else {
            Duration::from_nanos(0)
        };

        // Ensure delay doesn't exceed maximum
        let total_delay = base_delay + jitter;
        std::cmp::min(total_delay, self.config.max_delay)
    }

    /// Update statistics for successful operation
    async fn update_stats_success(&self, attempts: u32, total_time: Duration) {
        let mut stats = self.stats.write().await;
        stats.total_attempts += attempts;
        stats.successful_attempts += 1;
        stats.total_retry_time += total_time;
        
        // Recalculate derived statistics
        let total_operations = stats.successful_attempts + stats.failed_attempts;
        if total_operations > 0 {
            stats.average_attempts_per_operation = stats.total_attempts as f32 / total_operations as f32;
            stats.success_rate = stats.successful_attempts as f32 / total_operations as f32;
        }
    }

    /// Update statistics for failed operation
    async fn update_stats_failure(&self, attempts: u32, total_time: Duration) {
        let mut stats = self.stats.write().await;
        stats.total_attempts += attempts;
        stats.failed_attempts += 1;
        stats.total_retry_time += total_time;
        
        // Recalculate derived statistics
        let total_operations = stats.successful_attempts + stats.failed_attempts;
        if total_operations > 0 {
            stats.average_attempts_per_operation = stats.total_attempts as f32 / total_operations as f32;
            stats.success_rate = stats.successful_attempts as f32 / total_operations as f32;
        }
    }
}

/// Retry middleware for automatic retry logic
pub struct RetryMiddleware {
    retry_manager: RetryManager,
}

impl RetryMiddleware {
    /// Create new retry middleware
    pub fn new(config: RetryConfig) -> Self {
        Self {
            retry_manager: RetryManager::new(config),
        }
    }

    /// Execute operation with retry
    pub async fn execute<T, F, E>(
        &self,
        operation_name: &str,
        operation: F,
    ) -> Result<T>
    where
        F: Fn() -> std::result::Result<T, E> + Send + Sync + Clone,
        E: std::error::Error + Send + Sync + 'static,
    {
        self.retry_manager.execute_with_retry(operation_name, operation).await
    }

    /// Get retry manager reference
    pub fn retry_manager(&self) -> &RetryManager {
        &self.retry_manager
    }
}

/// Common retry policies
pub struct RetryPolicies;

impl RetryPolicies {
    /// Create a policy for network operations
    pub fn network_operations() -> RetryConfig {
        RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(10),
            strategy: RetryStrategy::ExponentialBackoff,
            backoff_multiplier: 2.0,
            jitter_factor: 0.2,
            attempt_timeout: Some(Duration::from_secs(5)),
        }
    }

    /// Create a policy for database operations
    pub fn database_operations() -> RetryConfig {
        RetryConfig {
            max_attempts: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            strategy: RetryStrategy::ExponentialBackoff,
            backoff_multiplier: 1.5,
            jitter_factor: 0.1,
            attempt_timeout: Some(Duration::from_secs(30)),
        }
    }

    /// Create a policy for file operations
    pub fn file_operations() -> RetryConfig {
        RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(1),
            strategy: RetryStrategy::LinearBackoff,
            backoff_multiplier: 1.0,
            jitter_factor: 0.05,
            attempt_timeout: Some(Duration::from_secs(10)),
        }
    }

    /// Create a policy for container operations
    pub fn container_operations() -> RetryConfig {
        RetryConfig {
            max_attempts: 4,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(15),
            strategy: RetryStrategy::ExponentialBackoff,
            backoff_multiplier: 2.0,
            jitter_factor: 0.15,
            attempt_timeout: Some(Duration::from_secs(60)),
        }
    }

    /// Determine if an error should be retried
    pub fn is_retryable_error(error: &dyn std::error::Error) -> bool {
        let error_msg = error.to_string().to_lowercase();
        
        // Network-related errors that are typically retryable
        if error_msg.contains("connection refused") ||
           error_msg.contains("connection reset") ||
           error_msg.contains("timeout") ||
           error_msg.contains("temporary failure") ||
           error_msg.contains("service unavailable") ||
           error_msg.contains("too many requests") {
            return true;
        }

        // Database-related retryable errors
        if error_msg.contains("deadlock") ||
           error_msg.contains("lock timeout") ||
           error_msg.contains("connection lost") {
            return true;
        }

        // Container-related retryable errors
        if error_msg.contains("container not found") ||
           error_msg.contains("image pull") ||
           error_msg.contains("resource temporarily unavailable") {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_manager_success() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(10),
            ..Default::default()
        };
        let retry_manager = RetryManager::new(config);
        
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let result = retry_manager.execute_with_retry(
            "test_operation",
            move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, String>(42)
            }
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1); // Should succeed on first attempt
    }

    #[tokio::test]
    async fn test_retry_manager_failure_then_success() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(10),
            ..Default::default()
        };
        let retry_manager = RetryManager::new(config);
        
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let result = retry_manager.execute_with_retry(
            "test_operation",
            move || {
                let count = counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
                if count < 3 {
                    Err("Temporary failure".to_string())
                } else {
                    Ok(42)
                }
            }
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3); // Should succeed on third attempt
    }

    #[tokio::test]
    async fn test_retry_manager_all_failures() {
        let config = RetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(10),
            ..Default::default()
        };
        let retry_manager = RetryManager::new(config);
        
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let result = retry_manager.execute_with_retry(
            "test_operation",
            move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                Err::<i32, String>("Persistent failure".to_string())
            }
        ).await;
        
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 2); // Should try max_attempts times
    }

    #[tokio::test]
    async fn test_exponential_backoff_delay() {
        let config = RetryConfig {
            base_delay: Duration::from_millis(100),
            strategy: RetryStrategy::ExponentialBackoff,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0, // No jitter for predictable testing
            max_delay: Duration::from_secs(10),
            ..Default::default()
        };
        let retry_manager = RetryManager::new(config);
        
        // Test delay calculation
        let delay1 = retry_manager.calculate_delay(1);
        let delay2 = retry_manager.calculate_delay(2);
        let delay3 = retry_manager.calculate_delay(3);
        
        assert_eq!(delay1, Duration::from_millis(100)); // base_delay * 2^0
        assert_eq!(delay2, Duration::from_millis(200)); // base_delay * 2^1
        assert_eq!(delay3, Duration::from_millis(400)); // base_delay * 2^2
    }

    #[tokio::test]
    async fn test_custom_retry_condition() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(10),
            ..Default::default()
        };
        let retry_manager = RetryManager::new(config);
        
        // Only retry on "retryable" errors
        let should_retry = |error: &String| error.contains("retryable");
        
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let result = retry_manager.execute_with_custom_retry(
            "test_operation",
            move || {
                let count = counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
                if count == 1 {
                    Err("retryable error".to_string())
                } else if count == 2 {
                    Err("non-retryable error".to_string()) // Should not retry this
                } else {
                    Ok(42)
                }
            },
            should_retry
        ).await;
        
        assert!(result.is_err()); // Should fail on non-retryable error
        assert_eq!(counter.load(Ordering::SeqCst), 2); // Should stop after non-retryable error
    }

    #[test]
    fn test_retry_policies() {
        let network_policy = RetryPolicies::network_operations();
        assert_eq!(network_policy.max_attempts, 3);
        assert_eq!(network_policy.strategy, RetryStrategy::ExponentialBackoff);
        
        let db_policy = RetryPolicies::database_operations();
        assert_eq!(db_policy.max_attempts, 5);
        
        let file_policy = RetryPolicies::file_operations();
        assert_eq!(file_policy.strategy, RetryStrategy::LinearBackoff);
    }

    #[test]
    fn test_retryable_error_detection() {
        // Test retryable errors
        let retryable_errors = [
            "connection refused",
            "timeout occurred",
            "service unavailable",
            "deadlock detected",
        ];
        
        for error_msg in &retryable_errors {
            let error = std::io::Error::new(std::io::ErrorKind::Other, *error_msg);
            assert!(RetryPolicies::is_retryable_error(&error));
        }
        
        // Test non-retryable errors
        let non_retryable_errors = [
            "permission denied",
            "file not found",
            "invalid argument",
        ];
        
        for error_msg in &non_retryable_errors {
            let error = std::io::Error::new(std::io::ErrorKind::Other, *error_msg);
            assert!(!RetryPolicies::is_retryable_error(&error));
        }
    }
}