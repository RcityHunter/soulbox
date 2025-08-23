//! CLI module for SoulBox command-line interface
//! 
//! This module provides a complete command-line interface for managing
//! SoulBox sandboxes, including creation, execution, logging, and management.

pub mod commands;
pub mod config;

use clap::{Parser, Subcommand};
use colored::*;
use std::process;
use tracing::info;

use crate::error::Result;
pub use commands::*;
pub use config::CliConfig;

#[derive(Parser)]
#[command(name = "soulbox")]
#[command(about = "SoulBox CLI - Manage AI code execution sandboxes")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = "SoulBox Team")]
pub struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Configuration file path
    #[arg(short, long, global = true)]
    pub config: Option<String>,

    /// Server endpoint URL
    #[arg(short, long, global = true)]
    pub server: Option<String>,

    /// API key for authentication
    #[arg(long, global = true)]
    pub api_key: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new SoulBox project
    Init(commands::init::InitArgs),
    
    /// Create a new sandbox
    Create(commands::create::CreateArgs),
    
    /// Run code in a sandbox (simplified exec)
    Run(commands::run::RunArgs),
    
    /// Execute code in a sandbox (advanced)
    Exec(commands::exec::ExecArgs),
    
    /// Stop running sandboxes
    Stop(commands::stop::StopArgs),
    
    /// View sandbox logs
    Logs(commands::logs::LogsArgs),
    
    /// List sandboxes
    List(commands::list::ListArgs),
}

impl Cli {
    /// Run the CLI application
    pub async fn run() -> Result<()> {
        let cli = Cli::parse();
        
        // Initialize tracing based on verbosity
        let filter = if cli.verbose {
            "soulbox=debug,info"
        } else {
            "soulbox=info"
        };
        
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .init();

        // Load configuration
        let config = CliConfig::load(cli.config.as_deref()).await?;
        
        // Override config with CLI args if provided
        let config = config
            .with_server_url(cli.server.as_deref())
            .with_api_key(cli.api_key.as_deref());

        info!("SoulBox CLI v{}", env!("CARGO_PKG_VERSION"));
        
        // Execute the command
        match cli.command {
            Commands::Init(args) => commands::init::run(args, &config).await,
            Commands::Create(args) => commands::create::run(args, &config).await,
            Commands::Run(args) => args.execute().await,
            Commands::Exec(args) => commands::exec::run(args, &config).await,
            Commands::Stop(args) => args.execute().await,
            Commands::Logs(args) => commands::logs::run(args, &config).await,
            Commands::List(args) => commands::list::run(args, &config).await,
        }
    }

    /// Print success message
    pub fn success(message: &str) {
        println!("{} {}", "✓".green().bold(), message);
    }

    /// Print error message and exit
    pub fn error_exit(message: &str) -> ! {
        eprintln!("{} {}", "✗".red().bold(), message.red());
        process::exit(1);
    }

    /// Print warning message
    pub fn warning(message: &str) {
        println!("{} {}", "⚠".yellow().bold(), message.yellow());
    }

    /// Print info message
    pub fn info(message: &str) {
        println!("{} {}", "ℹ".blue().bold(), message);
    }
}