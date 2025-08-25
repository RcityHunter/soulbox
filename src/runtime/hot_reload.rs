//! Hot Reload Mechanism
//! 
//! Provides automatic code reloading without container restart

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{RwLock, mpsc};
use tokio::time::interval;
// use notify::{Watcher, RecursiveMode, Event, EventKind}; // Simplified for MVP
use tracing::{info, error, debug, warn};
use uuid::Uuid;

use crate::runtime::RuntimeType;
use crate::container::CodeExecutor;
use crate::error::{Result, SoulBoxError};

/// Serde helper for Duration
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_millis().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

/// File change event
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: PathBuf,
    pub change_type: FileChangeType,
    pub timestamp: SystemTime,
}

/// Type of file change
#[derive(Debug, Clone, PartialEq)]
pub enum FileChangeType {
    Created,
    Modified,
    Deleted,
    Renamed { from: PathBuf, to: PathBuf },
}

/// Hot reload configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HotReloadConfig {
    /// Enable hot reload
    pub enabled: bool,
    /// Debounce delay to avoid rapid reloads (in milliseconds)
    #[serde(with = "duration_serde")]
    pub debounce_delay: Duration,
    /// File patterns to watch
    pub watch_patterns: Vec<String>,
    /// File patterns to ignore
    pub ignore_patterns: Vec<String>,
    /// Maximum reload frequency (in milliseconds)
    #[serde(with = "duration_serde")]
    pub max_reload_frequency: Duration,
    /// Auto-save before reload
    pub auto_save: bool,
}

impl Default for HotReloadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_delay: Duration::from_millis(500),
            watch_patterns: vec![
                "*.py".to_string(),
                "*.js".to_string(),
                "*.ts".to_string(),
                "*.rs".to_string(),
                "*.go".to_string(),
                "*.java".to_string(),
            ],
            ignore_patterns: vec![
                "*.tmp".to_string(),
                "*.log".to_string(),
                "*~".to_string(),
                ".git/**".to_string(),
                "node_modules/**".to_string(),
                "target/**".to_string(),
            ],
            max_reload_frequency: Duration::from_millis(1000),
            auto_save: true,
        }
    }
}

/// Hot reload session
struct HotReloadSession {
    id: Uuid,
    container_id: String,
    runtime: RuntimeType,
    main_file: PathBuf,
    watched_files: Vec<PathBuf>,
    last_reload: SystemTime,
    pending_changes: Vec<FileChangeEvent>,
    executor: Arc<CodeExecutor>,
}

/// Hot reload manager
pub struct HotReloadManager {
    config: HotReloadConfig,
    sessions: Arc<RwLock<HashMap<Uuid, Arc<RwLock<HotReloadSession>>>>>,
    change_tx: mpsc::Sender<FileChangeEvent>,
    change_rx: Arc<RwLock<mpsc::Receiver<FileChangeEvent>>>,
}

