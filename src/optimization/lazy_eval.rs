//! Lazy evaluation optimization
//! 
//! This module implements lazy evaluation patterns to reduce unnecessary
//! computation and improve CPU efficiency.

use anyhow::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::OnceCell;

/// Lazy value that computes its result only when first accessed
pub struct LazyValue<T> {
    cell: OnceCell<T>,
    initializer: Option<Box<dyn Fn() -> T + Send + Sync>>,
}

impl<T> LazyValue<T> {
    /// Create a new lazy value with the given initializer
    pub fn new<F>(initializer: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            cell: OnceCell::new(),
            initializer: Some(Box::new(initializer)),
        }
    }

    /// Get the value, computing it if necessary
    pub async fn get(&self) -> &T {
        self.cell
            .get_or_init(|| async {
                if let Some(ref init) = self.initializer {
                    init()
                } else {
                    panic!("LazyValue has no initializer")
                }
            })
            .await
    }

    /// Check if the value has been computed
    pub fn is_computed(&self) -> bool {
        self.cell.get().is_some()
    }
}

/// Lazy async value for expensive computations
pub struct LazyAsyncValue<T> {
    cell: OnceCell<T>,
    initializer: Option<Pin<Box<dyn Future<Output = T> + Send + Sync>>>,
}

impl<T> LazyAsyncValue<T> {
    /// Create a new lazy async value
    pub fn new<F, Fut>(initializer: F) -> Self
    where
        F: FnOnce() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = T> + Send + Sync + 'static,
    {
        Self {
            cell: OnceCell::new(),
            initializer: Some(Box::pin(initializer())),
        }
    }

    /// Get the value, computing it asynchronously if necessary
    pub async fn get(&self) -> &T {
        if let Some(value) = self.cell.get() {
            return value;
        }

        // This is a simplified implementation
        // In a real implementation, we'd need proper async initialization
        panic!("LazyAsyncValue async initialization not fully implemented");
    }
}

/// Lazy computation cache for expensive operations
#[derive(Clone)]
pub struct LazyCache<K, V> {
    cache: Arc<RwLock<HashMap<K, Arc<LazyValue<V>>>>>,
}

impl<K, V> LazyCache<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone + Send + Sync + 'static,
{
    /// Create a new lazy cache
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or compute a value
    pub async fn get_or_compute<F>(&self, key: K, compute: F) -> V
    where
        F: Fn() -> V + Send + Sync + 'static,
    {
        // Check if we already have a lazy value for this key
        {
            let cache = self.cache.read();
            if let Some(lazy_value) = cache.get(&key) {
                return lazy_value.get().await.clone();
            }
        }

        // Create new lazy value
        let lazy_value = Arc::new(LazyValue::new(compute));
        
        // Insert into cache
        {
            let mut cache = self.cache.write();
            cache.insert(key.clone(), lazy_value.clone());
        }

        lazy_value.get().await.clone()
    }

    /// Clear the cache
    pub fn clear(&self) {
        let mut cache = self.cache.write();
        cache.clear();
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        let cache = self.cache.read();
        cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        let cache = self.cache.read();
        cache.is_empty()
    }
}

/// Lazy evaluator for managing component-level lazy evaluation
#[derive(Clone)]
pub struct LazyEvaluator {
    enabled_components: Arc<RwLock<HashMap<String, LazyComponentConfig>>>,
    global_cache: Arc<RwLock<HashMap<String, Box<dyn std::any::Any + Send + Sync>>>>,
}

/// Configuration for lazy evaluation in a component
#[derive(Debug, Clone)]
struct LazyComponentConfig {
    pub enabled: bool,
    pub cache_size_limit: usize,
    pub ttl_seconds: u64,
    pub cache_hit_ratio: f32,
}

