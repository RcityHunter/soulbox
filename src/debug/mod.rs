//! Enhanced debugging system for sandbox code execution
//! 
//! This module provides comprehensive debugging capabilities including:
//! - Breakpoint management and step-through debugging
//! - Variable inspection and modification
//! - Call stack analysis
//! - Debug session management
//! - Multi-language debugging support

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, BTreeMap};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, warn, error, info};

pub mod breakpoint;
pub mod inspector;

pub use breakpoint::{BreakpointManager, Breakpoint, BreakpointType, BreakpointCondition};

/// Debug system errors
#[derive(Error, Debug)]
pub enum DebugError {
    #[error("Debug session not found: {0}")]
    SessionNotFound(String),
    
    #[error("Breakpoint error: {0}")]
    Breakpoint(String),
    
    #[error("Variable inspection error: {0}")]
    VariableInspection(String),
    
    #[error("Debug protocol error: {0}")]
    Protocol(String),
    
    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),
    
    #[error("Debug session already active: {0}")]
    SessionAlreadyActive(String),
    
    #[error("Debug runtime error: {0}")]
    Runtime(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Supported programming languages for debugging
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DebugLanguage {
    Python,
    JavaScript,
    TypeScript,
    Rust,
    Go,
    Java,
    CSharp,
    Cpp,
    Ruby,
    PHP,
}

/// Debug session state
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DebugSessionState {
    /// Debug session is starting
    Starting,
    /// Debug session is active and running
    Running,
    /// Execution is paused (hit breakpoint, step, etc.)
    Paused,
    /// Stepping through code
    Stepping,
    /// Debug session is terminating
    Terminating,
    /// Debug session has ended
    Terminated,
    /// Debug session encountered an error
    Error,
}

/// Simplified debug step type - only essential operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepType {
    /// Step to next line
    StepOver,
    /// Continue execution until next breakpoint
    Continue,
}

/// Simplified debug event types - only essential events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebugEvent {
    /// Debug session started
    SessionStarted {
        session_id: String,
        language: DebugLanguage,
    },
    /// Execution paused at breakpoint
    BreakpointHit {
        session_id: String,
        breakpoint_id: String,
        file_path: String,
        line_number: u32,
    },
    /// Debug session terminated
    SessionTerminated {
        session_id: String,
        reason: String,
    },
}

/// Simplified stack frame information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    /// Frame ID
    pub id: String,
    /// Function or method name
    pub name: String,
    /// Source file path
    pub file_path: String,
    /// Line number
    pub line_number: u32,
}

/// Simplified debug session configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugConfig {
    /// Programming language
    pub language: DebugLanguage,
    /// Working directory for debug session
    pub working_directory: PathBuf,
    /// Timeout for debug operations in seconds
    pub timeout_secs: u64,
}

/// Simplified active debug session
#[derive(Debug, Clone)]
pub struct DebugSession {
    /// Session ID
    pub id: String,
    /// Sandbox ID this session belongs to
    pub sandbox_id: String,
    /// Debug configuration
    pub config: DebugConfig,
    /// Current session state
    pub state: DebugSessionState,
    /// Current file being debugged
    pub current_file: Option<PathBuf>,
    /// Current line number
    pub current_line: Option<u32>,
    /// Session start time
    pub started_at: chrono::DateTime<chrono::Utc>,
}

/// Debug session statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugStats {
    /// Total debug sessions created
    pub total_sessions: u64,
    /// Currently active sessions
    pub active_sessions: u32,
    /// Total breakpoints hit
    pub breakpoints_hit: u64,
    /// Total steps executed
    pub steps_executed: u64,
    /// Total variables inspected
    pub variables_inspected: u64,
    /// Average session duration in seconds
    pub avg_session_duration_secs: f64,
}

/// Simplified debug system manager
pub struct DebugManager {
    /// Active debug sessions
    sessions: Arc<RwLock<HashMap<String, DebugSession>>>,
    /// Breakpoint manager
    breakpoint_manager: BreakpointManager,
    /// Debug event sender
    event_sender: Option<mpsc::UnboundedSender<DebugEvent>>,
    /// Debug statistics
    stats: Arc<RwLock<DebugStats>>,
}

