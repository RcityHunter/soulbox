//! System status monitoring CLI command
//! 
//! Provides real-time monitoring of SoulBox system status

use clap::Args;
use colored::*;
use prettytable::{Table, row, format};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use crate::cli::CliConfig;
use crate::error::Result;

#[derive(Args)]
pub struct StatusArgs {
    /// Show detailed system information
    #[arg(short, long)]
    pub detailed: bool,
    
    /// Watch status with auto-refresh (seconds)
    #[arg(short, long)]
    pub watch: Option<u64>,
    
    /// Show only specific component
    #[arg(short, long)]
    pub component: Option<String>,
    
    /// Output format (text, json, yaml)
    #[arg(short, long, default_value = "text")]
    pub format: String,
}

#[derive(Debug)]
struct SystemStatus {
    api_status: ServiceStatus,
    grpc_status: ServiceStatus,
    websocket_status: ServiceStatus,
    database_status: ServiceStatus,
    redis_status: ServiceStatus,
    docker_status: ServiceStatus,
    container_stats: ContainerStats,
    resource_usage: ResourceUsage,
}

#[derive(Debug)]
struct ServiceStatus {
    name: String,
    status: Status,
    latency_ms: Option<u64>,
    message: Option<String>,
}

#[derive(Debug)]
enum Status {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug)]
struct ContainerStats {
    total: usize,
    running: usize,
    stopped: usize,
    failed: usize,
    pool_size: usize,
    pool_available: usize,
}

#[derive(Debug)]
struct ResourceUsage {
    cpu_percent: f64,
    memory_mb: u64,
    memory_percent: f64,
    disk_gb: f64,
    disk_percent: f64,
    network_connections: usize,
}

pub async fn run(args: StatusArgs, config: &CliConfig) -> Result<()> {
    if let Some(interval) = args.watch {
        // Watch mode
        watch_status(config, interval, args.detailed).await
    } else {
        // Single status check
        show_status(config, args.detailed, &args.component, &args.format).await
    }
}

async fn show_status(
    _config: &CliConfig,
    detailed: bool,
    component: &Option<String>,
    format: &str,
) -> Result<()> {
    // Mock status data
    let status = get_system_status().await?;
    
    match format {
        "json" => print_json_status(&status),
        "yaml" => print_yaml_status(&status),
        _ => print_text_status(&status, detailed, component),
    }
    
    Ok(())
}

