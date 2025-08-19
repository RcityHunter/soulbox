//! Breakpoint management system for debugging
//! 
//! This module provides comprehensive breakpoint functionality including:
//! - Line breakpoints with optional conditions
//! - Function breakpoints
//! - Exception breakpoints
//! - Conditional and hit count breakpoints
//! - Breakpoint persistence and management

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, BTreeMap};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, warn, error, info};

/// Breakpoint management errors
#[derive(Error, Debug)]
pub enum BreakpointError {
    #[error("Breakpoint not found: {0}")]
    BreakpointNotFound(String),
    
    #[error("Invalid breakpoint location: {0}")]
    InvalidLocation(String),
    
    #[error("Invalid condition expression: {0}")]
    InvalidCondition(String),
    
    #[error("Breakpoint already exists at {0}:{1}")]
    BreakpointExists(String, u32),
    
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    
    #[error("File not found: {0}")]
    FileNotFound(String),
    
    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),
}

/// Types of breakpoints
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BreakpointType {
    /// Line breakpoint (most common)
    Line,
    /// Function entry breakpoint
    Function,
    /// Exception breakpoint
    Exception,
    /// Conditional breakpoint
    Conditional,
    /// Logpoint (logs message without stopping)
    Logpoint,
    /// Watchpoint (breaks when variable changes)
    Watchpoint,
}

/// Breakpoint conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BreakpointCondition {
    /// Always break (no condition)
    Always,
    /// Break when expression evaluates to true
    Expression(String),
    /// Break after N hits
    HitCount(u32),
    /// Break when hit count equals N
    HitCountEquals(u32),
    /// Break when hit count is multiple of N
    HitCountMultiple(u32),
    /// Combined conditions (all must be true)
    And(Vec<BreakpointCondition>),
    /// Alternative conditions (any can be true)
    Or(Vec<BreakpointCondition>),
}

/// Breakpoint state
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BreakpointState {
    /// Breakpoint is active and will trigger
    Enabled,
    /// Breakpoint is disabled and will not trigger
    Disabled,
    /// Breakpoint is pending verification
    Pending,
    /// Breakpoint verification failed
    Invalid,
}

/// A debugging breakpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    /// Unique breakpoint ID
    pub id: String,
    /// Debug session ID this breakpoint belongs to
    pub session_id: String,
    /// Breakpoint type
    pub breakpoint_type: BreakpointType,
    /// Source file path
    pub file_path: String,
    /// Line number (for line breakpoints)
    pub line_number: Option<u32>,
    /// Function name (for function breakpoints)
    pub function_name: Option<String>,
    /// Exception type (for exception breakpoints)
    pub exception_type: Option<String>,
    /// Variable name (for watchpoints)
    pub variable_name: Option<String>,
    /// Breakpoint condition
    pub condition: BreakpointCondition,
    /// Current state
    pub state: BreakpointState,
    /// Hit count
    pub hit_count: u32,
    /// Message to log (for logpoints)
    pub log_message: Option<String>,
    /// When breakpoint was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When breakpoint was last modified
    pub modified_at: chrono::DateTime<chrono::Utc>,
    /// When breakpoint was last hit
    pub last_hit_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Breakpoint statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakpointStats {
    /// Total breakpoints created
    pub total_created: u64,
    /// Currently active breakpoints
    pub active_breakpoints: u32,
    /// Total breakpoint hits
    pub total_hits: u64,
    /// Breakpoints by type
    pub by_type: HashMap<String, u32>,
    /// Average hits per breakpoint
    pub avg_hits_per_breakpoint: f64,
}

/// Breakpoint manager
pub struct BreakpointManager {
    /// All breakpoints indexed by ID
    breakpoints: Arc<RwLock<HashMap<String, Breakpoint>>>,
    /// Breakpoints by session ID
    session_breakpoints: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Breakpoints by file and line
    file_breakpoints: Arc<RwLock<HashMap<String, BTreeMap<u32, String>>>>,
    /// Breakpoint statistics
    stats: Arc<RwLock<BreakpointStats>>,
}

