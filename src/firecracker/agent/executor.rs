use anyhow::{Result, Context};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tracing::{info, warn, error, debug};

use super::protocol::{
    ExecutionRequest, ExecutionResponse, ExecutionStatus, 
    StreamEvent, StreamType
};

/// 代码执行器（在 VM 内部运行）
pub struct CodeExecutor {
    /// 临时文件目录
    temp_dir: String,
}

impl CodeExecutor {
    /// 创建新的代码执行器
    pub fn new() -> Self {
        Self {
            temp_dir: "/tmp/soulbox".to_string(),
        }
    }

    /// 执行代码
    pub async fn execute(&self, request: ExecutionRequest) -> ExecutionResponse {
        let start_time = Instant::now();
        
        match self.execute_internal(request.clone()).await {
            Ok((stdout, stderr, exit_code)) => ExecutionResponse {
                execution_id: request.execution_id,
                status: ExecutionStatus::Completed,
                stdout,
                stderr,
                exit_code: Some(exit_code),
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: None,
            },
            Err(e) => ExecutionResponse {
                execution_id: request.execution_id,
                status: ExecutionStatus::Failed,
                stdout: String::new(),
                stderr: String::new(),
                exit_code: None,
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some(e.to_string()),
            },
        }
    }

    /// 流式执行代码
    pub async fn execute_stream(
        &self,
        request: ExecutionRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<ExecutionResponse> {
        let start_time = Instant::now();
        
        // 准备执行环境
        let (command, args, code_file) = self.prepare_execution(&request).await?;
        
        // 创建子进程
        let mut child = Command::new(&command)
            .args(&args)
            .envs(&request.env_vars)
            .current_dir(request.working_dir.as_deref().unwrap_or("/tmp"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to spawn process")?;

        let stdout = child.stdout.take().expect("Failed to get stdout");
        let stderr = child.stderr.take().expect("Failed to get stderr");

        // 创建输出读取任务
        let execution_id = request.execution_id.clone();
        let tx_stdout = tx.clone();
        let stdout_reader = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx_stdout.send(StreamEvent::Output {
                    execution_id: execution_id.clone(),
                    stream_type: StreamType::Stdout,
                    data: format!("{}\n", line),
                }).await;
            }
        });

        let execution_id = request.execution_id.clone();
        let tx_stderr = tx.clone();
        let stderr_reader = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = tx_stderr.send(StreamEvent::Output {
                    execution_id: execution_id.clone(),
                    stream_type: StreamType::Stderr,
                    data: format!("{}\n", line),
                }).await;
            }
        });

        // 等待进程完成或超时
        let timeout = Duration::from_secs(request.timeout_seconds);
        let wait_result = tokio::time::timeout(timeout, child.wait()).await;

        let (exit_code, status) = match wait_result {
            Ok(Ok(exit_status)) => {
                let code = exit_status.code().unwrap_or(-1);
                (code, ExecutionStatus::Completed)
            }
            Ok(Err(e)) => {
                error!("Process wait error: {}", e);
                (-1, ExecutionStatus::Failed)
            }
            Err(_) => {
                // 超时，终止进程
                let _ = child.kill().await;
                (-1, ExecutionStatus::TimedOut)
            }
        };

        // 等待输出读取完成
        let _ = tokio::join!(stdout_reader, stderr_reader);

        // 清理临时文件
        if let Some(file_path) = code_file {
            let _ = tokio::fs::remove_file(&file_path).await;
        }

        Ok(ExecutionResponse {
            execution_id: request.execution_id,
            status,
            stdout: String::new(), // 已通过流发送
            stderr: String::new(), // 已通过流发送
            exit_code: Some(exit_code),
            execution_time_ms: start_time.elapsed().as_millis() as u64,
            error: None,
        })
    }

    /// 内部执行逻辑
    async fn execute_internal(&self, request: ExecutionRequest) -> Result<(String, String, i32)> {
        // 准备执行环境
        let (command, args, code_file) = self.prepare_execution(&request).await?;
        
        // 执行命令
        let output = tokio::time::timeout(
            Duration::from_secs(request.timeout_seconds),
            Command::new(&command)
                .args(&args)
                .envs(&request.env_vars)
                .current_dir(request.working_dir.as_deref().unwrap_or("/tmp"))
                .output()
        )
        .await
        .context("Execution timeout")?
        .context("Failed to execute command")?;

        // 清理临时文件
        if let Some(file_path) = code_file {
            let _ = tokio::fs::remove_file(&file_path).await;
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok((stdout, stderr, exit_code))
    }

    /// 准备执行环境
    async fn prepare_execution(&self, request: &ExecutionRequest) -> Result<(String, Vec<String>, Option<String>)> {
        // 确保临时目录存在
        tokio::fs::create_dir_all(&self.temp_dir).await?;

        match request.language.as_str() {
            "python" | "python3" => {
                let file_path = format!("{}/{}.py", self.temp_dir, request.execution_id);
                tokio::fs::write(&file_path, &request.code).await?;
                Ok(("python3".to_string(), vec![file_path.clone()], Some(file_path)))
            }
            "javascript" | "node" | "nodejs" => {
                let file_path = format!("{}/{}.js", self.temp_dir, request.execution_id);
                tokio::fs::write(&file_path, &request.code).await?;
                Ok(("node".to_string(), vec![file_path.clone()], Some(file_path)))
            }
            "typescript" | "ts" => {
                let file_path = format!("{}/{}.ts", self.temp_dir, request.execution_id);
                tokio::fs::write(&file_path, &request.code).await?;
                Ok(("ts-node".to_string(), vec![file_path.clone()], Some(file_path)))
            }
            "rust" => {
                let file_path = format!("{}/{}.rs", self.temp_dir, request.execution_id);
                let exe_path = format!("{}/{}", self.temp_dir, request.execution_id);
                tokio::fs::write(&file_path, &request.code).await?;
                
                // 编译 Rust 代码
                let compile = Command::new("rustc")
                    .args(&[&file_path, "-o", &exe_path])
                    .output()
                    .await?;
                
                if !compile.status.success() {
                    let stderr = String::from_utf8_lossy(&compile.stderr);
                    return Err(anyhow::anyhow!("Compilation failed: {}", stderr));
                }
                
                let _ = tokio::fs::remove_file(&file_path).await;
                Ok((exe_path.clone(), vec![], Some(exe_path)))
            }
            "go" | "golang" => {
                let file_path = format!("{}/{}.go", self.temp_dir, request.execution_id);
                tokio::fs::write(&file_path, &request.code).await?;
                Ok(("go".to_string(), vec!["run".to_string(), file_path.clone()], Some(file_path)))
            }
            "ruby" => {
                let file_path = format!("{}/{}.rb", self.temp_dir, request.execution_id);
                tokio::fs::write(&file_path, &request.code).await?;
                Ok(("ruby".to_string(), vec![file_path.clone()], Some(file_path)))
            }
            "java" => {
                let file_path = format!("{}/Main.java", self.temp_dir);
                tokio::fs::write(&file_path, &request.code).await?;
                
                // 编译 Java 代码
                let compile = Command::new("javac")
                    .args(&[&file_path])
                    .current_dir(&self.temp_dir)
                    .output()
                    .await?;
                
                if !compile.status.success() {
                    let stderr = String::from_utf8_lossy(&compile.stderr);
                    return Err(anyhow::anyhow!("Compilation failed: {}", stderr));
                }
                
                Ok(("java".to_string(), vec!["-cp".to_string(), self.temp_dir.clone(), "Main".to_string()], Some(file_path)))
            }
            "bash" | "sh" => {
                Ok(("bash".to_string(), vec!["-c".to_string(), request.code.clone()], None))
            }
            _ => Err(anyhow::anyhow!("Unsupported language: {}", request.language)),
        }
    }
}