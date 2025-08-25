use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use crate::error::{Result, SoulBoxError};

pub mod python;
pub mod nodejs;
pub mod detector;

/// Supported runtime types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimeType {
    Python,
    NodeJS,
    Rust,
    Go,
    Java,
    Shell,
}

impl RuntimeType {
    /// Get the string representation of the runtime
    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeType::Python => "python",
            RuntimeType::NodeJS => "nodejs",
            RuntimeType::Rust => "rust",
            RuntimeType::Go => "go",
            RuntimeType::Java => "java",
            RuntimeType::Shell => "shell",
        }
    }

    /// Parse runtime type from string
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "python" | "py" => Some(RuntimeType::Python),
            "nodejs" | "node" | "javascript" | "js" => Some(RuntimeType::NodeJS),
            "rust" | "rs" => Some(RuntimeType::Rust),
            "go" | "golang" => Some(RuntimeType::Go),
            "java" => Some(RuntimeType::Java),
            "shell" | "bash" | "sh" => Some(RuntimeType::Shell),
            _ => None,
        }
    }
}

impl FromStr for RuntimeType {
    type Err = SoulBoxError;

    fn from_str(s: &str) -> Result<Self> {
        Self::from_str_opt(s).ok_or_else(|| {
            SoulBoxError::ValidationError {
                field: "runtime_type".to_string(),
                reason: format!("Unknown runtime type: '{}'. Expected one of: python, nodejs, rust, go, java, shell", s),
                provided_value: Some(s.to_string()),
                security_context: None,
            }
        })
    }
}

/// Runtime configuration for a specific language environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Runtime type
    pub runtime_type: RuntimeType,
    /// Version of the runtime (e.g., "3.11" for Python, "20" for Node.js)
    pub version: String,
    /// Docker image to use for this runtime
    pub docker_image: String,
    /// Default working directory
    pub working_directory: String,
    /// Environment variables to set
    pub environment: HashMap<String, String>,
    /// Package installation commands
    pub install_commands: Vec<String>,
    /// Execution command template
    pub exec_command: String,
    /// Supported file extensions
    pub file_extensions: Vec<String>,
    /// Resource limits
    pub memory_limit: Option<u64>,
    pub cpu_limit: Option<f64>,
    /// Timeout for execution (in seconds)
    pub execution_timeout: u64,
}

impl RuntimeConfig {
    /// Create a new runtime configuration
    pub fn new(runtime_type: RuntimeType, version: String) -> Self {
        let (docker_image, working_dir, env, install_cmds, exec_cmd, extensions) = match runtime_type {
            RuntimeType::Python => (
                format!("python:{}-slim", version),
                "/workspace".to_string(),
                [("PYTHONUNBUFFERED".to_string(), "1".to_string())].iter().cloned().collect(),
                vec!["pip install".to_string()],
                "python {file}".to_string(),
                vec!["py".to_string(), "python".to_string()],
            ),
            RuntimeType::NodeJS => (
                format!("node:{}-alpine", version),
                "/workspace".to_string(),
                HashMap::new(),
                vec!["npm install".to_string()],
                "node {file}".to_string(),
                vec!["js".to_string(), "mjs".to_string(), "ts".to_string()],
            ),
            RuntimeType::Rust => (
                "rust:1.75-slim".to_string(),
                "/workspace".to_string(),
                HashMap::new(),
                vec!["cargo build".to_string()],
                "cargo run".to_string(),
                vec!["rs".to_string()],
            ),
            RuntimeType::Go => (
                "golang:1.21-alpine".to_string(),
                "/workspace".to_string(),
                HashMap::new(),
                vec!["go mod download".to_string()],
                "go run {file}".to_string(),
                vec!["go".to_string()],
            ),
            RuntimeType::Java => (
                "openjdk:21-slim".to_string(),
                "/workspace".to_string(),
                HashMap::new(),
                vec![],
                "java {file}".to_string(),
                vec!["java".to_string()],
            ),
            RuntimeType::Shell => (
                "ubuntu:22.04".to_string(),
                "/workspace".to_string(),
                HashMap::new(),
                vec![],
                "bash {file}".to_string(),
                vec!["sh".to_string(), "bash".to_string()],
            ),
        };

        Self {
            runtime_type,
            version,
            docker_image,
            working_directory: working_dir,
            environment: env,
            install_commands: install_cmds,
            exec_command: exec_cmd,
            file_extensions: extensions,
            memory_limit: Some(512 * 1024 * 1024), // 512MB default
            cpu_limit: Some(1.0), // 1 CPU default
            execution_timeout: 30, // 30 seconds default
        }
    }

