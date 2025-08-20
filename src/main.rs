use anyhow::Result;
use tracing::{info, error};
use std::net::SocketAddr;
use std::sync::Arc;

mod config;
mod error;
mod server;
mod grpc;
mod container;
mod sandbox;
mod firecracker;
mod auth;
mod api;
mod audit;
mod database;
mod websocket;
mod filesystem;

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
    let grpc_port = config.server.port + 1000;
    let grpc_addr: SocketAddr = format!("{}:{}", config.server.host, grpc_port)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid gRPC address: {}", e))?;
    
    let grpc_service = grpc::service::SoulBoxServiceImpl::new()
        .map_err(|e| anyhow::anyhow!("Failed to create gRPC service: {}", e))?;
    
    // Initialize runtime based on configuration
    let runtime: Arc<dyn sandbox::SandboxRuntime> = match config.sandbox.runtime.runtime_type.as_str() {
        "docker" => {
            info!("Using Docker runtime");
            let container_manager = container::ContainerManager::new(config.clone()).await?;
            Arc::new(sandbox::DockerRuntime::new(Arc::new(container_manager)))
        }
        "firecracker" => {
            info!("Using Firecracker runtime");
            let vm_manager = firecracker::FirecrackerManager::new(config.clone()).await?;
            Arc::new(sandbox::FirecrackerRuntime::new(Arc::new(vm_manager)))
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown runtime type: {}", config.sandbox.runtime.runtime_type));
        }
    };
    
    grpc_service.set_runtime(runtime).await;
    
    let grpc_handle = tokio::spawn(async move {
        info!("🚀 Starting gRPC server on {}", grpc_addr);
        
        // Build reflection service
        let reflection_service = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(grpc::FILE_DESCRIPTOR_SET)
            .build()
            .unwrap();
        
        match tonic::transport::Server::builder()
            .add_service(grpc::service::soul_box_service_server::SoulBoxServiceServer::new(grpc_service))
            .add_service(reflection_service)
            .serve(grpc_addr)
            .await
        {
            Ok(_) => info!("gRPC server stopped gracefully"),
            Err(e) => error!("gRPC server error: {}", e),
        }
    });

    info!("🦀 SoulBox is ready! REST API on port {}, gRPC on port {}", 
          config.server.port, grpc_port);
    info!("Press Ctrl+C to stop the server");

    // Set up graceful shutdown
    let shutdown_signal = async {
        // Listen for SIGINT (Ctrl+C)
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .expect("Failed to install SIGINT handler");
        
        // Listen for SIGTERM (systemd, docker stop, etc.)
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler");
        
        tokio::select! {
            _ = sigint.recv() => {
                info!("Received SIGINT (Ctrl+C), initiating graceful shutdown...");
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM, initiating graceful shutdown...");
            }
        }
    };

    // Wait for shutdown signal
    shutdown_signal.await;
    
    info!("Shutting down SoulBox server...");
    
    // Graceful shutdown with timeout
    let shutdown_timeout = tokio::time::Duration::from_secs(30);
    
    info!("Stopping REST server...");
    rest_handle.abort();
    
    info!("Stopping gRPC server...");
    grpc_handle.abort();
    
    // Wait for tasks to complete with timeout
    let shutdown_result = tokio::time::timeout(shutdown_timeout, async {
        // Wait for both servers to shut down
        let _ = tokio::join!(rest_handle, grpc_handle);
    }).await;
    
    match shutdown_result {
        Ok(_) => info!("✅ All servers shut down gracefully"),
        Err(_) => {
            error!("⚠️  Shutdown timeout reached, some resources may not have been cleaned up properly");
        }
    }
    
    info!("🛑 SoulBox server stopped");
    Ok(())
}