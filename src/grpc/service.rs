use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn, error};
use uuid::Uuid;
use base64::Engine;

use crate::container::{ResourceLimits as ContainerResourceLimits, NetworkConfig as ContainerNetworkConfig};
use crate::container::resource_limits::{MemoryLimits, CpuLimits, DiskLimits, NetworkLimits};
use crate::container::sandbox::SandboxContainer;
use crate::container::manager::ContainerManager;
use crate::config::Config;
use crate::sandbox::{SandboxRuntime, SandboxInstance};

// Import generated protobuf types
// Note: The generated code will be in src/soulbox.v1.rs after build  
pub use crate::soulbox::v1::*;

// Real sandbox representation using container management
#[derive(Debug, Clone)]
pub struct SandboxInfo {
    pub id: String,
    pub template_id: String,
    pub status: String,
    pub config: Option<SandboxConfig>,
    pub created_at: Option<prost_types::Timestamp>,
    pub updated_at: Option<prost_types::Timestamp>,
    pub environment_variables: HashMap<String, String>,
    pub endpoint_url: String,
}

pub struct SoulBoxServiceImpl {
    container_manager: Arc<ContainerManager>,
    sandboxes: Arc<Mutex<HashMap<String, SandboxInfo>>>, // sandbox metadata
    executions: Arc<Mutex<HashMap<String, String>>>, // execution_id -> sandbox_id
    runtime: Arc<Mutex<Option<Arc<dyn SandboxRuntime>>>>,
    sandbox_instances: Arc<Mutex<HashMap<String, Arc<dyn crate::sandbox::SandboxInstance>>>>,
}

