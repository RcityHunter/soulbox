use anyhow::Result;
use tracing::{info, error};
use tracing_subscriber;

mod config;
mod error;

use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("soulbox=debug,info")
        .init();

    info!("🦀 Starting SoulBox server...");

    // Load configuration
    let config = Config::from_env().await.map_err(|e| {
        error!("Failed to load configuration: {}", e);
        e
    })?;

    info!("Configuration loaded successfully");
    info!("Server will listen on: {}:{}", config.server.host, config.server.port);

    // TODO: Start the actual server
    info!("SoulBox is ready! (placeholder implementation)");
    info!("Press Ctrl+C to stop the server");

    // Keep the process running
    tokio::signal::ctrl_c().await?;
    info!("Shutting down SoulBox server...");

    Ok(())
}