//! Logs command implementation
//! 
//! Views and streams logs from sandbox instances with filtering capabilities.

use clap::Args;
use colored::*;
use std::time::Duration;
use tokio::time::{interval, sleep};
use tracing::{debug, info};

use crate::cli::{Cli, CliConfig};
use crate::error::Result;

#[derive(Args)]
pub struct LogsArgs {
    /// Sandbox ID to view logs for
    pub sandbox_id: String,

    /// Follow log output (stream new logs)
    #[arg(short, long)]
    pub follow: bool,

    /// Number of lines to show from the end
    #[arg(short, long, default_value = "100")]
    pub tail: usize,

    /// Show timestamps
    #[arg(short, long)]
    pub timestamps: bool,

    /// Filter logs by level (debug, info, warn, error)
    #[arg(long)]
    pub level: Option<String>,

    /// Filter logs containing this text
    #[arg(long)]
    pub filter: Option<String>,

    /// Output format (text, json)
    #[arg(long)]
    pub output: Option<String>,

    /// Show logs since this time (e.g., "1h", "30m", "1d")
    #[arg(long)]
    pub since: Option<String>,

    /// Show logs until this time
    #[arg(long)]
    pub until: Option<String>,

    /// Only show container logs (not SoulBox system logs)
    #[arg(long)]
    pub container_only: bool,
}

pub async fn run(args: LogsArgs, config: &CliConfig) -> Result<()> {
    info!("Fetching logs for sandbox: {}", args.sandbox_id);

    // Validate log level filter
    if let Some(ref level) = args.level {
        match level.to_lowercase().as_str() {
            "debug" | "info" | "warn" | "error" => {}
            _ => {
                Cli::error_exit("Invalid log level. Must be one of: debug, info, warn, error");
            }
        }
    }

    // Show header information
    if args.output.as_deref() != Some("json") {
        println!("{}", format!("Logs for sandbox: {}", args.sandbox_id).bold());
        
        let mut filters = vec![];
        if let Some(ref level) = args.level {
            filters.push(format!("level={}", level));
        }
        if let Some(ref filter_text) = args.filter {
            filters.push(format!("contains='{}'", filter_text));
        }
        if let Some(ref since) = args.since {
            filters.push(format!("since={}", since));
        }
        if args.container_only {
            filters.push("container-only".to_string());
        }
        
        if !filters.is_empty() {
            println!("{} {}", "Filters:".bold(), filters.join(", "));
        }
        
        println!("{} {}", "Tail:".bold(), args.tail);
        println!("{} {}", "Follow:".bold(), if args.follow { "Yes" } else { "No" });
        println!("{}", "─".repeat(80));
    }

    // Fetch initial logs
    let mut logs = fetch_logs(&args, config).await?;
    
    // Apply tail limit
    if logs.len() > args.tail {
        logs = logs[logs.len() - args.tail..].to_vec();
    }

    // Display logs
    display_logs(&logs, &args)?;

    // Follow mode: stream new logs
    if args.follow {
        let mut last_log_id = logs.last().map(|log| log.id.clone()).unwrap_or_default();
        
        if args.output.as_deref() != Some("json") {
            println!("{}", "─".repeat(80));
            println!("{}", "Following new logs... (Press Ctrl+C to stop)".green());
            println!("{}", "─".repeat(80));
        }

        let mut ticker = interval(Duration::from_secs(1));
        
        loop {
            ticker.tick().await;
            
            let new_logs = fetch_new_logs(&args.sandbox_id, &last_log_id, config).await?;
            
            if !new_logs.is_empty() {
                display_logs(&new_logs, &args)?;
                last_log_id = new_logs.last().unwrap().id.clone();
            }
        }
    }

    Ok(())
}

