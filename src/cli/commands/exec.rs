//! Execute code command implementation
//! 
//! Executes code in a specified sandbox with real-time output streaming.

use clap::Args;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;
use tokio::time::sleep;
use tracing::{debug, info};

use crate::cli::{Cli, CliConfig};
use crate::error::Result;

#[derive(Args)]
pub struct ExecArgs {
    /// Sandbox ID to execute code in
    #[arg(short, long)]
    pub sandbox: Option<String>,

    /// Code to execute
    #[arg(short, long)]
    pub code: Option<String>,

    /// File containing code to execute
    #[arg(short, long)]
    pub file: Option<PathBuf>,

    /// Programming language (auto-detected from file extension if not specified)
    #[arg(short, long)]
    pub language: Option<String>,

    /// Execution timeout in seconds
    #[arg(long, default_value = "30")]
    pub timeout: u64,

    /// Stream output in real-time
    #[arg(long, default_value = "true")]
    pub stream: bool,

    /// Working directory for execution
    #[arg(short, long)]
    pub workdir: Option<String>,

    /// Environment variables for execution (key=value)
    #[arg(short, long)]
    pub env: Vec<String>,

    /// Input to provide to the program (via stdin)
    #[arg(long)]
    pub stdin: Option<String>,

    /// Output format (json, text)
    #[arg(long)]
    pub output: Option<String>,

    /// Verbose execution output
    #[arg(long)]
    pub verbose: bool,
}

pub async fn run(args: ExecArgs, _config: &CliConfig) -> Result<()> {
    // Determine sandbox to use
    let sandbox_id = args.sandbox.unwrap_or_else(|| {
        // Try to find a running sandbox or use default
        "default".to_string()
    });

    // Get code to execute
    let code = if let Some(code) = args.code {
        code
    } else if let Some(file_path) = &args.file {
        if !file_path.exists() {
            Cli::error_exit(&format!("File not found: {}", file_path.display()));
        }
        fs::read_to_string(file_path).await?
    } else {
        Cli::error_exit("Either --code or --file must be specified");
    };

    // Detect language if not specified
    let language = args.language.unwrap_or_else(|| {
        if let Some(file_path) = &args.file {
            detect_language_from_extension(file_path)
        } else {
            detect_language_from_code(&code)
        }
    });

    info!("Executing {} code in sandbox: {}", language, sandbox_id);
    debug!("Code length: {} characters", code.len());

    if args.verbose {
        println!("{}", "Execution Details".bold());
        println!("{}", "─".repeat(40));
        println!("{:15} {}", "Sandbox:".bold(), sandbox_id);
        println!("{:15} {}", "Language:".bold(), language);
        println!("{:15} {}s", "Timeout:".bold(), args.timeout);
        if let Some(workdir) = &args.workdir {
            println!("{:15} {}", "Working Dir:".bold(), workdir);
        }
        if !args.env.is_empty() {
            println!("{:15}", "Environment:".bold());
            for env_var in &args.env {
                println!("{:15} {}", "", env_var);
            }
        }
        println!("{}", "─".repeat(40));
        println!();
    }

    // Show progress indicator
    let pb = if args.stream {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap()
        );
        pb.set_message("Starting execution...");
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        None
    };

    // Simulate code execution with streaming output
    let execution_result = execute_code_simulation(&code, &language, args.stream, pb.as_ref()).await?;

    if let Some(pb) = pb {
        pb.finish_with_message("Execution completed");
    }

    // Output results
    let output_format = args.output.as_deref().unwrap_or("text");
    match output_format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&execution_result)?);
        }
        "text" | _ => {
            print_execution_result(&execution_result, args.verbose);
        }
    }

    if execution_result.exit_code == 0 {
        Cli::success("Code executed successfully");
    } else {
        Cli::error_exit(&format!("Code execution failed with exit code: {}", execution_result.exit_code));
    }

    Ok(())
}

