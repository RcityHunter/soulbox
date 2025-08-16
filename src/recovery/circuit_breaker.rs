//! Circuit breaker implementation for fault tolerance
//! 
//! Circuit breakers prevent cascading failures by temporarily blocking
//! requests to failing services and allowing them time to recover.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Circuit breaker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to open the circuit
    pub failure_threshold: u32,
    /// Success threshold to close the circuit from half-open
    pub success_threshold: u32,
    /// Timeout duration when circuit is open
    pub timeout_duration: Duration,
    /// Window size for failure tracking
    pub window_size: Duration,
    /// Minimum number of requests before considering failure rate
    pub minimum_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_duration: Duration::from_secs(60),
            window_size: Duration::from_secs(60),
            minimum_requests: 10,
        }
    }
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Circuit is closed, requests are allowed
    Closed,
    /// Circuit is open, requests are blocked
    Open,
    /// Circuit is half-open, limited requests are allowed for testing
    HalfOpen,
}

/// Circuit breaker statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub total_requests: u32,
    pub last_failure_time: Option<Instant>,
    pub last_success_time: Option<Instant>,
    pub state_changed_at: Instant,
}

/// Circuit breaker implementation
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    component: String,
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitBreakerState>>,
}

/// Internal circuit breaker state
#[derive(Debug)]
struct CircuitBreakerState {
    current_state: CircuitState,
    failure_count: u32,
    success_count: u32,
    total_requests: u32,
    last_failure_time: Option<Instant>,
    last_success_time: Option<Instant>,
    state_changed_at: Instant,
    request_window: Vec<RequestResult>,
}

/// Individual request result
#[derive(Debug, Clone)]
struct RequestResult {
    timestamp: Instant,
    success: bool,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(component: &str, config: CircuitBreakerConfig) -> Self {
        let state = CircuitBreakerState {
            current_state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            total_requests: 0,
            last_failure_time: None,
            last_success_time: None,
            state_changed_at: Instant::now(),
            request_window: Vec::new(),
        };

        Self {
            component: component.to_string(),
            config,
            state: Arc::new(RwLock::new(state)),
        }
    }

    /// Check if a request can be executed
    pub async fn can_execute(&self) -> bool {
        let mut state = self.state.write().await;
        
        // Clean old requests from window
        self.clean_request_window(&mut state).await;

        match state.current_state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has passed
                if let Some(last_failure) = state.last_failure_time {
                    if last_failure.elapsed() >= self.config.timeout_duration {
                        // Transition to half-open
                        state.current_state = CircuitState::HalfOpen;
                        state.state_changed_at = Instant::now();
                        state.success_count = 0;
                        tracing::info!(
                            component = self.component,
                            "Circuit breaker transitioned to half-open"
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                // Allow limited requests for testing
                true
            }
        }
    }

    /// Record a successful request
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;
        
        state.success_count += 1;
        state.total_requests += 1;
        state.last_success_time = Some(Instant::now());
        
        // Add to request window
        state.request_window.push(RequestResult {
            timestamp: Instant::now(),
            success: true,
        });

        match state.current_state {
            CircuitState::HalfOpen => {
                // Check if we should close the circuit
                if state.success_count >= self.config.success_threshold {
                    state.current_state = CircuitState::Closed;
                    state.state_changed_at = Instant::now();
                    state.failure_count = 0;
                    tracing::info!(
                        component = self.component,
                        "Circuit breaker closed after successful recovery"
                    );
                }
            }
            CircuitState::Open => {
                // This shouldn't happen, but reset to half-open if it does
                state.current_state = CircuitState::HalfOpen;
                state.state_changed_at = Instant::now();
            }
            CircuitState::Closed => {
                // Reset failure count on success
                if state.failure_count > 0 {
                    state.failure_count = 0;
                }
            }
        }
    }

    /// Record a failed request
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;
        
        state.failure_count += 1;
        state.total_requests += 1;
        state.last_failure_time = Some(Instant::now());
        
        // Add to request window
        state.request_window.push(RequestResult {
            timestamp: Instant::now(),
            success: false,
        });

        // Clean old requests from window
        self.clean_request_window(&mut state).await;

