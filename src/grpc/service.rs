use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn, error};
use uuid::Uuid;

// Import generated protobuf types
mod soulbox_proto {
    tonic::include_proto!("soulbox.v1");
}

pub use soulbox_proto::*;

// Mock sandbox storage for testing
#[derive(Debug, Clone)]
pub struct MockSandbox {
    pub id: String,
    pub template_id: String,
    pub status: String,
    pub config: Option<SandboxConfig>,
    pub created_at: Option<prost_types::Timestamp>,
    pub updated_at: Option<prost_types::Timestamp>,
    pub environment_variables: HashMap<String, String>,
    pub endpoint_url: String,
}

#[derive(Debug)]
pub struct SoulBoxServiceImpl {
    sandboxes: Arc<Mutex<HashMap<String, MockSandbox>>>,
    executions: Arc<Mutex<HashMap<String, String>>>, // execution_id -> sandbox_id
}

impl SoulBoxServiceImpl {
    pub fn new() -> Self {
        Self {
            sandboxes: Arc::new(Mutex::new(HashMap::new())),
            executions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn create_mock_sandbox(&self, request: &CreateSandboxRequest) -> MockSandbox {
        let sandbox_id = format!("sb_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        
        let timestamp = prost_types::Timestamp {
            seconds: now.as_secs() as i64,
            nanos: now.subsec_nanos() as i32,
        };

        MockSandbox {
            id: sandbox_id.clone(),
            template_id: request.template_id.clone(),
            status: "running".to_string(),
            config: request.config.clone(),
            created_at: Some(timestamp.clone()),
            updated_at: Some(timestamp),
            environment_variables: request.environment_variables.clone(),
            endpoint_url: format!("https://sandbox-{}.soulbox.dev", sandbox_id),
        }
    }
}

#[tonic::async_trait]
impl soul_box_service_server::SoulBoxService for SoulBoxServiceImpl {
    async fn create_sandbox(
        &self,
        request: Request<CreateSandboxRequest>,
    ) -> Result<Response<CreateSandboxResponse>, Status> {
        let req = request.into_inner();
        info!("Creating sandbox with template: {}", req.template_id);

        // Validate request
        if req.template_id.is_empty() {
            return Err(Status::invalid_argument("Template ID is required"));
        }

        // Create mock sandbox
        let sandbox = self.create_mock_sandbox(&req).await;
        
        // Store sandbox
        let mut sandboxes = self.sandboxes.lock().await;
        sandboxes.insert(sandbox.id.clone(), sandbox.clone());

        let response = CreateSandboxResponse {
            sandbox_id: sandbox.id,
            status: sandbox.status,
            created_at: sandbox.created_at,
            endpoint_url: sandbox.endpoint_url,
        };

        Ok(Response::new(response))
    }

    async fn get_sandbox(
        &self,
        request: Request<GetSandboxRequest>,
    ) -> Result<Response<GetSandboxResponse>, Status> {
        let req = request.into_inner();
        
        if req.sandbox_id.is_empty() {
            return Err(Status::invalid_argument("Sandbox ID is required"));
        }

        let sandboxes = self.sandboxes.lock().await;
        let sandbox = sandboxes.get(&req.sandbox_id)
            .ok_or_else(|| Status::not_found("Sandbox not found"))?;

        let response = GetSandboxResponse {
            sandbox: Some(Sandbox {
                id: sandbox.id.clone(),
                template_id: sandbox.template_id.clone(),
                status: sandbox.status.clone(),
                config: sandbox.config.clone(),
                created_at: sandbox.created_at.clone(),
                updated_at: sandbox.updated_at.clone(),
                environment_variables: sandbox.environment_variables.clone(),
                endpoint_url: sandbox.endpoint_url.clone(),
            }),
        };

        Ok(Response::new(response))
    }

    async fn list_sandboxes(
        &self,
        request: Request<ListSandboxesRequest>,
    ) -> Result<Response<ListSandboxesResponse>, Status> {
        let req = request.into_inner();
        let page = req.page.max(1);
        let page_size = req.page_size.clamp(1, 100);

        let sandboxes = self.sandboxes.lock().await;
        let all_sandboxes: Vec<_> = sandboxes.values().collect();
        
        let total_count = all_sandboxes.len() as i32;
        let start_index = ((page - 1) * page_size) as usize;
        let end_index = (start_index + page_size as usize).min(all_sandboxes.len());

        let page_sandboxes = all_sandboxes[start_index..end_index]
            .iter()
            .map(|sandbox| Sandbox {
                id: sandbox.id.clone(),
                template_id: sandbox.template_id.clone(),
                status: sandbox.status.clone(),
                config: sandbox.config.clone(),
                created_at: sandbox.created_at.clone(),
                updated_at: sandbox.updated_at.clone(),
                environment_variables: sandbox.environment_variables.clone(),
                endpoint_url: sandbox.endpoint_url.clone(),
            })
            .collect();

        let response = ListSandboxesResponse {
            sandboxes: page_sandboxes,
            total_count,
            page,
            page_size,
        };

        Ok(Response::new(response))
    }

    async fn delete_sandbox(
        &self,
        request: Request<DeleteSandboxRequest>,
    ) -> Result<Response<DeleteSandboxResponse>, Status> {
        let req = request.into_inner();
        
        if req.sandbox_id.is_empty() {
            return Err(Status::invalid_argument("Sandbox ID is required"));
        }

        let mut sandboxes = self.sandboxes.lock().await;
        let removed = sandboxes.remove(&req.sandbox_id).is_some();

        if removed {
            info!("Deleted sandbox: {}", req.sandbox_id);
            Ok(Response::new(DeleteSandboxResponse {
                success: true,
                message: "Sandbox deleted successfully".to_string(),
            }))
        } else {
            Err(Status::not_found("Sandbox not found"))
        }
    }

    async fn execute_code(
        &self,
        request: Request<ExecuteCodeRequest>,
    ) -> Result<Response<ExecuteCodeResponse>, Status> {
        let req = request.into_inner();
        
        // Validate request
        if req.sandbox_id.is_empty() {
            return Err(Status::invalid_argument("Sandbox ID is required"));
        }
        if req.code.is_empty() {
            return Err(Status::invalid_argument("Code is required"));
        }

        // Check if sandbox exists
        let sandboxes = self.sandboxes.lock().await;
        if !sandboxes.contains_key(&req.sandbox_id) {
            return Err(Status::not_found("Sandbox not found"));
        }
        drop(sandboxes);

        // Generate execution ID
        let execution_id = format!("exec_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
        
        // Store execution
        let mut executions = self.executions.lock().await;
        executions.insert(execution_id.clone(), req.sandbox_id.clone());
        drop(executions);

        // Mock execution based on language and code
        let (stdout, stderr, exit_code) = match req.language.as_str() {
            "javascript" => {
                if req.code.contains("console.log") {
                    // Extract console.log messages
                    let logs: Vec<&str> = req.code
                        .split("console.log(")
                        .skip(1)
                        .map(|part| {
                            let end = part.find(')').unwrap_or(part.len());
                            &part[..end]
                        })
                        .collect();
                    
                    let output = logs.iter()
                        .map(|log| log.trim_matches(['\'', '"']))
                        .collect::<Vec<_>>()
                        .join("\n");
                    
                    (output, String::new(), 0)
                } else {
                    (String::new(), String::new(), 0)
                }
            }
            "python" => {
                if req.code.contains("print") {
                    ("Hello from Python".to_string(), String::new(), 0)
                } else {
                    (String::new(), String::new(), 0)
                }
            }
            _ => {
                return Err(Status::invalid_argument("Unsupported language"));
            }
        };

        let execution_time = prost_types::Duration {
            seconds: 0,
            nanos: 100_000_000, // 100ms
        };

        let response = ExecuteCodeResponse {
            execution_id,
            stdout,
            stderr,
            exit_code,
            execution_time: Some(execution_time),
            timed_out: false,
        };

        Ok(Response::new(response))
    }

    type StreamExecuteCodeStream = Pin<Box<dyn Stream<Item = Result<ExecuteCodeStreamResponse, Status>> + Send>>;

    async fn stream_execute_code(
        &self,
        request: Request<ExecuteCodeRequest>,
    ) -> Result<Response<Self::StreamExecuteCodeStream>, Status> {
        let req = request.into_inner();
        
        // Validate request
        if req.sandbox_id.is_empty() {
            return Err(Status::invalid_argument("Sandbox ID is required"));
        }

        // Check if sandbox exists
        let sandboxes = self.sandboxes.lock().await;
        if !sandboxes.contains_key(&req.sandbox_id) {
            return Err(Status::not_found("Sandbox not found"));
        }
        drop(sandboxes);

        let execution_id = format!("exec_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
        
        // Create streaming response
        let stream = async_stream::stream! {
            // Send execution started
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap();
            
            let timestamp = prost_types::Timestamp {
                seconds: now.as_secs() as i64,
                nanos: now.subsec_nanos() as i32,
            };

            yield Ok(ExecuteCodeStreamResponse {
                response: Some(execute_code_stream_response::Response::Started(
                    ExecutionStarted {
                        execution_id: execution_id.clone(),
                        started_at: Some(timestamp),
                    }
                ))
            });

            // Simulate streaming output
            if req.language == "javascript" && req.code.contains("Step") {
                for i in 1..=3 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    
                    yield Ok(ExecuteCodeStreamResponse {
                        response: Some(execute_code_stream_response::Response::Output(
                            ExecutionOutput {
                                execution_id: execution_id.clone(),
                                r#type: OutputType::Stdout as i32,
                                data: format!("Step {}\n", i),
                            }
                        ))
                    });
                }
            }

            // Send execution completed
            yield Ok(ExecuteCodeStreamResponse {
                response: Some(execute_code_stream_response::Response::Completed(
                    ExecutionCompleted {
                        execution_id: execution_id.clone(),
                        exit_code: 0,
                        execution_time: Some(prost_types::Duration {
                            seconds: 0,
                            nanos: 300_000_000, // 300ms
                        }),
                    }
                ))
            });
        };

        Ok(Response::new(Box::pin(stream)))
    }

    async fn upload_file(
        &self,
        request: Request<Streaming<UploadFileRequest>>,
    ) -> Result<Response<UploadFileResponse>, Status> {
        let mut stream = request.into_inner();
        let mut sandbox_id = String::new();
        let mut file_path = String::new();
        let mut expected_size = 0u64;
        let mut bytes_uploaded = 0u64;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            
            match chunk.request {
                Some(upload_file_request::Request::Metadata(metadata)) => {
                    sandbox_id = metadata.sandbox_id;
                    file_path = metadata.file_path;
                    expected_size = metadata.file_size;
                    
                    // Validate sandbox exists
                    let sandboxes = self.sandboxes.lock().await;
                    if !sandboxes.contains_key(&sandbox_id) {
                        return Err(Status::not_found("Sandbox not found"));
                    }
                }
                Some(upload_file_request::Request::Chunk(data)) => {
                    bytes_uploaded += data.len() as u64;
                    // In a real implementation, we would write the chunk to the file system
                }
                None => {
                    return Err(Status::invalid_argument("Invalid upload request"));
                }
            }
        }

        if bytes_uploaded != expected_size {
            warn!("Upload size mismatch: expected {}, got {}", expected_size, bytes_uploaded);
        }

        let response = UploadFileResponse {
            success: true,
            message: "File uploaded successfully".to_string(),
            file_path,
            bytes_uploaded,
        };

        Ok(Response::new(response))
    }

    type DownloadFileStream = Pin<Box<dyn Stream<Item = Result<DownloadFileResponse, Status>> + Send>>;

    async fn download_file(
        &self,
        request: Request<DownloadFileRequest>,
    ) -> Result<Response<Self::DownloadFileStream>, Status> {
        let req = request.into_inner();
        
        // Validate request
        if req.sandbox_id.is_empty() {
            return Err(Status::invalid_argument("Sandbox ID is required"));
        }

        // Check if sandbox exists
        let sandboxes = self.sandboxes.lock().await;
        if !sandboxes.contains_key(&req.sandbox_id) {
            return Err(Status::not_found("Sandbox not found"));
        }
        drop(sandboxes);

        // Mock file content
        let file_content = b"Hello, SoulBox file system!";
        
        let stream = async_stream::stream! {
            // Send metadata first
            yield Ok(DownloadFileResponse {
                response: Some(download_file_response::Response::Metadata(
                    FileDownloadMetadata {
                        file_path: req.file_path.clone(),
                        file_size: file_content.len() as u64,
                        content_type: "text/plain".to_string(),
                    }
                ))
            });

            // Send file content in chunks
            let chunk_size = 1024;
            for chunk in file_content.chunks(chunk_size) {
                yield Ok(DownloadFileResponse {
                    response: Some(download_file_response::Response::Chunk(chunk.to_vec()))
                });
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    async fn list_files(
        &self,
        request: Request<ListFilesRequest>,
    ) -> Result<Response<ListFilesResponse>, Status> {
        let req = request.into_inner();
        
        // Validate request
        if req.sandbox_id.is_empty() {
            return Err(Status::invalid_argument("Sandbox ID is required"));
        }

        // Check if sandbox exists
        let sandboxes = self.sandboxes.lock().await;
        if !sandboxes.contains_key(&req.sandbox_id) {
            return Err(Status::not_found("Sandbox not found"));
        }
        drop(sandboxes);

        // Mock file listing
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        
        let timestamp = prost_types::Timestamp {
            seconds: now.as_secs() as i64,
            nanos: now.subsec_nanos() as i32,
        };

        let files = vec![
            FileInfo {
                name: "package.json".to_string(),
                path: "/workspace/package.json".to_string(),
                size: 324,
                is_directory: false,
                modified_at: Some(timestamp.clone()),
                permissions: "644".to_string(),
            },
            FileInfo {
                name: "src".to_string(),
                path: "/workspace/src".to_string(),
                size: 4096,
                is_directory: true,
                modified_at: Some(timestamp.clone()),
                permissions: "755".to_string(),
            },
            FileInfo {
                name: "index.js".to_string(),
                path: "/workspace/src/index.js".to_string(),
                size: 156,
                is_directory: false,
                modified_at: Some(timestamp),
                permissions: "644".to_string(),
            },
        ];

        let response = ListFilesResponse { files };
        Ok(Response::new(response))
    }

    async fn delete_file(
        &self,
        request: Request<DeleteFileRequest>,
    ) -> Result<Response<DeleteFileResponse>, Status> {
        let req = request.into_inner();
        
        // Validate request
        if req.sandbox_id.is_empty() {
            return Err(Status::invalid_argument("Sandbox ID is required"));
        }
        if req.file_path.is_empty() {
            return Err(Status::invalid_argument("File path is required"));
        }

        // Check if sandbox exists
        let sandboxes = self.sandboxes.lock().await;
        if !sandboxes.contains_key(&req.sandbox_id) {
            return Err(Status::not_found("Sandbox not found"));
        }
        drop(sandboxes);

        // Mock file deletion
        let response = DeleteFileResponse {
            success: true,
            message: format!("File {} deleted successfully", req.file_path),
        };

        Ok(Response::new(response))
    }

    async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let req = request.into_inner();
        
        let mut details = HashMap::new();
        details.insert("version".to_string(), "0.1.0".to_string());
        details.insert("uptime".to_string(), "running".to_string());
        
        if req.service.is_empty() || req.service == "soulbox" {
            let response = HealthCheckResponse {
                status: HealthStatus::Serving as i32,
                message: "SoulBox service is healthy".to_string(),
                details,
            };
            Ok(Response::new(response))
        } else {
            let response = HealthCheckResponse {
                status: HealthStatus::Unknown as i32,
                message: format!("Unknown service: {}", req.service),
                details,
            };
            Ok(Response::new(response))
        }
    }
}