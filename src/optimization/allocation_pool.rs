//! Memory allocation pool optimization
//! 
//! This module implements object pooling and allocation optimization
//! to reduce memory allocation overhead and improve CPU efficiency.

use anyhow::{Context, Result};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Allocation pool for reusing objects
#[derive(Clone)]
pub struct AllocationPool {
    pools: Arc<RwLock<HashMap<String, Arc<Mutex<ObjectPool>>>>>,
    statistics: Arc<RwLock<AllocationStatistics>>,
    config: AllocationConfig,
}

/// Configuration for allocation pooling
#[derive(Debug, Clone)]
pub struct AllocationConfig {
    /// Maximum number of objects per pool
    pub max_pool_size: usize,
    /// Initial pool size
    pub initial_pool_size: usize,
    /// Pool cleanup interval
    pub cleanup_interval: Duration,
    /// Maximum idle time before object removal
    pub max_idle_time: Duration,
    /// Enable pool statistics
    pub enable_statistics: bool,
}

impl Default for AllocationConfig {
    fn default() -> Self {
        Self {
            max_pool_size: 1000,
            initial_pool_size: 10,
            cleanup_interval: Duration::from_secs(60),
            max_idle_time: Duration::from_secs(300),
            enable_statistics: true,
        }
    }
}

/// Generic object pool
struct ObjectPool {
    objects: VecDeque<PooledObject>,
    created_count: u64,
    borrowed_count: u64,
    returned_count: u64,
    max_size: usize,
}

/// Pooled object wrapper
struct PooledObject {
    data: Box<dyn std::any::Any + Send + Sync>,
    created_at: Instant,
    last_used: Instant,
}

/// Allocation statistics
#[derive(Debug, Clone, Default)]
pub struct AllocationStatistics {
    pub total_allocations: u64,
    pub pool_hits: u64,
    pub pool_misses: u64,
    pub objects_created: u64,
    pub objects_reused: u64,
    pub memory_saved_bytes: u64,
    pub pools_count: usize,
    pub total_pooled_objects: usize,
}

impl AllocationPool {
    /// Create a new allocation pool
    pub fn new(initial_size: usize) -> Result<Self> {
        let config = AllocationConfig {
            initial_pool_size: initial_size,
            ..Default::default()
        };

        Self::with_config(config)
    }

    /// Create allocation pool with custom configuration
    pub fn with_config(config: AllocationConfig) -> Result<Self> {
        let pool = Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
            statistics: Arc::new(RwLock::new(AllocationStatistics::default())),
            config,
        };

        // Start cleanup task
        pool.start_cleanup_task();

