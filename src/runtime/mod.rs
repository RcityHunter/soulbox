use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use crate::error::{Result, SoulBoxError};

pub mod python;
pub mod nodejs;
pub mod detector;
pub mod extended;
pub mod ruby;
pub mod php;
pub mod typescript;

/// Supported runtime types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeType {
    Python,
    NodeJS,
    Rust,
    Go,
    Java,
    Shell,
    Ruby,
    PHP,
    TypeScript,
    Cpp,
    C,
    R,
    Julia,
    Kotlin,
    Swift,
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
            RuntimeType::Ruby => "ruby",
            RuntimeType::PHP => "php",
            RuntimeType::TypeScript => "typescript",
            RuntimeType::Cpp => "cpp",
            RuntimeType::C => "c",
            RuntimeType::R => "r",
            RuntimeType::Julia => "julia",
            RuntimeType::Kotlin => "kotlin",
            RuntimeType::Swift => "swift",
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
            "ruby" | "rb" => Some(RuntimeType::Ruby),
            "php" => Some(RuntimeType::PHP),
            "typescript" | "ts" => Some(RuntimeType::TypeScript),
            "cpp" | "c++" | "cxx" => Some(RuntimeType::Cpp),
            "c" => Some(RuntimeType::C),
            "r" => Some(RuntimeType::R),
            "julia" | "jl" => Some(RuntimeType::Julia),
            "kotlin" | "kt" => Some(RuntimeType::Kotlin),
            "swift" => Some(RuntimeType::Swift),
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

impl std::fmt::Display for RuntimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
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
                vec!["js".to_string(), "mjs".to_string()],
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
            RuntimeType::Ruby => (
                format!("ruby:{}-slim", version),
                "/workspace".to_string(),
                HashMap::new(),
                vec!["gem install bundler".to_string()],
                "ruby {file}".to_string(),
                vec!["rb".to_string()],
            ),
            RuntimeType::PHP => (
                format!("php:{}-cli", version),
                "/workspace".to_string(),
                HashMap::new(),
                vec!["apt-get update && apt-get install -y zip unzip".to_string()],
                "php {file}".to_string(),
                vec!["php".to_string()],
            ),
            RuntimeType::TypeScript => (
                "denoland/deno:latest".to_string(),
                "/workspace".to_string(),
                HashMap::new(),
                vec![],
                "deno run --allow-all {file}".to_string(),
                vec!["ts".to_string(), "tsx".to_string()],
            ),
            RuntimeType::Cpp => (
                "gcc:latest".to_string(),
                "/workspace".to_string(),
                HashMap::new(),
                vec![],
                "g++ -o /tmp/a.out {file} && /tmp/a.out".to_string(),
                vec!["cpp".to_string(), "cc".to_string(), "cxx".to_string()],
            ),
            RuntimeType::C => (
                "gcc:latest".to_string(),
                "/workspace".to_string(),
                HashMap::new(),
                vec![],
                "gcc -o /tmp/a.out {file} && /tmp/a.out".to_string(),
                vec!["c".to_string()],
            ),
            RuntimeType::R => (
                format!("r-base:{}", version),
                "/workspace".to_string(),
                HashMap::new(),
                vec![],
                "Rscript {file}".to_string(),
                vec!["r".to_string(), "R".to_string()],
            ),
            RuntimeType::Julia => (
                format!("julia:{}", version),
                "/workspace".to_string(),
                HashMap::new(),
                vec![],
                "julia {file}".to_string(),
                vec!["jl".to_string()],
            ),
            RuntimeType::Kotlin => (
                "zenika/kotlin:latest".to_string(),
                "/workspace".to_string(),
                HashMap::new(),
                vec![],
                "kotlinc {file} -include-runtime -d /tmp/app.jar && java -jar /tmp/app.jar".to_string(),
                vec!["kt".to_string(), "kts".to_string()],
            ),
            RuntimeType::Swift => (
                "swift:latest".to_string(),
                "/workspace".to_string(),
                HashMap::new(),
                vec![],
                "swift {file}".to_string(),
                vec!["swift".to_string()],
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
        
        // Add Ruby runtime
        let ruby_config = RuntimeConfig::new(RuntimeType::Ruby, "3.2".to_string());
        runtimes.insert("ruby".to_string(), ruby_config);
        
        // Add PHP runtime
        let php_config = RuntimeConfig::new(RuntimeType::PHP, "8.2".to_string());
        runtimes.insert("php".to_string(), php_config);
        
        // Add TypeScript runtime
        let typescript_config = RuntimeConfig::new(RuntimeType::TypeScript, "latest".to_string());
        runtimes.insert("typescript".to_string(), typescript_config);
        
        // Add C++ runtime
        let cpp_config = RuntimeConfig::new(RuntimeType::Cpp, "latest".to_string());
        runtimes.insert("cpp".to_string(), cpp_config);
        
        // Add C runtime
        let c_config = RuntimeConfig::new(RuntimeType::C, "latest".to_string());
        runtimes.insert("c".to_string(), c_config);
        
        // Add R runtime
        let r_config = RuntimeConfig::new(RuntimeType::R, "latest".to_string());
        runtimes.insert("r".to_string(), r_config);
        
        // Add Julia runtime
        let julia_config = RuntimeConfig::new(RuntimeType::Julia, "1.9".to_string());
        runtimes.insert("julia".to_string(), julia_config);
        
        // Add Kotlin runtime
        let kotlin_config = RuntimeConfig::new(RuntimeType::Kotlin, "latest".to_string());
        runtimes.insert("kotlin".to_string(), kotlin_config);
        
        // Add Swift runtime
        let swift_config = RuntimeConfig::new(RuntimeType::Swift, "latest".to_string());
        runtimes.insert("swift".to_string(), swift_config);
        
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
        assert_eq!(RuntimeType::from_str_opt("python"), Some(RuntimeType::Python));
        assert_eq!(RuntimeType::from_str_opt("py"), Some(RuntimeType::Python));
        assert_eq!(RuntimeType::from_str_opt("nodejs"), Some(RuntimeType::NodeJS));
        assert_eq!(RuntimeType::from_str_opt("node"), Some(RuntimeType::NodeJS));
        assert_eq!(RuntimeType::from_str_opt("javascript"), Some(RuntimeType::NodeJS));
        assert_eq!(RuntimeType::from_str_opt("rust"), Some(RuntimeType::Rust));
        assert_eq!(RuntimeType::from_str_opt("unknown"), None);
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