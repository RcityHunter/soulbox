//! Dependency caching system for optimizing package installations
//! 
//! This module provides:
//! - Intelligent package caching to reduce download times
//! - Version-aware cache management
//! - Cache integrity verification
//! - Automatic cache cleanup and pruning
//! - Compressed storage for space efficiency

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use thiserror::Error;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, warn, error, info};

/// Cache-related errors
#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Cache entry not found: {0}")]
    EntryNotFound(String),
    
    #[error("Cache corruption detected: {0}")]
    Corruption(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Compression error: {0}")]
    Compression(String),
    
    #[error("Cache full: {0}")]
    CacheFull(String),
    
    #[error("Invalid cache configuration: {0}")]
    InvalidConfig(String),
}

/// Cache entry metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Package name
    pub package_name: String,
    /// Package version
    pub version: String,
    /// Package manager used
    pub package_manager: String,
    /// Size in bytes (uncompressed)
    pub size_bytes: u64,
    /// Compressed size in bytes
    pub compressed_size_bytes: u64,
    /// SHA256 checksum of cached content
    pub checksum: String,
    /// When this entry was cached
    pub cached_at: SystemTime,
    /// When this entry was last accessed
    pub last_accessed: SystemTime,
    /// Number of times this entry has been accessed
    pub access_count: u64,
    /// Cache file path (relative to cache root)
    pub file_path: PathBuf,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum cache size in bytes
    pub max_size_bytes: u64,
    /// Maximum number of entries
    pub max_entries: usize,
    /// Cache expiry time in seconds
    pub expiry_secs: u64,
    /// Enable compression
    pub enable_compression: bool,
    /// Compression level (0-9)
    pub compression_level: u32,
    /// Cleanup interval in seconds
    pub cleanup_interval_secs: u64,
    /// Cache directory path
    pub cache_dir: PathBuf,
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total entries in cache
    pub total_entries: usize,
    /// Total cache size in bytes
    pub total_size_bytes: u64,
    /// Cache hits
    pub cache_hits: u64,
    /// Cache misses
    pub cache_misses: u64,
    /// Hit ratio (0.0 - 1.0)
    pub hit_ratio: f64,
    /// Total space saved by compression
    pub compression_savings_bytes: u64,
    /// Last cleanup time
    pub last_cleanup: Option<SystemTime>,
}

/// Cache eviction policy
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EvictionPolicy {
    /// Least Recently Used
    LRU,
    /// Least Frequently Used
    LFU,
    /// First In, First Out
    FIFO,
    /// Largest First (by size)
    LargestFirst,
}

/// Dependency cache manager
pub struct DependencyCache {
    /// Cache configuration
    config: CacheConfig,
    /// Cache index (in memory for fast access)
    index: tokio::sync::RwLock<HashMap<String, CacheEntry>>,
    /// Cache statistics
    stats: tokio::sync::RwLock<CacheStats>,
    /// Eviction policy
    eviction_policy: EvictionPolicy,
}

impl DependencyCache {
    /// Create a new dependency cache
    pub async fn new() -> Result<Self, CacheError> {
        let config = CacheConfig::default();
        Self::with_config(config).await
    }
    
    /// Create cache with custom configuration
    pub async fn with_config(config: CacheConfig) -> Result<Self, CacheError> {
        // Create cache directory
        fs::create_dir_all(&config.cache_dir).await?;
        
        let cache = Self {
            config,
            index: tokio::sync::RwLock::new(HashMap::new()),
            stats: tokio::sync::RwLock::new(CacheStats::default()),
            eviction_policy: EvictionPolicy::LRU,
        };
        
        // Load existing cache index
        cache.load_cache_index().await?;
        
        info!("Dependency cache initialized at {:?}", cache.config.cache_dir);
        
        Ok(cache)
    }
    
    /// Load cache index from disk
    async fn load_cache_index(&self) -> Result<(), CacheError> {
        let index_path = self.config.cache_dir.join("cache_index.json");
        
        if index_path.exists() {
            let content = fs::read_to_string(&index_path).await?;
            let entries: HashMap<String, CacheEntry> = serde_json::from_str(&content)
                .map_err(|e| CacheError::Serialization(e.to_string()))?;
            
            // Verify cache entries exist on disk
            let mut valid_entries = HashMap::new();
            for (key, entry) in entries {
                let cache_file = self.config.cache_dir.join(&entry.file_path);
                if cache_file.exists() {
                    valid_entries.insert(key, entry);
                } else {
                    warn!("Cache entry file missing: {:?}", cache_file);
                }
            }
            
            *self.index.write().await = valid_entries;
            info!("Loaded {} cache entries", self.index.read().await.len());
        }
        
        Ok(())
    }
    