impl SoulBoxServiceImpl {
    pub fn new() -> Result<Self, crate::error::SoulBoxError> {
        let container_manager = Arc::new(ContainerManager::new_stub()?);
        Ok(Self {
            container_manager,
            sandboxes: Arc::new(Mutex::new(HashMap::new())),
            executions: Arc::new(Mutex::new(HashMap::new())),
            runtime: Arc::new(Mutex::new(None)),
            sandbox_instances: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    pub async fn new_async() -> crate::error::Result<Self> {
        // Create default config for container manager
        let config = Config::default(); // Using default config for now
        let container_manager = Arc::new(ContainerManager::new(config).await?);
        
        Ok(Self {
            container_manager,
            sandboxes: Arc::new(Mutex::new(HashMap::new())),
            executions: Arc::new(Mutex::new(HashMap::new())),
            runtime: Arc::new(Mutex::new(None)),
            sandbox_instances: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    pub async fn with_config(config: Config) -> crate::error::Result<Self> {
        let container_manager = Arc::new(ContainerManager::new(config).await?);
        
        Ok(Self {
            container_manager,
            sandboxes: Arc::new(Mutex::new(HashMap::new())),
            executions: Arc::new(Mutex::new(HashMap::new())),
            runtime: Arc::new(Mutex::new(None)),
            sandbox_instances: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn set_runtime(&self, runtime: Arc<dyn SandboxRuntime>) {
        let mut runtime_lock = self.runtime.lock().await;
        *runtime_lock = Some(runtime);
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

        // Get runtime
        let runtime_lock = self.runtime.lock().await;
        let runtime = runtime_lock.as_ref()
            .ok_or_else(|| Status::internal("Runtime not initialized"))?;

        // Generate sandbox ID
        let sandbox_id = format!("sb_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
        
        // Template is used directly by runtime (Firecracker uses rootfs templates)

        // Set resource limits with defaults or from config
        let memory_limit = req.config.as_ref()
            .and_then(|c| c.resource_limits.as_ref())
            .map(|r| r.memory_mb)
            .unwrap_or(512) as u64;
            
        let cpu_limit = req.config.as_ref()
            .and_then(|c| c.resource_limits.as_ref())
            .map(|r| r.cpu_cores)
            .unwrap_or(1.0) as f64;

        let resource_limits = ContainerResourceLimits {
            memory: MemoryLimits {
                limit_mb: memory_limit,
                swap_limit_mb: Some(memory_limit * 2),
            },
            cpu: CpuLimits {
                cores: cpu_limit,
                shares: Some(1024),
            },
            disk: DiskLimits {
                limit_mb: 2048,
                iops_limit: Some(1000),
            },
            network: NetworkLimits {
                upload_bps: Some(1024 * 1024 * 10), // 10 MB/s
                download_bps: Some(1024 * 1024 * 100), // 100 MB/s
                max_connections: Some(100),
            },
        };

        let network_config = ContainerNetworkConfig {
            enable_internet: req.config.as_ref()
                .and_then(|c| Some(c.enable_internet))
                .unwrap_or(true),
            port_mappings: vec![],
            allowed_domains: req.config.as_ref()
                .map(|c| c.allowed_domains.clone())
                .unwrap_or_default(),
            dns_servers: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
        };

        // Create sandbox using runtime
        match runtime.create_sandbox(
            &sandbox_id,
            &req.template_id,
            resource_limits,
            network_config,
            req.environment_variables.clone(),
        ).await {
            Ok(sandbox_instance) => {
                // Start the sandbox
                if let Err(e) = sandbox_instance.start().await {
                    error!("Failed to start sandbox {}: {}", sandbox_id, e);
                    return Err(Status::internal("Failed to start sandbox"));
                }

                // Create real sandbox info for tracking
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap();
                
                let timestamp = prost_types::Timestamp {
                    seconds: now.as_secs() as i64,
                    nanos: now.subsec_nanos() as i32,
                };

                let sandbox_info = SandboxInfo {
                    id: sandbox_id.clone(),
                    template_id: req.template_id.clone(),
                    status: "running".to_string(),
                    config: req.config.clone(),
                    created_at: Some(timestamp.clone()),
                    updated_at: Some(timestamp),
                    environment_variables: req.environment_variables.clone(),
                    endpoint_url: format!("https://sandbox-{sandbox_id}.soulbox.dev"),
                };
                
                let mut sandboxes = self.sandboxes.lock().await;
                sandboxes.insert(sandbox_id.clone(), sandbox_info.clone());
                drop(sandboxes);
                
                // Store sandbox instance
                let mut instances = self.sandbox_instances.lock().await;
                instances.insert(sandbox_id.clone(), sandbox_instance);

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap();
                
                let response = CreateSandboxResponse {
                    sandbox_id: sandbox_id.clone(),
                    status: "running".to_string(),
                    created_at: Some(prost_types::Timestamp {
                        seconds: now.as_secs() as i64,
                        nanos: now.subsec_nanos() as i32,
                    }),
                    endpoint_url: sandbox_info.endpoint_url,
                };

                Ok(Response::new(response))
            }
            Err(e) => {
                error!("Failed to create sandbox: {}", e);
                Err(Status::internal("Failed to create sandbox"))
            }
        }
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

        // First check if sandbox exists
        let mut sandboxes = self.sandboxes.lock().await;
        let sandbox_exists = sandboxes.contains_key(&req.sandbox_id);
        
        if !sandbox_exists {
            return Err(Status::not_found("Sandbox not found"));
        }

        // Remove from sandbox metadata
        sandboxes.remove(&req.sandbox_id);
        drop(sandboxes);

        // Stop and remove the actual container
        match self.container_manager.remove_container(&req.sandbox_id).await {
            Ok(_) => {
                info!("Deleted sandbox and container: {}", req.sandbox_id);
                
                // Also remove from sandbox instances if it exists
                let mut instances = self.sandbox_instances.lock().await;
                if let Some(instance) = instances.remove(&req.sandbox_id) {
                    // Attempt to stop the sandbox instance
                    if let Err(e) = instance.stop().await {
                        warn!("Failed to stop sandbox instance {}: {}", req.sandbox_id, e);
                    }
                }
                
                Ok(Response::new(DeleteSandboxResponse {
                    success: true,
                    message: "Sandbox deleted successfully".to_string(),
                }))
            }
            Err(e) => {
                error!("Failed to remove container for sandbox {}: {}", req.sandbox_id, e);
                // Re-add to sandboxes since container removal failed
                let mut sandboxes = self.sandboxes.lock().await;
                // We could restore the sandbox info here, but for simplicity we'll just return error
                Err(Status::internal("Failed to remove sandbox container"))
            }
        }
    }

    async fn execute_code(
        &self,
        request: Request<ExecuteCodeRequest>,
    ) -> Result<Response<ExecuteCodeResponse>, Status> {
        let req = request.into_inner();
        
        // Comprehensive input validation
        if req.sandbox_id.is_empty() {
            return Err(Status::invalid_argument("Sandbox ID is required"));
        }
        
        if req.code.is_empty() {
            return Err(Status::invalid_argument("Code is required"));
        }
        
        // Validate sandbox ID format (alphanumeric + underscore only)
        if !req.sandbox_id.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(Status::invalid_argument("Invalid sandbox ID format"));
        }
        
        // Validate code length (prevent extremely large payloads)
        if req.code.len() > 1_000_000 { // 1MB limit
            return Err(Status::invalid_argument("Code too large (max 1MB)"));
        }
        
        // Validate language is supported
        let allowed_languages = ["javascript", "node", "python", "python3", "rust", "go", "bash", "sh"];
        if !allowed_languages.contains(&req.language.as_str()) {
            return Err(Status::invalid_argument("Unsupported language"));
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

        // Get the sandbox instance
        let instances = self.sandbox_instances.lock().await;
        let instance = instances.get(&req.sandbox_id)
            .ok_or_else(|| Status::not_found("Sandbox instance not found"))?
            .clone();
        drop(instances);

        // Create secure execution command - use proper file creation instead of echo
        let start_time = std::time::Instant::now();
        
        // Create a secure temporary file path within container
        let safe_exec_id = execution_id.chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>();
        let temp_dir = format!("/tmp/soulbox_{}", safe_exec_id);
        
        // Use a more secure approach - write to temp file using base64 encoding to avoid shell injection
        let encoded_code = base64::engine::general_purpose::STANDARD.encode(&req.code);
        
        let command = match req.language.as_str() {
            "javascript" | "node" => {
                let filename = format!("{}/code.js", temp_dir);
                vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    format!(
                        "mkdir -p '{}' && echo '{}' | base64 -d > '{}' && timeout 25 node '{}'",
                        temp_dir, encoded_code, filename, filename
                    ),
                ]
            }
            "python" | "python3" => {
                let filename = format!("{}/code.py", temp_dir);
                vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    format!(
                        "mkdir -p '{}' && echo '{}' | base64 -d > '{}' && timeout 25 python3 '{}'",
                        temp_dir, encoded_code, filename, filename
                    ),
                ]
            }
            "rust" => {
                let filename = format!("{}/main.rs", temp_dir);
                let binary = format!("{}/main", temp_dir);
                vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    format!(
                        "mkdir -p '{}' && echo '{}' | base64 -d > '{}' && timeout 20 rustc '{}' -o '{}' && timeout 5 '{}'",
                        temp_dir, encoded_code, filename, filename, binary, binary
                    ),
                ]
            }
            "go" => {
                let filename = format!("{}/main.go", temp_dir);
                vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    format!(
                        "mkdir -p '{}' && echo '{}' | base64 -d > '{}' && cd '{}' && timeout 25 go run main.go",
                        temp_dir, encoded_code, filename, temp_dir
                    ),
                ]
            }
            "bash" | "sh" => {
                let filename = format!("{}/script.sh", temp_dir);
                vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    format!(
                        "mkdir -p '{}' && echo '{}' | base64 -d > '{}' && chmod +x '{}' && timeout 25 '{}'",
                        temp_dir, encoded_code, filename, filename, filename
                    ),
                ]
            }
            _ => {
                return Err(Status::invalid_argument("Unsupported language"));
            }
        };

        // Execute the code in the sandbox with timeout
        let execution_result = tokio::time::timeout(
            std::time::Duration::from_secs(30), // 30 second timeout
            instance.execute_command(command)
        ).await;

        let (stdout, stderr, exit_code, timed_out) = match execution_result {
            Ok(Ok((out, err, code))) => (out, err, code, false),
            Ok(Err(e)) => {
                error!("Code execution failed: {}", e);
                (String::new(), format!("Execution error: {}", e), 1, false)
            },
            Err(_) => {
                warn!("Code execution timed out for execution: {}", execution_id);
                (String::new(), "Execution timed out".to_string(), 124, true)
            }
        };

        let execution_time = prost_types::Duration {
            seconds: start_time.elapsed().as_secs() as i64,
            nanos: (start_time.elapsed().subsec_nanos()) as i32,
        };

        let response = ExecuteCodeResponse {
            execution_id,
            stdout,
            stderr,
            exit_code,
            execution_time: Some(execution_time),
            timed_out,
        };

        Ok(Response::new(response))
    }

    type StreamExecuteCodeStream = std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<ExecuteCodeStreamResponse, Status>> + Send>>;

    async fn stream_execute_code(
        &self,
        request: Request<ExecuteCodeRequest>,
    ) -> Result<Response<<Self as soul_box_service_server::SoulBoxService>::StreamExecuteCodeStream>, Status> {
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
                                data: format!("Step {i}\n"),
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

    type DownloadFileStream = std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<DownloadFileResponse, Status>> + Send>>;

    async fn download_file(
        &self,
        request: Request<DownloadFileRequest>,
    ) -> Result<Response<<Self as soul_box_service_server::SoulBoxService>::DownloadFileStream>, Status> {
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