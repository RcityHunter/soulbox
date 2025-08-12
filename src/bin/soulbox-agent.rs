use anyhow::Result;
use tracing::{info, error};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("soulbox=debug".parse()?))
        .init();

    info!("Starting SoulBox VM Agent...");

    // Default vsock path for guest VM
    let vsock_path = std::env::var("VSOCK_PATH")
        .unwrap_or_else(|_| "/tmp/vsock.sock".to_string());

    // Create and start vsock server
    let server = soulbox::firecracker::agent::VsockServer::new(vsock_path);
    
    info!("VM Agent ready to receive execution requests");
    
    // Start server (blocks forever)
    if let Err(e) = server.start().await {
        error!("Agent server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}