        match state.current_state {
            CircuitState::Closed => {
                // Check if we should open the circuit
                if self.should_open_circuit(&state) {
                    state.current_state = CircuitState::Open;
                    state.state_changed_at = Instant::now();
                    tracing::warn!(
                        component = self.component,
                        failure_count = state.failure_count,
                        "Circuit breaker opened due to excessive failures"
                    );
                }
            }
            CircuitState::HalfOpen => {
                // Return to open state on any failure during half-open
                state.current_state = CircuitState::Open;
                state.state_changed_at = Instant::now();
                state.success_count = 0;
                tracing::warn!(
                    component = self.component,
                    "Circuit breaker returned to open state after failure in half-open"
                );
            }
            CircuitState::Open => {
                // Already open, just update failure time
            }
        }
    }

    /// Get current circuit breaker statistics
    pub async fn get_stats(&self) -> CircuitBreakerStats {
        let state = self.state.read().await;
        
        CircuitBreakerStats {
            state: state.current_state.clone(),
            failure_count: state.failure_count,
            success_count: state.success_count,
            total_requests: state.total_requests,
            last_failure_time: state.last_failure_time,
            last_success_time: state.last_success_time,
            state_changed_at: state.state_changed_at,
        }
    }

    /// Reset the circuit breaker to closed state
    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        
        state.current_state = CircuitState::Closed;
        state.failure_count = 0;
        state.success_count = 0;
        state.state_changed_at = Instant::now();
        state.request_window.clear();
        
        tracing::info!(
            component = self.component,
            "Circuit breaker manually reset"
        );
    }

    /// Force the circuit to open
    pub async fn force_open(&self) {
        let mut state = self.state.write().await;
        
        state.current_state = CircuitState::Open;
        state.state_changed_at = Instant::now();
        
        tracing::warn!(
            component = self.component,
            "Circuit breaker manually forced open"
        );
    }

    /// Get failure rate within the current window
    pub async fn get_failure_rate(&self) -> f32 {
        let state = self.state.read().await;
        
        if state.request_window.is_empty() {
            return 0.0;
        }

        let failures = state.request_window.iter()
            .filter(|r| !r.success)
            .count();
            
        failures as f32 / state.request_window.len() as f32
    }

    /// Check if circuit should be opened based on current state
    fn should_open_circuit(&self, state: &CircuitBreakerState) -> bool {
        // Check if we have enough requests to make a decision
        if state.total_requests < self.config.minimum_requests {
            return false;
        }

        // Check failure threshold
        if state.failure_count >= self.config.failure_threshold {
            return true;
        }

        // Check failure rate within window
        if !state.request_window.is_empty() {
            let failures = state.request_window.iter()
                .filter(|r| !r.success)
                .count();
            let failure_rate = failures as f32 / state.request_window.len() as f32;
            
            // Open if failure rate is above 50% and we have enough samples
            if failure_rate > 0.5 && state.request_window.len() >= self.config.minimum_requests as usize {
                return true;
            }
        }

        false
    }

    /// Clean old requests from the request window
    async fn clean_request_window(&self, state: &mut CircuitBreakerState) {
        let cutoff_time = Instant::now() - self.config.window_size;
        state.request_window.retain(|r| r.timestamp >= cutoff_time);
    }
}

/// Circuit breaker middleware for automatic protection
pub struct CircuitBreakerMiddleware {
    circuit_breaker: CircuitBreaker,
}

impl CircuitBreakerMiddleware {
    /// Create new middleware with circuit breaker
    pub fn new(component: &str, config: CircuitBreakerConfig) -> Self {
        Self {
            circuit_breaker: CircuitBreaker::new(component, config),
        }
    }

    /// Execute operation with circuit breaker protection
    pub async fn execute<T, F, E>(&self, operation: F) -> Result<T>
    where
        F: FnOnce() -> std::result::Result<T, E>,
        E: std::error::Error + Send + Sync + 'static,
    {
        // Check if circuit allows execution
        if !self.circuit_breaker.can_execute().await {
            return Err(anyhow::anyhow!("Circuit breaker is open"));
        }

        // Execute operation
        match operation() {
            Ok(result) => {
                self.circuit_breaker.record_success().await;
                Ok(result)
            }
            Err(e) => {
                self.circuit_breaker.record_failure().await;
                Err(anyhow::anyhow!("Operation failed: {}", e))
            }
        }
    }

    /// Get circuit breaker reference for direct access
    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_circuit_breaker_creation() {
        let config = CircuitBreakerConfig::default();
        let cb = CircuitBreaker::new("test_component", config);
        
        assert!(cb.can_execute().await);
        
        let stats = cb.get_stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.failure_count, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_failure_threshold() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            minimum_requests: 1,
            ..Default::default()
        };
        let cb = CircuitBreaker::new("test_component", config);
        
        // Record failures up to threshold
        for _ in 0..3 {
            cb.record_failure().await;
        }
        
        let stats = cb.get_stats().await;
        assert_eq!(stats.state, CircuitState::Open);
        assert!(!cb.can_execute().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout_duration: Duration::from_millis(100),
            minimum_requests: 1,
            ..Default::default()
        };
        let cb = CircuitBreaker::new("test_component", config);
        
        // Open the circuit
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.get_stats().await.state, CircuitState::Open);
        
        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Should transition to half-open
        assert!(cb.can_execute().await);
        assert_eq!(cb.get_stats().await.state, CircuitState::HalfOpen);
        
        // Record successes to close circuit
        cb.record_success().await;
        cb.record_success().await;
        
        assert_eq!(cb.get_stats().await.state, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_middleware() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            minimum_requests: 1,
            ..Default::default()
        };
        let middleware = CircuitBreakerMiddleware::new("test_component", config);
        
        let counter = AtomicU32::new(0);
        
        // Execute successful operation
        let result = middleware.execute(|| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok::<u32, String>(42)
        }).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        
        // Execute failing operations to open circuit
        for _ in 0..2 {
            let _ = middleware.execute(|| {
                Err::<u32, String>("Test error".to_string())
            }).await;
        }
        
        // Circuit should be open now
        let stats = middleware.circuit_breaker().get_stats().await;
        assert_eq!(stats.state, CircuitState::Open);
        
        // Next execution should be blocked
        let result = middleware.execute(|| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok::<u32, String>(100)
        }).await;
        
        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1); // Should not increment
    }

    #[tokio::test]
    async fn test_failure_rate_calculation() {
        let cb = CircuitBreaker::new("test", CircuitBreakerConfig::default());
        
        // Record mixed results
        cb.record_success().await;
        cb.record_failure().await;
        cb.record_failure().await;
        cb.record_success().await;
        
        let failure_rate = cb.get_failure_rate().await;
        assert_eq!(failure_rate, 0.5); // 2 failures out of 4 requests
    }
}