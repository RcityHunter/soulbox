use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::error::{Result, SoulBoxError};
use bollard::container::Config as ContainerConfig;
use super::{RuntimeConfig, RuntimeType, ExecutionContext, ExecutionFile};

/// Python-specific runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonConfig {
    /// Python version (e.g., "3.11", "3.12")
    pub version: String,
    /// Package manager to use (pip, conda, poetry)
    pub package_manager: PythonPackageManager,
    /// Virtual environment configuration
    pub virtual_env: Option<VirtualEnvConfig>,
    /// Pre-installed packages
    pub pre_installed_packages: Vec<String>,
    /// Requirements file content
    pub requirements: Option<String>,
    /// Python optimization flags
    pub optimization_level: u8,
    /// Enable debugging features
    pub debug: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PythonPackageManager {
    Pip,
    Conda,
    Poetry,
    Pipenv,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualEnvConfig {
    /// Virtual environment name
    pub name: String,
    /// Whether to create a new venv
    pub create_new: bool,
    /// Path to existing venv
    pub path: Option<String>,
}

impl Default for PythonConfig {
    fn default() -> Self {
        Self {
            version: "3.11".to_string(),
            package_manager: PythonPackageManager::Pip,
            virtual_env: None,
            pre_installed_packages: vec![
                "requests".to_string(),
                "numpy".to_string(),
                "pandas".to_string(),
                "matplotlib".to_string(),
                "jupyter".to_string(),
            ],
            requirements: None,
            optimization_level: 0,
            debug: false,
        }
    }
}

impl PythonConfig {
    /// Create a new Python configuration
    pub fn new(version: String) -> Self {
        Self {
            version,
            ..Default::default()
        }
    }

    /// Set package manager
    pub fn with_package_manager(mut self, pm: PythonPackageManager) -> Self {
        self.package_manager = pm;
        self
    }

    /// Enable virtual environment
    pub fn with_virtual_env(mut self, config: VirtualEnvConfig) -> Self {
        self.virtual_env = Some(config);
        self
    }

    /// Set pre-installed packages
    pub fn with_packages(mut self, packages: Vec<String>) -> Self {
        self.pre_installed_packages = packages;
        self
    }

    /// Set requirements from file content
    pub fn with_requirements(mut self, requirements: String) -> Self {
        self.requirements = Some(requirements);
        self
    }

    /// Enable debug mode
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }
}

/// Python runtime handler
pub struct PythonRuntime {
    config: PythonConfig,
}

impl PythonRuntime {
    /// Create a new Python runtime
    pub fn new(config: PythonConfig) -> Self {
        Self { config }
    }

    /// Create a default Python runtime
    pub fn default() -> Self {
        Self::new(PythonConfig::default())
    }

    /// Create runtime configuration for container
    pub fn create_runtime_config(&self) -> RuntimeConfig {
        let mut env = HashMap::new();
        env.insert("PYTHONUNBUFFERED".to_string(), "1".to_string());
        env.insert("PYTHONDONTWRITEBYTECODE".to_string(), "1".to_string());
        
        if self.config.debug {
            env.insert("PYTHONDEBUG".to_string(), "1".to_string());
        }

        if self.config.optimization_level > 0 {
            env.insert("PYTHONOPTIMIZE".to_string(), self.config.optimization_level.to_string());
        }

        RuntimeConfig {
            runtime_type: RuntimeType::Python,
            version: self.config.version.clone(),
            docker_image: self.get_docker_image(),
            working_directory: "/workspace".to_string(),
            environment: env,
            install_commands: self.get_install_commands(),
            exec_command: self.get_exec_command(),
            file_extensions: vec!["py".to_string(), "python".to_string(), "pyw".to_string()],
            memory_limit: Some(1024 * 1024 * 1024), // 1GB for Python
            cpu_limit: Some(2.0), // 2 CPUs
            execution_timeout: 60, // 60 seconds
        }
    }

    /// Get Docker image for this Python version
    fn get_docker_image(&self) -> String {
        match self.config.package_manager {
            PythonPackageManager::Conda => format!("continuumio/miniconda3:latest"),
            _ => format!("python:{}-slim", self.config.version),
        }
    }

