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

#[derive(Debug)]
pub struct StreamingServiceImpl {
    active_streams: Arc<Mutex<HashMap<String, String>>>, // stream_id -> sandbox_id
    active_terminals: Arc<Mutex<HashMap<String, String>>>, // terminal_id -> sandbox_id
}

impl StreamingServiceImpl {
    pub fn new() -> Self {
        Self {
            active_streams: Arc::new(Mutex::new(HashMap::new())),
            active_terminals: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[tonic::async_trait]
impl streaming_service_server::StreamingService for StreamingServiceImpl {
    type SandboxStreamStream = Pin<Box<dyn Stream<Item = Result<SandboxStreamResponse, Status>> + Send>>;

    async fn sandbox_stream(
        &self,
        request: Request<Streaming<SandboxStreamRequest>>,
    ) -> Result<Response<Self::SandboxStreamStream>, Status> {
        let mut stream = request.into_inner();
        let active_streams = Arc::clone(&self.active_streams);
        
        let output_stream = async_stream::stream! {
            let mut stream_id = String::new();
            let mut sandbox_id = String::new();
            let mut stream_type = StreamType::Unspecified;

            while let Some(request_result) = stream.next().await {
                match request_result {
                    Ok(req) => {
                        match req.request {
                            Some(sandbox_stream_request::Request::Init(init)) => {
                                sandbox_id = init.sandbox_id.clone();
                                stream_type = StreamType::from_i32(init.stream_type).unwrap_or(StreamType::Unspecified);
                                stream_id = format!("stream_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
                                
                                // Store active stream
                                let mut streams = active_streams.lock().await;
                                streams.insert(stream_id.clone(), sandbox_id.clone());
                                
                                info!("Initialized sandbox stream: {} for sandbox: {}", stream_id, sandbox_id);
                                
                                yield Ok(SandboxStreamResponse {
                                    response: Some(sandbox_stream_response::Response::Ready(
                                        SandboxStreamReady {
                                            stream_id: stream_id.clone(),
                                        }
                                    ))
                                });
                            }
                            Some(sandbox_stream_request::Request::Command(command)) => {
                                // Mock command processing
                                match command.command_type.as_str() {
                                    "ping" => {
                                        yield Ok(SandboxStreamResponse {
                                            response: Some(sandbox_stream_response::Response::Output(
                                                SandboxStreamOutput {
                                                    stream_id: stream_id.clone(),
                                                    data: b"pong".to_vec(),
                                                    output_type: OutputType::Stdout as i32,
                                                }
                                            ))
                                        });
                                    }
                                    "execute" => {
                                        // Mock code execution output
                                        yield Ok(SandboxStreamResponse {
                                            response: Some(sandbox_stream_response::Response::Output(
                                                SandboxStreamOutput {
                                                    stream_id: stream_id.clone(),
                                                    data: b"Executing command...\n".to_vec(),
                                                    output_type: OutputType::Stdout as i32,
                                                }
                                            ))
                                        });
                                        
                                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                        
                                        yield Ok(SandboxStreamResponse {
                                            response: Some(sandbox_stream_response::Response::Output(
                                                SandboxStreamOutput {
                                                    stream_id: stream_id.clone(),
                                                    data: b"Command completed successfully\n".to_vec(),
                                                    output_type: OutputType::Stdout as i32,
                                                }
                                            ))
                                        });
                                    }
                                    _ => {
                                        yield Ok(SandboxStreamResponse {
                                            response: Some(sandbox_stream_response::Response::Error(
                                                SandboxStreamError {
                                                    stream_id: stream_id.clone(),
                                                    error_message: format!("Unknown command: {}", command.command_type),
                                                }
                                            ))
                                        });
                                    }
                                }
                            }
                            Some(sandbox_stream_request::Request::Data(data)) => {
                                // Echo back the data (mock processing)
                                yield Ok(SandboxStreamResponse {
                                    response: Some(sandbox_stream_response::Response::Output(
                                        SandboxStreamOutput {
                                            stream_id: stream_id.clone(),
                                            data: data.data,
                                            output_type: OutputType::Stdout as i32,
                                        }
                                    ))
                                });
                            }
                            None => {
                                yield Ok(SandboxStreamResponse {
                                    response: Some(sandbox_stream_response::Response::Error(
                                        SandboxStreamError {
                                            stream_id: stream_id.clone(),
                                            error_message: "Invalid stream request".to_string(),
                                        }
                                    ))
                                });
                            }
                        }
                    }
                    Err(e) => {
                        error!("Stream error: {}", e);
                        yield Ok(SandboxStreamResponse {
                            response: Some(sandbox_stream_response::Response::Error(
                                SandboxStreamError {
                                    stream_id: stream_id.clone(),
                                    error_message: format!("Stream error: {}", e),
                                }
                            ))
                        });
                        break;
                    }
                }
            }
            
            // Cleanup stream
            let mut streams = active_streams.lock().await;
            streams.remove(&stream_id);
            info!("Cleaned up sandbox stream: {}", stream_id);
        };

        Ok(Response::new(Box::pin(output_stream)))
    }

    type TerminalStreamStream = Pin<Box<dyn Stream<Item = Result<TerminalStreamResponse, Status>> + Send>>;

    async fn terminal_stream(
        &self,
        request: Request<Streaming<TerminalStreamRequest>>,
    ) -> Result<Response<Self::TerminalStreamStream>, Status> {
        let mut stream = request.into_inner();
        let active_terminals = Arc::clone(&self.active_terminals);
        
        let output_stream = async_stream::stream! {
            let mut terminal_id = String::new();
            let mut sandbox_id = String::new();
            let mut terminal_config: Option<TerminalConfig> = None;

            while let Some(request_result) = stream.next().await {
                match request_result {
                    Ok(req) => {
                        match req.request {
                            Some(terminal_stream_request::Request::Init(init)) => {
                                sandbox_id = init.sandbox_id.clone();
                                terminal_config = init.config;
                                terminal_id = format!("term_{}", Uuid::new_v4().to_string().replace("-", "")[..8].to_lowercase());
                                
                                // Store active terminal
                                let mut terminals = active_terminals.lock().await;
                                terminals.insert(terminal_id.clone(), sandbox_id.clone());
                                
                                info!("Initialized terminal: {} for sandbox: {}", terminal_id, sandbox_id);
                                
                                yield Ok(TerminalStreamResponse {
                                    response: Some(terminal_stream_response::Response::Ready(
                                        TerminalReady {
                                            terminal_id: terminal_id.clone(),
                                        }
                                    ))
                                });
                                
                                // Send initial shell prompt
                                let shell_prompt = if let Some(config) = &terminal_config {
                                    format!("{}@soulbox:{}$ ", "user", config.working_directory)
                                } else {
                                    "user@soulbox:/workspace$ ".to_string()
                                };
                                
                                yield Ok(TerminalStreamResponse {
                                    response: Some(terminal_stream_response::Response::Output(
                                        TerminalOutput {
                                            terminal_id: terminal_id.clone(),
                                            data: shell_prompt.as_bytes().to_vec(),
                                        }
                                    ))
                                });
                            }
                            Some(terminal_stream_request::Request::Input(input)) => {
                                let input_str = String::from_utf8_lossy(&input.data);
                                
                                // Mock terminal command processing
                                if input_str.trim() == "echo 'Hello Terminal'" {
                                    yield Ok(TerminalStreamResponse {
                                        response: Some(terminal_stream_response::Response::Output(
                                            TerminalOutput {
                                                terminal_id: terminal_id.clone(),
                                                data: b"Hello Terminal\n".to_vec(),
                                            }
                                        ))
                                    });
                                } else if input_str.trim().starts_with("ls") {
                                    yield Ok(TerminalStreamResponse {
                                        response: Some(terminal_stream_response::Response::Output(
                                            TerminalOutput {
                                                terminal_id: terminal_id.clone(),
                                                data: b"package.json  src  index.js\n".to_vec(),
                                            }
                                        ))
                                    });
                                } else if input_str.trim() == "pwd" {
                                    let working_dir = terminal_config
                                        .as_ref()
                                        .map(|c| c.working_directory.clone())
                                        .unwrap_or_else(|| "/workspace".to_string());
                                    
                                    yield Ok(TerminalStreamResponse {
                                        response: Some(terminal_stream_response::Response::Output(
                                            TerminalOutput {
                                                terminal_id: terminal_id.clone(),
                                                data: format!("{}\n", working_dir).as_bytes().to_vec(),
                                            }
                                        ))
                                    });
                                } else if input_str.trim() == "exit" {
                                    yield Ok(TerminalStreamResponse {
                                        response: Some(terminal_stream_response::Response::Closed(
                                            TerminalClosed {
                                                terminal_id: terminal_id.clone(),
                                                exit_code: 0,
                                            }
                                        ))
                                    });
                                    break;
                                } else if !input_str.trim().is_empty() {
                                    // Echo the command and simulate unknown command
                                    yield Ok(TerminalStreamResponse {
                                        response: Some(terminal_stream_response::Response::Output(
                                            TerminalOutput {
                                                terminal_id: terminal_id.clone(),
                                                data: format!("bash: {}: command not found\n", input_str.trim()).as_bytes().to_vec(),
                                            }
                                        ))
                                    });
                                }
                                
                                // Send new prompt after processing command
                                if input_str.contains('\n') && !input_str.trim().is_empty() {
                                    let shell_prompt = if let Some(config) = &terminal_config {
                                        format!("user@soulbox:{}$ ", config.working_directory)
                                    } else {
                                        "user@soulbox:/workspace$ ".to_string()
                                    };
                                    
                                    yield Ok(TerminalStreamResponse {
                                        response: Some(terminal_stream_response::Response::Output(
                                            TerminalOutput {
                                                terminal_id: terminal_id.clone(),
                                                data: shell_prompt.as_bytes().to_vec(),
                                            }
                                        ))
                                    });
                                }
                            }
                            Some(terminal_stream_request::Request::Resize(resize)) => {
                                info!("Terminal resize: {}x{}", resize.cols, resize.rows);
                                // In a real implementation, we would resize the terminal
                            }
                            None => {
                                yield Ok(TerminalStreamResponse {
                                    response: Some(terminal_stream_response::Response::Error(
                                        TerminalError {
                                            terminal_id: terminal_id.clone(),
                                            error_message: "Invalid terminal request".to_string(),
                                        }
                                    ))
                                });
                            }
                        }
                    }
                    Err(e) => {
                        error!("Terminal stream error: {}", e);
                        yield Ok(TerminalStreamResponse {
                            response: Some(terminal_stream_response::Response::Error(
                                TerminalError {
                                    terminal_id: terminal_id.clone(),
                                    error_message: format!("Terminal stream error: {}", e),
                                }
                            ))
                        });
                        break;
                    }
                }
            }
            
            // Cleanup terminal
            let mut terminals = active_terminals.lock().await;
            terminals.remove(&terminal_id);
            info!("Cleaned up terminal: {}", terminal_id);
        };

        Ok(Response::new(Box::pin(output_stream)))
    }
}