        Ok(pool)
    }

    /// Get or create a pool for a specific type
    fn get_or_create_pool(&self, pool_name: &str) -> Arc<Mutex<ObjectPool>> {
        let mut pools = self.pools.write();
        
        pools.entry(pool_name.to_string()).or_insert_with(|| {
            Arc::new(Mutex::new(ObjectPool {
                objects: VecDeque::with_capacity(self.config.initial_pool_size),
                created_count: 0,
                borrowed_count: 0,
                returned_count: 0,
                max_size: self.config.max_pool_size,
            }))
        }).clone()
    }

    /// Allocate an object from the pool or create new one
    pub fn allocate<T>(&self, pool_name: &str, factory: impl Fn() -> T) -> PooledItem<T>
    where
        T: Send + Sync + 'static,
    {
        let pool = self.get_or_create_pool(pool_name);
        let mut pool_guard = pool.lock();

        // Try to get from pool first
        if let Some(pooled_obj) = pool_guard.objects.pop_front() {
            pool_guard.borrowed_count += 1;
            
            if let Ok(item) = pooled_obj.data.downcast::<T>() {
                self.record_pool_hit();
                return PooledItem {
                    data: Some(*item),
                    pool: pool.clone(),
                    pool_name: pool_name.to_string(),
                    allocation_pool: self.clone(),
                };
            }
        }

        // Create new object if pool is empty or downcast failed
        let new_item = factory();
        pool_guard.created_count += 1;
        pool_guard.borrowed_count += 1;

        self.record_pool_miss();
        self.record_allocation();

        PooledItem {
            data: Some(new_item),
            pool: pool.clone(),
            pool_name: pool_name.to_string(),
            allocation_pool: self.clone(),
        }
    }

    /// Return an object to the pool
    fn return_to_pool<T>(&self, pool_name: &str, item: T)
    where
        T: Send + Sync + 'static,
    {
        let pool = self.get_or_create_pool(pool_name);
        let mut pool_guard = pool.lock();

        // Only return if pool isn't full
        if pool_guard.objects.len() < pool_guard.max_size {
            let pooled_obj = PooledObject {
                data: Box::new(item),
                created_at: Instant::now(),
                last_used: Instant::now(),
            };

            pool_guard.objects.push_back(pooled_obj);
            pool_guard.returned_count += 1;
        }
    }

    /// Optimize allocation for a specific component
    pub async fn optimize_for_component(&self, component: &str) -> Result<()> {
        tracing::info!(
            component = component,
            "Optimizing allocations for component"
        );

        // Pre-warm pools for common object types used by the component
        match component {
            "container" => {
                self.prewarm_pool("docker_config", 10, || HashMap::<String, String>::new());
                self.prewarm_pool("container_state", 5, || Vec::<u8>::with_capacity(1024));
            },
            "filesystem" => {
                self.prewarm_pool("path_buffer", 20, || Vec::<u8>::with_capacity(256));
                self.prewarm_pool("file_metadata", 15, || HashMap::<String, String>::new());
            },
            "network" => {
                self.prewarm_pool("buffer", 30, || Vec::<u8>::with_capacity(4096));
                self.prewarm_pool("connection_state", 10, || HashMap::<String, String>::new());
            },
            "serialization" => {
                self.prewarm_pool("json_buffer", 25, || Vec::<u8>::with_capacity(2048));
                self.prewarm_pool("serialize_state", 10, || HashMap::<String, serde_json::Value>::new());
            },
            _ => {
                // Generic optimization
                self.prewarm_pool("generic_buffer", 10, || Vec::<u8>::with_capacity(1024));
            }
        }

        Ok(())
    }

    /// Pre-warm a pool with objects
    fn prewarm_pool<T>(&self, pool_name: &str, count: usize, factory: impl Fn() -> T)
    where
        T: Send + Sync + 'static,
    {
        let pool = self.get_or_create_pool(pool_name);
        let mut pool_guard = pool.lock();

        for _ in 0..count {
            if pool_guard.objects.len() >= pool_guard.max_size {
                break;
            }

            let item = factory();
            let pooled_obj = PooledObject {
                data: Box::new(item),
                created_at: Instant::now(),
                last_used: Instant::now(),
            };

            pool_guard.objects.push_back(pooled_obj);
            pool_guard.created_count += 1;
        }

        tracing::debug!(
            pool_name = pool_name,
            objects_created = count,
            "Pre-warmed allocation pool"
        );
    }

    /// Get allocation statistics
    pub fn get_statistics(&self) -> AllocationStatistics {
        let mut stats = self.statistics.read().clone();
        
        // Update current pool statistics
        let pools = self.pools.read();
        stats.pools_count = pools.len();
        stats.total_pooled_objects = pools
            .values()
            .map(|pool| pool.lock().objects.len())
            .sum();

        stats
    }

    /// Clear all pools
    pub fn clear_pools(&self) {
        let mut pools = self.pools.write();
        pools.clear();
        tracing::info!("Cleared all allocation pools");
    }

    /// Get pool information for debugging
    pub fn get_pool_info(&self) -> Vec<PoolInfo> {
        let pools = self.pools.read();
        pools
            .iter()
            .map(|(name, pool)| {
                let pool_guard = pool.lock();
                PoolInfo {
                    name: name.clone(),
                    size: pool_guard.objects.len(),
                    max_size: pool_guard.max_size,
                    created_count: pool_guard.created_count,
                    borrowed_count: pool_guard.borrowed_count,
                    returned_count: pool_guard.returned_count,
                }
            })
            .collect()
    }

    /// Start cleanup task for removing old objects
    fn start_cleanup_task(&self) {
        let pools = self.pools.clone();
        let cleanup_interval = self.config.cleanup_interval;
        let max_idle_time = self.config.max_idle_time;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);
            
            loop {
                interval.tick().await;

                let pool_names: Vec<String> = {
                    let pools_guard = pools.read();
                    pools_guard.keys().cloned().collect()
                };

                for pool_name in pool_names {
                    if let Some(pool) = pools.read().get(&pool_name).cloned() {
                        let mut pool_guard = pool.lock();
                        let mut objects_to_remove = 0;
                        
                        // Count objects that are too old
                        for obj in &pool_guard.objects {
                            if obj.last_used.elapsed() > max_idle_time {
                                objects_to_remove += 1;
                            } else {
                                break; // Objects are ordered by age
                            }
                        }

                        // Remove old objects
                        for _ in 0..objects_to_remove {
                            pool_guard.objects.pop_front();
                        }

                        if objects_to_remove > 0 {
                            tracing::debug!(
                                pool_name = pool_name,
                                objects_removed = objects_to_remove,
                                "Cleaned up old objects from pool"
                            );
                        }
                    }
                }
            }
        });
    }

    /// Record statistics
    fn record_allocation(&self) {
        if self.config.enable_statistics {
            let mut stats = self.statistics.write();
            stats.total_allocations += 1;
            stats.objects_created += 1;
        }
    }

    fn record_pool_hit(&self) {
        if self.config.enable_statistics {
            let mut stats = self.statistics.write();
            stats.pool_hits += 1;
            stats.objects_reused += 1;
            // Estimate memory saved (rough approximation)
            stats.memory_saved_bytes += 1024; // Average object size
        }
    }

    fn record_pool_miss(&self) {
        if self.config.enable_statistics {
            let mut stats = self.statistics.write();
            stats.pool_misses += 1;
        }
    }
}