impl DebugManager {
    /// Create a new debug manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            breakpoint_manager: BreakpointManager::new(),
            event_sender: None,
            stats: Arc::new(RwLock::new(DebugStats::default())),
        }
    }
    
    /// Set event channel for debug events
    pub fn set_event_channel(&mut self, sender: mpsc::UnboundedSender<DebugEvent>) {
        self.event_sender = Some(sender);
    }
    
    /// Start a new debug session
    pub async fn start_debug_session(
        &mut self,
        sandbox_id: &str,
        config: DebugConfig,
    ) -> Result<String, DebugError> {
        let session_id = format!("debug-{}-{}", sandbox_id, uuid::Uuid::new_v4());
        
        // Check if a debug session is already active for this sandbox
        {
            let sessions = self.sessions.read().await;
            if sessions.values().any(|s| s.sandbox_id == sandbox_id && s.state != DebugSessionState::Terminated) {
                return Err(DebugError::SessionAlreadyActive(sandbox_id.to_string()));
            }
        }
        
        let session = DebugSession {
            id: session_id.clone(),
            sandbox_id: sandbox_id.to_string(),
            config: config.clone(),
            state: DebugSessionState::Starting,
            current_file: None,
            current_line: None,
            started_at: chrono::Utc::now(),
        };
        
        // Initialize debug session based on language
        match config.language {
            DebugLanguage::Python => self.start_python_debug_session(&session).await?,
            DebugLanguage::JavaScript | DebugLanguage::TypeScript => self.start_node_debug_session(&session).await?,
            _ => return Err(DebugError::UnsupportedLanguage(format!("{:?}", config.language))),
        }
        
        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), session);
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_sessions += 1;
            stats.active_sessions += 1;
        }
        
        // Send event
        if let Some(sender) = &self.event_sender {
            let _ = sender.send(DebugEvent::SessionStarted {
                session_id: session_id.clone(),
                language: config.language,
            });
        }
        
        info!("Started debug session {} for sandbox {}", session_id, sandbox_id);
        Ok(session_id)
    }
    
    /// Start Python debug session
    async fn start_python_debug_session(&self, session: &DebugSession) -> Result<(), DebugError> {
        // Python debugging can use debugpy (Python Debug Protocol)
        // This is a simplified implementation
        info!("Starting Python debug session for {}", session.id);
        Ok(())
    }
    
    /// Start Node.js debug session
    async fn start_node_debug_session(&self, session: &DebugSession) -> Result<(), DebugError> {
        // Node.js debugging can use the Chrome DevTools Protocol
        // This is a simplified implementation
        info!("Starting Node.js debug session for {}", session.id);
        Ok(())
    }
    
    /// Stop a debug session
    pub async fn stop_debug_session(&mut self, session_id: &str) -> Result<(), DebugError> {
        let session = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(session_id)
        };
        
        if let Some(mut session) = session {
            session.state = DebugSessionState::Terminated;
            
            // Clean up breakpoints for this session
            self.breakpoint_manager.clear_session_breakpoints(session_id).await;
            
            // Update statistics
            {
                let mut stats = self.stats.write().await;
                stats.active_sessions = stats.active_sessions.saturating_sub(1);
                
                let duration = chrono::Utc::now()
                    .signed_duration_since(session.started_at)
                    .num_seconds() as f64;
                    
                if stats.avg_session_duration_secs == 0.0 {
                    stats.avg_session_duration_secs = duration;
                } else {
                    stats.avg_session_duration_secs = 
                        0.7 * stats.avg_session_duration_secs + 0.3 * duration;
                }
            }
            
            // Send event
            if let Some(sender) = &self.event_sender {
                let _ = sender.send(DebugEvent::SessionTerminated {
                    session_id: session_id.to_string(),
                    reason: "User requested".to_string(),
                });
            }
            
            info!("Stopped debug session {}", session_id);
            Ok(())
        } else {
            Err(DebugError::SessionNotFound(session_id.to_string()))
        }
    }
    
    /// Set breakpoint in a debug session
    pub async fn set_breakpoint(
        &mut self,
        session_id: &str,
        file_path: &str,
        line_number: u32,
        condition: Option<String>,
    ) -> Result<String, DebugError> {
        // Verify session exists
        {
            let sessions = self.sessions.read().await;
            if !sessions.contains_key(session_id) {
                return Err(DebugError::SessionNotFound(session_id.to_string()));
            }
        }
        
        let breakpoint_condition = condition.map(|c| BreakpointCondition::Expression(c));
        let breakpoint_id = self.breakpoint_manager.set_breakpoint(
            session_id,
            file_path,
            line_number,
            BreakpointType::Line,
            breakpoint_condition,
        ).await?;
        
        info!("Set breakpoint {} at {}:{} for session {}", breakpoint_id, file_path, line_number, session_id);
        Ok(breakpoint_id)
    }
    
    /// Remove breakpoint from a debug session
    pub async fn remove_breakpoint(&mut self, session_id: &str, breakpoint_id: &str) -> Result<(), DebugError> {
        self.breakpoint_manager.remove_breakpoint(session_id, breakpoint_id).await
            .map_err(|e| DebugError::Breakpoint(e.to_string()))?;
        
        info!("Removed breakpoint {} from session {}", breakpoint_id, session_id);
        Ok(())
    }
    
    /// Execute a debug step
    pub async fn step(&mut self, session_id: &str, step_type: StepType) -> Result<(), DebugError> {
        let mut session = {
            let mut sessions = self.sessions.write().await;
            sessions.get_mut(session_id)
                .ok_or_else(|| DebugError::SessionNotFound(session_id.to_string()))?
                .clone()
        };
        
        if session.state != DebugSessionState::Paused {
            return Err(DebugError::Runtime(
                "Can only step when debug session is paused".to_string()
            ));
        }
        
        // Update session state
        session.state = DebugSessionState::Stepping;
        
        // Execute simplified step operations
        match step_type {
            StepType::StepOver => self.execute_step_over(&mut session).await?,
            StepType::Continue => self.execute_continue(&mut session).await?,
        }
        
        // Update session in storage
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.to_string(), session.clone());
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.steps_executed += 1;
        }
        
        // Send event
        if let Some(sender) = &self.event_sender {
            let _ = sender.send(DebugEvent::StepCompleted {
                session_id: session_id.to_string(),
                file_path: session.current_file.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                line_number: session.current_line.unwrap_or(0),
            });
        }
        
        debug!("Executed {:?} step for session {}", step_type, session_id);
        Ok(())
    }
    
    /// Execute step over
    async fn execute_step_over(&self, session: &mut DebugSession) -> Result<(), DebugError> {
        // Implementation would depend on the debugging protocol for each language
        session.state = DebugSessionState::Running;
        Ok(())
    }
    
    
    /// Execute continue
    async fn execute_continue(&self, session: &mut DebugSession) -> Result<(), DebugError> {
        session.state = DebugSessionState::Running;
        Ok(())
    }
    
    /// Get current call stack for a debug session (simplified - just returns empty for now)
    pub async fn get_call_stack(&self, session_id: &str) -> Result<Vec<StackFrame>, DebugError> {
        let sessions = self.sessions.read().await;
        let _session = sessions.get(session_id)
            .ok_or_else(|| DebugError::SessionNotFound(session_id.to_string()))?;
        
        // Simplified: return empty call stack
        Ok(Vec::new())
    }
    
    
    /// Get debug session information
    pub async fn get_session(&self, session_id: &str) -> Result<DebugSession, DebugError> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id)
            .cloned()
            .ok_or_else(|| DebugError::SessionNotFound(session_id.to_string()))
    }
    
    /// List all active debug sessions
    pub async fn list_active_sessions(&self) -> Vec<DebugSession> {
        let sessions = self.sessions.read().await;
        sessions.values()
            .filter(|s| s.state != DebugSessionState::Terminated)
            .cloned()
            .collect()
    }
    
    /// Get debug statistics
    pub async fn get_stats(&self) -> DebugStats {
        self.stats.read().await.clone()
    }
    
    /// Handle breakpoint hit
    pub async fn handle_breakpoint_hit(
        &mut self,
        session_id: &str,
        breakpoint_id: &str,
        file_path: &str,
        line_number: u32,
    ) -> Result<(), DebugError> {
        // Update session state
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(session_id) {
                session.state = DebugSessionState::Paused;
                session.current_file = Some(PathBuf::from(file_path));
                session.current_line = Some(line_number);
            }
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.breakpoints_hit += 1;
        }
        
        // Send event
        if let Some(sender) = &self.event_sender {
            let _ = sender.send(DebugEvent::BreakpointHit {
                session_id: session_id.to_string(),
                breakpoint_id: breakpoint_id.to_string(),
                file_path: file_path.to_string(),
                line_number,
                thread_id: None,
            });
        }
        
        info!("Breakpoint {} hit at {}:{} for session {}", breakpoint_id, file_path, line_number, session_id);
        Ok(())
    }
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            language: DebugLanguage::Python,
            working_directory: PathBuf::from("/workspace"),
            timeout_secs: 300, // 5 minutes
        }
    }
}