    /// Check if this runtime supports the given file extension
    pub fn supports_extension(&self, extension: &str) -> bool {
        self.file_extensions.iter().any(|ext| ext == extension)
    }

    /// Generate the execution command for a specific file
    pub fn generate_exec_command(&self, filename: &str) -> String {
        self.exec_command.replace("{file}", filename)
    }
}

/// Runtime manager for handling multiple language environments
#[derive(Debug)]
pub struct RuntimeManager {
    /// Available runtime configurations
    runtimes: HashMap<String, RuntimeConfig>,
    /// Default runtime to use when none is specified
    default_runtime: String,
}

impl RuntimeManager {
    /// Create a new runtime manager with default configurations
    pub fn new() -> Self {
        let mut runtimes = HashMap::new();
        
        // Add default Python runtime
        let python_config = RuntimeConfig::new(RuntimeType::Python, "3.11".to_string());
        runtimes.insert("python".to_string(), python_config);
        
        // Add default Node.js runtime
        let nodejs_config = RuntimeConfig::new(RuntimeType::NodeJS, "20".to_string());
        runtimes.insert("nodejs".to_string(), nodejs_config);
        
        // Add default Rust runtime
        let rust_config = RuntimeConfig::new(RuntimeType::Rust, "1.75".to_string());
        runtimes.insert("rust".to_string(), rust_config);
        
        // Add default Go runtime
        let go_config = RuntimeConfig::new(RuntimeType::Go, "1.21".to_string());
        runtimes.insert("go".to_string(), go_config);
        
        // Add default Java runtime
        let java_config = RuntimeConfig::new(RuntimeType::Java, "21".to_string());
        runtimes.insert("java".to_string(), java_config);
        
        // Add default Shell runtime
        let shell_config = RuntimeConfig::new(RuntimeType::Shell, "latest".to_string());
        runtimes.insert("shell".to_string(), shell_config);
        
        Self {
            runtimes,
            default_runtime: "python".to_string(),
        }
    }

    /// Add a new runtime configuration
    pub fn add_runtime(&mut self, name: String, config: RuntimeConfig) {
        self.runtimes.insert(name, config);
    }

    /// Get runtime configuration by name
    pub fn get_runtime(&self, name: &str) -> Option<&RuntimeConfig> {
        self.runtimes.get(name)
    }

    /// Get runtime configuration by runtime type
    pub fn get_runtime_by_type(&self, runtime_type: &RuntimeType) -> Option<&RuntimeConfig> {
        self.runtimes.values().find(|config| &config.runtime_type == runtime_type)
    }

    /// List all available runtimes
    pub fn list_runtimes(&self) -> Vec<&str> {
        self.runtimes.keys().map(|s| s.as_str()).collect()
    }

    /// Detect runtime from file extension
    pub fn detect_runtime_from_extension(&self, extension: &str) -> Option<&RuntimeConfig> {
        self.runtimes.values().find(|config| config.supports_extension(extension))
    }

    /// Detect runtime from file content
    pub fn detect_runtime_from_content(&self, content: &str) -> Option<&RuntimeConfig> {
        detector::detect_runtime_from_content(content, &self.runtimes)
    }

    /// Get the default runtime configuration
    pub fn get_default_runtime(&self) -> Option<&RuntimeConfig> {
        self.runtimes.get(&self.default_runtime)
    }

    /// Set the default runtime
    pub fn set_default_runtime(&mut self, name: String) -> Result<()> {
        if self.runtimes.contains_key(&name) {
            self.default_runtime = name;
            Ok(())
        } else {
            Err(SoulBoxError::RuntimeError(format!("Runtime '{}' not found", name)))
        }
    }

