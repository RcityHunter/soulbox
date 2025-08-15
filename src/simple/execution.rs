// Code execution logic - no special cases, just configuration
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};

/// Language configuration - no hardcoded special cases
#[derive(Debug, Clone)]
pub struct LanguageConfig {
    pub image: String,
    pub file_extension: String,
    pub execute_command: Vec<String>,
}

/// Code execution request
#[derive(Debug)]
pub struct CodeExecution {
    pub language: String,
    pub code: String,
    pub timeout_seconds: Option<u64>,
}

/// Execution result - simple and clear
#[derive(Debug, Serialize)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub execution_time_ms: u64,
}

/// Language registry - configuration driven, not hardcoded
pub struct LanguageRegistry {
    languages: HashMap<String, LanguageConfig>,
}

impl LanguageRegistry {
    /// Create with default languages - but extensible
    pub fn new() -> Self {
        let mut languages = HashMap::new();
        
        // Python
        languages.insert("python".to_string(), LanguageConfig {
            image: "python:3.11-slim".to_string(),
            file_extension: "py".to_string(),
            execute_command: vec!["python".to_string(), "/workspace/code.py".to_string()],
        });
        
        // Node.js
        languages.insert("javascript".to_string(), LanguageConfig {
            image: "node:18-slim".to_string(),
            file_extension: "js".to_string(),
            execute_command: vec!["node".to_string(), "/workspace/code.js".to_string()],
        });
        
        // Rust
        languages.insert("rust".to_string(), LanguageConfig {
            image: "rust:1.70-slim".to_string(),
            file_extension: "rs".to_string(),
            execute_command: vec![
                "sh".to_string(), 
                "-c".to_string(),
                "cd /workspace && rustc code.rs -o code && ./code".to_string()
            ],
        });
        
        // Go
        languages.insert("go".to_string(), LanguageConfig {
            image: "golang:1.21-slim".to_string(),
            file_extension: "go".to_string(),
            execute_command: vec!["go".to_string(), "run".to_string(), "/workspace/code.go".to_string()],
        });

        Self { languages }
    }

    /// Get language config - no special cases
    pub fn get_config(&self, language: &str) -> Result<&LanguageConfig> {
        self.languages
            .get(language)
            .ok_or_else(|| anyhow!("Unsupported language: {}", language))
    }

    /// Add new language - extensible design
    pub fn add_language(&mut self, name: String, config: LanguageConfig) {
        self.languages.insert(name, config);
    }

    /// List supported languages
    pub fn supported_languages(&self) -> Vec<&String> {
        self.languages.keys().collect()
    }
}

impl CodeExecution {
    /// Create execution command - unified logic, no special cases
    pub fn to_command(&self, config: &LanguageConfig) -> Vec<String> {
        // Create file with code
        let create_file = format!(
            "cat > /workspace/code.{} << 'EOF'\n{}\nEOF",
            config.file_extension,
            self.code
        );

        // Combine file creation with execution
        vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("{} && {}", create_file, config.execute_command.join(" "))
        ]
    }

    /// Validate execution request
    pub fn validate(&self) -> Result<()> {
        if self.code.trim().is_empty() {
            return Err(anyhow!("Code cannot be empty"));
        }
        
        if let Some(timeout) = self.timeout_seconds {
            if timeout > 300 {  // 5 minutes max
                return Err(anyhow!("Timeout cannot exceed 300 seconds"));
            }
        }
        
        Ok(())
    }
}

// Helper function to measure execution time
pub fn measure_execution<F, T>(f: F) -> (T, u64) 
where
    F: FnOnce() -> T,
{
    let start = std::time::Instant::now();
    let result = f();
    let duration = start.elapsed().as_millis() as u64;
    (result, duration)
}