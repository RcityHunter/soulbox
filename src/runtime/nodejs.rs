use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::error::{Result, SoulBoxError};
use bollard::container::Config;
use super::{RuntimeConfig, RuntimeType, ExecutionContext, ExecutionFile};

/// Node.js-specific runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeJSConfig {
    /// Node.js version (e.g., "20", "18", "16")
    pub version: String,
    /// Package manager to use
    pub package_manager: NodePackageManager,
    /// TypeScript configuration
    pub typescript: Option<TypeScriptConfig>,
    /// Pre-installed packages
    pub pre_installed_packages: Vec<String>,
    /// Package.json content
    pub package_json: Option<String>,
    /// Enable Node.js debugging
    pub debug: bool,
    /// Node.js optimization flags
    pub node_options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodePackageManager {
    Npm,
    Yarn,
    Pnpm,
    Bun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeScriptConfig {
    /// TypeScript version
    pub version: String,
    /// tsconfig.json content
    pub tsconfig: Option<String>,
    /// Compilation target (es5, es6, etc.)
    pub target: String,
    /// Module system (commonjs, es6, etc.)
    pub module: String,
    /// Enable strict mode
    pub strict: bool,
}

impl Default for NodeJSConfig {
    fn default() -> Self {
        Self {
            version: "20".to_string(),
            package_manager: NodePackageManager::Npm,
            typescript: None,
            pre_installed_packages: vec![
                "lodash".to_string(),
                "axios".to_string(),
                "express".to_string(),
                "moment".to_string(),
            ],
            package_json: None,
            debug: false,
            node_options: vec!["--max-old-space-size=1024".to_string()],
        }
    }
}

impl NodeJSConfig {
    /// Create a new Node.js configuration
    pub fn new(version: String) -> Self {
        Self {
            version,
            ..Default::default()
        }
    }

    /// Set package manager
    pub fn with_package_manager(mut self, pm: NodePackageManager) -> Self {
        self.package_manager = pm;
        self
    }

    /// Enable TypeScript support
    pub fn with_typescript(mut self, config: TypeScriptConfig) -> Self {
        self.typescript = Some(config);
        self
    }

    /// Set pre-installed packages
    pub fn with_packages(mut self, packages: Vec<String>) -> Self {
        self.pre_installed_packages = packages;
        self
    }

    /// Set package.json content
    pub fn with_package_json(mut self, package_json: String) -> Self {
        self.package_json = Some(package_json);
        self
    }

    /// Enable debug mode
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    /// Add Node.js option
    pub fn with_node_option(mut self, option: String) -> Self {
        self.node_options.push(option);
        self
    }
}

impl Default for TypeScriptConfig {
    fn default() -> Self {
        Self {
            version: "latest".to_string(),
            tsconfig: None,
            target: "es2020".to_string(),
            module: "commonjs".to_string(),
            strict: true,
        }
    }
}

/// Node.js runtime handler
pub struct NodeJSRuntime {
    config: NodeJSConfig,
}

impl NodeJSRuntime {
    /// Create a new Node.js runtime
    pub fn new(config: NodeJSConfig) -> Self {
        Self { config }
    }

    /// Create a default Node.js runtime
    pub fn default() -> Self {
        Self::new(NodeJSConfig::default())
    }

    /// Create runtime configuration for container
    pub fn create_runtime_config(&self) -> RuntimeConfig {
        let mut env = HashMap::new();
        env.insert("NODE_ENV".to_string(), "development".to_string());
        
        // Set Node.js options
        if !self.config.node_options.is_empty() {
            env.insert("NODE_OPTIONS".to_string(), self.config.node_options.join(" "));
        }

        RuntimeConfig {
            runtime_type: RuntimeType::NodeJS,
            version: self.config.version.clone(),
            docker_image: self.get_docker_image(),
            working_directory: "/workspace".to_string(),
            environment: env,
            install_commands: self.get_install_commands(),
            exec_command: self.get_exec_command(),
            file_extensions: vec![
                "js".to_string(), 
                "mjs".to_string(), 
                "ts".to_string(),
                "jsx".to_string(),
                "tsx".to_string(),
            ],
            memory_limit: Some(1024 * 1024 * 1024), // 1GB for Node.js
            cpu_limit: Some(2.0), // 2 CPUs
            execution_timeout: 60, // 60 seconds
        }
    }

    /// Get Docker image for this Node.js version
    fn get_docker_image(&self) -> String {
        match self.config.package_manager {
            NodePackageManager::Bun => "oven/bun:latest".to_string(),
            _ => format!("node:{}-alpine", self.config.version),
        }
    }

    /// Generate package installation commands
    fn get_install_commands(&self) -> Vec<String> {
        let mut commands = Vec::new();

        // Update package manager
        match self.config.package_manager {
            NodePackageManager::Npm => {
                commands.push("npm install -g npm@latest".to_string());
            }
            NodePackageManager::Yarn => {
                commands.push("npm install -g yarn".to_string());
            }
            NodePackageManager::Pnpm => {
                commands.push("npm install -g pnpm".to_string());
            }
            NodePackageManager::Bun => {
                // Bun is already installed in the image
            }
        }

        // Install TypeScript if configured
        if let Some(ts_config) = &self.config.typescript {
            match self.config.package_manager {
                NodePackageManager::Npm => {
                    commands.push(format!("npm install -g typescript@{}", ts_config.version));
                    commands.push("npm install -g ts-node".to_string());
                }
                NodePackageManager::Yarn => {
                    commands.push(format!("yarn global add typescript@{}", ts_config.version));
                    commands.push("yarn global add ts-node".to_string());
                }
                NodePackageManager::Pnpm => {
                    commands.push(format!("pnpm install -g typescript@{}", ts_config.version));
                    commands.push("pnpm install -g ts-node".to_string());
                }
                NodePackageManager::Bun => {
                    commands.push(format!("bun add -g typescript@{}", ts_config.version));
                }
            }
        }

        // Install pre-installed packages
        if !self.config.pre_installed_packages.is_empty() {
            let packages = self.config.pre_installed_packages.join(" ");
            match self.config.package_manager {
                NodePackageManager::Npm => {
                    commands.push(format!("npm install -g {}", packages));
                }
                NodePackageManager::Yarn => {
                    commands.push(format!("yarn global add {}", packages));
                }
                NodePackageManager::Pnpm => {
                    commands.push(format!("pnpm install -g {}", packages));
                }
                NodePackageManager::Bun => {
                    commands.push(format!("bun add -g {}", packages));
                }
            }
        }

        commands
    }

    /// Get execution command template
    fn get_exec_command(&self) -> String {
        if self.config.debug {
            "node --inspect {file}".to_string()
        } else {
            "node {file}".to_string()
        }
    }

    /// Create container configuration for Node.js execution
    pub fn create_container_config(&self, context: &ExecutionContext) -> Result<Config<String>> {
        let runtime_config = self.create_runtime_config();
        
        // Convert HashMap to Vec for bollard
        let mut env_vec: Vec<String> = context.environment.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
            
        // Add runtime-specific environment variables
        for (key, value) in &runtime_config.environment {
            env_vec.push(format!("{}={}", key, value));
        }
        
        let config = Config {
            image: Some(runtime_config.docker_image),
            working_dir: Some(context.working_dir.clone()),
            env: Some(env_vec),
            ..Default::default()
        };

        Ok(config)
    }

    /// Prepare execution environment
    pub async fn prepare_execution(&self, context: &mut ExecutionContext) -> Result<Vec<String>> {
        let mut commands = Vec::new();

        // Create package.json if provided
        if let Some(package_json) = &self.config.package_json {
            context.add_file("package.json".to_string(), package_json.clone(), false);
            
            // Install dependencies
            match self.config.package_manager {
                NodePackageManager::Npm => {
                    commands.push("npm install".to_string());
                }
                NodePackageManager::Yarn => {
                    commands.push("yarn install".to_string());
                }
                NodePackageManager::Pnpm => {
                    commands.push("pnpm install".to_string());
                }
                NodePackageManager::Bun => {
                    commands.push("bun install".to_string());
                }
            }
        }

        // Create TypeScript configuration if enabled
        if let Some(ts_config) = &self.config.typescript {
            if let Some(tsconfig) = &ts_config.tsconfig {
                context.add_file("tsconfig.json".to_string(), tsconfig.clone(), false);
            } else {
                // Generate default tsconfig.json
                let default_tsconfig = self.generate_default_tsconfig(ts_config);
                context.add_file("tsconfig.json".to_string(), default_tsconfig, false);
            }
        }

        // Run pre-installation commands
        commands.extend(self.get_install_commands());

        Ok(commands)
    }

    /// Execute Node.js code
    pub async fn execute(&self, context: &ExecutionContext) -> Result<ExecutionResult> {
        let main_file = context.get_main_file()
            .ok_or_else(|| SoulBoxError::RuntimeError("No main file specified".to_string()))?;

        let mut command = if self.is_typescript_file(&main_file.name) && self.config.typescript.is_some() {
            // Use ts-node for TypeScript files
            if self.config.debug {
                format!("ts-node --inspect {}", main_file.name)
            } else {
                format!("ts-node {}", main_file.name)
            }
        } else {
            self.get_exec_command().replace("{file}", &main_file.name)
        };
        
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

    /// Check if file is a TypeScript file
    fn is_typescript_file(&self, filename: &str) -> bool {
        filename.ends_with(".ts") || filename.ends_with(".tsx")
    }

    /// Generate default tsconfig.json
    fn generate_default_tsconfig(&self, ts_config: &TypeScriptConfig) -> String {
        serde_json::json!({
            "compilerOptions": {
                "target": ts_config.target,
                "module": ts_config.module,
                "strict": ts_config.strict,
                "esModuleInterop": true,
                "skipLibCheck": true,
                "forceConsistentCasingInFileNames": true,
                "resolveJsonModule": true,
                "allowSyntheticDefaultImports": true,
                "outDir": "./dist",
                "rootDir": "./src"
            },
            "include": ["src/**/*"],
            "exclude": ["node_modules", "dist"]
        }).to_string()
    }

    /// Validate JavaScript/TypeScript code syntax
    pub fn validate_syntax(&self, code: &str, filename: &str) -> Result<()> {
        if code.trim().is_empty() {
            return Err(SoulBoxError::RuntimeError("Code cannot be empty".to_string()));
        }

        // Basic syntax validation
        let mut brace_count = 0;
        let mut paren_count = 0;
        let mut bracket_count = 0;
        let mut in_string = false;
        let mut in_comment = false;
        let mut escape_next = false;

        let chars: Vec<char> = code.chars().collect();
        for i in 0..chars.len() {
            let ch = chars[i];

            if escape_next {
                escape_next = false;
                continue;
            }

            if ch == '\\' {
                escape_next = true;
                continue;
            }

            // Handle string literals
            if ch == '"' || ch == '\'' || ch == '`' {
                in_string = !in_string;
                continue;
            }

            if in_string {
                continue;
            }

            // Handle comments
            if i < chars.len() - 1 {
                let next_ch = chars[i + 1];
                if ch == '/' && next_ch == '/' {
                    in_comment = true;
                    continue;
                } else if ch == '/' && next_ch == '*' {
                    // TODO: Handle block comments properly
                }
            }

            if ch == '\n' {
                in_comment = false;
                continue;
            }

            if in_comment {
                continue;
            }

            // Count brackets/parentheses
            match ch {
                '{' => brace_count += 1,
                '}' => brace_count -= 1,
                '(' => paren_count += 1,
                ')' => paren_count -= 1,
                '[' => bracket_count += 1,
                ']' => bracket_count -= 1,
                _ => {}
            }

            // Check for negative counts (closing before opening)
            if brace_count < 0 || paren_count < 0 || bracket_count < 0 {
                return Err(SoulBoxError::RuntimeError(
                    "Syntax error: Unmatched closing bracket/parenthesis".to_string()
                ));
            }
        }

        // Check for unmatched opening brackets/parentheses
        if brace_count != 0 || paren_count != 0 || bracket_count != 0 {
            return Err(SoulBoxError::RuntimeError(
                "Syntax error: Unmatched opening bracket/parenthesis".to_string()
            ));
        }

        // TypeScript-specific validation
        if self.is_typescript_file(filename) && self.config.typescript.is_some() {
            self.validate_typescript_syntax(code)?;
        }

        Ok(())
    }

    /// Validate TypeScript-specific syntax
    fn validate_typescript_syntax(&self, code: &str) -> Result<()> {
        // Basic TypeScript syntax checks
        if code.contains("interface") && !code.contains("{") {
            return Err(SoulBoxError::RuntimeError(
                "TypeScript interface must have a body".to_string()
            ));
        }

        // Check for type annotations (basic validation)
        let lines: Vec<&str> = code.lines().collect();
        for (line_num, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            
            // Check for invalid type syntax
            if trimmed.contains(": ") && !trimmed.contains("=") {
                // This is likely a type annotation, do basic validation
                if trimmed.ends_with(":") {
                    return Err(SoulBoxError::RuntimeError(
                        format!("Incomplete type annotation on line {}", line_num + 1)
                    ));
                }
            }
        }

        Ok(())
    }

    /// Get Node.js version information
    pub fn get_version_info(&self) -> NodeJSVersionInfo {
        NodeJSVersionInfo {
            version: self.config.version.clone(),
            package_manager: format!("{:?}", self.config.package_manager).to_lowercase(),
            has_typescript: self.config.typescript.is_some(),
            debug_enabled: self.config.debug,
            node_options: self.config.node_options.clone(),
        }
    }
}

/// Execution result for Node.js runtime
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub command: String,
    pub working_dir: String,
    pub environment: HashMap<String, String>,
    pub stdin: Option<String>,
}

/// Node.js version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeJSVersionInfo {
    pub version: String,
    pub package_manager: String,
    pub has_typescript: bool,
    pub debug_enabled: bool,
    pub node_options: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nodejs_config_creation() {
        let config = NodeJSConfig::new("20".to_string())
            .with_package_manager(NodePackageManager::Yarn)
            .with_debug(true);
        
        assert_eq!(config.version, "20");
        assert!(matches!(config.package_manager, NodePackageManager::Yarn));
        assert!(config.debug);
    }

    #[test]
    fn test_typescript_config() {
        let ts_config = TypeScriptConfig::default();
        let config = NodeJSConfig::new("20".to_string())
            .with_typescript(ts_config);
        
        assert!(config.typescript.is_some());
    }

    #[test]
    fn test_nodejs_runtime_creation() {
        let config = NodeJSConfig::default();
        let runtime = NodeJSRuntime::new(config);
        let runtime_config = runtime.create_runtime_config();
        
        assert_eq!(runtime_config.runtime_type, RuntimeType::NodeJS);
        assert!(runtime_config.file_extensions.contains(&"js".to_string()));
        assert!(runtime_config.file_extensions.contains(&"ts".to_string()));
        assert!(runtime_config.environment.contains_key("NODE_ENV"));
    }

    #[test]
    fn test_docker_image_selection() {
        let config = NodeJSConfig::new("20".to_string());
        let runtime = NodeJSRuntime::new(config);
        assert_eq!(runtime.get_docker_image(), "node:20-alpine");

        let bun_config = NodeJSConfig::new("20".to_string())
            .with_package_manager(NodePackageManager::Bun);
        let bun_runtime = NodeJSRuntime::new(bun_config);
        assert_eq!(bun_runtime.get_docker_image(), "oven/bun:latest");
    }

    #[test]
    fn test_typescript_file_detection() {
        let runtime = NodeJSRuntime::default();
        
        assert!(runtime.is_typescript_file("app.ts"));
        assert!(runtime.is_typescript_file("component.tsx"));
        assert!(!runtime.is_typescript_file("app.js"));
        assert!(!runtime.is_typescript_file("script.mjs"));
    }

    #[test]
    fn test_syntax_validation() {
        let runtime = NodeJSRuntime::default();
        
        // Valid JavaScript
        assert!(runtime.validate_syntax("console.log('hello');", "test.js").is_ok());
        assert!(runtime.validate_syntax("function foo() { return 42; }", "test.js").is_ok());
        
        // Invalid syntax
        assert!(runtime.validate_syntax("function foo() { return 42;", "test.js").is_err());
        assert!(runtime.validate_syntax("console.log('hello'", "test.js").is_err());
        
        // Empty code
        assert!(runtime.validate_syntax("", "test.js").is_err());
        assert!(runtime.validate_syntax("   ", "test.js").is_err());
    }

    #[tokio::test]
    async fn test_execution_context() {
        let config = NodeJSConfig::default();
        let runtime = NodeJSRuntime::new(config);
        let runtime_config = runtime.create_runtime_config();
        let mut context = ExecutionContext::new(runtime_config);
        
        context.add_file("index.js".to_string(), "console.log('hello')".to_string(), true);
        context.add_arg("--version".to_string());
        
        let result = runtime.execute(&context).await.unwrap();
        assert!(result.command.contains("index.js"));
        assert!(result.command.contains("--version"));
    }

    #[test]
    fn test_tsconfig_generation() {
        let ts_config = TypeScriptConfig::default();
        let runtime = NodeJSRuntime::new(NodeJSConfig::new("20".to_string()));
        let tsconfig = runtime.generate_default_tsconfig(&ts_config);
        
        assert!(tsconfig.contains("\"target\""));
        assert!(tsconfig.contains("\"module\""));
        assert!(tsconfig.contains("\"strict\""));
    }
}