    /// Save cache index to disk
    async fn save_cache_index(&self) -> Result<(), CacheError> {
        let index_path = self.config.cache_dir.join("cache_index.json");
        let index = self.index.read().await;
        
        let content = serde_json::to_string_pretty(&*index)
            .map_err(|e| CacheError::Serialization(e.to_string()))?;
        
        fs::write(&index_path, content).await?;
        Ok(())
    }
    
    /// Generate cache key for a package
    fn cache_key(&self, package_name: &str, version: Option<&str>, package_manager: &str) -> String {
        match version {
            Some(v) => format!("{}:{}:{}", package_manager, package_name, v),
            None => format!("{}:{}:latest", package_manager, package_name),
        }
    }
    
    /// Check if a package is cached
    pub async fn has_package(&self, package_name: &str, version: Option<&str>) -> bool {
        let key = self.cache_key(package_name, version, "pip"); // Default to pip for now
        self.index.read().await.contains_key(&key)
    }
    
    /// Get a cached package
    pub async fn get_package(&self, package_name: &str, version: Option<&str>) -> Result<CacheEntry, CacheError> {
        let key = self.cache_key(package_name, version, "pip");
        
        let mut index = self.index.write().await;
        if let Some(mut entry) = index.get(&key).cloned() {
            // Update access statistics
            entry.last_accessed = SystemTime::now();
            entry.access_count += 1;
            index.insert(key.clone(), entry.clone());
            
            // Update cache stats
            {
                let mut stats = self.stats.write().await;
                stats.cache_hits += 1;
                stats.hit_ratio = stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64;
            }
            
            debug!("Cache hit for package: {}", package_name);
            Ok(entry)
        } else {
            // Update miss statistics
            {
                let mut stats = self.stats.write().await;
                stats.cache_misses += 1;
                stats.hit_ratio = stats.cache_hits as f64 / (stats.cache_hits + stats.cache_misses) as f64;
            }
            
            debug!("Cache miss for package: {}", package_name);
            Err(CacheError::EntryNotFound(key))
        }
    }
    
    /// Store a package in cache
    pub async fn store_package(
        &self,
        package_name: &str,
        version: &str,
        package_manager: &str,
        content: &[u8],
    ) -> Result<CacheEntry, CacheError> {
        let key = self.cache_key(package_name, Some(version), package_manager);
        
        // Check if we need to make space
        self.ensure_cache_space(content.len() as u64).await?;
        
        // Generate file path
        let file_name = format!("{}.tar.gz", self.sanitize_filename(&key));
        let file_path = PathBuf::from("packages").join(&file_name);
        let full_path = self.config.cache_dir.join(&file_path);
        
        // Create package directory if needed
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        // Compress content if enabled
        let (stored_content, compressed_size) = if self.config.enable_compression {
            let compressed = self.compress_content(content)?;
            let compressed_size = compressed.len() as u64;
            (compressed, compressed_size)
        } else {
            (content.to_vec(), content.len() as u64)
        };
        
        // Write to cache file
        fs::write(&full_path, &stored_content).await?;
        
        // Calculate checksum
        let checksum = self.calculate_checksum(content);
        
        // Create cache entry
        let entry = CacheEntry {
            package_name: package_name.to_string(),
            version: version.to_string(),
            package_manager: package_manager.to_string(),
            size_bytes: content.len() as u64,
            compressed_size_bytes: compressed_size,
            checksum,
            cached_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 0,
            file_path,
            metadata: HashMap::new(),
        };
        
        // Store in index
        {
            let mut index = self.index.write().await;
            index.insert(key, entry.clone());
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_entries = self.index.read().await.len();
            stats.total_size_bytes += compressed_size;
            if self.config.enable_compression {
                stats.compression_savings_bytes += content.len() as u64 - compressed_size;
            }
        }
        
        // Save index
        self.save_cache_index().await?;
        
        info!("Cached package: {} v{} ({} bytes)", package_name, version, content.len());
        Ok(entry)
    }
    