impl Default for DebugStats {
    fn default() -> Self {
        Self {
            total_sessions: 0,
            active_sessions: 0,
            breakpoints_hit: 0,
            steps_executed: 0,
            variables_inspected: 0,
            avg_session_duration_secs: 0.0,
        }
    }
}

impl DebugLanguage {
    /// Get the debugger command for this language
    pub fn debugger_command(&self) -> &'static str {
        match self {
            DebugLanguage::Python => "python",
            DebugLanguage::JavaScript => "node",
            DebugLanguage::TypeScript => "node",
            DebugLanguage::Rust => "rust-gdb",
            DebugLanguage::Go => "dlv",
            DebugLanguage::Java => "jdb",
            DebugLanguage::CSharp => "dotnet",
            DebugLanguage::Cpp => "gdb",
            DebugLanguage::Ruby => "ruby",
            DebugLanguage::PHP => "php",
        }
    }
    
    /// Get typical file extensions for this language
    pub fn file_extensions(&self) -> Vec<&'static str> {
        match self {
            DebugLanguage::Python => vec!["py", "pyw"],
            DebugLanguage::JavaScript => vec!["js", "mjs"],
            DebugLanguage::TypeScript => vec!["ts", "tsx"],
            DebugLanguage::Rust => vec!["rs"],
            DebugLanguage::Go => vec!["go"],
            DebugLanguage::Java => vec!["java"],
            DebugLanguage::CSharp => vec!["cs"],
            DebugLanguage::Cpp => vec!["cpp", "cxx", "cc", "c++", "c"],
            DebugLanguage::Ruby => vec!["rb"],
            DebugLanguage::PHP => vec!["php"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_debug_manager_creation() {
        let manager = DebugManager::new();
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_sessions, 0);
        assert_eq!(stats.active_sessions, 0);
    }
    
    #[tokio::test]
    async fn test_debug_session_start_stop() {
        let mut manager = DebugManager::new();
        let config = DebugConfig::default();
        
        // Start session
        let session_id = manager.start_debug_session("test-sandbox", config).await.unwrap();
        assert!(!session_id.is_empty());
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_sessions, 1);
        assert_eq!(stats.active_sessions, 1);
        
        // Stop session
        manager.stop_debug_session(&session_id).await.unwrap();
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.active_sessions, 0);
    }
    
    #[test]
    fn test_debug_language_properties() {
        assert_eq!(DebugLanguage::Python.debugger_command(), "python");
        assert!(DebugLanguage::Python.file_extensions().contains(&"py"));
        
        assert_eq!(DebugLanguage::JavaScript.debugger_command(), "node");
        assert!(DebugLanguage::JavaScript.file_extensions().contains(&"js"));
    }
    
    #[test]
    fn test_debug_config_default() {
        let config = DebugConfig::default();
        assert_eq!(config.language, DebugLanguage::Python);
        assert_eq!(config.working_directory, PathBuf::from("/workspace"));
        assert_eq!(config.timeout_secs, 300);
        assert!(config.auto_inspect_variables);
    }
    
    #[tokio::test]
    async fn test_duplicate_session_prevention() {
        let mut manager = DebugManager::new();
        let config = DebugConfig::default();
        
        // Start first session
        let _session_id1 = manager.start_debug_session("test-sandbox", config.clone()).await.unwrap();
        
        // Try to start second session for same sandbox
        let result = manager.start_debug_session("test-sandbox", config).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DebugError::SessionAlreadyActive(_)));
    }
}