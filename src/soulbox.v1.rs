// Mock protobuf definitions for development
// TODO: Replace with actual generated code once tonic-build 0.14 is properly configured

//! This module contains mock protobuf definitions that provide the same interface
//! as the generated code would, allowing the rest of the system to compile and work
//! during development phase.

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
            
        // Streaming methods
        type StreamExecuteCodeStream: futures::Stream<Item = Result<ExecuteCodeStreamResponse, Status>> + Send + 'static;
        
        async fn stream_execute_code(
            &self,
            request: Request<ExecuteCodeRequest>,
        ) -> Result<Response<Self::StreamExecuteCodeStream>, Status>;
        
        async fn upload_file(
            &self,
            request: Request<tonic::Streaming<UploadFileRequest>>,
        ) -> Result<Response<UploadFileResponse>, Status>;
        
        type DownloadFileStream: futures::Stream<Item = Result<DownloadFileResponse, Status>> + Send + 'static;
        
        async fn download_file(
            &self,
            request: Request<DownloadFileRequest>,
        ) -> Result<Response<Self::DownloadFileStream>, Status>;
        
        async fn list_files(
            &self,
            request: Request<ListFilesRequest>,
        ) -> Result<Response<ListFilesResponse>, Status>;
        
        async fn delete_file(
            &self,
            request: Request<DeleteFileRequest>,
        ) -> Result<Response<DeleteFileResponse>, Status>;
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

// Additional types needed by service implementation
#[derive(Clone, Debug, Default)]
pub struct ExecuteCodeStreamResponse {
    pub response: Option<execute_code_stream_response::Response>,
}

