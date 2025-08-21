use anyhow::Result;
use tracing::{info, error};
use std::net::SocketAddr;
use std::sync::Arc;

use soulbox::{
    Config,
    server::Server,
    grpc::service::SoulBoxServiceImpl,
    container::ContainerManager,
    sandbox::{SandboxRuntime, DockerRuntime, FirecrackerRuntime},
    firecracker::FirecrackerManager,
};

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

    // Start the HTTP/REST server
    let rest_server = Server::new(config.clone()).await?;
    let rest_handle = tokio::spawn(async move {
        if let Err(e) = rest_server.run().await {
            error!("REST server error: {}", e);
        }
    });

    // Start the gRPC server
    let grpc_port = config.server.port + 1000;
    let grpc_addr: SocketAddr = format!("{}:{}", config.server.host, grpc_port)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid gRPC address: {}", e))?;
    
    let grpc_service = SoulBoxServiceImpl::new()
        .map_err(|e| anyhow::anyhow!("Failed to create gRPC service: {}", e))?;
    
    // Initialize runtime based on configuration
    let runtime: Arc<dyn SandboxRuntime> = match config.sandbox.runtime.runtime_type.as_str() {
        "docker" => {
            info!("Using Docker runtime");
            let container_manager = ContainerManager::new(config.clone()).await?;
            Arc::new(DockerRuntime::new(Arc::new(container_manager)))
        }
        "firecracker" => {
            info!("Using Firecracker runtime");
            let vm_manager = FirecrackerManager::new(config.clone()).await?;
            Arc::new(FirecrackerRuntime::new(Arc::new(vm_manager)))
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown runtime type: {}", config.sandbox.runtime.runtime_type));
        }
    };

    info!("🚀 SoulBox server started successfully!");
    info!("REST API: http://{}:{}", config.server.host, config.server.port);
    info!("gRPC API: http://{}", grpc_addr);

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    info!("Shutting down SoulBox server...");

    Ok(())
}