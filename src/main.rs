use anyhow::Result;
use tracing::{info, error};
use tracing_subscriber;

mod config;
mod server;
mod sandbox;
mod auth;
mod api;
mod error;

use config::Config;
use server::Server;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("soulbox=debug,info")
        .json()
        .init();

    info!("🦀 Starting SoulBox server...");

    // Load configuration
    let config = Config::from_env().await.map_err(|e| {
        error!("Failed to load configuration: {}", e);
        e
    })?;

    info!("Configuration loaded successfully");
    info!("Server will listen on: {}:{}", config.server.host, config.server.port);

    // Start the server
    let server = Server::new(config).await?;
    server.run().await?;

    Ok(())
}