pub mod service;
pub mod streaming_service;
pub mod service_traits;

// Generated file descriptor set for gRPC reflection support
pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!("../../proto/soulbox_descriptor.bin");

// Re-export service traits for easier use
pub use service_traits::{SoulBoxService, StreamingService};
pub use service_traits::{soul_box_service_server, streaming_service_server};

