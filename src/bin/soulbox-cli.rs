//! SoulBox CLI - Command Line Interface
//! 
//! A comprehensive CLI tool for managing SoulBox sandboxes, executing code,
//! viewing logs, and managing projects.

use soulbox::cli::Cli;
use soulbox::error::Result;
use tracing::error;

#[tokio::main]
async fn main() -> Result<()> {
    // Handle errors gracefully with colored output
    if let Err(e) = Cli::run().await {
        error!("CLI error: {}", e);
        soulbox::cli::Cli::error_exit(&format!("Error: {}", e));
    }
    
    Ok(())
}