/// RAII wrapper for pooled objects
pub struct PooledItem<T>
where
    T: Send + Sync + 'static,
{
    data: Option<T>,
    pool: Arc<Mutex<ObjectPool>>,
    pool_name: String,
    allocation_pool: AllocationPool,
}

impl<T> PooledItem<T>
where
    T: Send + Sync + 'static,
{
    /// Get reference to the pooled item
    pub fn as_ref(&self) -> Option<&T> {
        self.data.as_ref()
    }

    /// Get mutable reference to the pooled item
    pub fn as_mut(&mut self) -> Option<&mut T> {
        self.data.as_mut()
    }

    /// Take ownership of the item (prevents return to pool)
    pub fn take(mut self) -> Option<T> {
        self.data.take()
    }
}

impl<T> Drop for PooledItem<T>
where
    T: Send + Sync + 'static,
{
    fn drop(&mut self) {
        if let Some(item) = self.data.take() {
            self.allocation_pool.return_to_pool(&self.pool_name, item);
        }
    }
}

impl<T> std::ops::Deref for PooledItem<T>
where
    T: Send + Sync + 'static,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.data.as_ref().expect("PooledItem data is None")
    }
}

impl<T> std::ops::DerefMut for PooledItem<T>
where
    T: Send + Sync + 'static,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data.as_mut().expect("PooledItem data is None")
    }
}

/// Pool information for debugging
#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub name: String,
    pub size: usize,
    pub max_size: usize,
    pub created_count: u64,
    pub borrowed_count: u64,
    pub returned_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocation_pool_creation() {
        let pool = AllocationPool::new(10).unwrap();
        let stats = pool.get_statistics();
        assert_eq!(stats.pools_count, 0);
        assert_eq!(stats.total_allocations, 0);
    }

    #[test]
    fn test_object_allocation_and_reuse() {
        let pool = AllocationPool::new(10).unwrap();
        
        // Allocate an object
        {
            let _item = pool.allocate("test_pool", || String::from("test"));
        } // Object returned to pool here

        // Allocate again - should reuse the object
        {
            let _item = pool.allocate("test_pool", || String::from("new"));
        }

        let stats = pool.get_statistics();
        assert!(stats.pool_hits > 0);
    }

    #[test]
    fn test_pool_info() {
        let pool = AllocationPool::new(5).unwrap();
        
        // Allocate and return some objects
        for i in 0..3 {
            let _item = pool.allocate("test_pool", || format!("item_{}", i));
        }

        let pool_info = pool.get_pool_info();
        assert!(!pool_info.is_empty());
        
        let test_pool_info = pool_info.iter().find(|info| info.name == "test_pool");
        assert!(test_pool_info.is_some());
    }

    #[tokio::test]
    async fn test_component_optimization() {
        let pool = AllocationPool::new(10).unwrap();
        
        let result = pool.optimize_for_component("container").await;
        assert!(result.is_ok());

        let pool_info = pool.get_pool_info();
        assert!(!pool_info.is_empty());
        
        // Should have created pools for container-related objects
        let has_docker_config = pool_info.iter().any(|info| info.name == "docker_config");
        assert!(has_docker_config);
    }

    #[test]
    fn test_pooled_item_deref() {
        let pool = AllocationPool::new(10).unwrap();
        let item = pool.allocate("test_pool", || String::from("hello"));
        
        assert_eq!(*item, "hello");
        assert_eq!(item.len(), 5);
    }

    #[test]
    fn test_pooled_item_take() {
        let pool = AllocationPool::new(10).unwrap();
        let item = pool.allocate("test_pool", || String::from("hello"));
        
        // Take consumes the item, so we can't access it afterwards
        let taken = item.take();
        assert_eq!(taken, Some(String::from("hello")));
        // Cannot access item.data after take() consumes the item
    }
}