//! Lock-free concurrent data structures for high performance
//! 
//! This module implements lock-free patterns using modern concurrent
//! data structures to maximize throughput and minimize contention.

use dashmap::DashMap;
use crossbeam::queue::SegQueue;
use tokio::sync::{mpsc, oneshot};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use uuid::Uuid;
use std::time::{Duration, Instant};

/// Sandbox identifier type
pub type SandboxId = Uuid;

/// Lock-free sandbox pool for managing active sandboxes
pub struct LockFreeSandboxPool {
    /// Lock-free concurrent hashmap for active sandboxes
    active_sandboxes: Arc<DashMap<SandboxId, SandboxHandle>>,
    /// Lock-free queue for available sandboxes
    available_sandboxes: Arc<SegQueue<SandboxHandle>>,
    /// Request queue using mpsc channel
    request_queue: mpsc::Sender<ExecutionRequest>,
    /// Atomic counter for active sandbox count
    active_count: Arc<AtomicU64>,
    /// Maximum number of sandboxes
    max_sandboxes: u64,
}

impl LockFreeSandboxPool {
    pub fn new(max_sandboxes: u64) -> (Self, mpsc::Receiver<ExecutionRequest>) {
        let (tx, rx) = mpsc::channel(1000);
        
        (
            Self {
                active_sandboxes: Arc::new(DashMap::new()),
                available_sandboxes: Arc::new(SegQueue::new()),
                request_queue: tx,
                active_count: Arc::new(AtomicU64::new(0)),
                max_sandboxes,
            },
            rx,
        )
    }

    /// Acquire a sandbox from the pool (lock-free)
    pub async fn acquire_sandbox(&self) -> Result<SandboxHandle, PoolError> {
        // First try to get from available pool
        if let Some(handle) = self.available_sandboxes.pop() {
            self.activate_sandbox(handle.id, handle.clone());
            return Ok(handle);
        }

        // Check if we can create a new sandbox
        let current = self.active_count.load(Ordering::Relaxed);
        if current < self.max_sandboxes {
            // Try to increment atomically
            if self.active_count.compare_exchange(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ).is_ok() {
                let handle = self.create_new_sandbox().await?;
                self.activate_sandbox(handle.id, handle.clone());
                return Ok(handle);
            }
        }

        Err(PoolError::NoAvailableSandboxes)
    }

    /// Release a sandbox back to the pool (lock-free)
    pub fn release_sandbox(&self, handle: SandboxHandle) {
        // Remove from active map
        self.active_sandboxes.remove(&handle.id);
        
        // Reset and add to available queue
        let reset_handle = handle.reset();
        self.available_sandboxes.push(reset_handle);
    }

    /// Submit an execution request (non-blocking)
    pub async fn submit_request(&self, request: ExecutionRequest) -> Result<(), PoolError> {
        self.request_queue
            .send(request)
            .await
            .map_err(|_| PoolError::QueueFull)
    }

    fn activate_sandbox(&self, id: SandboxId, handle: SandboxHandle) {
        self.active_sandboxes.insert(id, handle);
    }

    async fn create_new_sandbox(&self) -> Result<SandboxHandle, PoolError> {
        // Placeholder for actual sandbox creation
        Ok(SandboxHandle {
            id: Uuid::new_v4(),
            container_id: format!("container_{}", Uuid::new_v4()),
            created_at: Instant::now(),
            in_use: Arc::new(AtomicBool::new(true)),
        })
    }
}

/// Handle to a sandbox instance
#[derive(Clone, Debug)]
pub struct SandboxHandle {
    pub id: SandboxId,
    pub container_id: String,
    pub created_at: Instant,
    pub in_use: Arc<AtomicBool>,
}

impl SandboxHandle {
    /// Reset the sandbox for reuse
    pub fn reset(self) -> Self {
        self.in_use.store(false, Ordering::Relaxed);
        self
    }

    /// Check if sandbox is in use
    pub fn is_in_use(&self) -> bool {
        self.in_use.load(Ordering::Relaxed)
    }
}

/// Execution request for queuing
#[derive(Debug)]
pub struct ExecutionRequest {
    pub id: Uuid,
    pub code: String,
    pub language: String,
    pub timeout: Duration,
    pub response_channel: oneshot::Sender<ExecutionResponse>,
}

/// Execution response
#[derive(Debug)]
pub struct ExecutionResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub execution_time: Duration,
}

/// Lock-free metrics collector using atomic operations
pub struct LockFreeMetrics {
    /// Total execution count
    pub executions: AtomicU64,
    /// Failed execution count
    pub failures: AtomicU64,
    /// Total execution time in microseconds
    pub total_time_us: AtomicU64,
    /// Current active sandboxes
    pub active_sandboxes: AtomicU64,
}

