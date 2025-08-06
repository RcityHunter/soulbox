pub mod config;
pub mod error;
pub mod grpc;
pub mod websocket;

// TODO: Add other modules as we implement them
// pub mod server;
// pub mod sandbox;
// pub mod auth;
// pub mod api;

pub use config::Config;
pub use error::{SoulBoxError, Result};

// Include generated protobuf code
pub mod proto {
    tonic::include_proto!("soulbox.v1");
}