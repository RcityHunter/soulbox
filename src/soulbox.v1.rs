// Mock protobuf definitions for development
// TODO: Replace with actual generated code once tonic-build 0.14 is properly configured

use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct CreateSandboxRequest {
    pub template_id: String,
    pub config: Option<SandboxConfig>,
    pub environment_variables: HashMap<String, String>,
    pub timeout: Option<prost_types::Duration>,
}

#[derive(Clone, Debug, Default)]
pub struct CreateSandboxResponse {
    pub sandbox_id: String,
    pub status: String,
    pub created_at: Option<prost_types::Timestamp>,
    pub endpoint_url: String,
}

#[derive(Clone, Debug, Default)]
pub struct SandboxConfig {
    pub resource_limits: Option<ResourceLimits>,
    pub network_config: Option<NetworkConfig>,
    pub allowed_domains: Vec<String>,
    pub enable_internet: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ResourceLimits {
    pub memory_mb: u64,
    pub cpu_cores: f32,
    pub disk_mb: u64,
    pub max_processes: u32,
}

#[derive(Clone, Debug, Default)]
pub struct NetworkConfig {
    pub exposed_ports: Vec<u32>,
    pub enable_port_forwarding: bool,
    pub subnet: String,
}

#[derive(Clone, Debug, Default)]
pub struct GetSandboxRequest {
    pub sandbox_id: String,
}

#[derive(Clone, Debug, Default)]
pub struct GetSandboxResponse {
    pub sandbox: Option<Sandbox>,
}

#[derive(Clone, Debug, Default)]
pub struct Sandbox {
    pub id: String,
    pub template_id: String,
    pub status: String,
    pub config: Option<SandboxConfig>,
    pub created_at: Option<prost_types::Timestamp>,
    pub updated_at: Option<prost_types::Timestamp>,
    pub environment_variables: HashMap<String, String>,
    pub endpoint_url: String,
}

#[derive(Clone, Debug, Default)]
pub struct ListSandboxesRequest {
    pub page: i32,
    pub page_size: i32,
    pub filter: String,
}

#[derive(Clone, Debug, Default)]
pub struct ListSandboxesResponse {
    pub sandboxes: Vec<Sandbox>,
    pub total_count: i32,
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Default)]
pub struct DeleteSandboxRequest {
    pub sandbox_id: String,
}

#[derive(Clone, Debug, Default)]
pub struct DeleteSandboxResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct ExecuteCodeRequest {
    pub sandbox_id: String,
    pub code: String,
    pub language: String,
    pub environment_variables: HashMap<String, String>,
    pub timeout: Option<prost_types::Duration>,
    pub working_directory: String,
}

#[derive(Clone, Debug, Default)]
pub struct ExecuteCodeResponse {
    pub execution_id: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub execution_time: Option<prost_types::Duration>,
    pub timed_out: bool,
}

#[derive(Clone, Debug, Default)]
pub struct HealthCheckRequest {
    pub service: String,
}

#[derive(Clone, Debug, Default)]
pub struct HealthCheckResponse {
    pub status: i32, // HealthStatus enum as i32
    pub message: String,
    pub details: HashMap<String, String>,
}

// Mock trait implementations for gRPC service
pub mod soul_box_service_server {
    use super::*;
    use tonic::{Request, Response, Status};
    
    #[tonic::async_trait]
    pub trait SoulBoxService: Send + Sync + 'static {
        async fn create_sandbox(
            &self,
            request: Request<CreateSandboxRequest>,
        ) -> Result<Response<CreateSandboxResponse>, Status>;
        
        async fn get_sandbox(
            &self,
            request: Request<GetSandboxRequest>,
        ) -> Result<Response<GetSandboxResponse>, Status>;
        
        async fn list_sandboxes(
            &self,
            request: Request<ListSandboxesRequest>,
        ) -> Result<Response<ListSandboxesResponse>, Status>;
        
        async fn delete_sandbox(
            &self,
            request: Request<DeleteSandboxRequest>,
        ) -> Result<Response<DeleteSandboxResponse>, Status>;
        
        async fn execute_code(
            &self,
            request: Request<ExecuteCodeRequest>,
        ) -> Result<Response<ExecuteCodeResponse>, Status>;
        
        async fn health_check(
            &self,
            request: Request<HealthCheckRequest>,
        ) -> Result<Response<HealthCheckResponse>, Status>;
    }
    
    pub struct SoulBoxServiceServer<T> {
        inner: T,
    }
    
    impl<T> SoulBoxServiceServer<T> {
        pub fn new(inner: T) -> Self {
            Self { inner }
        }
    }
    
    // Mock implementation for tower service integration
    impl<T> tonic::server::NamedService for SoulBoxServiceServer<T> {
        const NAME: &'static str = "soulbox.v1.SoulBoxService";
    }
}

pub use soul_box_service_server::*;