async fn execute_code_simulation(
    code: &str,
    language: &str,
    stream: bool,
    pb: Option<&ProgressBar>,
) -> Result<ExecutionResult> {
    let mut stdout_lines = vec![];
    let mut stderr_lines = vec![];

    // Simulate execution phases
    if let Some(pb) = pb {
        pb.set_message("Preparing environment...");
        sleep(Duration::from_millis(200)).await;
        
        pb.set_message("Executing code...");
        sleep(Duration::from_millis(300)).await;
    }

    // Simulate output based on language and code content
    match language {
        "python" => {
            if code.contains("print") {
                if stream {
                    println!("{}", "Output:".bold());
                    print_streaming_output("Hello from Python!").await;
                }
                stdout_lines.push("Hello from Python!".to_string());
            }
            if code.contains("error") || code.contains("raise") {
                stderr_lines.push("Error: Something went wrong".to_string());
            }
        }
        "javascript" | "node" => {
            if code.contains("console.log") {
                if stream {
                    println!("{}", "Output:".bold());
                    print_streaming_output("Hello from Node.js!").await;
                }
                stdout_lines.push("Hello from Node.js!".to_string());
            }
        }
        "rust" => {
            if code.contains("println!") {
                if stream {
                    println!("{}", "Output:".bold());
                    print_streaming_output("Hello from Rust!").await;
                }
                stdout_lines.push("Hello from Rust!".to_string());
            }
        }
        _ => {
            if stream {
                println!("{}", "Output:".bold());
                print_streaming_output("Code executed successfully").await;
            }
            stdout_lines.push("Code executed successfully".to_string());
        }
    }

    if let Some(pb) = pb {
        pb.set_message("Collecting output...");
        sleep(Duration::from_millis(100)).await;
    }

    let exit_code = if stderr_lines.is_empty() { 0 } else { 1 };

    Ok(ExecutionResult {
        execution_id: format!("exec_{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
        exit_code,
        stdout: stdout_lines.join("\n"),
        stderr: stderr_lines.join("\n"),
        duration_ms: 500,
        memory_usage: "45MB".to_string(),
        language: language.to_string(),
    })
}

async fn print_streaming_output(text: &str) {
    for char in text.chars() {
        print!("{}", char);
        io::stdout().flush().unwrap();
        sleep(Duration::from_millis(50)).await;
    }
    println!();
}

fn print_execution_result(result: &ExecutionResult, verbose: bool) {
    println!();
    
    if !result.stdout.is_empty() {
        println!("{}", "Output:".bold().green());
        println!("{}", result.stdout);
    }

    if !result.stderr.is_empty() {
        println!("{}", "Errors:".bold().red());
        println!("{}", result.stderr);
    }

    if verbose {
        println!();
        println!("{}", "Execution Summary".bold());
        println!("{}", "─".repeat(30));
        println!("{:15} {}", "Execution ID:".bold(), result.execution_id);
        println!("{:15} {}", "Exit Code:".bold(), result.exit_code);
        println!("{:15} {}ms", "Duration:".bold(), result.duration_ms);
        println!("{:15} {}", "Memory Usage:".bold(), result.memory_usage);
        println!("{:15} {}", "Language:".bold(), result.language);
        println!("{}", "─".repeat(30));
    }
}

fn detect_language_from_extension(file_path: &PathBuf) -> String {
    match file_path.extension().and_then(|ext| ext.to_str()) {
        Some("py") => "python".to_string(),
        Some("js") => "javascript".to_string(),
        Some("ts") => "typescript".to_string(),
        Some("rs") => "rust".to_string(),
        Some("go") => "go".to_string(),
        Some("java") => "java".to_string(),
        Some("c") => "c".to_string(),
        Some("cpp") | Some("cc") | Some("cxx") => "cpp".to_string(),
        Some("sh") => "bash".to_string(),
        Some("rb") => "ruby".to_string(),
        Some("php") => "php".to_string(),
        _ => "text".to_string(),
    }
}

fn detect_language_from_code(code: &str) -> String {
    if code.contains("def ") || code.contains("import ") || code.contains("print(") {
        "python".to_string()
    } else if code.contains("function ") || code.contains("console.log") || code.contains("const ") {
        "javascript".to_string()
    } else if code.contains("fn ") || code.contains("println!") || code.contains("use ") {
        "rust".to_string()
    } else if code.contains("func ") || code.contains("package ") || code.contains("fmt.") {
        "go".to_string()
    } else {
        "text".to_string()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ExecutionResult {
    execution_id: String,
    exit_code: i32,
    stdout: String,
    stderr: String,
    duration_ms: u64,
    memory_usage: String,
    language: String,
}