    /// Read cached package content
    pub async fn read_package_content(&self, entry: &CacheEntry) -> Result<Vec<u8>, CacheError> {
        let full_path = self.config.cache_dir.join(&entry.file_path);
        let content = fs::read(&full_path).await?;
        
        // Decompress if needed
        if self.config.enable_compression {
            self.decompress_content(&content)
        } else {
            Ok(content)
        }
    }
    
    /// Ensure there's enough space in cache
    async fn ensure_cache_space(&self, required_bytes: u64) -> Result<(), CacheError> {
        let current_size = {
            let stats = self.stats.read().await;
            stats.total_size_bytes
        };
        
        if current_size + required_bytes > self.config.max_size_bytes {
            self.evict_entries(required_bytes).await?;
        }
        
        Ok(())
    }
    
    /// Evict cache entries based on policy with atomic operations
    async fn evict_entries(&self, space_needed: u64) -> Result<(), CacheError> {
        let entries_to_evict = {
            let index = self.index.read().await;
            let mut entries: Vec<_> = index.values().cloned().collect();
            
            // Sort entries based on eviction policy
            match self.eviction_policy {
                EvictionPolicy::LRU => {
                    entries.sort_by(|a, b| a.last_accessed.cmp(&b.last_accessed));
                },
                EvictionPolicy::LFU => {
                    entries.sort_by(|a, b| a.access_count.cmp(&b.access_count));
                },
                EvictionPolicy::FIFO => {
                    entries.sort_by(|a, b| a.cached_at.cmp(&b.cached_at));
                },
                EvictionPolicy::LargestFirst => {
                    entries.sort_by(|a, b| b.compressed_size_bytes.cmp(&a.compressed_size_bytes));
                },
            }
            
            // Select entries to evict
            let mut selected_entries = Vec::new();
            let mut freed_space = 0u64;
            
            for entry in entries {
                if freed_space >= space_needed {
                    break;
                }
                freed_space += entry.compressed_size_bytes;
                selected_entries.push(entry);
            }
            
            selected_entries
        };
        
        // Perform atomic eviction of selected entries
        let mut evicted_count = 0;
        let mut total_freed = 0u64;
        
        for entry in &entries_to_evict {
            match self.atomic_evict_entry(entry).await {
                Ok(freed_bytes) => {
                    evicted_count += 1;
                    total_freed += freed_bytes;
                },
                Err(e) => {
                    warn!("Failed to evict entry {}: {}", entry.package_name, e);
                }
            }
        }
        
        // Update global statistics
        {
            let mut stats = self.stats.write().await;
            let index = self.index.read().await;
            stats.total_entries = index.len();
            stats.total_size_bytes = stats.total_size_bytes.saturating_sub(total_freed);
        }
        
        // Save updated index atomically
        self.save_cache_index().await?;
        
        info!("Evicted {} cache entries, freed {} bytes", evicted_count, total_freed);
        Ok(())
    }
    
    /// Atomically evict a single cache entry
    async fn atomic_evict_entry(&self, entry: &CacheEntry) -> Result<u64, CacheError> {
        let key = self.cache_key(&entry.package_name, Some(&entry.version), &entry.package_manager);
        let full_path = self.config.cache_dir.join(&entry.file_path);
        
        // Create a transaction-like operation
        // 1. First, remove from index
        let removed_entry = {
            let mut index = self.index.write().await;
            index.remove(&key)
        };
        
        if let Some(removed_entry) = removed_entry {
            // 2. Then try to remove the file
            match fs::remove_file(&full_path).await {
                Ok(()) => {
                    debug!("Successfully evicted cache entry: {}", key);
                    Ok(removed_entry.compressed_size_bytes)
                },
                Err(e) => {
                    // Rollback: restore entry to index if file removal failed
                    {
                        let mut index = self.index.write().await;
                        index.insert(key.clone(), removed_entry);
                    }
                    Err(CacheError::Io(e))
                }
            }
        } else {
            // Entry was already removed by another operation
            Ok(0)
        }
    }
    
