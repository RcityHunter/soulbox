// Manually defined gRPC service traits
// TODO: Replace with proper tonic-build generated code once API issue is resolved

use crate::soulbox::v1::*;
use tonic::{Request, Response, Status, Streaming};
// use std::pin::Pin; // Currently unused
use tokio_stream::Stream;

// Soul Box Service trait
#[tonic::async_trait]
pub trait SoulBoxService: Send + Sync + 'static {
    // Sandbox Management
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

    // Code Execution
    async fn execute_code(
        &self,
        request: Request<ExecuteCodeRequest>,
    ) -> Result<Response<ExecuteCodeResponse>, Status>;

    type StreamExecuteCodeStream: Stream<Item = Result<ExecuteCodeStreamResponse, Status>> 
        + Send + 'static;

    async fn stream_execute_code(
        &self,
        request: Request<ExecuteCodeRequest>,
    ) -> Result<Response<Self::StreamExecuteCodeStream>, Status>;

    // File Operations
    async fn upload_file(
        &self,
        request: Request<Streaming<UploadFileRequest>>,
    ) -> Result<Response<UploadFileResponse>, Status>;

    type DownloadFileStream: Stream<Item = Result<DownloadFileResponse, Status>>
        + Send + 'static;

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

    // Health Check
    async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status>;
}

// Streaming Service trait
#[tonic::async_trait]
pub trait StreamingService: Send + Sync + 'static {
    type SandboxStreamStream: Stream<Item = Result<SandboxStreamResponse, Status>>
        + Send + 'static;

    async fn sandbox_stream(
        &self,
        request: Request<Streaming<SandboxStreamRequest>>,
    ) -> Result<Response<Self::SandboxStreamStream>, Status>;

    type TerminalStreamStream: Stream<Item = Result<TerminalStreamResponse, Status>>
        + Send + 'static;

    async fn terminal_stream(
        &self,
        request: Request<Streaming<TerminalStreamRequest>>,
    ) -> Result<Response<Self::TerminalStreamStream>, Status>;
}

// Server implementation wrapper (mimics tonic-build generated code)
pub mod soul_box_service_server {
    use super::*;
    use tonic::codegen::*;

    // This is a simplified version of what tonic-build would generate
    pub fn new<T>(service: T) -> SoulBoxServiceServer<T>
    where
        T: SoulBoxService,
    {
        SoulBoxServiceServer::new(service)
    }

    #[derive(Debug)]
    pub struct SoulBoxServiceServer<T> {
        inner: T,
    }

    impl<T> SoulBoxServiceServer<T>
    where
        T: SoulBoxService,
    {
        pub fn new(inner: T) -> Self {
            Self { inner }
        }
    }

    impl<T> Clone for SoulBoxServiceServer<T>
    where
        T: Clone,
    {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
            }
        }
    }

    impl<T> tonic::server::NamedService for SoulBoxServiceServer<T> {
        const NAME: &'static str = "soulbox.v1.SoulBoxService";
    }

    #[tonic::async_trait]
    impl<T> tonic::server::UnaryService<CreateSandboxRequest> for SoulBoxServiceServer<T>
    where
        T: SoulBoxService + Clone,
    {
        type Response = CreateSandboxResponse;
        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

        fn call(&mut self, request: tonic::Request<CreateSandboxRequest>) -> Self::Future {
            let inner = self.inner.clone();
            Box::pin(async move { inner.create_sandbox(request).await })
        }
    }

    // NOTE: This is a minimal implementation
    // In a complete implementation, we would need to implement all service methods
    // For now, we'll implement them directly on the service struct
}

pub mod streaming_service_server {
    use super::*;

    pub fn new<T>(service: T) -> StreamingServiceServer<T>
    where
        T: StreamingService,
    {
        StreamingServiceServer::new(service)
    }

    #[derive(Debug)]
    pub struct StreamingServiceServer<T> {
        inner: T,
    }

    impl<T> StreamingServiceServer<T>
    where
        T: StreamingService,
    {
        pub fn new(inner: T) -> Self {
            Self { inner }
        }
    }

    impl<T> tonic::server::NamedService for StreamingServiceServer<T> {
        const NAME: &'static str = "soulbox.v1.StreamingService";
    }
}