impl LockFreeMetrics {
    pub fn new() -> Self {
        Self {
            executions: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            total_time_us: AtomicU64::new(0),
            active_sandboxes: AtomicU64::new(0),
        }
    }

    /// Record an execution (lock-free)
    pub fn record_execution(&self, success: bool, duration: Duration) {
        self.executions.fetch_add(1, Ordering::Relaxed);
        
        if !success {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
        
        let micros = duration.as_micros() as u64;
        self.total_time_us.fetch_add(micros, Ordering::Relaxed);
    }

    /// Update active sandbox count (lock-free)
    pub fn update_active_sandboxes(&self, delta: i64) {
        if delta > 0 {
            self.active_sandboxes.fetch_add(delta as u64, Ordering::Relaxed);
        } else {
            self.active_sandboxes.fetch_sub((-delta) as u64, Ordering::Relaxed);
        }
    }

    /// Get current metrics snapshot
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            executions: self.executions.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            total_time_us: self.total_time_us.load(Ordering::Relaxed),
            active_sandboxes: self.active_sandboxes.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub executions: u64,
    pub failures: u64,
    pub total_time_us: u64,
    pub active_sandboxes: u64,
}

impl MetricsSnapshot {
    pub fn success_rate(&self) -> f64 {
        if self.executions == 0 {
            0.0
        } else {
            ((self.executions - self.failures) as f64) / (self.executions as f64)
        }
    }

    pub fn average_time_ms(&self) -> f64 {
        if self.executions == 0 {
            0.0
        } else {
            (self.total_time_us as f64) / (self.executions as f64) / 1000.0
        }
    }
}

/// Lock-free cache using DashMap
pub struct LockFreeCache<K, V> 
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    data: Arc<DashMap<K, CacheEntry<V>>>,
    max_size: usize,
}

#[derive(Clone)]
struct CacheEntry<V> {
    value: V,
    last_access: Instant,
    access_count: Arc<AtomicU64>,
}

impl<K, V> LockFreeCache<K, V>
where
    K: Eq + std::hash::Hash + Clone,
    V: Clone,
{
    pub fn new(max_size: usize) -> Self {
        Self {
            data: Arc::new(DashMap::new()),
            max_size,
        }
    }

    /// Get value from cache (lock-free read)
    pub fn get(&self, key: &K) -> Option<V> {
        self.data.get(key).map(|entry| {
            entry.access_count.fetch_add(1, Ordering::Relaxed);
            entry.value.clone()
        })
    }

    /// Insert value into cache (lock-free write)
    pub fn insert(&self, key: K, value: V) {
        // Simple eviction if cache is full
        if self.data.len() >= self.max_size {
            // Remove least recently used (simplified)
            if let Some(oldest_key) = self.find_oldest_key() {
                self.data.remove(&oldest_key);
            }
        }

        let entry = CacheEntry {
            value,
            last_access: Instant::now(),
            access_count: Arc::new(AtomicU64::new(0)),
        };
        
        self.data.insert(key, entry);
    }

    fn find_oldest_key(&self) -> Option<K> {
        self.data
            .iter()
            .min_by_key(|entry| entry.last_access)
            .map(|entry| entry.key().clone())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("No available sandboxes")]
    NoAvailableSandboxes,
    #[error("Queue is full")]
    QueueFull,
    #[error("Sandbox creation failed")]
    CreationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_pool() {
        let (pool, _rx) = LockFreeSandboxPool::new(10);
        
        // Acquire sandbox
        let handle1 = pool.acquire_sandbox().await.unwrap();
        assert!(handle1.is_in_use());
        
        // Release sandbox
        pool.release_sandbox(handle1.clone());
        
        // Should be able to acquire again
        let handle2 = pool.acquire_sandbox().await.unwrap();
        assert!(handle2.is_in_use());
    }

    #[test]
    fn test_lock_free_metrics() {
        let metrics = LockFreeMetrics::new();
        
        // Record some executions
        metrics.record_execution(true, Duration::from_millis(100));
        metrics.record_execution(false, Duration::from_millis(50));
        metrics.record_execution(true, Duration::from_millis(75));
        
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.executions, 3);
        assert_eq!(snapshot.failures, 1);
        assert_eq!(snapshot.success_rate(), 2.0 / 3.0);
    }

    #[test]
    fn test_lock_free_cache() {
        let cache = LockFreeCache::new(3);
        
        // Insert values
        cache.insert("key1", "value1");
        cache.insert("key2", "value2");
        cache.insert("key3", "value3");
        
        // Get values
        assert_eq!(cache.get(&"key1"), Some("value1"));
        assert_eq!(cache.get(&"key2"), Some("value2"));
        
        // Insert when full (should evict)
        cache.insert("key4", "value4");
        assert_eq!(cache.get(&"key4"), Some("value4"));
    }
}