//! List sandboxes command implementation
//! 
//! Lists all sandboxes with filtering and formatting options.

use clap::Args;
use colored::*;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info};

use crate::cli::{Cli, CliConfig};
use crate::error::Result;

#[derive(Args)]
pub struct ListArgs {
    /// Show all sandboxes (including stopped ones)
    #[arg(short, long)]
    pub all: bool,

    /// Filter by status (running, stopped, error)
    #[arg(long)]
    pub status: Option<String>,

    /// Filter by template/image
    #[arg(long)]
    pub template: Option<String>,

    /// Output format (table, json, yaml)
    #[arg(short, long)]
    pub output: Option<String>,

    /// Show detailed information
    #[arg(long)]
    pub verbose: bool,

    /// Sort by field (name, created, status, memory)
    #[arg(long, default_value = "created")]
    pub sort: String,

    /// Reverse sort order
    #[arg(long)]
    pub reverse: bool,

    /// Limit number of results
    #[arg(long)]
    pub limit: Option<usize>,
}

pub async fn run(args: ListArgs, config: &CliConfig) -> Result<()> {
    info!("Fetching sandbox list...");

    // Validate sort field
    match args.sort.as_str() {
        "name" | "created" | "status" | "memory" | "cpu" => {}
        _ => {
            Cli::error_exit("Invalid sort field. Must be one of: name, created, status, memory, cpu");
        }
    }

    // Validate status filter
    if let Some(ref status) = args.status {
        match status.to_lowercase().as_str() {
            "running" | "stopped" | "error" | "starting" | "stopping" => {}
            _ => {
                Cli::error_exit("Invalid status. Must be one of: running, stopped, error, starting, stopping");
            }
        }
    }

    // Fetch sandboxes
    let mut sandboxes = fetch_sandboxes(&args, config).await?;

    // Apply filters
    if !args.all {
        sandboxes.retain(|s| s.status != "stopped");
    }

    if let Some(ref status_filter) = args.status {
        sandboxes.retain(|s| s.status.to_lowercase() == status_filter.to_lowercase());
    }

    if let Some(ref template_filter) = args.template {
        sandboxes.retain(|s| s.template.to_lowercase().contains(&template_filter.to_lowercase()) ||
                               s.image.to_lowercase().contains(&template_filter.to_lowercase()));
    }

    // Sort sandboxes
    sort_sandboxes(&mut sandboxes, &args.sort, args.reverse);

    // Apply limit
    if let Some(limit) = args.limit {
        sandboxes.truncate(limit);
    }

    // Display results
    let output_format = args.output.as_deref().unwrap_or(&config.output_format);
    match output_format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&sandboxes)?);
        }
        "yaml" => {
            // Simple YAML output (would use serde_yaml in real implementation)
            for sandbox in &sandboxes {
                println!("- id: {}", sandbox.id);
                println!("  name: {}", sandbox.name);
                println!("  status: {}", sandbox.status);
                println!("  image: {}", sandbox.image);
                println!("  created: {}", sandbox.created_at.format("%Y-%m-%d %H:%M:%S"));
                println!();
            }
        }
        "table" | _ => {
            display_table(&sandboxes, args.verbose)?;
        }
    }

    // Show summary
    if output_format == "table" {
        println!();
        let running_count = sandboxes.iter().filter(|s| s.status == "running").count();
        let stopped_count = sandboxes.iter().filter(|s| s.status == "stopped").count();
        let error_count = sandboxes.iter().filter(|s| s.status == "error").count();

        println!("{}: {} | {}: {} | {}: {}",
                 "Running".green(), running_count,
                 "Stopped".yellow(), stopped_count,
                 "Error".red(), error_count);
    }

    Ok(())
}

async fn fetch_sandboxes(_args: &ListArgs, _config: &CliConfig) -> Result<Vec<SandboxInfo>> {
    debug!("Fetching sandbox list from server");
    
    // Simulate API call
    sleep(Duration::from_millis(300)).await;

    // Generate sample sandbox data
    let sandboxes = vec![
        SandboxInfo {
            id: "sb_abc123456789".to_string(),
            name: "python-dev".to_string(),
            status: "running".to_string(),
            template: "python".to_string(),
            image: "python:3.11-slim".to_string(),
            created_at: chrono::Utc::now() - chrono::Duration::hours(2),
            updated_at: chrono::Utc::now() - chrono::Duration::minutes(5),
            memory_usage: "128M".to_string(),
            cpu_usage: "0.2".to_string(),
            uptime: "2h 15m".to_string(),
            environment: {
                let mut env = HashMap::new();
                env.insert("PYTHON_PATH".to_string(), "/usr/local/bin/python".to_string());
                env
            },
            ports: vec!["8080:80".to_string()],
            working_directory: "/workspace".to_string(),
        },
        SandboxInfo {
            id: "sb_def987654321".to_string(),
            name: "node-api".to_string(),
            status: "running".to_string(),
            template: "node".to_string(),
            image: "node:18-alpine".to_string(),
            created_at: chrono::Utc::now() - chrono::Duration::hours(1),
            updated_at: chrono::Utc::now() - chrono::Duration::minutes(2),
            memory_usage: "89M".to_string(),
            cpu_usage: "0.1".to_string(),
            uptime: "1h 32m".to_string(),
            environment: HashMap::new(),
            ports: vec!["3000:3000".to_string()],
            working_directory: "/app".to_string(),
        },
        SandboxInfo {
            id: "sb_ghi555666777".to_string(),
            name: "rust-cli".to_string(),
            status: "stopped".to_string(),
            template: "rust".to_string(),
            image: "rust:1.75-slim".to_string(),
            created_at: chrono::Utc::now() - chrono::Duration::days(1),
            updated_at: chrono::Utc::now() - chrono::Duration::hours(3),
            memory_usage: "0M".to_string(),
            cpu_usage: "0.0".to_string(),
            uptime: "0m".to_string(),
            environment: HashMap::new(),
            ports: vec![],
            working_directory: "/workspace".to_string(),
        },
        SandboxInfo {
            id: "sb_jkl888999000".to_string(),
            name: "debug-env".to_string(),
            status: "error".to_string(),
            template: "ubuntu".to_string(),
            image: "ubuntu:22.04".to_string(),
            created_at: chrono::Utc::now() - chrono::Duration::minutes(30),
            updated_at: chrono::Utc::now() - chrono::Duration::minutes(28),
            memory_usage: "0M".to_string(),
            cpu_usage: "0.0".to_string(),
            uptime: "0m".to_string(),
            environment: HashMap::new(),
            ports: vec![],
            working_directory: "/root".to_string(),
        },
    ];

    Ok(sandboxes)
}