impl LazyEvaluator {
    /// Create a new lazy evaluator
    pub fn new() -> Self {
        Self {
            enabled_components: Arc::new(RwLock::new(HashMap::new())),
            global_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Enable lazy evaluation for a component
    pub async fn enable_for_component(&self, component: &str) -> Result<()> {
        let config = LazyComponentConfig {
            enabled: true,
            cache_size_limit: 1000,
            ttl_seconds: 300, // 5 minutes
            cache_hit_ratio: 0.0,
        };

        {
            let mut components = self.enabled_components.write();
            components.insert(component.to_string(), config);
        }

        tracing::info!(
            component = component,
            "Enabled lazy evaluation for component"
        );

        Ok(())
    }

    /// Disable lazy evaluation for a component
    pub async fn disable_for_component(&self, component: &str) -> Result<()> {
        {
            let mut components = self.enabled_components.write();
            if let Some(mut config) = components.get_mut(component) {
                config.enabled = false;
            }
        }

        tracing::info!(
            component = component,
            "Disabled lazy evaluation for component"
        );

        Ok(())
    }

    /// Check if lazy evaluation is enabled for a component
    pub fn is_enabled_for_component(&self, component: &str) -> bool {
        let components = self.enabled_components.read();
        components
            .get(component)
            .map(|config| config.enabled)
            .unwrap_or(false)
    }

    /// Create a lazy computation for a component
    pub fn create_lazy_computation<T, F>(
        &self,
        component: &str,
        key: &str,
        computation: F,
    ) -> Option<LazyValue<T>>
    where
        T: Send + Sync + 'static,
        F: Fn() -> T + Send + Sync + 'static,
    {
        if !self.is_enabled_for_component(component) {
            return None;
        }

        Some(LazyValue::new(computation))
    }

    /// Execute with lazy evaluation if enabled
    pub async fn execute_with_lazy<T, F>(&self, component: &str, operation: F) -> T
    where
        T: Clone + Send + Sync + 'static,
        F: Fn() -> T + Send + Sync + 'static,
    {
        if !self.is_enabled_for_component(component) {
            // Execute immediately if lazy evaluation is disabled
            return operation();
        }

        // Create cache key based on component and operation
        let cache_key = format!("{}::lazy_op", component);
        
        // Try to get from cache first
        {
            let cache = self.global_cache.read();
            if let Some(cached) = cache.get(&cache_key) {
                if let Some(value) = cached.downcast_ref::<T>() {
                    tracing::debug!(
                        component = component,
                        cache_key = cache_key,
                        "Cache hit for lazy operation"
                    );
                    return value.clone();
                }
            }
        }

        // Execute and cache result
        let result = operation();
        
        {
            let mut cache = self.global_cache.write();
            cache.insert(cache_key.clone(), Box::new(result.clone()));
        }

        tracing::debug!(
            component = component,
            cache_key = cache_key,
            "Cached result for lazy operation"
        );

        result
    }

    /// Get lazy evaluation statistics
    pub fn get_statistics(&self) -> LazyEvaluationStats {
        let components = self.enabled_components.read();
        let cache = self.global_cache.read();

        let enabled_components: Vec<String> = components
            .iter()
            .filter(|(_, config)| config.enabled)
            .map(|(name, _)| name.clone())
            .collect();

        LazyEvaluationStats {
            enabled_components,
            total_cached_items: cache.len(),
            cache_memory_usage: cache.len() * std::mem::size_of::<Box<dyn std::any::Any>>(), // Approximation
        }
    }

    /// Clear all caches
    pub fn clear_all_caches(&self) {
        let mut cache = self.global_cache.write();
        cache.clear();
        tracing::info!("Cleared all lazy evaluation caches");
    }
}

/// Statistics for lazy evaluation
#[derive(Debug, Clone)]
pub struct LazyEvaluationStats {
    pub enabled_components: Vec<String>,
    pub total_cached_items: usize,
    pub cache_memory_usage: usize,
}

/// Macro for creating lazy static values
#[macro_export]
macro_rules! lazy_static_value {
    ($name:ident: $type:ty = $init:expr) => {
        static $name: std::sync::OnceLock<$type> = std::sync::OnceLock::new();
        
        impl $name {
            pub fn get() -> &'static $type {
                $name.get_or_init(|| $init)
            }
        }
    };
}

/// Trait for types that support lazy initialization
pub trait LazyInitializable<T> {
    fn lazy_init<F>(&self, initializer: F) -> &T
    where
        F: FnOnce() -> T;
}

/// Future wrapper for lazy async operations
pub struct LazyFuture<T> {
    inner: Option<Pin<Box<dyn Future<Output = T> + Send + Sync>>>,
    result: Option<T>,
}

impl<T> LazyFuture<T> {
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = T> + Send + Sync + 'static,
    {
        Self {
            inner: Some(Box::pin(future)),
            result: None,
        }
    }
}

impl<T> Future for LazyFuture<T>
where
    T: Clone,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        
        // If we already have the result, return it
        if let Some(ref result) = this.result {
            return Poll::Ready(result.clone());
        }

        // Poll the inner future
        if let Some(mut inner) = this.inner.take() {
            match inner.as_mut().poll(cx) {
                Poll::Ready(result) => {
                    this.result = Some(result.clone());
                    Poll::Ready(result)
                }
                Poll::Pending => {
                    this.inner = Some(inner);
                    Poll::Pending
                }
            }
        } else {
            panic!("LazyFuture polled after completion");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_lazy_value() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let lazy_value = LazyValue::new(move || {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            42
        });

        assert!(!lazy_value.is_computed());

        let value1 = lazy_value.get().await;
        assert_eq!(*value1, 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        let value2 = lazy_value.get().await;
        assert_eq!(*value2, 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Should not increment again

        assert!(lazy_value.is_computed());
    }

    #[tokio::test]
    async fn test_lazy_cache() {
        let cache = LazyCache::<String, i32>::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        let key = "test_key".to_string();
        let call_count_clone = call_count.clone();

        let value1 = cache
            .get_or_compute(key.clone(), move || {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
                100
            })
            .await;

        assert_eq!(value1, 100);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        let call_count_clone2 = call_count.clone();
        let value2 = cache
            .get_or_compute(key.clone(), move || {
                call_count_clone2.fetch_add(1, Ordering::SeqCst);
                200 // Different value, but shouldn't be called
            })
            .await;

        assert_eq!(value2, 100); // Should return cached value
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Should not increment

        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());
    }

    #[tokio::test]
    async fn test_lazy_evaluator() {
        let evaluator = LazyEvaluator::new();
        let component = "test_component";

        assert!(!evaluator.is_enabled_for_component(component));

        evaluator.enable_for_component(component).await.unwrap();
        assert!(evaluator.is_enabled_for_component(component));

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let result1 = evaluator
            .execute_with_lazy(component, move || {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
                "expensive_computation_result"
            })
            .await;

        assert_eq!(result1, "expensive_computation_result");
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        let call_count_clone2 = call_count.clone();
        let result2 = evaluator
            .execute_with_lazy(component, move || {
                call_count_clone2.fetch_add(1, Ordering::SeqCst);
                "expensive_computation_result"
            })
            .await;

        assert_eq!(result2, "expensive_computation_result");
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // Should use cached result

        let stats = evaluator.get_statistics();
        assert!(stats.enabled_components.contains(&component.to_string()));
        assert!(stats.total_cached_items > 0);
    }
}