    /// Generate package installation commands
    fn get_install_commands(&self) -> Vec<String> {
        let mut commands = Vec::new();

        // Update package manager
        match self.config.package_manager {
            PythonPackageManager::Pip => {
                commands.push("python -m pip install --upgrade pip".to_string());
            }
            PythonPackageManager::Conda => {
                commands.push("conda update -n base -c defaults conda".to_string());
            }
            PythonPackageManager::Poetry => {
                commands.push("pip install poetry".to_string());
            }
            PythonPackageManager::Pipenv => {
                commands.push("pip install pipenv".to_string());
            }
        }

        // Install pre-installed packages
        if !self.config.pre_installed_packages.is_empty() {
            let packages = self.config.pre_installed_packages.join(" ");
            match self.config.package_manager {
                PythonPackageManager::Pip => {
                    commands.push(format!("pip install {}", packages));
                }
                PythonPackageManager::Conda => {
                    commands.push(format!("conda install -y {}", packages));
                }
                PythonPackageManager::Poetry => {
                    for package in &self.config.pre_installed_packages {
                        commands.push(format!("poetry add {}", package));
                    }
                }
                PythonPackageManager::Pipenv => {
                    commands.push(format!("pipenv install {}", packages));
                }
            }
        }

        commands
    }

    /// Get execution command template
    fn get_exec_command(&self) -> String {
        if self.config.debug {
            "python -u -d {file}".to_string()
        } else {
            "python -u {file}".to_string()
        }
    }

    /// Create container configuration for Python execution
    pub fn create_container_config(&self, context: &ExecutionContext) -> Result<ContainerConfig> {
        let runtime_config = self.create_runtime_config();
        
        let mut config = ContainerConfig {
            image: runtime_config.docker_image,
            working_dir: Some(context.working_dir.clone()),
            environment: context.environment.clone(),
            memory_limit: runtime_config.memory_limit,
            cpu_limit: runtime_config.cpu_limit,
            ..Default::default()
        };

        // Add Python-specific environment variables
        for (key, value) in &runtime_config.environment {
            config.environment.insert(key.clone(), value.clone());
        }

        Ok(config)
    }

    /// Prepare execution environment
    pub async fn prepare_execution(&self, context: &mut ExecutionContext) -> Result<Vec<String>> {
        let mut commands = Vec::new();

        // Create virtual environment if configured
        if let Some(venv_config) = &self.config.virtual_env {
            if venv_config.create_new {
                commands.push(format!("python -m venv {}", venv_config.name));
                commands.push(format!("source {}/bin/activate", venv_config.name));
            } else if let Some(path) = &venv_config.path {
                commands.push(format!("source {}/bin/activate", path));
            }
        }

        // Install requirements if provided
        if let Some(requirements) = &self.config.requirements {
            // Write requirements.txt file
            context.add_file("requirements.txt".to_string(), requirements.clone(), false);
            
            match self.config.package_manager {
                PythonPackageManager::Pip => {
                    commands.push("pip install -r requirements.txt".to_string());
                }
                PythonPackageManager::Conda => {
                    commands.push("conda install --file requirements.txt".to_string());
                }
                PythonPackageManager::Poetry => {
                    commands.push("poetry install".to_string());
                }
                PythonPackageManager::Pipenv => {
                    commands.push("pipenv install -r requirements.txt".to_string());
                }
            }
        }

        // Run pre-installation commands
        commands.extend(self.get_install_commands());

        Ok(commands)
    }

    /// Execute Python code
    pub async fn execute(&self, context: &ExecutionContext) -> Result<ExecutionResult> {
        let main_file = context.get_main_file()
            .ok_or_else(|| SoulBoxError::RuntimeError("No main file specified".to_string()))?;

        let mut command = self.get_exec_command().replace("{file}", &main_file.name);
        
        // Add command line arguments
        if !context.args.is_empty() {
            command.push(' ');
            command.push_str(&context.args.join(" "));
        }

        Ok(ExecutionResult {
            command,
            working_dir: context.working_dir.clone(),
            environment: context.environment.clone(),
            stdin: context.stdin.clone(),
        })
    }

