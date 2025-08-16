//! Variable inspection and evaluation system for debugging
//! 
//! This module provides comprehensive variable inspection capabilities including:
//! - Variable value inspection across different scopes
//! - Expression evaluation in debug context
//! - Variable modification during debugging
//! - Type information and structure analysis
//! - Watch expressions and variable tracking

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, warn, error, info};

/// Variable inspection errors
#[derive(Error, Debug)]
pub enum InspectorError {
    #[error("Variable not found: {0}")]
    VariableNotFound(String),
    
    #[error("Invalid expression: {0}")]
    InvalidExpression(String),
    
    #[error("Evaluation failed: {0}")]
    EvaluationFailed(String),
    
    #[error("Type conversion error: {0}")]
    TypeConversion(String),
    
    #[error("Scope not accessible: {0}")]
    ScopeNotAccessible(String),
    
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    
    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),
    
    #[error("Runtime error: {0}")]
    Runtime(String),
}

/// Variable types supported by the inspector
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VariableType {
    /// Primitive types
    Integer,
    Float,
    Boolean,
    String,
    Character,
    Null,
    
    /// Collection types
    Array,
    List,
    Tuple,
    Set,
    Map,
    Dictionary,
    
    /// Object types
    Object,
    Class,
    Instance,
    Struct,
    
    /// Function types
    Function,
    Method,
    Closure,
    Lambda,
    
    /// Special types
    Pointer,
    Reference,
    Union,
    Enum,
    Generic,
    Unknown,
}

/// Variable scope levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum VariableScope {
    /// Local variables in current function
    Local,
    /// Function parameters
    Parameter,
    /// Global variables
    Global,
    /// Class/instance variables
    Instance,
    /// Static variables
    Static,
    /// Closure captures
    Closure,
    /// Built-in variables
    Builtin,
}

/// A variable with its metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    /// Variable name
    pub name: String,
    /// Variable value (string representation)
    pub value: String,
    /// Variable type
    pub var_type: VariableType,
    /// Variable scope
    pub scope: VariableScope,
    /// Memory address (if applicable)
    pub memory_address: Option<String>,
    /// Size in bytes (if applicable)
    pub size_bytes: Option<u64>,
    /// Whether the variable can be modified
    pub mutable: bool,
    /// Child variables (for objects, arrays, etc.)
    pub children: Vec<Variable>,
    /// Type information (language-specific)
    pub type_info: Option<String>,
    /// Variable description or documentation
    pub description: Option<String>,
    /// Last modification time
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
}

/// Watch expression for monitoring variable changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchExpression {
    /// Unique watch ID
    pub id: String,
    /// Expression to evaluate
    pub expression: String,
    /// Current value
    pub current_value: Option<String>,
    /// Previous value (for change detection)
    pub previous_value: Option<String>,
    /// Whether the value has changed since last evaluation
    pub has_changed: bool,
    /// Evaluation error (if any)
    pub error: Option<String>,
    /// When the watch was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last evaluation time
    pub last_evaluated: Option<chrono::DateTime<chrono::Utc>>,
}

/// Variable modification record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariableModification {
    /// Session ID
    pub session_id: String,
    /// Variable name
    pub variable_name: String,
    /// Old value
    pub old_value: String,
    /// New value
    pub new_value: String,
    /// Modification timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Stack frame where modification occurred
    pub stack_frame: Option<String>,
}

/// Inspector statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectorStats {
    /// Total variables inspected
    pub total_inspected: u64,
    /// Total expressions evaluated
    pub expressions_evaluated: u64,
    /// Total variable modifications
    pub modifications_made: u64,
    /// Active watch expressions
    pub active_watches: u32,
    /// Average evaluation time in milliseconds
    pub avg_evaluation_time_ms: f64,
    /// Variables by type
    pub by_type: HashMap<String, u32>,
    /// Variables by scope
    pub by_scope: HashMap<String, u32>,
}

/// Variable inspector engine
pub struct VariableInspector {
    /// Variables by session and scope
    session_variables: Arc<RwLock<HashMap<String, HashMap<VariableScope, Vec<Variable>>>>>,
    /// Watch expressions by session
    watch_expressions: Arc<RwLock<HashMap<String, Vec<WatchExpression>>>>,
    /// Variable modification history
    modifications: Arc<RwLock<Vec<VariableModification>>>,
    /// Inspector statistics
    stats: Arc<RwLock<InspectorStats>>,
}

