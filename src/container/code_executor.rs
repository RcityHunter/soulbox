use crate::error::Result;
use crate::container::sandbox::SandboxContainer;
use std::sync::Arc;
use tracing::{info, error};
use std::time::Duration;
use base64::{Engine as _, engine::general_purpose};

/// Code executor for running code inside containers
pub struct CodeExecutor {
    container: Arc<SandboxContainer>,
}

impl CodeExecutor {
    pub fn new(container: Arc<SandboxContainer>) -> Self {
        Self { container }
    }

    /// Execute Python code in container
    pub async fn execute_python(&self, code: &str, timeout: Duration) -> Result<CodeExecutionResult> {
        info!("Executing Python code in container {}", self.container.get_id());
        
        // Encode code to base64 to prevent injection attacks
        let encoded_code = general_purpose::STANDARD.encode(code.as_bytes());
        
        // Write code to a temporary file in the container using base64 decoding
        let file_path = "/tmp/code.py";
        let write_cmd = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("echo '{}' | base64 -d > {}", encoded_code, file_path)
        ];
        
        // Write the code file
        let write_result = self.container.execute_command(write_cmd).await?;
        if write_result.exit_code != 0 {
            error!("Failed to write Python code: {}", write_result.stderr);
            return Err(crate::error::SoulBoxError::RuntimeError(
                format!("Failed to write code: {}", write_result.stderr)
            ));
        }
        
        // Execute the Python code with timeout
        let exec_cmd = vec![
            "timeout".to_string(),
            format!("{}", timeout.as_secs()),
            "python3".to_string(),
            file_path.to_string()
        ];
        
        let start_time = std::time::Instant::now();
        let exec_result = self.container.execute_command(exec_cmd).await?;
        let execution_time = start_time.elapsed();
        
        // Clean up the temporary file
        let cleanup_cmd = vec!["rm".to_string(), "-f".to_string(), file_path.to_string()];
        let _ = self.container.execute_command(cleanup_cmd).await;
        
        Ok(CodeExecutionResult {
            stdout: exec_result.stdout,
            stderr: exec_result.stderr,
            exit_code: exec_result.exit_code,
            execution_time,
            language: "python".to_string(),
        })
    }

    /// Execute Node.js/JavaScript code in container
    pub async fn execute_nodejs(&self, code: &str, timeout: Duration) -> Result<CodeExecutionResult> {
        info!("Executing Node.js code in container {}", self.container.get_id());
        
        // Encode code to base64 to prevent injection attacks
        let encoded_code = general_purpose::STANDARD.encode(code.as_bytes());
        
        // Write code to a temporary file in the container using base64 decoding
        let file_path = "/tmp/code.js";
        let write_cmd = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("echo '{}' | base64 -d > {}", encoded_code, file_path)
        ];
        
        // Write the code file
        let write_result = self.container.execute_command(write_cmd).await?;
        if write_result.exit_code != 0 {
            error!("Failed to write JavaScript code: {}", write_result.stderr);
            return Err(crate::error::SoulBoxError::RuntimeError(
                format!("Failed to write code: {}", write_result.stderr)
            ));
        }
        
        // Execute the Node.js code with timeout
        let exec_cmd = vec![
            "timeout".to_string(),
            format!("{}", timeout.as_secs()),
            "node".to_string(),
            file_path.to_string()
        ];
        
        let start_time = std::time::Instant::now();
        let exec_result = self.container.execute_command(exec_cmd).await?;
        let execution_time = start_time.elapsed();
        
        // Clean up the temporary file
        let cleanup_cmd = vec!["rm".to_string(), "-f".to_string(), file_path.to_string()];
        let _ = self.container.execute_command(cleanup_cmd).await;
        
        Ok(CodeExecutionResult {
            stdout: exec_result.stdout,
            stderr: exec_result.stderr,
            exit_code: exec_result.exit_code,
            execution_time,
            language: "nodejs".to_string(),
        })
    }

    /// Execute shell commands in container
    pub async fn execute_shell(&self, command: &str, timeout: Duration) -> Result<CodeExecutionResult> {
        info!("Executing shell command in container {}", self.container.get_id());
        
        // Encode command to base64 to prevent injection attacks
        let encoded_cmd = general_purpose::STANDARD.encode(command.as_bytes());
        
        // Execute command using base64 decoding
        let exec_cmd = vec![
            "timeout".to_string(),
            format!("{}", timeout.as_secs()),
            "sh".to_string(),
            "-c".to_string(),
            format!("echo '{}' | base64 -d | sh", encoded_cmd)
        ];
        
        let start_time = std::time::Instant::now();
        let exec_result = self.container.execute_command(exec_cmd).await?;
        let execution_time = start_time.elapsed();
        
        Ok(CodeExecutionResult {
            stdout: exec_result.stdout,
            stderr: exec_result.stderr,
            exit_code: exec_result.exit_code,
            execution_time,
            language: "shell".to_string(),
        })
    }

    /// Execute code with language detection
    pub async fn execute(&self, code: &str, language: Option<&str>, timeout: Duration) -> Result<CodeExecutionResult> {
        let lang = language.unwrap_or_else(|| {
            // Simple language detection based on code patterns
            if code.contains("import ") || code.contains("def ") || code.contains("print(") {
                "python"
            } else if code.contains("const ") || code.contains("let ") || code.contains("console.log") {
                "nodejs"
            } else {
                "shell"
            }
        });

        match lang.to_lowercase().as_str() {
            "python" | "py" => self.execute_python(code, timeout).await,
            "javascript" | "js" | "node" | "nodejs" => self.execute_nodejs(code, timeout).await,
            "shell" | "sh" | "bash" => self.execute_shell(code, timeout).await,
            _ => {
                error!("Unsupported language: {}", lang);
                Err(crate::error::SoulBoxError::RuntimeError(
                    format!("Unsupported language: {}", lang)
                ))
            }
        }
    }
}

/// Result of code execution
#[derive(Debug, Clone)]
pub struct CodeExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub execution_time: Duration,
    pub language: String,
}

impl CodeExecutionResult {
    /// Check if execution was successful
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// Get combined output (stdout + stderr)
    pub fn output(&self) -> String {
        if self.stderr.is_empty() {
            self.stdout.clone()
        } else if self.stdout.is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_result() {
        let result = CodeExecutionResult {
            stdout: "Hello World".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
            execution_time: Duration::from_secs(1),
            language: "python".to_string(),
        };

        assert!(result.is_success());
        assert_eq!(result.output(), "Hello World");
    }

    #[test]
    fn test_execution_result_with_error() {
        let result = CodeExecutionResult {
            stdout: "".to_string(),
            stderr: "Error: Division by zero".to_string(),
            exit_code: 1,
            execution_time: Duration::from_millis(100),
            language: "python".to_string(),
        };

        assert!(!result.is_success());
        assert_eq!(result.output(), "Error: Division by zero");
    }
}