fn sort_sandboxes(sandboxes: &mut Vec<SandboxInfo>, sort_by: &str, reverse: bool) {
    match sort_by {
        "name" => sandboxes.sort_by(|a, b| a.name.cmp(&b.name)),
        "created" => sandboxes.sort_by(|a, b| a.created_at.cmp(&b.created_at)),
        "status" => sandboxes.sort_by(|a, b| a.status.cmp(&b.status)),
        "memory" => sandboxes.sort_by(|a, b| {
            let a_mem = parse_memory(&a.memory_usage);
            let b_mem = parse_memory(&b.memory_usage);
            a_mem.cmp(&b_mem)
        }),
        "cpu" => sandboxes.sort_by(|a, b| {
            let a_cpu: f64 = a.cpu_usage.parse().unwrap_or(0.0);
            let b_cpu: f64 = b.cpu_usage.parse().unwrap_or(0.0);
            a_cpu.partial_cmp(&b_cpu).unwrap_or(std::cmp::Ordering::Equal)
        }),
        _ => {}
    }

    if reverse {
        sandboxes.reverse();
    }
}

fn parse_memory(memory_str: &str) -> u64 {
    let memory_str = memory_str.trim_end_matches('M').trim_end_matches('G');
    let base: u64 = memory_str.parse().unwrap_or(0);
    
    if memory_str.ends_with('G') {
        base * 1024
    } else {
        base
    }
}

fn display_table(sandboxes: &[SandboxInfo], verbose: bool) -> Result<()> {
    if sandboxes.is_empty() {
        println!("{}", "No sandboxes found".yellow());
        return Ok(());
    }

    // Print header
    if verbose {
        println!("{:20} {:15} {:10} {:15} {:20} {:10} {:8} {:8}",
                 "ID".bold(),
                 "NAME".bold(),
                 "STATUS".bold(),
                 "TEMPLATE".bold(),
                 "IMAGE".bold(),
                 "CREATED".bold(),
                 "MEMORY".bold(),
                 "CPU".bold());
    } else {
        println!("{:20} {:15} {:10} {:15} {:10} {:8}",
                 "ID".bold(),
                 "NAME".bold(),
                 "STATUS".bold(),
                 "TEMPLATE".bold(),
                 "CREATED".bold(),
                 "MEMORY".bold());
    }
    
    println!("{}", "─".repeat(if verbose { 110 } else { 85 }));

    // Print sandbox rows
    for sandbox in sandboxes {
        let status_colored = match sandbox.status.as_str() {
            "running" => sandbox.status.green(),
            "stopped" => sandbox.status.yellow(),
            "error" => sandbox.status.red(),
            "starting" => sandbox.status.blue(),
            _ => sandbox.status.normal(),
        };

        let created_str = format_duration_ago(sandbox.created_at);
        
        if verbose {
            println!("{:20} {:15} {:10} {:15} {:20} {:10} {:8} {:8}",
                     &sandbox.id[..20],
                     sandbox.name,
                     status_colored,
                     sandbox.template,
                     &truncate_string(&sandbox.image, 20),
                     created_str,
                     sandbox.memory_usage,
                     sandbox.cpu_usage);
        } else {
            println!("{:20} {:15} {:10} {:15} {:10} {:8}",
                     &sandbox.id[..20],
                     sandbox.name,
                     status_colored,
                     sandbox.template,
                     created_str,
                     sandbox.memory_usage);
        }
    }

    Ok(())
}

fn format_duration_ago(timestamp: chrono::DateTime<chrono::Utc>) -> String {
    let duration = chrono::Utc::now() - timestamp;
    
    if duration.num_days() > 0 {
        format!("{}d", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{}h", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{}m", duration.num_minutes())
    } else {
        "now".to_string()
    }
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len-3])
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SandboxInfo {
    id: String,
    name: String,
    status: String,
    template: String,
    image: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    memory_usage: String,
    cpu_usage: String,
    uptime: String,
    environment: HashMap<String, String>,
    ports: Vec<String>,
    working_directory: String,
}