async fn watch_status(
    config: &CliConfig,
    interval: u64,
    detailed: bool,
) -> Result<()> {
    println!("{}", "Watching system status... (Press Ctrl+C to stop)".yellow());
    
    loop {
        // Clear screen
        print!("\x1B[2J\x1B[1;1H");
        
        show_status(config, detailed, &None, "text").await?;
        
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn get_system_status() -> Result<SystemStatus> {
    // Mock implementation
    Ok(SystemStatus {
        api_status: ServiceStatus {
            name: "REST API".to_string(),
            status: Status::Healthy,
            latency_ms: Some(12),
            message: None,
        },
        grpc_status: ServiceStatus {
            name: "gRPC Server".to_string(),
            status: Status::Healthy,
            latency_ms: Some(8),
            message: None,
        },
        websocket_status: ServiceStatus {
            name: "WebSocket".to_string(),
            status: Status::Healthy,
            latency_ms: Some(5),
            message: None,
        },
        database_status: ServiceStatus {
            name: "SurrealDB".to_string(),
            status: Status::Healthy,
            latency_ms: Some(3),
            message: None,
        },
        redis_status: ServiceStatus {
            name: "Redis".to_string(),
            status: Status::Healthy,
            latency_ms: Some(1),
            message: None,
        },
        docker_status: ServiceStatus {
            name: "Docker".to_string(),
            status: Status::Healthy,
            latency_ms: None,
            message: Some("Docker 24.0.7".to_string()),
        },
        container_stats: ContainerStats {
            total: 15,
            running: 8,
            stopped: 5,
            failed: 2,
            pool_size: 20,
            pool_available: 12,
        },
        resource_usage: ResourceUsage {
            cpu_percent: 23.5,
            memory_mb: 1024,
            memory_percent: 12.5,
            disk_gb: 5.2,
            disk_percent: 8.7,
            network_connections: 42,
        },
    })
}

fn print_text_status(status: &SystemStatus, detailed: bool, component: &Option<String>) {
    println!("\n{}", "SoulBox System Status".bold().cyan());
    println!("{}", "═".repeat(50));
    
    // Overall health
    let overall_health = calculate_overall_health(status);
    let health_str = format_health(&overall_health);
    println!("{}: {}", "Overall Health".bold(), health_str);
    println!();
    
    // Services table
    if component.is_none() || component.as_ref().map(|c| c == "services").unwrap_or(false) {
        println!("{}", "Services".bold());
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
        
        table.add_row(row![
            b->"Service",
            b->"Status",
            b->"Latency",
            b->"Details"
        ]);
        
        add_service_row(&mut table, &status.api_status);
        add_service_row(&mut table, &status.grpc_status);
        add_service_row(&mut table, &status.websocket_status);
        add_service_row(&mut table, &status.database_status);
        add_service_row(&mut table, &status.redis_status);
        add_service_row(&mut table, &status.docker_status);
        
        table.printstd();
        println!();
    }
    
    // Container stats
    if component.is_none() || component.as_ref().map(|c| c == "containers").unwrap_or(false) {
        println!("{}", "Containers".bold());
        println!("  {}: {} ({} running, {} stopped, {} failed)",
            "Total".bold(),
            status.container_stats.total,
            status.container_stats.running.to_string().green(),
            status.container_stats.stopped.to_string().yellow(),
            status.container_stats.failed.to_string().red()
        );
        println!("  {}: {}/{}",
            "Pool".bold(),
            status.container_stats.pool_available,
            status.container_stats.pool_size
        );
        
        // Pool usage bar
        let pool_percent = (status.container_stats.pool_available as f64 / status.container_stats.pool_size as f64 * 100.0) as u64;
        let pb = ProgressBar::new(100);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("  Pool Usage: [{bar:40.cyan/blue}] {pos}%")
                .unwrap()
                .progress_chars("█▓░")
        );
        pb.set_position(100 - pool_percent);
        pb.finish();
        println!();
    }
    
    // Resource usage
    if detailed || component.as_ref().map(|c| c == "resources").unwrap_or(false) {
        println!("{}", "Resource Usage".bold());
        
        // CPU
        print_resource_bar("CPU", status.resource_usage.cpu_percent, 100.0);
        
        // Memory
        print_resource_bar("Memory", status.resource_usage.memory_percent, 100.0);
        println!("         {} MB used", status.resource_usage.memory_mb);
        
        // Disk
        print_resource_bar("Disk", status.resource_usage.disk_percent, 100.0);
        println!("         {} GB used", status.resource_usage.disk_gb);
        
        // Network
        println!("  {}: {} active connections", "Network".bold(), status.resource_usage.network_connections);
        println!();
    }
    
    // Footer
    if detailed {
        println!("{}", "─".repeat(50));
        println!("Server: {}", "http://localhost:3000".cyan());
        println!("Version: {}", env!("CARGO_PKG_VERSION"));
    }
}

fn add_service_row(table: &mut Table, service: &ServiceStatus) {
    let status_str = format_status(&service.status);
    let latency_str = service.latency_ms
        .map(|ms| format!("{} ms", ms))
        .unwrap_or_else(|| "-".to_string());
    let details = service.message.clone().unwrap_or_else(|| "-".to_string());
    
    table.add_row(row![
        service.name,
        status_str,
        latency_str,
        details
    ]);
}

fn format_status(status: &Status) -> String {
    match status {
        Status::Healthy => "● Healthy".green().to_string(),
        Status::Degraded => "● Degraded".yellow().to_string(),
        Status::Unhealthy => "● Unhealthy".red().to_string(),
        Status::Unknown => "● Unknown".white().dimmed().to_string(),
    }
}

fn format_health(health: &str) -> String {
    match health {
        "Healthy" => health.green().bold().to_string(),
        "Degraded" => health.yellow().bold().to_string(),
        "Critical" => health.red().bold().to_string(),
        _ => health.white().to_string(),
    }
}

fn calculate_overall_health(status: &SystemStatus) -> String {
    let unhealthy_count = [
        &status.api_status,
        &status.grpc_status,
        &status.websocket_status,
        &status.database_status,
        &status.redis_status,
        &status.docker_status,
    ].iter().filter(|s| matches!(s.status, Status::Unhealthy)).count();
    
    let degraded_count = [
        &status.api_status,
        &status.grpc_status,
        &status.websocket_status,
        &status.database_status,
        &status.redis_status,
        &status.docker_status,
    ].iter().filter(|s| matches!(s.status, Status::Degraded)).count();
    
    if unhealthy_count > 0 {
        "Critical".to_string()
    } else if degraded_count > 0 {
        "Degraded".to_string()
    } else {
        "Healthy".to_string()
    }
}

fn print_resource_bar(name: &str, percent: f64, max: f64) {
    let bar_width = 30;
    let filled = ((percent / max) * bar_width as f64) as usize;
    let empty = bar_width - filled;
    
    let color = if percent > 80.0 {
        "red"
    } else if percent > 60.0 {
        "yellow"
    } else {
        "green"
    };
    
    let bar = match color {
        "red" => format!("{}{}", "█".repeat(filled).red(), "░".repeat(empty)),
        "yellow" => format!("{}{}", "█".repeat(filled).yellow(), "░".repeat(empty)),
        _ => format!("{}{}", "█".repeat(filled).green(), "░".repeat(empty)),
    };
    
    println!("  {:8} [{}] {:.1}%", format!("{}:", name).bold(), bar, percent);
}

fn print_json_status(_status: &SystemStatus) {
    // Mock JSON output
    let json = r#"{
  "overall_health": "Healthy",
  "services": {
    "api": {"status": "healthy", "latency_ms": 12},
    "grpc": {"status": "healthy", "latency_ms": 8},
    "websocket": {"status": "healthy", "latency_ms": 5},
    "database": {"status": "healthy", "latency_ms": 3},
    "redis": {"status": "healthy", "latency_ms": 1},
    "docker": {"status": "healthy", "version": "24.0.7"}
  },
  "containers": {
    "total": 15,
    "running": 8,
    "stopped": 5,
    "failed": 2,
    "pool_size": 20,
    "pool_available": 12
  },
  "resources": {
    "cpu_percent": 23.5,
    "memory_mb": 1024,
    "memory_percent": 12.5,
    "disk_gb": 5.2,
    "disk_percent": 8.7,
    "network_connections": 42
  }
}"#;
    println!("{}", json);
}

fn print_yaml_status(_status: &SystemStatus) {
    // Mock YAML output
    let yaml = r#"overall_health: Healthy
services:
  api:
    status: healthy
    latency_ms: 12
  grpc:
    status: healthy
    latency_ms: 8
  websocket:
    status: healthy
    latency_ms: 5
  database:
    status: healthy
    latency_ms: 3
  redis:
    status: healthy
    latency_ms: 1
  docker:
    status: healthy
    version: 24.0.7
containers:
  total: 15
  running: 8
  stopped: 5
  failed: 2
  pool_size: 20
  pool_available: 12
resources:
  cpu_percent: 23.5
  memory_mb: 1024
  memory_percent: 12.5
  disk_gb: 5.2
  disk_percent: 8.7
  network_connections: 42"#;
    println!("{}", yaml);
}