    /// Validate runtime configuration
    pub fn validate_config(&self, config: &RuntimeConfig) -> Result<()> {
        // Basic validation
        if config.version.is_empty() {
            return Err(SoulBoxError::RuntimeError("Runtime version cannot be empty".to_string()));
        }

        if config.docker_image.is_empty() {
            return Err(SoulBoxError::RuntimeError("Docker image cannot be empty".to_string()));
        }

        if config.exec_command.is_empty() {
            return Err(SoulBoxError::RuntimeError("Execution command cannot be empty".to_string()));
        }

        if config.file_extensions.is_empty() {
            return Err(SoulBoxError::RuntimeError("At least one file extension must be specified".to_string()));
        }

        Ok(())
    }
}

impl Default for RuntimeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Runtime execution context
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Runtime configuration to use
    pub runtime: RuntimeConfig,
    /// Working directory for execution
    pub working_dir: String,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Input files for execution
    pub files: Vec<ExecutionFile>,
    /// Command arguments
    pub args: Vec<String>,
    /// Standard input for the process
    pub stdin: Option<String>,
}

/// File for execution
#[derive(Debug, Clone)]
pub struct ExecutionFile {
    /// File name
    pub name: String,
    /// File content
    pub content: String,
    /// Whether this is the main file to execute
    pub is_main: bool,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new(runtime: RuntimeConfig) -> Self {
        Self {
            working_dir: runtime.working_directory.clone(),
            environment: runtime.environment.clone(),
            runtime,
            files: Vec::new(),
            args: Vec::new(),
            stdin: None,
        }
    }

    /// Add a file to the execution context
    pub fn add_file(&mut self, name: String, content: String, is_main: bool) {
        self.files.push(ExecutionFile {
            name,
            content,
            is_main,
        });
    }

    /// Get the main file for execution
    pub fn get_main_file(&self) -> Option<&ExecutionFile> {
        self.files.iter().find(|f| f.is_main)
    }

    /// Set environment variable
    pub fn set_env(&mut self, key: String, value: String) {
        self.environment.insert(key, value);
    }

    /// Add command line argument
    pub fn add_arg(&mut self, arg: String) {
        self.args.push(arg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_type_parsing() {
        assert_eq!(RuntimeType::from_str("python"), Some(RuntimeType::Python));
        assert_eq!(RuntimeType::from_str("py"), Some(RuntimeType::Python));
        assert_eq!(RuntimeType::from_str("nodejs"), Some(RuntimeType::NodeJS));
        assert_eq!(RuntimeType::from_str("node"), Some(RuntimeType::NodeJS));
        assert_eq!(RuntimeType::from_str("javascript"), Some(RuntimeType::NodeJS));
        assert_eq!(RuntimeType::from_str("rust"), Some(RuntimeType::Rust));
        assert_eq!(RuntimeType::from_str("unknown"), None);
    }

    #[test]
    fn test_runtime_config_creation() {
        let config = RuntimeConfig::new(RuntimeType::Python, "3.11".to_string());
        assert_eq!(config.runtime_type, RuntimeType::Python);
        assert_eq!(config.version, "3.11");
        assert!(config.supports_extension("py"));
        assert!(!config.supports_extension("js"));
    }

    #[test]
    fn test_runtime_manager() {
        let manager = RuntimeManager::new();
        
        assert!(manager.get_runtime("python").is_some());
        assert!(manager.get_runtime("nodejs").is_some());
        assert!(manager.get_runtime("unknown").is_none());
        
        let runtimes = manager.list_runtimes();
        assert!(runtimes.contains(&"python"));
        assert!(runtimes.contains(&"nodejs"));
    }

    #[test]
    fn test_execution_context() {
        let config = RuntimeConfig::new(RuntimeType::Python, "3.11".to_string());
        let mut context = ExecutionContext::new(config);
        
        context.add_file("main.py".to_string(), "print('hello')".to_string(), true);
        context.add_file("utils.py".to_string(), "def helper(): pass".to_string(), false);
        
        assert_eq!(context.files.len(), 2);
        assert!(context.get_main_file().is_some());
        assert_eq!(context.get_main_file().unwrap().name, "main.py");
    }

    #[test]
    fn test_exec_command_generation() {
        let config = RuntimeConfig::new(RuntimeType::Python, "3.11".to_string());
        let command = config.generate_exec_command("script.py");
        assert_eq!(command, "python script.py");
    }
}