pub mod execute_code_stream_response {
    #[derive(Clone, Debug)]
    pub enum Response {
        Started(super::ExecutionStarted),
        Output(super::ExecutionOutput),
        Completed(super::ExecutionCompleted),
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionStarted {
    pub execution_id: String,
    pub started_at: Option<prost_types::Timestamp>,
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionOutput {
    pub execution_id: String,
    pub r#type: i32,
    pub data: String,
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionCompleted {
    pub execution_id: String,
    pub exit_code: i32,
    pub execution_time: Option<prost_types::Duration>,
}

#[derive(Clone, Debug)]
pub enum OutputType {
    Stdout = 1,
    Stderr = 2,
}

// File operations types
#[derive(Clone, Debug, Default)]
pub struct UploadFileRequest {
    pub request: Option<upload_file_request::Request>,
}

pub mod upload_file_request {
    #[derive(Clone, Debug)]
    pub enum Request {
        Metadata(super::FileUploadMetadata),
        Chunk(Vec<u8>),
    }
}

#[derive(Clone, Debug, Default)]
pub struct FileUploadMetadata {
    pub sandbox_id: String,
    pub file_path: String,
    pub file_size: u64,
}

#[derive(Clone, Debug, Default)]
pub struct UploadFileResponse {
    pub success: bool,
    pub message: String,
    pub file_path: String,
    pub bytes_uploaded: u64,
}

#[derive(Clone, Debug, Default)]
pub struct DownloadFileRequest {
    pub sandbox_id: String,
    pub file_path: String,
}

#[derive(Clone, Debug, Default)]
pub struct DownloadFileResponse {
    pub response: Option<download_file_response::Response>,
}

pub mod download_file_response {
    #[derive(Clone, Debug)]
    pub enum Response {
        Metadata(super::FileDownloadMetadata),
        Chunk(Vec<u8>),
    }
}

#[derive(Clone, Debug, Default)]
pub struct FileDownloadMetadata {
    pub file_path: String,
    pub file_size: u64,
    pub content_type: String,
}

#[derive(Clone, Debug, Default)]
pub struct ListFilesRequest {
    pub sandbox_id: String,
    pub path: String,
    pub recursive: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ListFilesResponse {
    pub files: Vec<FileInfo>,
}

#[derive(Clone, Debug, Default)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_directory: bool,
    pub modified_at: Option<prost_types::Timestamp>,
    pub permissions: String,
}

#[derive(Clone, Debug, Default)]
pub struct DeleteFileRequest {
    pub sandbox_id: String,
    pub file_path: String,
}

#[derive(Clone, Debug, Default)]
pub struct DeleteFileResponse {
    pub success: bool,
    pub message: String,
}

// HealthCheckRequest and HealthCheckResponse already defined above

#[derive(Clone, Debug)]
pub enum HealthStatus {
    Unknown = 0,
    Serving = 1,
    NotServing = 2,
}

// Streaming service types
#[derive(Clone, Debug, Default)]
pub struct SandboxStreamRequest {
    pub request: Option<sandbox_stream_request::Request>,
}

pub mod sandbox_stream_request {
    #[derive(Clone, Debug)]
    pub enum Request {
        Init(super::SandboxStreamInit),
        Data(Vec<u8>),
        Command(super::SandboxStreamCommand),
        Close(super::SandboxStreamClose),
    }
}

#[derive(Clone, Debug, Default)]
pub struct SandboxStreamInit {
    pub sandbox_id: String,
    pub stream_type: i32,
}

#[derive(Clone, Debug, Default)]
pub struct SandboxStreamCommand {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: String,
}

#[derive(Clone, Debug, Default)]
pub struct SandboxStreamClose {
    pub reason: String,
}

#[derive(Clone, Debug, Default)]
pub struct SandboxStreamResponse {
    pub response: Option<sandbox_stream_response::Response>,
}

pub mod sandbox_stream_response {
    #[derive(Clone, Debug)]
    pub enum Response {
        Ready(super::SandboxStreamReady),
        Data(Vec<u8>),
        Output(super::SandboxStreamOutput),
        Error(super::SandboxStreamError),
        Closed(super::SandboxStreamClosed),
    }
}

#[derive(Clone, Debug, Default)]
pub struct SandboxStreamReady {
    pub stream_id: String,
}

#[derive(Clone, Debug, Default)]
pub struct SandboxStreamError {
    pub stream_id: String,
    pub message: String,
    pub code: i32,
}

#[derive(Clone, Debug, Default)]
pub struct SandboxStreamClosed {
    pub reason: String,
}

#[derive(Clone, Debug)]
pub enum StreamType {
    Unspecified = 0,
    Stdout = 1,
    Stderr = 2,
    Terminal = 3,
    FileWatch = 4,
}

impl StreamType {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Unspecified),
            1 => Some(Self::Stdout),
            2 => Some(Self::Stderr),  
            3 => Some(Self::Terminal),
            4 => Some(Self::FileWatch),
            _ => None,
        }
    }
}

// Server module for streaming service
pub mod streaming_service_server {
    use tonic::{async_trait, Request, Response, Status, Streaming};
    use futures::Stream;
    use std::pin::Pin;
    
    #[async_trait]
    pub trait StreamingService: Send + Sync + 'static {
        type SandboxStreamStream: Stream<Item = Result<super::SandboxStreamResponse, Status>> + Send + 'static;
        
        async fn sandbox_stream(
            &self,
            request: Request<Streaming<super::SandboxStreamRequest>>,
        ) -> Result<Response<Self::SandboxStreamStream>, Status>;
        
        type TerminalStreamStream: Stream<Item = Result<super::TerminalStreamResponse, Status>> + Send + 'static;
        
        async fn terminal_stream(
            &self,
            request: Request<Streaming<super::TerminalStreamRequest>>,
        ) -> Result<Response<Self::TerminalStreamStream>, Status>;
    }
    
    pub struct StreamingServiceServer<T> {
        inner: T,
    }
    
    impl<T> StreamingServiceServer<T> {
        pub fn new(inner: T) -> Self {
            Self { inner }
        }
    }
    
    impl<T> tonic::server::NamedService for StreamingServiceServer<T> {
        const NAME: &'static str = "soulbox.v1.StreamingService";
    }
}

// Additional streaming types  
#[derive(Clone, Debug, Default)]
pub struct SandboxStreamOutput {
    pub stream_id: String,
    pub data: Vec<u8>,
    pub stream_type: i32,
    pub output_type: i32,
}

#[derive(Clone, Debug, Default)]
pub struct TerminalStreamRequest {
    pub request: Option<terminal_stream_request::Request>,
}