impl BreakpointManager {
    /// Create a new breakpoint manager
    pub fn new() -> Self {
        Self {
            breakpoints: Arc::new(RwLock::new(HashMap::new())),
            session_breakpoints: Arc::new(RwLock::new(HashMap::new())),
            file_breakpoints: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(BreakpointStats::default())),
        }
    }
    
    /// Set a breakpoint
    pub async fn set_breakpoint(
        &self,
        session_id: &str,
        file_path: &str,
        line_number: u32,
        breakpoint_type: BreakpointType,
        condition: Option<BreakpointCondition>,
    ) -> Result<String, BreakpointError> {
        // Check if breakpoint already exists at this location
        {
            let file_breakpoints = self.file_breakpoints.read().await;
            if let Some(line_breakpoints) = file_breakpoints.get(file_path) {
                if line_breakpoints.contains_key(&line_number) {
                    return Err(BreakpointError::BreakpointExists(
                        file_path.to_string(),
                        line_number,
                    ));
                }
            }
        }
        
        let breakpoint_id = format!("bp-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now();
        
        let breakpoint = Breakpoint {
            id: breakpoint_id.clone(),
            session_id: session_id.to_string(),
            breakpoint_type,
            file_path: file_path.to_string(),
            line_number: Some(line_number),
            function_name: None,
            exception_type: None,
            variable_name: None,
            condition: condition.unwrap_or(BreakpointCondition::Always),
            state: BreakpointState::Enabled,
            hit_count: 0,
            log_message: None,
            created_at: now,
            modified_at: now,
            last_hit_at: None,
        };
        
        // Store breakpoint
        {
            let mut breakpoints = self.breakpoints.write().await;
            breakpoints.insert(breakpoint_id.clone(), breakpoint.clone());
        }
        
        // Index by session
        {
            let mut session_breakpoints = self.session_breakpoints.write().await;
            session_breakpoints
                .entry(session_id.to_string())
                .or_insert_with(Vec::new)
                .push(breakpoint_id.clone());
        }
        
        // Index by file and line
        {
            let mut file_breakpoints = self.file_breakpoints.write().await;
            file_breakpoints
                .entry(file_path.to_string())
                .or_insert_with(BTreeMap::new)
                .insert(line_number, breakpoint_id.clone());
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_created += 1;
            stats.active_breakpoints += 1;
            let type_name = format!("{:?}", breakpoint_type);
            *stats.by_type.entry(type_name).or_insert(0) += 1;
        }
        
        info!("Set breakpoint {} at {}:{} for session {}", breakpoint_id, file_path, line_number, session_id);
        Ok(breakpoint_id)
    }
    
    /// Set a function breakpoint
    pub async fn set_function_breakpoint(
        &self,
        session_id: &str,
        function_name: &str,
        condition: Option<BreakpointCondition>,
    ) -> Result<String, BreakpointError> {
        let breakpoint_id = format!("bp-func-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now();
        
        let breakpoint = Breakpoint {
            id: breakpoint_id.clone(),
            session_id: session_id.to_string(),
            breakpoint_type: BreakpointType::Function,
            file_path: String::new(), // Not applicable for function breakpoints
            line_number: None,
            function_name: Some(function_name.to_string()),
            exception_type: None,
            variable_name: None,
            condition: condition.unwrap_or(BreakpointCondition::Always),
            state: BreakpointState::Enabled,
            hit_count: 0,
            log_message: None,
            created_at: now,
            modified_at: now,
            last_hit_at: None,
        };
        
        // Store and index breakpoint
        self.store_and_index_breakpoint(breakpoint).await?;
        
        info!("Set function breakpoint {} for '{}' in session {}", breakpoint_id, function_name, session_id);
        Ok(breakpoint_id)
    }
    
    /// Set an exception breakpoint
    pub async fn set_exception_breakpoint(
        &self,
        session_id: &str,
        exception_type: &str,
        condition: Option<BreakpointCondition>,
    ) -> Result<String, BreakpointError> {
        let breakpoint_id = format!("bp-ex-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now();
        
        let breakpoint = Breakpoint {
            id: breakpoint_id.clone(),
            session_id: session_id.to_string(),
            breakpoint_type: BreakpointType::Exception,
            file_path: String::new(),
            line_number: None,
            function_name: None,
            exception_type: Some(exception_type.to_string()),
            variable_name: None,
            condition: condition.unwrap_or(BreakpointCondition::Always),
            state: BreakpointState::Enabled,
            hit_count: 0,
            log_message: None,
            created_at: now,
            modified_at: now,
            last_hit_at: None,
        };
        
        self.store_and_index_breakpoint(breakpoint).await?;
        
        info!("Set exception breakpoint {} for '{}' in session {}", breakpoint_id, exception_type, session_id);
        Ok(breakpoint_id)
    }
    
    /// Set a logpoint (logs message without stopping)
    pub async fn set_logpoint(
        &self,
        session_id: &str,
        file_path: &str,
        line_number: u32,
        log_message: &str,
    ) -> Result<String, BreakpointError> {
        let breakpoint_id = format!("bp-log-{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now();
        
        let breakpoint = Breakpoint {
            id: breakpoint_id.clone(),
            session_id: session_id.to_string(),
            breakpoint_type: BreakpointType::Logpoint,
            file_path: file_path.to_string(),
            line_number: Some(line_number),
            function_name: None,
            exception_type: None,
            variable_name: None,
            condition: BreakpointCondition::Always,
            state: BreakpointState::Enabled,
            hit_count: 0,
            log_message: Some(log_message.to_string()),
            created_at: now,
            modified_at: now,
            last_hit_at: None,
        };
        
        self.store_and_index_breakpoint(breakpoint).await?;
        
        info!("Set logpoint {} at {}:{} for session {}", breakpoint_id, file_path, line_number, session_id);
        Ok(breakpoint_id)
    }
    
    /// Store and index a breakpoint
    async fn store_and_index_breakpoint(&self, breakpoint: Breakpoint) -> Result<(), BreakpointError> {
        let breakpoint_id = breakpoint.id.clone();
        let session_id = breakpoint.session_id.clone();
        let breakpoint_type = breakpoint.breakpoint_type;
        
        // Store breakpoint
        {
            let mut breakpoints = self.breakpoints.write().await;
            breakpoints.insert(breakpoint_id.clone(), breakpoint);
        }
        
        // Index by session
        {
            let mut session_breakpoints = self.session_breakpoints.write().await;
            session_breakpoints
                .entry(session_id)
                .or_insert_with(Vec::new)
                .push(breakpoint_id);
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_created += 1;
            stats.active_breakpoints += 1;
            let type_name = format!("{:?}", breakpoint_type);
            *stats.by_type.entry(type_name).or_insert(0) += 1;
        }
        
        Ok(())
    }
    
    /// Remove a breakpoint
    pub async fn remove_breakpoint(
        &self,
        session_id: &str,
        breakpoint_id: &str,
    ) -> Result<(), BreakpointError> {
        let breakpoint = {
            let mut breakpoints = self.breakpoints.write().await;
            breakpoints.remove(breakpoint_id)
        };
        
        if let Some(breakpoint) = breakpoint {
            // Verify session ownership
            if breakpoint.session_id != session_id {
                return Err(BreakpointError::SessionNotFound(session_id.to_string()));
            }
            
            // Remove from session index
            {
                let mut session_breakpoints = self.session_breakpoints.write().await;
                if let Some(session_bp_list) = session_breakpoints.get_mut(session_id) {
                    session_bp_list.retain(|id| id != breakpoint_id);
                    if session_bp_list.is_empty() {
                        session_breakpoints.remove(session_id);
                    }
                }
            }
            
            // Remove from file index (if applicable)
            if let (Some(line_number), file_path) = (breakpoint.line_number, &breakpoint.file_path) {
                if !file_path.is_empty() {
                    let mut file_breakpoints = self.file_breakpoints.write().await;
                    if let Some(line_breakpoints) = file_breakpoints.get_mut(file_path) {
                        line_breakpoints.remove(&line_number);
                        if line_breakpoints.is_empty() {
                            file_breakpoints.remove(file_path);
                        }
                    }
                }
            }
            
            // Update statistics
            {
                let mut stats = self.stats.write().await;
                stats.active_breakpoints = stats.active_breakpoints.saturating_sub(1);
                let type_name = format!("{:?}", breakpoint.breakpoint_type);
                if let Some(count) = stats.by_type.get_mut(&type_name) {
                    *count = count.saturating_sub(1);
                }
            }
            
            info!("Removed breakpoint {} from session {}", breakpoint_id, session_id);
            Ok(())
        } else {
            Err(BreakpointError::BreakpointNotFound(breakpoint_id.to_string()))
        }
    }
    
    /// Enable/disable a breakpoint
    pub async fn set_breakpoint_enabled(
        &self,
        session_id: &str,
        breakpoint_id: &str,
        enabled: bool,
    ) -> Result<(), BreakpointError> {
        let mut breakpoints = self.breakpoints.write().await;
        
        if let Some(breakpoint) = breakpoints.get_mut(breakpoint_id) {
            if breakpoint.session_id != session_id {
                return Err(BreakpointError::SessionNotFound(session_id.to_string()));
            }
            
            breakpoint.state = if enabled {
                BreakpointState::Enabled
            } else {
                BreakpointState::Disabled
            };
            breakpoint.modified_at = chrono::Utc::now();
            
            debug!("Breakpoint {} {} for session {}", 
                   breakpoint_id, 
                   if enabled { "enabled" } else { "disabled" }, 
                   session_id);
            Ok(())
        } else {
            Err(BreakpointError::BreakpointNotFound(breakpoint_id.to_string()))
        }
    }
    
    /// Update breakpoint condition
    pub async fn update_breakpoint_condition(
        &self,
        session_id: &str,
        breakpoint_id: &str,
        condition: BreakpointCondition,
    ) -> Result<(), BreakpointError> {
        let mut breakpoints = self.breakpoints.write().await;
        
        if let Some(breakpoint) = breakpoints.get_mut(breakpoint_id) {
            if breakpoint.session_id != session_id {
                return Err(BreakpointError::SessionNotFound(session_id.to_string()));
            }
            
            breakpoint.condition = condition;
            breakpoint.modified_at = chrono::Utc::now();
            
            debug!("Updated condition for breakpoint {} in session {}", breakpoint_id, session_id);
            Ok(())
        } else {
            Err(BreakpointError::BreakpointNotFound(breakpoint_id.to_string()))
        }
    }
    
    /// Record a breakpoint hit
    pub async fn record_breakpoint_hit(&self, breakpoint_id: &str) -> Result<(), BreakpointError> {
        let mut breakpoints = self.breakpoints.write().await;
        
        if let Some(breakpoint) = breakpoints.get_mut(breakpoint_id) {
            breakpoint.hit_count += 1;
            breakpoint.last_hit_at = Some(chrono::Utc::now());
            
            // Update global statistics
            {
                let mut stats = self.stats.write().await;
                stats.total_hits += 1;
                
                // Recalculate average hits per breakpoint
                let total_breakpoints = stats.active_breakpoints.max(1) as f64;
                stats.avg_hits_per_breakpoint = stats.total_hits as f64 / total_breakpoints;
            }
            
            debug!("Recorded hit for breakpoint {} (total hits: {})", breakpoint_id, breakpoint.hit_count);
            Ok(())
        } else {
            Err(BreakpointError::BreakpointNotFound(breakpoint_id.to_string()))
        }
    }
    
    /// Check if a breakpoint condition is met
    pub async fn should_break(&self, breakpoint_id: &str, context: &BreakpointContext) -> Result<bool, BreakpointError> {
        let breakpoints = self.breakpoints.read().await;
        
        if let Some(breakpoint) = breakpoints.get(breakpoint_id) {
            if breakpoint.state != BreakpointState::Enabled {
                return Ok(false);
            }
            
            Ok(self.evaluate_condition(&breakpoint.condition, context).await)
        } else {
            Err(BreakpointError::BreakpointNotFound(breakpoint_id.to_string()))
        }
    }
    
    /// Evaluate a breakpoint condition with proper expression evaluation
    async fn evaluate_condition(&self, condition: &BreakpointCondition, context: &BreakpointContext) -> bool {
        match condition {
            BreakpointCondition::Always => true,
            BreakpointCondition::Expression(expr) => {
                // Implement actual expression evaluation
                match self.evaluate_expression(expr, context).await {
                    Ok(result) => result,
                    Err(e) => {
                        warn!("Failed to evaluate breakpoint expression '{}': {}", expr, e);
                        false
                    }
                }
            },
            BreakpointCondition::HitCount(target) => context.hit_count >= *target,
            BreakpointCondition::HitCountEquals(target) => context.hit_count == *target,
            BreakpointCondition::HitCountMultiple(divisor) => {
                if *divisor == 0 {
                    false // Avoid division by zero
                } else {
                    context.hit_count % divisor == 0
                }
            },
            BreakpointCondition::And(conditions) => {
                for cond in conditions {
                    if !Box::pin(self.evaluate_condition(cond, context)).await {
                        return false;
                    }
                }
                true
            },
            BreakpointCondition::Or(conditions) => {
                for cond in conditions {
                    if Box::pin(self.evaluate_condition(cond, context)).await {
                        return true;
                    }
                }
                false
            },
        }
    }
    
    /// Evaluate a boolean expression using variable context
    fn evaluate_expression<'a>(&'a self, expr: &'a str, context: &'a BreakpointContext) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<bool, String>> + Send + 'a>> {
        Box::pin(async move {
            self.evaluate_expression_recursive(expr, context, 0).await
        })
    }

    // Internal recursive method with depth limiting to prevent infinite recursion
    async fn evaluate_expression_recursive(&self, expr: &str, context: &BreakpointContext, depth: u32) -> Result<bool, String> {
        // Prevent infinite recursion by limiting depth
        const MAX_RECURSION_DEPTH: u32 = 10;
        if depth > MAX_RECURSION_DEPTH {
            return Err("Expression evaluation depth exceeded".to_string());
        }

        let expr = expr.trim();
        
        // Handle empty expression
        if expr.is_empty() {
            return Ok(false);
        }
        
        // Simple expression evaluator - supports basic comparisons and variable substitution
        // This is a simplified implementation. In production, you might want to use a proper expression parser
        
        // Handle simple boolean literals
        match expr.to_lowercase().as_str() {
            "true" => return Ok(true),
            "false" => return Ok(false),
            _ => {} // Continue to other evaluation methods
        }
        
        // Handle variable existence checks
        if let Some(var_name) = expr.strip_prefix("exists(").and_then(|s| s.strip_suffix(")")) {
            return Ok(context.variables.contains_key(var_name.trim()));
        }
        
        // Handle simple comparisons
        if let Some(result) = self.evaluate_comparison(expr, context) {
            return result;
        }
        
        // Handle logical operators with depth control
        if let Some(result) = Box::pin(self.evaluate_logical_expression_with_depth(expr, context, depth + 1)).await {
            return result;
        }
        
        // If we can't parse the expression, be conservative and return false
        Err(format!("Unable to evaluate expression: '{}'", expr))
    }
    
    /// Evaluate comparison expressions (==, !=, <, >, <=, >=)
    fn evaluate_comparison(&self, expr: &str, context: &BreakpointContext) -> Option<Result<bool, String>> {
        let operators = ["==", "!=", "<=", ">=", "<", ">"];
        
        for op in &operators {
            if let Some(idx) = expr.find(op) {
                let left = expr[..idx].trim();
                let right = expr[idx + op.len()..].trim();
                
                // Get values for left and right sides
                let left_val = self.get_expression_value(left, context)?;
                let right_val = self.get_expression_value(right, context)?;
                
                let result = match *op {
                    "==" => left_val == right_val,
                    "!=" => left_val != right_val,
                    "<" => self.compare_values(&left_val, &right_val) == Some(std::cmp::Ordering::Less),
                    ">" => self.compare_values(&left_val, &right_val) == Some(std::cmp::Ordering::Greater),
                    "<=" => {
                        let cmp = self.compare_values(&left_val, &right_val);
                        cmp == Some(std::cmp::Ordering::Less) || cmp == Some(std::cmp::Ordering::Equal)
                    },
                    ">=" => {
                        let cmp = self.compare_values(&left_val, &right_val);
                        cmp == Some(std::cmp::Ordering::Greater) || cmp == Some(std::cmp::Ordering::Equal)
                    },
                    _ => false,
                };
                
                return Some(Ok(result));
            }
        }
        
        None
    }
    
    /// Evaluate logical expressions (&&, ||)
    async fn evaluate_logical_expression_with_depth(&self, expr: &str, context: &BreakpointContext, depth: u32) -> Option<Result<bool, String>> {
        // Handle AND operator
        if let Some(idx) = expr.find("&&") {
            let left = expr[..idx].trim();
            let right = expr[idx + 2..].trim();
            
            let left_result = Box::pin(self.evaluate_expression_recursive(left, context, depth)).await;
            let right_result = Box::pin(self.evaluate_expression_recursive(right, context, depth)).await;
            
            return Some(match (left_result, right_result) {
                (Ok(left_val), Ok(right_val)) => Ok(left_val && right_val),
                (Err(e), _) | (_, Err(e)) => Err(e),
            });
        }
        
        // Handle OR operator
        if let Some(idx) = expr.find("||") {
            let left = expr[..idx].trim();
            let right = expr[idx + 2..].trim();
            
            let left_result = Box::pin(self.evaluate_expression_recursive(left, context, depth)).await;
            let right_result = Box::pin(self.evaluate_expression_recursive(right, context, depth)).await;
            
            return Some(match (left_result, right_result) {
                (Ok(left_val), Ok(right_val)) => Ok(left_val || right_val),
                (Err(e), _) | (_, Err(e)) => Err(e),
            });
        }
        
        None
    }
    
    /// Get the value of an expression component (variable or literal)
    fn get_expression_value(&self, expr: &str, context: &BreakpointContext) -> Option<String> {
        let expr = expr.trim();
        
        // Check if it's a quoted string
        if (expr.starts_with('"') && expr.ends_with('"')) || 
           (expr.starts_with('\'') && expr.ends_with('\'')) {
            return Some(expr[1..expr.len()-1].to_string());
        }
        
        // Check if it's a number
        if expr.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Some(expr.to_string());
        }
        
        // Check if it's a variable
        if let Some(value) = context.variables.get(expr) {
            return Some(value.clone());
        }
        
        // Unknown variable or literal
        None
    }
    
    /// Compare two string values, attempting numeric comparison if both are numbers
    fn compare_values(&self, left: &str, right: &str) -> Option<std::cmp::Ordering> {
        // Try numeric comparison first
        if let (Ok(left_num), Ok(right_num)) = (left.parse::<f64>(), right.parse::<f64>()) {
            return left_num.partial_cmp(&right_num);
        }
        
        // Fall back to string comparison
        Some(left.cmp(right))
    }
    
    /// Get all breakpoints for a session
    pub async fn get_session_breakpoints(&self, session_id: &str) -> Vec<Breakpoint> {
        let session_breakpoints = self.session_breakpoints.read().await;
        let breakpoints = self.breakpoints.read().await;
        
        if let Some(breakpoint_ids) = session_breakpoints.get(session_id) {
            breakpoint_ids
                .iter()
                .filter_map(|id| breakpoints.get(id).cloned())
                .collect()
        } else {
            Vec::new()
        }
    }
    
    /// Get breakpoints for a specific file
    pub async fn get_file_breakpoints(&self, file_path: &str) -> Vec<Breakpoint> {
        let file_breakpoints = self.file_breakpoints.read().await;
        let breakpoints = self.breakpoints.read().await;
        
        if let Some(line_breakpoints) = file_breakpoints.get(file_path) {
            line_breakpoints
                .values()
                .filter_map(|id| breakpoints.get(id).cloned())
                .collect()
        } else {
            Vec::new()
        }
    }
    
    /// Get a specific breakpoint
    pub async fn get_breakpoint(&self, breakpoint_id: &str) -> Option<Breakpoint> {
        let breakpoints = self.breakpoints.read().await;
        breakpoints.get(breakpoint_id).cloned()
    }
    
    /// Clear all breakpoints for a session
    pub async fn clear_session_breakpoints(&self, session_id: &str) {
        let breakpoint_ids = {
            let mut session_breakpoints = self.session_breakpoints.write().await;
            session_breakpoints.remove(session_id).unwrap_or_default()
        };
        
        let mut removed_count = 0;
        {
            let mut breakpoints = self.breakpoints.write().await;
            for breakpoint_id in &breakpoint_ids {
                if breakpoints.remove(breakpoint_id).is_some() {
                    removed_count += 1;
                }
            }
        }
        
        // Clean up file index
        {
            let mut file_breakpoints = self.file_breakpoints.write().await;
            file_breakpoints.retain(|_, line_breakpoints| {
                line_breakpoints.retain(|_, bp_id| !breakpoint_ids.contains(bp_id));
                !line_breakpoints.is_empty()
            });
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.active_breakpoints = stats.active_breakpoints.saturating_sub(removed_count);
        }
        
        if removed_count > 0 {
            info!("Cleared {} breakpoints for session {}", removed_count, session_id);
        }
    }
    
    /// Get breakpoint statistics
    pub async fn get_stats(&self) -> BreakpointStats {
        self.stats.read().await.clone()
    }
    
    /// List all breakpoints
    pub async fn list_all_breakpoints(&self) -> Vec<Breakpoint> {
        let breakpoints = self.breakpoints.read().await;
        breakpoints.values().cloned().collect()
    }
}

