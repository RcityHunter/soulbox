use anyhow::Result;
use tracing::{info, error};
use std::net::SocketAddr;

mod config;
mod error;
mod server;
mod grpc;
mod container;

use config::Config;
use server::Server;

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
    let grpc_addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port + 1000)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid gRPC address: {}", e))?;
    
    let grpc_service = grpc::service::SoulBoxServiceImpl::new();
    
    // Initialize container manager
    let container_manager = container::ContainerManager::new(config.clone()).await?;
    grpc_service.set_container_manager(container_manager).await;
    
    let grpc_handle = tokio::spawn(async move {
        info!("🚀 Starting gRPC server on {}", grpc_addr);
        
        if let Err(e) = tonic::transport::Server::builder()
            .add_service(grpc::service::soul_box_service_server::SoulBoxServiceServer::new(grpc_service))
            .serve(grpc_addr)
            .await
        {
            error!("gRPC server error: {}", e);
        }
    });

    info!("🦀 SoulBox is ready! REST API on port {}, gRPC on port {}", 
          config.server.port, config.server.port + 1000);
    info!("Press Ctrl+C to stop the server");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    info!("Shutting down SoulBox server...");
    
    // Cancel the server tasks
    rest_handle.abort();
    grpc_handle.abort();

    Ok(())
}