impl VariableInspector {
    /// Create a new variable inspector
    pub fn new() -> Self {
        Self {
            session_variables: Arc::new(RwLock::new(HashMap::new())),
            watch_expressions: Arc::new(RwLock::new(HashMap::new())),
            modifications: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(InspectorStats::default())),
        }
    }
    
    /// Get variables for a specific scope in a debug session
    pub async fn get_variables(
        &self,
        session_id: &str,
        scope: VariableScope,
    ) -> Result<Vec<Variable>, InspectorError> {
        let session_variables = self.session_variables.read().await;
        
        if let Some(session_vars) = session_variables.get(session_id) {
            if let Some(scope_vars) = session_vars.get(&scope) {
                // Update statistics
                {
                    let mut stats = self.stats.write().await;
                    stats.total_inspected += scope_vars.len() as u64;
                    
                    // Update scope statistics
                    let scope_name = format!("{:?}", scope);
                    *stats.by_scope.entry(scope_name).or_insert(0) += scope_vars.len() as u32;
                    
                    // Update type statistics
                    for var in scope_vars {
                        let type_name = format!("{:?}", var.var_type);
                        *stats.by_type.entry(type_name).or_insert(0) += 1;
                    }
                }
                
                Ok(scope_vars.clone())
            } else {
                Ok(Vec::new())
            }
        } else {
            Err(InspectorError::SessionNotFound(session_id.to_string()))
        }
    }
    
    /// Get a specific variable by name and scope
    pub async fn get_variable(
        &self,
        session_id: &str,
        variable_name: &str,
        scope: VariableScope,
    ) -> Result<Variable, InspectorError> {
        let variables = self.get_variables(session_id, scope).await?;
        
        variables
            .into_iter()
            .find(|var| var.name == variable_name)
            .ok_or_else(|| InspectorError::VariableNotFound(variable_name.to_string()))
    }
    
    /// Set variables for a debug session (typically called by debugger backend)
    pub async fn set_session_variables(
        &self,
        session_id: &str,
        scope: VariableScope,
        variables: Vec<Variable>,
    ) -> Result<(), InspectorError> {
        let mut session_variables = self.session_variables.write().await;
        
        session_variables
            .entry(session_id.to_string())
            .or_insert_with(HashMap::new)
            .insert(scope, variables);
        
        debug!("Updated {:?} variables for session {}", scope, session_id);
        Ok(())
    }
    
    /// Evaluate an expression in the debug context
    pub async fn evaluate_expression(
        &self,
        session_id: &str,
        expression: &str,
    ) -> Result<Variable, InspectorError> {
        let start_time = std::time::Instant::now();
        
        // This is a simplified evaluation - in production, this would integrate
        // with language-specific debugger protocols
        let result = self.simple_expression_evaluation(session_id, expression).await?;
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.expressions_evaluated += 1;
            
            let evaluation_time = start_time.elapsed().as_millis() as f64;
            if stats.avg_evaluation_time_ms == 0.0 {
                stats.avg_evaluation_time_ms = evaluation_time;
            } else {
                stats.avg_evaluation_time_ms = 
                    0.7 * stats.avg_evaluation_time_ms + 0.3 * evaluation_time;
            }
        }
        
        debug!("Evaluated expression '{}' for session {}", expression, session_id);
        Ok(result)
    }
    
    /// Simple expression evaluation (placeholder implementation)
    async fn simple_expression_evaluation(
        &self,
        session_id: &str,
        expression: &str,
    ) -> Result<Variable, InspectorError> {
        // Check if it's a simple variable reference
        if expression.chars().all(|c| c.is_alphanumeric() || c == '_') {
            // Try to find the variable in different scopes
            for scope in [VariableScope::Local, VariableScope::Parameter, VariableScope::Global] {
                if let Ok(var) = self.get_variable(session_id, expression, scope).await {
                    return Ok(var);
                }
            }
            return Err(InspectorError::VariableNotFound(expression.to_string()));
        }
        
        // For more complex expressions, return a placeholder result
        // In production, this would use language-specific evaluation
        Ok(Variable {
            name: format!("eval({})", expression),
            value: "expression_result".to_string(),
            var_type: VariableType::Unknown,
            scope: VariableScope::Local,
            memory_address: None,
            size_bytes: None,
            mutable: false,
            children: Vec::new(),
            type_info: Some("Evaluated expression".to_string()),
            description: Some(format!("Result of evaluating: {}", expression)),
            last_modified: Some(chrono::Utc::now()),
        })
    }
    
    /// Modify a variable value
    pub async fn modify_variable(
        &self,
        session_id: &str,
        variable_name: &str,
        scope: VariableScope,
        new_value: &str,
    ) -> Result<(), InspectorError> {
        // Get current variable
        let old_var = self.get_variable(session_id, variable_name, scope).await?;
        
        if !old_var.mutable {
            return Err(InspectorError::Runtime(
                format!("Variable '{}' is not mutable", variable_name)
            ));
        }
        
        // Create modified variable
        let mut new_var = old_var.clone();
        new_var.value = new_value.to_string();
        new_var.last_modified = Some(chrono::Utc::now());
        
        // Update in storage
        {
            let mut session_variables = self.session_variables.write().await;
            if let Some(session_vars) = session_variables.get_mut(session_id) {
                if let Some(scope_vars) = session_vars.get_mut(&scope) {
                    if let Some(var) = scope_vars.iter_mut().find(|v| v.name == variable_name) {
                        *var = new_var;
                    }
                }
            }
        }
        
        // Record modification
        let modification = VariableModification {
            session_id: session_id.to_string(),
            variable_name: variable_name.to_string(),
            old_value: old_var.value,
            new_value: new_value.to_string(),
            timestamp: chrono::Utc::now(),
            stack_frame: None, // Would be populated with actual stack frame info
        };
        
        {
            let mut modifications = self.modifications.write().await;
            modifications.push(modification);
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.modifications_made += 1;
        }
        
        info!("Modified variable '{}' in session {} from '{}' to '{}'", 
              variable_name, session_id, old_var.value, new_value);
        Ok(())
    }
    
    /// Add a watch expression
    pub async fn add_watch(
        &self,
        session_id: &str,
        expression: &str,
    ) -> Result<String, InspectorError> {
        let watch_id = format!("watch-{}", uuid::Uuid::new_v4());
        
        let watch = WatchExpression {
            id: watch_id.clone(),
            expression: expression.to_string(),
            current_value: None,
            previous_value: None,
            has_changed: false,
            error: None,
            created_at: chrono::Utc::now(),
            last_evaluated: None,
        };
        
        {
            let mut watch_expressions = self.watch_expressions.write().await;
            watch_expressions
                .entry(session_id.to_string())
                .or_insert_with(Vec::new)
                .push(watch);
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.active_watches += 1;
        }
        
        // Evaluate the watch expression immediately
        self.evaluate_watch(session_id, &watch_id).await?;
        
        info!("Added watch expression '{}' for session {}", expression, session_id);
        Ok(watch_id)
    }
    
    /// Remove a watch expression
    pub async fn remove_watch(&self, session_id: &str, watch_id: &str) -> Result<(), InspectorError> {
        {
            let mut watch_expressions = self.watch_expressions.write().await;
            if let Some(watches) = watch_expressions.get_mut(session_id) {
                watches.retain(|w| w.id != watch_id);
                if watches.is_empty() {
                    watch_expressions.remove(session_id);
                }
            }
        }
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.active_watches = stats.active_watches.saturating_sub(1);
        }
        
        info!("Removed watch expression {} for session {}", watch_id, session_id);
        Ok(())
    }
    
    /// Evaluate a specific watch expression
    pub async fn evaluate_watch(&self, session_id: &str, watch_id: &str) -> Result<(), InspectorError> {
        let mut watch_expressions = self.watch_expressions.write().await;
        
        if let Some(watches) = watch_expressions.get_mut(session_id) {
            if let Some(watch) = watches.iter_mut().find(|w| w.id == watch_id) {
                watch.previous_value = watch.current_value.clone();
                
                match self.evaluate_expression(session_id, &watch.expression).await {
                    Ok(result) => {
                        watch.current_value = Some(result.value);
                        watch.error = None;
                        watch.has_changed = watch.previous_value != watch.current_value;
                    },
                    Err(e) => {
                        watch.current_value = None;
                        watch.error = Some(e.to_string());
                        watch.has_changed = false;
                    },
                }
                
                watch.last_evaluated = Some(chrono::Utc::now());
                Ok(())
            } else {
                Err(InspectorError::VariableNotFound(watch_id.to_string()))
            }
        } else {
            Err(InspectorError::SessionNotFound(session_id.to_string()))
        }
    }
    
    /// Evaluate all watch expressions for a session
    pub async fn evaluate_all_watches(&self, session_id: &str) -> Result<(), InspectorError> {
        let watch_ids: Vec<String> = {
            let watch_expressions = self.watch_expressions.read().await;
            if let Some(watches) = watch_expressions.get(session_id) {
                watches.iter().map(|w| w.id.clone()).collect()
            } else {
                return Ok(()); // No watches to evaluate
            }
        };
        
        for watch_id in watch_ids {
            if let Err(e) = self.evaluate_watch(session_id, &watch_id).await {
                warn!("Failed to evaluate watch {}: {}", watch_id, e);
            }
        }
        
        Ok(())
    }
    
    /// Get all watch expressions for a session
    pub async fn get_watches(&self, session_id: &str) -> Vec<WatchExpression> {
        let watch_expressions = self.watch_expressions.read().await;
        watch_expressions
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }
    
    /// Get variable modification history
    pub async fn get_modification_history(&self, session_id: &str) -> Vec<VariableModification> {
        let modifications = self.modifications.read().await;
        modifications
            .iter()
            .filter(|m| m.session_id == session_id)
            .cloned()
            .collect()
    }
    
    /// Clear session data
    pub async fn clear_session(&self, session_id: &str) {
        {
            let mut session_variables = self.session_variables.write().await;
            session_variables.remove(session_id);
        }
        
        {
            let mut watch_expressions = self.watch_expressions.write().await;
            if let Some(watches) = watch_expressions.remove(session_id) {
                let mut stats = self.stats.write().await;
                stats.active_watches = stats.active_watches.saturating_sub(watches.len() as u32);
            }
        }
        
        info!("Cleared inspector data for session {}", session_id);
    }
    
    /// Get inspector statistics
    pub async fn get_stats(&self) -> InspectorStats {
        self.stats.read().await.clone()
    }
    
    /// Search for variables by name pattern
    pub async fn search_variables(
        &self,
        session_id: &str,
        pattern: &str,
    ) -> Result<Vec<Variable>, InspectorError> {
        let session_variables = self.session_variables.read().await;
        
        if let Some(session_vars) = session_variables.get(session_id) {
            let mut results = Vec::new();
            
            for scope_vars in session_vars.values() {
                for var in scope_vars {
                    if var.name.contains(pattern) {
                        results.push(var.clone());
                    }
                }
            }
            
            Ok(results)
        } else {
            Err(InspectorError::SessionNotFound(session_id.to_string()))
        }
    }
    
    /// Get variable type information
    pub async fn get_type_info(
        &self,
        session_id: &str,
        variable_name: &str,
        scope: VariableScope,
    ) -> Result<String, InspectorError> {
        let var = self.get_variable(session_id, variable_name, scope).await?;
        
        let type_info = format!(
            "Type: {:?}\nScope: {:?}\nMutable: {}\nSize: {} bytes\nAddress: {}",
            var.var_type,
            var.scope,
            var.mutable,
            var.size_bytes.map(|s| s.to_string()).unwrap_or_else(|| "unknown".to_string()),
            var.memory_address.unwrap_or_else(|| "unknown".to_string())
        );
        
        Ok(type_info)
    }
}