/// Context information for breakpoint evaluation
#[derive(Debug, Clone)]
pub struct BreakpointContext {
    /// Current hit count for this breakpoint
    pub hit_count: u32,
    /// Current variable values (for expression evaluation)
    pub variables: HashMap<String, String>,
    /// Current stack frame information
    pub stack_frame: Option<String>,
    /// Thread ID (if applicable)
    pub thread_id: Option<String>,
}

impl Default for BreakpointStats {
    fn default() -> Self {
        Self {
            total_created: 0,
            active_breakpoints: 0,
            total_hits: 0,
            by_type: HashMap::new(),
            avg_hits_per_breakpoint: 0.0,
        }
    }
}

impl BreakpointCondition {
    /// Create a simple expression condition
    pub fn expression(expr: &str) -> Self {
        Self::Expression(expr.to_string())
    }
    
    /// Create a hit count condition
    pub fn hit_count(count: u32) -> Self {
        Self::HitCount(count)
    }
    
    /// Create an AND condition
    pub fn and(conditions: Vec<BreakpointCondition>) -> Self {
        Self::And(conditions)
    }
    
    /// Create an OR condition
    pub fn or(conditions: Vec<BreakpointCondition>) -> Self {
        Self::Or(conditions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_breakpoint_manager_creation() {
        let manager = BreakpointManager::new();
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_created, 0);
        assert_eq!(stats.active_breakpoints, 0);
    }
    
    #[tokio::test]
    async fn test_set_line_breakpoint() {
        let manager = BreakpointManager::new();
        
        let breakpoint_id = manager.set_breakpoint(
            "session1",
            "main.py",
            10,
            BreakpointType::Line,
            None,
        ).await.unwrap();
        
        assert!(!breakpoint_id.is_empty());
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_created, 1);
        assert_eq!(stats.active_breakpoints, 1);
    }
    
