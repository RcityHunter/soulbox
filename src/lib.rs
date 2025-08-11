pub mod config;
pub mod error;
pub mod grpc;
pub mod websocket;
pub mod container;
pub mod filesystem;
pub mod server;
pub mod auth;
pub mod api;
pub mod sandbox;

pub use config::Config;
pub use error::{SoulBoxError, Result};

// Include generated protobuf code
pub mod proto {
    tonic::include_proto!("soulbox.v1");
}