impl Default for InspectorStats {
    fn default() -> Self {
        Self {
            total_inspected: 0,
            expressions_evaluated: 0,
            modifications_made: 0,
            active_watches: 0,
            avg_evaluation_time_ms: 0.0,
            by_type: HashMap::new(),
            by_scope: HashMap::new(),
        }
    }
}

impl Variable {
    /// Create a simple variable
    pub fn new(
        name: &str,
        value: &str,
        var_type: VariableType,
        scope: VariableScope,
    ) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            var_type,
            scope,
            memory_address: None,
            size_bytes: None,
            mutable: true,
            children: Vec::new(),
            type_info: None,
            description: None,
            last_modified: None,
        }
    }
    
    /// Add a child variable (for objects, arrays, etc.)
    pub fn add_child(&mut self, child: Variable) {
        self.children.push(child);
    }
    
    /// Check if variable has children
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }
    
    /// Get variable by path (e.g., "obj.field.subfield")
    pub fn get_by_path(&self, path: &str) -> Option<&Variable> {
        let parts: Vec<&str> = path.split('.').collect();
        self.get_by_path_parts(&parts)
    }
    
    fn get_by_path_parts(&self, parts: &[&str]) -> Option<&Variable> {
        if parts.is_empty() {
            return Some(self);
        }
        
        let current_part = parts[0];
        if current_part == self.name {
            if parts.len() == 1 {
                Some(self)
            } else {
                for child in &self.children {
                    if let Some(result) = child.get_by_path_parts(&parts[1..]) {
                        return Some(result);
                    }
                }
                None
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_inspector_creation() {
        let inspector = VariableInspector::new();
        let stats = inspector.get_stats().await;
        assert_eq!(stats.total_inspected, 0);
        assert_eq!(stats.active_watches, 0);
    }
    
    #[tokio::test]
    async fn test_variable_operations() {
        let inspector = VariableInspector::new();
        
        let variables = vec![
            Variable::new("x", "10", VariableType::Integer, VariableScope::Local),
            Variable::new("name", "hello", VariableType::String, VariableScope::Local),
        ];
        
        // Set variables
        inspector.set_session_variables("session1", VariableScope::Local, variables).await.unwrap();
        
        // Get variables
        let local_vars = inspector.get_variables("session1", VariableScope::Local).await.unwrap();
        assert_eq!(local_vars.len(), 2);
        
        // Get specific variable
        let x_var = inspector.get_variable("session1", "x", VariableScope::Local).await.unwrap();
        assert_eq!(x_var.value, "10");
        assert_eq!(x_var.var_type, VariableType::Integer);
    }
    
    #[tokio::test]
    async fn test_watch_expressions() {
        let inspector = VariableInspector::new();
        
        // Add watch
        let watch_id = inspector.add_watch("session1", "x + 1").await.unwrap();
        assert!(!watch_id.is_empty());
        
        let stats = inspector.get_stats().await;
        assert_eq!(stats.active_watches, 1);
        
        // Get watches
        let watches = inspector.get_watches("session1").await;
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0].expression, "x + 1");
        
        // Remove watch
        inspector.remove_watch("session1", &watch_id).await.unwrap();
        
        let stats = inspector.get_stats().await;
        assert_eq!(stats.active_watches, 0);
    }
    
    #[tokio::test]
    async fn test_variable_modification() {
        let inspector = VariableInspector::new();
        
        let variables = vec![
            Variable::new("x", "10", VariableType::Integer, VariableScope::Local),
        ];
        
        inspector.set_session_variables("session1", VariableScope::Local, variables).await.unwrap();
        
        // Modify variable
        inspector.modify_variable("session1", "x", VariableScope::Local, "20").await.unwrap();
        
        // Check modification
        let x_var = inspector.get_variable("session1", "x", VariableScope::Local).await.unwrap();
        assert_eq!(x_var.value, "20");
        
        // Check modification history
        let history = inspector.get_modification_history("session1").await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].old_value, "10");
        assert_eq!(history[0].new_value, "20");
        
        let stats = inspector.get_stats().await;
        assert_eq!(stats.modifications_made, 1);
    }
    
    #[test]
    fn test_variable_creation() {
        let var = Variable::new("test", "value", VariableType::String, VariableScope::Local);
        assert_eq!(var.name, "test");
        assert_eq!(var.value, "value");
        assert_eq!(var.var_type, VariableType::String);
        assert_eq!(var.scope, VariableScope::Local);
        assert!(!var.has_children());
    }
    
    #[test]
    fn test_variable_hierarchy() {
        let mut parent = Variable::new("parent", "object", VariableType::Object, VariableScope::Local);
        let child1 = Variable::new("field1", "value1", VariableType::String, VariableScope::Local);
        let child2 = Variable::new("field2", "42", VariableType::Integer, VariableScope::Local);
        
        parent.add_child(child1);
        parent.add_child(child2);
        
        assert!(parent.has_children());
        assert_eq!(parent.children.len(), 2);
        assert_eq!(parent.children[0].name, "field1");
        assert_eq!(parent.children[1].name, "field2");
    }
    
    #[tokio::test]
    async fn test_variable_search() {
        let inspector = VariableInspector::new();
        
        let variables = vec![
            Variable::new("my_var", "10", VariableType::Integer, VariableScope::Local),
            Variable::new("other_var", "hello", VariableType::String, VariableScope::Local),
            Variable::new("my_other", "true", VariableType::Boolean, VariableScope::Local),
        ];
        
        inspector.set_session_variables("session1", VariableScope::Local, variables).await.unwrap();
        
        // Search for variables containing "my"
        let results = inspector.search_variables("session1", "my").await.unwrap();
        assert_eq!(results.len(), 2);
        
        let names: Vec<&str> = results.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"my_var"));
        assert!(names.contains(&"my_other"));
    }
}