    #[tokio::test]
    async fn test_duplicate_breakpoint_prevention() {
        let manager = BreakpointManager::new();
        
        // Set first breakpoint
        let _bp1 = manager.set_breakpoint(
            "session1",
            "main.py",
            10,
            BreakpointType::Line,
            None,
        ).await.unwrap();
        
        // Try to set duplicate breakpoint
        let result = manager.set_breakpoint(
            "session1",
            "main.py",
            10,
            BreakpointType::Line,
            None,
        ).await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BreakpointError::BreakpointExists(_, _)));
    }
    
    #[tokio::test]
    async fn test_function_breakpoint() {
        let manager = BreakpointManager::new();
        
        let breakpoint_id = manager.set_function_breakpoint(
            "session1",
            "my_function",
            Some(BreakpointCondition::Always),
        ).await.unwrap();
        
        let breakpoint = manager.get_breakpoint(&breakpoint_id).await.unwrap();
        assert_eq!(breakpoint.breakpoint_type, BreakpointType::Function);
        assert_eq!(breakpoint.function_name, Some("my_function".to_string()));
    }
    
    #[tokio::test]
    async fn test_breakpoint_hit_recording() {
        let manager = BreakpointManager::new();
        
        let breakpoint_id = manager.set_breakpoint(
            "session1",
            "main.py",
            10,
            BreakpointType::Line,
            None,
        ).await.unwrap();
        
        // Record hits
        manager.record_breakpoint_hit(&breakpoint_id).await.unwrap();
        manager.record_breakpoint_hit(&breakpoint_id).await.unwrap();
        
        let breakpoint = manager.get_breakpoint(&breakpoint_id).await.unwrap();
        assert_eq!(breakpoint.hit_count, 2);
        assert!(breakpoint.last_hit_at.is_some());
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_hits, 2);
    }
    
    #[tokio::test]
    async fn test_breakpoint_enable_disable() {
        let manager = BreakpointManager::new();
        
        let breakpoint_id = manager.set_breakpoint(
            "session1",
            "main.py",
            10,
            BreakpointType::Line,
            None,
        ).await.unwrap();
        
        // Disable breakpoint
        manager.set_breakpoint_enabled("session1", &breakpoint_id, false).await.unwrap();
        
        let breakpoint = manager.get_breakpoint(&breakpoint_id).await.unwrap();
        assert_eq!(breakpoint.state, BreakpointState::Disabled);
        
        // Re-enable breakpoint
        manager.set_breakpoint_enabled("session1", &breakpoint_id, true).await.unwrap();
        
        let breakpoint = manager.get_breakpoint(&breakpoint_id).await.unwrap();
        assert_eq!(breakpoint.state, BreakpointState::Enabled);
    }
    
    #[tokio::test]
    async fn test_session_breakpoint_cleanup() {
        let manager = BreakpointManager::new();
        
        // Set multiple breakpoints
        let _bp1 = manager.set_breakpoint("session1", "main.py", 10, BreakpointType::Line, None).await.unwrap();
        let _bp2 = manager.set_breakpoint("session1", "main.py", 20, BreakpointType::Line, None).await.unwrap();
        let _bp3 = manager.set_function_breakpoint("session1", "func1", None).await.unwrap();
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.active_breakpoints, 3);
        
        // Clear all breakpoints for session
        manager.clear_session_breakpoints("session1").await;
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.active_breakpoints, 0);
        
        let session_breakpoints = manager.get_session_breakpoints("session1").await;
        assert_eq!(session_breakpoints.len(), 0);
    }
}