    /// Validate Python code syntax
    pub fn validate_syntax(&self, code: &str) -> Result<()> {
        // Basic Python syntax validation using AST
        // This is a simplified version - in production you might want to use
        // a proper Python parser or run a syntax check in a container
        
        if code.trim().is_empty() {
            return Err(SoulBoxError::RuntimeError("Code cannot be empty".to_string()));
        }

        // Check for basic Python syntax errors
        let lines: Vec<&str> = code.lines().collect();
        let mut indent_stack = vec![0];
        
        for (line_num, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            
            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Calculate indentation
            let indent = line.len() - line.trim_start().len();
            
            // Check for invalid indentation
            if indent % 4 != 0 {
                return Err(SoulBoxError::RuntimeError(
                    format!("Invalid indentation on line {}: expected multiple of 4 spaces", line_num + 1)
                ));
            }

            // Check for unmatched parentheses, brackets, braces
            let mut paren_count = 0;
            let mut bracket_count = 0;
            let mut brace_count = 0;
            
            for char in trimmed.chars() {
                match char {
                    '(' => paren_count += 1,
                    ')' => paren_count -= 1,
                    '[' => bracket_count += 1,
                    ']' => bracket_count -= 1,
                    '{' => brace_count += 1,
                    '}' => brace_count -= 1,
                    _ => {}
                }
            }

            if paren_count != 0 || bracket_count != 0 || brace_count != 0 {
                return Err(SoulBoxError::RuntimeError(
                    format!("Unmatched brackets/parentheses on line {}", line_num + 1)
                ));
            }
        }

        Ok(())
    }

    /// Get Python version information
    pub fn get_version_info(&self) -> PythonVersionInfo {
        PythonVersionInfo {
            version: self.config.version.clone(),
            package_manager: format!("{:?}", self.config.package_manager).to_lowercase(),
            has_virtual_env: self.config.virtual_env.is_some(),
            debug_enabled: self.config.debug,
            optimization_level: self.config.optimization_level,
        }
    }
}

/// Execution result for Python runtime
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub command: String,
    pub working_dir: String,
    pub environment: HashMap<String, String>,
    pub stdin: Option<String>,
}

/// Python version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonVersionInfo {
    pub version: String,
    pub package_manager: String,
    pub has_virtual_env: bool,
    pub debug_enabled: bool,
    pub optimization_level: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_config_creation() {
        let config = PythonConfig::new("3.11".to_string())
            .with_package_manager(PythonPackageManager::Poetry)
            .with_debug(true);
        
        assert_eq!(config.version, "3.11");
        assert!(matches!(config.package_manager, PythonPackageManager::Poetry));
        assert!(config.debug);
    }

    #[test]
    fn test_python_runtime_creation() {
        let config = PythonConfig::default();
        let runtime = PythonRuntime::new(config);
        let runtime_config = runtime.create_runtime_config();
        
        assert_eq!(runtime_config.runtime_type, RuntimeType::Python);
        assert!(runtime_config.supports_extension("py"));
        assert!(runtime_config.environment.contains_key("PYTHONUNBUFFERED"));
    }

    #[test]
    fn test_docker_image_selection() {
        let config = PythonConfig::new("3.11".to_string());
        let runtime = PythonRuntime::new(config);
        assert_eq!(runtime.get_docker_image(), "python:3.11-slim");

        let conda_config = PythonConfig::new("3.11".to_string())
            .with_package_manager(PythonPackageManager::Conda);
        let conda_runtime = PythonRuntime::new(conda_config);
        assert_eq!(conda_runtime.get_docker_image(), "continuumio/miniconda3:latest");
    }

    #[test]
    fn test_install_commands() {
        let config = PythonConfig::new("3.11".to_string())
            .with_packages(vec!["requests".to_string(), "numpy".to_string()]);
        let runtime = PythonRuntime::new(config);
        let commands = runtime.get_install_commands();
        
        assert!(commands.iter().any(|cmd| cmd.contains("pip install --upgrade pip")));
        assert!(commands.iter().any(|cmd| cmd.contains("requests numpy")));
    }

    #[test]
    fn test_syntax_validation() {
        let runtime = PythonRuntime::default();
        
        // Valid Python code
        assert!(runtime.validate_syntax("print('hello world')").is_ok());
        assert!(runtime.validate_syntax("def foo():\n    return 42").is_ok());
        
        // Invalid indentation
        assert!(runtime.validate_syntax("def foo():\n  return 42").is_err());
        
        // Empty code
        assert!(runtime.validate_syntax("").is_err());
        assert!(runtime.validate_syntax("   ").is_err());
    }

    #[tokio::test]
    async fn test_execution_context() {
        let config = PythonConfig::default();
        let runtime = PythonRuntime::new(config);
        let runtime_config = runtime.create_runtime_config();
        let mut context = ExecutionContext::new(runtime_config);
        
        context.add_file("main.py".to_string(), "print('hello')".to_string(), true);
        context.add_arg("--verbose".to_string());
        
        let result = runtime.execute(&context).await.unwrap();
        assert!(result.command.contains("main.py"));
        assert!(result.command.contains("--verbose"));
    }
}