// Simple SoulBox - Linus style implementation
// "If you need more than 300 lines, you're doing it wrong"

use anyhow::Result;
use tracing::{info, error};
use soulbox::simple::{SandboxManager, SimpleAPI};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging - keep it simple
    tracing_subscriber::fmt()
        .with_env_filter("simple_soulbox=info,soulbox=info")
        .init();

    info!("🦀 Starting Simple SoulBox...");

    // Create sandbox manager - one line
    let manager = SandboxManager::new()?;
    
    info!("✅ Docker connection established");
    info!("🚀 Starting HTTP API on http://0.0.0.0:8080");
    
    // Start the server - that's it
    if let Err(e) = SimpleAPI::serve(manager).await {
        error!("💥 Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}