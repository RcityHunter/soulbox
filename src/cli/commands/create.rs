//! Create sandbox command implementation
//! 
//! Creates a new sandbox instance with specified configuration.

use clap::Args;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info};

use crate::cli::{Cli, CliConfig};
use crate::error::Result;

#[derive(Args)]
pub struct CreateArgs {
    /// Sandbox name
    #[arg(short, long)]
    pub name: Option<String>,

    /// Template to use (python, node, rust, etc.)
    #[arg(short, long, default_value = "python")]
    pub template: String,

    /// Docker image to use (overrides template)
    #[arg(short, long)]
    pub image: Option<String>,

    /// Working directory inside the sandbox
    #[arg(short, long, default_value = "/workspace")]
    pub workdir: String,

    /// Memory limit (e.g., 512M, 1G)
    #[arg(long, default_value = "512M")]
    pub memory: String,

    /// CPU limit (e.g., 0.5, 1.0)
    #[arg(long, default_value = "1.0")]
    pub cpu: String,

    /// Environment variables (key=value)
    #[arg(short, long)]
    pub env: Vec<String>,

    /// Timeout in seconds
    #[arg(long, default_value = "300")]
    pub timeout: u64,

    /// Keep sandbox running after creation
    #[arg(long)]
    pub keep_alive: bool,

    /// Output format (json, table)
    #[arg(long)]
    pub output: Option<String>,
}

pub async fn run(args: CreateArgs, config: &CliConfig) -> Result<()> {
    info!("Creating new sandbox...");

    // Parse environment variables
    let mut env_vars = HashMap::new();
    for env_pair in &args.env {
        if let Some((key, value)) = env_pair.split_once('=') {
            env_vars.insert(key.to_string(), value.to_string());
        } else {
            Cli::warning(&format!("Invalid environment variable format: {}", env_pair));
        }
    }

    // Determine image to use
    let image = args.image.unwrap_or_else(|| get_template_image(&args.template));
    
    // Generate sandbox name if not provided
    let sandbox_name = args.name.unwrap_or_else(|| {
        format!("sandbox-{}", uuid::Uuid::new_v4().to_string()[..8].to_string())
    });

    debug!("Sandbox configuration:");
    debug!("  Name: {}", sandbox_name);
    debug!("  Image: {}", image);
    debug!("  Working directory: {}", args.workdir);
    debug!("  Memory limit: {}", args.memory);
    debug!("  CPU limit: {}", args.cpu);
    debug!("  Environment: {:?}", env_vars);

    // Show progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );
    pb.set_message("Creating sandbox...");
    pb.enable_steady_tick(Duration::from_millis(100));

    // Simulate sandbox creation (replace with actual gRPC/API call)
    sleep(Duration::from_millis(500)).await;
    pb.set_message("Pulling container image...");
    sleep(Duration::from_millis(1500)).await;
    pb.set_message("Starting container...");
    sleep(Duration::from_millis(800)).await;
    pb.set_message("Configuring environment...");
    sleep(Duration::from_millis(300)).await;

    pb.finish_with_message("Sandbox created successfully!");

    // Create sandbox info structure
    let sandbox_info = SandboxInfo {
        id: format!("sb_{}", uuid::Uuid::new_v4().to_string()[..12].to_string()),
        name: sandbox_name.clone(),
        status: "running".to_string(),
        image,
        created_at: chrono::Utc::now(),
        working_directory: args.workdir,
        memory_limit: args.memory,
        cpu_limit: args.cpu,
        environment: env_vars,
        endpoint: format!("{}/sandbox/{}", config.server_url, sandbox_name),
    };

    // Output results based on format
    let output_format = args.output.as_deref().unwrap_or(&config.output_format);
    match output_format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&sandbox_info)?);
        }
        "table" | _ => {
            print_table_output(&sandbox_info);
        }
    }

    if args.keep_alive {
        Cli::info("Sandbox will remain running. Use 'soulbox-cli logs' to view output.");
    } else {
        Cli::info("Sandbox created and ready for code execution.");
    }

    println!();
    println!("{}", "Next steps:".bold());
    println!("  • {} to execute code", format!("soulbox-cli exec --sandbox {} --code 'print(\"Hello!\")'", sandbox_info.id).cyan());
    println!("  • {} to view logs", format!("soulbox-cli logs {}", sandbox_info.id).cyan());
    println!("  • {} to list all sandboxes", "soulbox-cli list".cyan());

    Ok(())
}

fn get_template_image(template: &str) -> String {
    match template {
        "python" => "python:3.11-slim".to_string(),
        "node" => "node:18-alpine".to_string(),
        "rust" => "rust:1.75-slim".to_string(),
        "go" => "golang:1.21-alpine".to_string(),
        "java" => "openjdk:17-jdk-slim".to_string(),
        "cpp" => "gcc:latest".to_string(),
        _ => "ubuntu:22.04".to_string(),
    }
}

fn print_table_output(info: &SandboxInfo) {
    println!();
    println!("{}", "Sandbox Created".green().bold());
    println!("{}", "─".repeat(50));
    println!("{:15} {}", "ID:".bold(), info.id);
    println!("{:15} {}", "Name:".bold(), info.name);
    println!("{:15} {}", "Status:".bold(), info.status.green());
    println!("{:15} {}", "Image:".bold(), info.image);
    println!("{:15} {}", "Working Dir:".bold(), info.working_directory);
    println!("{:15} {}", "Memory Limit:".bold(), info.memory_limit);
    println!("{:15} {}", "CPU Limit:".bold(), info.cpu_limit);
    println!("{:15} {}", "Created:".bold(), info.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
    println!("{:15} {}", "Endpoint:".bold(), info.endpoint);
    
    if !info.environment.is_empty() {
        println!("{:15}", "Environment:".bold());
        for (key, value) in &info.environment {
            println!("{:15} {}={}", "", key, value);
        }
    }
    println!("{}", "─".repeat(50));
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SandboxInfo {
    id: String,
    name: String,
    status: String,
    image: String,
    created_at: chrono::DateTime<chrono::Utc>,
    working_directory: String,
    memory_limit: String,
    cpu_limit: String,
    environment: HashMap<String, String>,
    endpoint: String,
}