pub mod terminal_stream_request {
    #[derive(Clone, Debug)]
    pub enum Request {
        Init(super::TerminalStreamInit),
        Input(Vec<u8>),
        Resize(super::TerminalResize),
        Close(super::TerminalStreamClose),
    }
}

#[derive(Clone, Debug, Default)]
pub struct TerminalStreamInit {
    pub sandbox_id: String,
    pub terminal_type: String,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Clone, Debug, Default)]
pub struct TerminalResize {
    pub cols: u32,
    pub rows: u32,
}

#[derive(Clone, Debug, Default)]
pub struct TerminalStreamClose {
    pub reason: String,
}

#[derive(Clone, Debug, Default)]
pub struct TerminalStreamResponse {
    pub response: Option<terminal_stream_response::Response>,
}

pub mod terminal_stream_response {
    #[derive(Clone, Debug)]
    pub enum Response {
        Ready(super::TerminalStreamReady),
        Output(Vec<u8>),
        Error(super::TerminalStreamError),
        Closed(super::TerminalStreamClosed),
    }
}

#[derive(Clone, Debug, Default)]
pub struct TerminalStreamReady {
    pub terminal_id: String,
}

#[derive(Clone, Debug, Default)]
pub struct TerminalStreamError {
    pub terminal_id: String,
    pub message: String,
    pub code: i32,
}

#[derive(Clone, Debug, Default)]  
pub struct TerminalStreamClosed {
    pub terminal_id: String,
    pub reason: String,
    pub exit_code: i32,
}

// Client module for integration tests and client usage
pub mod soul_box_service_client {
    use tonic::{Request, Response, Status};
    
    #[derive(Clone)]
    pub struct SoulBoxServiceClient<T> {
        _inner: std::marker::PhantomData<T>,
    }
    
    impl SoulBoxServiceClient<tonic::transport::Channel> {
        pub async fn connect<D>(_dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: TryInto<tonic::transport::Endpoint>,
            D::Error: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
        {
            // Mock client for testing - doesn't actually connect
            Ok(Self { _inner: std::marker::PhantomData })
        }
        
        pub fn new(_inner: tonic::transport::Channel) -> Self {
            Self { _inner: std::marker::PhantomData }
        }
    }
    
    impl<T> SoulBoxServiceClient<T> {
        // Mock implementations for testing
        pub async fn health_check(
            &mut self,
            _request: Request<super::HealthCheckRequest>,
        ) -> Result<Response<super::HealthCheckResponse>, Status> {
            // Mock response for testing
            let response = super::HealthCheckResponse {
                status: super::HealthStatus::Serving as i32,
                message: "SoulBox service is healthy".to_string(),
                details: std::collections::HashMap::new(),
            };
            Ok(Response::new(response))
        }
        
        pub async fn create_sandbox(
            &mut self,
            request: Request<super::CreateSandboxRequest>,
        ) -> Result<Response<super::CreateSandboxResponse>, Status> {
            // Mock response for testing
            let _req = request.into_inner();
            let response = super::CreateSandboxResponse {
                sandbox_id: "test_sandbox_123".to_string(),
                status: "running".to_string(),
                created_at: Some(prost_types::Timestamp {
                    seconds: 1677600000,
                    nanos: 0,
                }),
                endpoint_url: "https://test-sandbox-123.soulbox.dev".to_string(),
            };
            Ok(Response::new(response))
        }
        
        pub async fn get_sandbox(
            &mut self,
            request: Request<super::GetSandboxRequest>,
        ) -> Result<Response<super::GetSandboxResponse>, Status> {
            // Mock response for testing
            let req = request.into_inner();
            let sandbox = super::Sandbox {
                id: req.sandbox_id.clone(),
                template_id: "python-3.11".to_string(),
                status: "running".to_string(),
                config: None,
                created_at: Some(prost_types::Timestamp {
                    seconds: 1677600000,
                    nanos: 0,
                }),
                updated_at: Some(prost_types::Timestamp {
                    seconds: 1677600000,
                    nanos: 0,
                }),
                environment_variables: std::collections::HashMap::new(),
                endpoint_url: format!("https://{}.soulbox.dev", req.sandbox_id),
            };
            
            let response = super::GetSandboxResponse {
                sandbox: Some(sandbox),
            };
            Ok(Response::new(response))
        }
    }
}