    /// Clean up expired cache entries with atomic operations
    pub async fn cleanup_expired(&self) -> Result<usize, CacheError> {
        let now = SystemTime::now();
        let expiry_duration = std::time::Duration::from_secs(self.config.expiry_secs);
        
        // First, identify expired entries without holding write lock
        let expired_entries = {
            let index = self.index.read().await;
            let mut expired = Vec::new();
            
            for (key, entry) in index.iter() {
                if let Ok(elapsed) = now.duration_since(entry.last_accessed) {
                    if elapsed > expiry_duration {
                        expired.push((key.clone(), entry.clone()));
                    }
                }
            }
            
            expired
        };
        
        if expired_entries.is_empty() {
            return Ok(0);
        }
        
        // Atomically remove expired entries
        let mut cleaned_count = 0;
        let mut total_freed = 0u64;
        
        for (key, entry) in expired_entries {
            match self.atomic_remove_entry(&key, &entry).await {
                Ok(freed_bytes) => {
                    cleaned_count += 1;
                    total_freed += freed_bytes;
                },
                Err(e) => {
                    warn!("Failed to remove expired entry {}: {}", key, e);
                }
            }
        }
        
        // Update global statistics
        if cleaned_count > 0 {
            {
                let mut stats = self.stats.write().await;
                let index = self.index.read().await;
                stats.total_entries = index.len();
                stats.total_size_bytes = stats.total_size_bytes.saturating_sub(total_freed);
                stats.last_cleanup = Some(now);
            }
            
            // Save updated index atomically
            self.save_cache_index().await?;
            
            info!("Cleaned up {} expired cache entries, freed {} bytes", cleaned_count, total_freed);
        }
        
        Ok(cleaned_count)
    }
    
    /// Atomically remove a cache entry by key and entry data
    async fn atomic_remove_entry(&self, key: &str, entry: &CacheEntry) -> Result<u64, CacheError> {
        let full_path = self.config.cache_dir.join(&entry.file_path);
        
        // Create atomic transaction: remove from index first, then file
        let removed_entry = {
            let mut index = self.index.write().await;
            index.remove(key)
        };
        
        if let Some(removed_entry) = removed_entry {
            // Try to remove the physical file
            match fs::remove_file(&full_path).await {
                Ok(()) => {
                    debug!("Successfully removed cache entry: {}", key);
                    Ok(removed_entry.compressed_size_bytes)
                },
                Err(e) => {
                    // Rollback: restore entry to index if file removal failed
                    {
                        let mut index = self.index.write().await;
                        index.insert(key.to_string(), removed_entry);
                    }
                    warn!("Failed to remove cache file, rolling back index change: {}", e);
                    Err(CacheError::Io(e))
                }
            }
        } else {
            // Entry was already removed by another operation
            Ok(0)
        }
    }
    
    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }
    
    /// Clear entire cache
    pub async fn clear_cache(&self) -> Result<(), CacheError> {
        // Remove all cache files
        let packages_dir = self.config.cache_dir.join("packages");
        if packages_dir.exists() {
            fs::remove_dir_all(&packages_dir).await?;
            fs::create_dir_all(&packages_dir).await?;
        }
        
        // Clear index
        {
            let mut index = self.index.write().await;
            index.clear();
        }
        
        // Reset statistics
        {
            let mut stats = self.stats.write().await;
            *stats = CacheStats::default();
        }
        
        // Save empty index
        self.save_cache_index().await?;
        
        info!("Cache cleared");
        Ok(())
    }
    
    /// Compress content using gzip
    fn compress_content(&self, content: &[u8]) -> Result<Vec<u8>, CacheError> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        
        let mut encoder = GzEncoder::new(Vec::new(), Compression::new(self.config.compression_level));
        encoder.write_all(content)
            .map_err(|e| CacheError::Compression(e.to_string()))?;
        encoder.finish()
            .map_err(|e| CacheError::Compression(e.to_string()))
    }
    
    /// Decompress content using gzip
    fn decompress_content(&self, content: &[u8]) -> Result<Vec<u8>, CacheError> {
        use flate2::read::GzDecoder;
        use std::io::Read;
        
        let mut decoder = GzDecoder::new(content);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| CacheError::Compression(e.to_string()))?;
        Ok(decompressed)
    }
    
    /// Calculate SHA256 checksum
    fn calculate_checksum(&self, content: &[u8]) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(content);
        format!("{:x}", hasher.finalize())
    }
    
    /// Sanitize filename for filesystem
    fn sanitize_filename(&self, name: &str) -> String {
        name.chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                c => c,
            })
            .collect()
    }
    
    /// Verify cache integrity
    pub async fn verify_integrity(&self) -> Result<Vec<String>, CacheError> {
        let mut corrupted_entries = Vec::new();
        let index = self.index.read().await;
        
        for (key, entry) in index.iter() {
            let full_path = self.config.cache_dir.join(&entry.file_path);
            
            if !full_path.exists() {
                corrupted_entries.push(format!("Missing file for entry: {}", key));
                continue;
            }
            
            // Read and verify checksum
            if let Ok(content) = fs::read(&full_path).await {
                let actual_checksum = self.calculate_checksum(&content);
                if actual_checksum != entry.checksum {
                    corrupted_entries.push(format!("Checksum mismatch for entry: {}", key));
                }
            } else {
                corrupted_entries.push(format!("Failed to read file for entry: {}", key));
            }
        }
        
        Ok(corrupted_entries)
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("soulbox-dependency-cache");
            
        Self {
            max_size_bytes: 1024 * 1024 * 1024, // 1GB
            max_entries: 10000,
            expiry_secs: 30 * 24 * 60 * 60, // 30 days
            enable_compression: true,
            compression_level: 6,
            cleanup_interval_secs: 24 * 60 * 60, // Daily
            cache_dir,
        }
    }
}