impl HotReloadManager {
    /// Create new hot reload manager
    pub fn new(config: HotReloadConfig) -> Result<Self> {
        let (change_tx, change_rx) = mpsc::channel(1000);
        
        Ok(Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            change_tx,
            change_rx: Arc::new(RwLock::new(change_rx)),
        })
    }

    /// Start hot reload manager
    pub async fn start(&self) -> Result<()> {
        if !self.config.enabled {
            info!("Hot reload is disabled");
            return Ok(());
        }

        info!("Starting hot reload manager");

        // Note: File watcher initialization simplified for MVP

        // Start change processor
        let sessions = Arc::clone(&self.sessions);
        let config = self.config.clone();
        let change_rx = Arc::clone(&self.change_rx);
        
        tokio::spawn(async move {
            Self::process_file_changes(sessions, config, change_rx).await;
        });

        // Start cleanup task
        let sessions_cleanup = Arc::clone(&self.sessions);
        tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(300)); // 5 minutes
            loop {
                cleanup_interval.tick().await;
                Self::cleanup_inactive_sessions(sessions_cleanup.clone()).await;
            }
        });

        info!("Hot reload manager started successfully");
        Ok(())
    }

    /// Initialize file system watcher (simplified for MVP)
    async fn init_file_watcher(&self) -> Result<()> {
        // Simplified implementation for MVP - would use notify in production
        info!("File watcher initialization simplified for MVP");
        Ok(())
    }

    /// Convert notify event to our file change event (simplified for MVP)
    fn convert_notify_event(_event: ()) -> Option<FileChangeEvent> {
        // Simplified implementation for MVP
        None
    }

    /// Process file changes
    async fn process_file_changes(
        sessions: Arc<RwLock<HashMap<Uuid, Arc<RwLock<HotReloadSession>>>>>,
        config: HotReloadConfig,
        change_rx: Arc<RwLock<mpsc::Receiver<FileChangeEvent>>>,
    ) {
        let mut debounce_timer = interval(config.debounce_delay);
        
        loop {
            tokio::select! {
                _ = debounce_timer.tick() => {
                    Self::process_pending_changes(Arc::clone(&sessions), &config).await;
                }
                
                change = async {
                    let mut rx = change_rx.write().await;
                    rx.recv().await
                } => {
                    if let Some(change_event) = change {
                        Self::handle_file_change(Arc::clone(&sessions), &config, change_event).await;
                    } else {
                        break;
                    }
                }
            }
        }
    }

    /// Handle individual file change
    async fn handle_file_change(
        sessions: Arc<RwLock<HashMap<Uuid, Arc<RwLock<HotReloadSession>>>>>,
        config: &HotReloadConfig,
        change_event: FileChangeEvent,
    ) {
        // Check if file matches watch patterns
        if !Self::should_watch_file(&change_event.path, config) {
            return;
        }

        debug!("File change detected: {:?}", change_event);

        let sessions_guard = sessions.read().await;
        for session in sessions_guard.values() {
            let mut session_guard = session.write().await;
            
            // Check if this file affects this session
            if Self::file_affects_session(&change_event.path, &session_guard) {
                session_guard.pending_changes.push(change_event.clone());
            }
        }
    }

    /// Check if file should be watched
    fn should_watch_file(path: &Path, config: &HotReloadConfig) -> bool {
        let path_str = path.to_string_lossy();
        
        // Check ignore patterns first
        for ignore_pattern in &config.ignore_patterns {
            if Self::matches_pattern(&path_str, ignore_pattern) {
                return false;
            }
        }
        
        // Check watch patterns
        for watch_pattern in &config.watch_patterns {
            if Self::matches_pattern(&path_str, watch_pattern) {
                return true;
            }
        }
        
        false
    }

    /// Simple pattern matching (supports * wildcard)
    fn matches_pattern(path: &str, pattern: &str) -> bool {
        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                let (prefix, suffix) = (parts[0], parts[1]);
                return path.starts_with(prefix) && path.ends_with(suffix);
            }
        }
        path == pattern
    }

    /// Check if file change affects a session
    fn file_affects_session(path: &Path, session: &HotReloadSession) -> bool {
        // Check if it's the main file
        if path == session.main_file {
            return true;
        }
        
        // Check if it's in watched files
        session.watched_files.iter().any(|watched| path == watched)
    }

    /// Process all pending changes
    async fn process_pending_changes(
        sessions: Arc<RwLock<HashMap<Uuid, Arc<RwLock<HotReloadSession>>>>>,
        config: &HotReloadConfig,
    ) {
        let sessions_guard = sessions.read().await;
        
        for session in sessions_guard.values() {
            let mut session_guard = session.write().await;
            
            if !session_guard.pending_changes.is_empty() {
                // Check reload frequency limit
                if let Ok(elapsed) = session_guard.last_reload.elapsed() {
                    if elapsed < config.max_reload_frequency {
                        continue;
                    }
                }
                
                info!("Triggering hot reload for session {}", session_guard.id);
                
                if let Err(e) = Self::trigger_reload(&mut session_guard, config).await {
                    error!("Hot reload failed for session {}: {}", session_guard.id, e);
                } else {
                    session_guard.last_reload = SystemTime::now();
                }
                
                session_guard.pending_changes.clear();
            }
        }
    }

    /// Trigger code reload
    async fn trigger_reload(
        session: &mut HotReloadSession,
        config: &HotReloadConfig,
    ) -> Result<()> {
        debug!("Reloading code for container {}", session.container_id);
        
        // Auto-save if enabled
        if config.auto_save {
            // In a real implementation, this would save any unsaved buffers
            debug!("Auto-save enabled, saving files...");
        }
        
        // Read main file content
        let main_file_content = std::fs::read_to_string(&session.main_file)
            .map_err(|e| SoulBoxError::RuntimeError(format!("Failed to read main file: {}", e)))?;
        
        // Execute code without restarting container
        let timeout = Duration::from_secs(30); // Default 30 second timeout
        let language = Some(session.runtime.as_str());
        
        match session.executor.execute(&main_file_content, language, timeout).await {
            Ok(result) => {
                if result.is_success() {
                    info!("Hot reload successful for session {}: exit_code={}", session.id, result.exit_code);
                } else {
                    warn!("Hot reload completed with errors for session {}: exit_code={}, stderr={}", 
                          session.id, result.exit_code, result.stderr);
                }
                Ok(())
            }
            Err(e) => {
                error!("Hot reload execution failed for session {}: {}", session.id, e);
                Err(e)
            }
        }
    }

    /// Cleanup inactive sessions
    async fn cleanup_inactive_sessions(
        sessions: Arc<RwLock<HashMap<Uuid, Arc<RwLock<HotReloadSession>>>>>,
    ) {
        let mut sessions_guard = sessions.write().await;
        let threshold = Duration::from_secs(3600); // 1 hour
        
        sessions_guard.retain(|id, session| {
            let session_guard = session.try_read();
            match session_guard {
                Ok(guard) => {
                    if let Ok(elapsed) = guard.last_reload.elapsed() {
                        if elapsed > threshold {
                            info!("Cleaning up inactive hot reload session: {}", id);
                            return false;
                        }
                    }
                    true
                }
                Err(_) => true, // Keep sessions that are currently being used
            }
        });
    }

    /// Create new hot reload session
    pub async fn create_session(
        &self,
        container_id: String,
        runtime: RuntimeType,
        main_file: PathBuf,
        executor: Arc<CodeExecutor>,
    ) -> Result<Uuid> {
        let session_id = Uuid::new_v4();
        
        let session = HotReloadSession {
            id: session_id,
            container_id: container_id.clone(),
            runtime,
            main_file: main_file.clone(),
            watched_files: vec![main_file.clone()],
            last_reload: SystemTime::now(),
            pending_changes: Vec::new(),
            executor,
        };
        
        // Watch the directory containing the main file (simplified for MVP)
        if let Some(parent_dir) = main_file.parent() {
            info!("Would watch directory: {:?} for hot reload (simplified for MVP)", parent_dir);
        }
        
        self.sessions.write().await.insert(session_id, Arc::new(RwLock::new(session)));
        
        info!("Created hot reload session {} for container {}", session_id, container_id);
        Ok(session_id)
    }

    /// Remove hot reload session
    pub async fn remove_session(&self, session_id: Uuid) -> Result<()> {
        if let Some(_session) = self.sessions.write().await.remove(&session_id) {
            info!("Removed hot reload session: {}", session_id);
        }
        Ok(())
    }

    /// Get session statistics
    pub async fn get_session_stats(&self, session_id: Uuid) -> Result<HotReloadStats> {
        let sessions_guard = self.sessions.read().await;
        let session = sessions_guard.get(&session_id)
            .ok_or_else(|| SoulBoxError::RuntimeError("Session not found".to_string()))?;
        
        let session_guard = session.read().await;
        
        Ok(HotReloadStats {
            session_id,
            container_id: session_guard.container_id.clone(),
            runtime: session_guard.runtime.clone(),
            main_file: session_guard.main_file.clone(),
            watched_files_count: session_guard.watched_files.len(),
            pending_changes_count: session_guard.pending_changes.len(),
            last_reload: session_guard.last_reload,
        })
    }

    /// List all active sessions
    pub async fn list_sessions(&self) -> Vec<Uuid> {
        self.sessions.read().await.keys().cloned().collect()
    }
}

/// Hot reload session statistics
#[derive(Debug, Clone)]
pub struct HotReloadStats {
    pub session_id: Uuid,
    pub container_id: String,
    pub runtime: RuntimeType,
    pub main_file: PathBuf,
    pub watched_files_count: usize,
    pub pending_changes_count: usize,
    pub last_reload: SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_hot_reload_manager_creation() {
        let config = HotReloadConfig::default();
        let manager = HotReloadManager::new(config);
        assert!(manager.is_ok());
    }
    
    #[test]
    fn test_pattern_matching() {
        assert!(HotReloadManager::matches_pattern("test.py", "*.py"));
        assert!(HotReloadManager::matches_pattern("app.js", "*.js"));
        assert!(!HotReloadManager::matches_pattern("test.py", "*.js"));
        assert!(HotReloadManager::matches_pattern("exact_match", "exact_match"));
    }
    
    #[test]
    fn test_should_watch_file() {
        let config = HotReloadConfig::default();
        let python_file = Path::new("test.py");
        let log_file = Path::new("test.log");
        
        assert!(HotReloadManager::should_watch_file(python_file, &config));
        assert!(!HotReloadManager::should_watch_file(log_file, &config));
    }
}