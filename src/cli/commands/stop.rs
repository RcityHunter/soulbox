use clap::Args;
use colored::*;
use tracing::{debug, info};

use crate::error::Result;

/// Arguments for the stop command
#[derive(Args)]
pub struct StopArgs {
    /// Sandbox ID(s) to stop
    #[arg(required = true)]
    pub sandboxes: Vec<String>,

    /// Force stop without graceful shutdown
    #[arg(short, long)]
    pub force: bool,

    /// Stop all running sandboxes
    #[arg(short, long, conflicts_with = "sandboxes")]
    pub all: bool,

    /// Don't ask for confirmation
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Timeout for graceful shutdown (seconds)
    #[arg(long, default_value = "10")]
    pub timeout: u64,
}

impl StopArgs {
    /// Execute the stop command
    pub async fn execute(&self) -> Result<()> {
        // Determine which sandboxes to stop
        let sandbox_ids = if self.all {
            info!("Stopping all running sandboxes");
            // TODO: Fetch all running sandbox IDs from API
            vec!["sandbox-1".to_string(), "sandbox-2".to_string()] // Placeholder
        } else {
            self.sandboxes.clone()
        };

        // Confirm if not using --yes flag
        if !self.yes && sandbox_ids.len() > 1 {
            println!("{}", format!("About to stop {} sandboxes:", sandbox_ids.len()).yellow());
            for id in &sandbox_ids {
                println!("  - {}", id);
            }
            print!("Continue? [y/N] ");
            
            use std::io::{self, Write};
            io::stdout().flush()?;
            
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Operation cancelled");
                return Ok(());
            }
        }

        // Stop each sandbox
        let mut success_count = 0;
        let mut failed_count = 0;

        for sandbox_id in &sandbox_ids {
            match stop_sandbox(sandbox_id, self.force, self.timeout).await {
                Ok(_) => {
                    println!("{} {}", "✓".green(), format!("Stopped sandbox: {}", sandbox_id).green());
                    success_count += 1;
                }
                Err(e) => {
                    println!("{} {}", "✗".red(), format!("Failed to stop {}: {}", sandbox_id, e).red());
                    failed_count += 1;
                }
            }
        }

        // Summary
        if sandbox_ids.len() > 1 {
            println!("\n{}", "Summary:".bold());
            if success_count > 0 {
                println!("  {} {} sandbox(es) stopped successfully", "✓".green(), success_count);
            }
            if failed_count > 0 {
                println!("  {} {} sandbox(es) failed to stop", "✗".red(), failed_count);
            }
        }

        if failed_count > 0 {
            return Err(crate::error::SoulBoxError::Internal(
                format!("Failed to stop {} sandbox(es)", failed_count)
            ));
        }

        Ok(())
    }
}

/// Stop a single sandbox
async fn stop_sandbox(sandbox_id: &str, force: bool, timeout: u64) -> Result<()> {
    if force {
        debug!("Force stopping sandbox: {}", sandbox_id);
        println!("  {} {}", "→".blue(), format!("Force stopping {}...", sandbox_id).dimmed());
    } else {
        debug!("Gracefully stopping sandbox: {}", sandbox_id);
        println!("  {} {}", "→".blue(), format!("Stopping {} (timeout: {}s)...", sandbox_id, timeout).dimmed());
    }

    // TODO: Implement actual stop via API client
    // For now, simulate the stop operation
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Simulate occasional failures for testing
    if sandbox_id.contains("fail") {
        return Err(crate::error::SoulBoxError::Internal(
            "Container not responding".to_string()
        ));
    }

    Ok(())
}