impl Default for CacheStats {
    fn default() -> Self {
        Self {
            total_entries: 0,
            total_size_bytes: 0,
            cache_hits: 0,
            cache_misses: 0,
            hit_ratio: 0.0,
            compression_savings_bytes: 0,
            last_cleanup: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    async fn create_test_cache() -> DependencyCache {
        let temp_dir = TempDir::new().unwrap();
        let config = CacheConfig {
            cache_dir: temp_dir.path().to_path_buf(),
            max_size_bytes: 1024 * 1024, // 1MB for testing
            ..Default::default()
        };
        
        DependencyCache::with_config(config).await.unwrap()
    }
    
    #[tokio::test]
    async fn test_cache_creation() {
        let cache = create_test_cache().await;
        let stats = cache.get_stats().await;
        assert_eq!(stats.total_entries, 0);
    }
    
    #[tokio::test]
    async fn test_package_storage_and_retrieval() {
        let cache = create_test_cache().await;
        let content = b"test package content";
        
        // Store package
        let entry = cache.store_package("test-pkg", "1.0.0", "pip", content).await.unwrap();
        assert_eq!(entry.package_name, "test-pkg");
        assert_eq!(entry.version, "1.0.0");
        
        // Check if package exists
        assert!(cache.has_package("test-pkg", Some("1.0.0")).await);
        
        // Retrieve package
        let retrieved_entry = cache.get_package("test-pkg", Some("1.0.0")).await.unwrap();
        assert_eq!(retrieved_entry.package_name, "test-pkg");
        
        // Read content
        let retrieved_content = cache.read_package_content(&retrieved_entry).await.unwrap();
        assert_eq!(retrieved_content, content);
    }
    
    #[tokio::test]
    async fn test_cache_miss() {
        let cache = create_test_cache().await;
        
        // Try to get non-existent package
        let result = cache.get_package("non-existent", Some("1.0.0")).await;
        assert!(result.is_err());
        
        let stats = cache.get_stats().await;
        assert_eq!(stats.cache_misses, 1);
        assert_eq!(stats.cache_hits, 0);
    }
    
    #[tokio::test]
    async fn test_cache_key_generation() {
        let cache = create_test_cache().await;
        
        let key1 = cache.cache_key("package", Some("1.0.0"), "pip");
        let key2 = cache.cache_key("package", None, "pip");
        
        assert_eq!(key1, "pip:package:1.0.0");
        assert_eq!(key2, "pip:package:latest");
    }
    
    #[tokio::test]
    async fn test_filename_sanitization() {
        let cache = create_test_cache().await;
        let unsafe_name = "package:with/unsafe\\chars";
        let safe_name = cache.sanitize_filename(unsafe_name);
        
        assert!(!safe_name.contains(':'));
        assert!(!safe_name.contains('/'));
        assert!(!safe_name.contains('\\'));
    }
}