async fn fetch_logs(args: &LogsArgs, _config: &CliConfig) -> Result<Vec<LogEntry>> {
    debug!("Fetching logs for sandbox: {}", args.sandbox_id);
    
    // Simulate fetching logs (replace with actual API call)
    sleep(Duration::from_millis(200)).await;
    
    let mut logs = vec![
        LogEntry {
            id: "1".to_string(),
            timestamp: chrono::Utc::now() - chrono::Duration::minutes(5),
            level: "info".to_string(),
            source: "system".to_string(),
            message: format!("Sandbox {} started", args.sandbox_id),
        },
        LogEntry {
            id: "2".to_string(),
            timestamp: chrono::Utc::now() - chrono::Duration::minutes(4),
            level: "info".to_string(),
            source: "container".to_string(),
            message: "Container initialization completed".to_string(),
        },
        LogEntry {
            id: "3".to_string(),
            timestamp: chrono::Utc::now() - chrono::Duration::minutes(3),
            level: "debug".to_string(),
            source: "system".to_string(),
            message: "Resource limits applied: memory=512M, cpu=1.0".to_string(),
        },
        LogEntry {
            id: "4".to_string(),
            timestamp: chrono::Utc::now() - chrono::Duration::minutes(2),
            level: "info".to_string(),
            source: "container".to_string(),
            message: "Ready to accept code execution requests".to_string(),
        },
        LogEntry {
            id: "5".to_string(),
            timestamp: chrono::Utc::now() - chrono::Duration::minutes(1),
            level: "info".to_string(),
            source: "execution".to_string(),
            message: "Code execution started: Python script".to_string(),
        },
        LogEntry {
            id: "6".to_string(),
            timestamp: chrono::Utc::now(),
            level: "info".to_string(),
            source: "execution".to_string(),
            message: "Code execution completed successfully (exit code: 0)".to_string(),
        },
    ];

    // Apply filters
    if let Some(ref level_filter) = args.level {
        let target_level = level_filter.to_lowercase();
        logs.retain(|log| log.level == target_level);
    }

    if let Some(ref text_filter) = args.filter {
        logs.retain(|log| log.message.to_lowercase().contains(&text_filter.to_lowercase()));
    }

    if args.container_only {
        logs.retain(|log| log.source == "container" || log.source == "execution");
    }

    Ok(logs)
}

async fn fetch_new_logs(sandbox_id: &str, since_id: &str, _config: &CliConfig) -> Result<Vec<LogEntry>> {
    debug!("Fetching new logs since ID: {}", since_id);
    
    // Simulate occasional new logs
    if rand::random::<f32>() < 0.3 {
        let log = LogEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            level: "info".to_string(),
            source: "system".to_string(),
            message: format!("Heartbeat from sandbox {}", sandbox_id),
        };
        Ok(vec![log])
    } else {
        Ok(vec![])
    }
}

fn display_logs(logs: &[LogEntry], args: &LogsArgs) -> Result<()> {
    match args.output.as_deref().unwrap_or("text") {
        "json" => {
            for log in logs {
                println!("{}", serde_json::to_string(log)?);
            }
        }
        "text" | _ => {
            for log in logs {
                let timestamp_str = if args.timestamps {
                    format!("{} ", log.timestamp.format("%Y-%m-%d %H:%M:%S"))
                } else {
                    String::new()
                };

                let level_colored = match log.level.as_str() {
                    "error" => log.level.red().bold(),
                    "warn" => log.level.yellow().bold(),
                    "info" => log.level.green(),
                    "debug" => log.level.cyan(),
                    _ => log.level.normal(),
                };

                let source_colored = match log.source.as_str() {
                    "system" => log.source.blue(),
                    "container" => log.source.magenta(),
                    "execution" => log.source.green(),
                    _ => log.source.normal(),
                };

                println!(
                    "{}{:5} {:10} {}",
                    timestamp_str.bright_black(),
                    level_colored,
                    format!("[{}]", source_colored),
                    log.message
                );
            }
        }
    }
    
    Ok(())
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct LogEntry {
    id: String,
    timestamp: chrono::DateTime<chrono::Utc>,
    level: String,
    source: String,
    message: String,
}