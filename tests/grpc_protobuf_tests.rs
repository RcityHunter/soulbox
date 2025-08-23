use soulbox::soulbox::v1::*;
use soulbox::grpc::service_traits::StreamingService;
use soulbox::grpc::streaming_service::StreamingServiceImpl;
use tonic::{Request, Status};
use tokio_stream::StreamExt;

#[tokio::test]
async fn test_protobuf_generation() {
    // Test that protobuf types are properly generated and usable
    let sandbox_request = CreateSandboxRequest {
        template_id: "test-template".to_string(),
        config: Some(SandboxConfig {
            resource_limits: Some(ResourceLimits {
                memory_mb: 512,
                cpu_cores: 1.0,
                disk_mb: 1024,
                max_processes: 10,
            }),
            network_config: Some(NetworkConfig {
                exposed_ports: vec![8080, 3000],
                enable_port_forwarding: true,
                subnet: "10.0.0.0/24".to_string(),
            }),
            allowed_domains: vec!["example.com".to_string()],
            enable_internet: true,
        }),
        environment_variables: std::collections::HashMap::new(),
        timeout: None,
    };

    // Verify fields are accessible
    assert_eq!(sandbox_request.template_id, "test-template");
    assert!(sandbox_request.config.is_some());
    
    let config = sandbox_request.config.unwrap();
    assert!(config.resource_limits.is_some());
    assert!(config.network_config.is_some());
    assert_eq!(config.allowed_domains.len(), 1);
    assert!(config.enable_internet);
}

#[tokio::test]
async fn test_streaming_service_creation() {
    // Test that the streaming service can be created
    let service = StreamingServiceImpl::new();
    
    // This test mainly verifies that the service compiles and can be instantiated
    assert!(format!("{:?}", service).contains("StreamingServiceImpl"));
}

#[tokio::test] 
async fn test_sandbox_stream_message_types() {
    // Test all the sandbox stream message types
    let init = SandboxStreamInit {
        sandbox_id: "test-sandbox".to_string(),
        stream_type: StreamType::Process as i32,
    };
    
    let command = SandboxStreamCommand {
        command_type: "execute".to_string(),
        parameters: None,
    };
    
    let data = SandboxStreamData {
        data: b"hello world".to_vec(),
    };
    
    let close = SandboxStreamClose {
        reason: "test complete".to_string(),
    };
    
    // Test that all request types can be constructed
    let _requests = vec![
        SandboxStreamRequest {
            request: Some(sandbox_stream_request::Request::Init(init)),
        },
        SandboxStreamRequest {
            request: Some(sandbox_stream_request::Request::Command(command)),
        },
        SandboxStreamRequest {
            request: Some(sandbox_stream_request::Request::Data(data)),
        },
        SandboxStreamRequest {
            request: Some(sandbox_stream_request::Request::Close(close)),
        },
    ];
}

#[tokio::test]
async fn test_terminal_stream_message_types() {
    // Test all the terminal stream message types
    let init = TerminalInit {
        sandbox_id: "test-sandbox".to_string(),
        config: Some(TerminalConfig {
            rows: 24,
            cols: 80,
            shell: "/bin/bash".to_string(),
            working_directory: "/workspace".to_string(),
            environment_variables: std::collections::HashMap::new(),
        }),
    };
    
    let input = TerminalInput {
        terminal_id: "test-terminal".to_string(),
        data: b"ls -la\n".to_vec(),
    };
    
    let resize = TerminalResize {
        terminal_id: "test-terminal".to_string(),
        rows: 30,
        cols: 120,
    };
    
    let close = TerminalClose {
        reason: "session ended".to_string(),
    };
    
    // Test that all request types can be constructed
    let _requests = vec![
        TerminalStreamRequest {
            request: Some(terminal_stream_request::Request::Init(init)),
        },
        TerminalStreamRequest {
            request: Some(terminal_stream_request::Request::Input(input)),
        },
        TerminalStreamRequest {
            request: Some(terminal_stream_request::Request::Resize(resize)),
        },
        TerminalStreamRequest {
            request: Some(terminal_stream_request::Request::Close(close)),
        },
    ];
}

#[tokio::test]
async fn test_response_message_types() {
    // Test sandbox stream responses
    let ready = SandboxStreamReady {
        stream_id: "stream-123".to_string(),
    };
    
    let output = SandboxStreamOutput {
        stream_id: "stream-123".to_string(),
        data: b"command output".to_vec(),
        output_type: OutputType::Stdout as i32,
    };
    
    let error = SandboxStreamError {
        stream_id: "stream-123".to_string(),
        error_message: "Command failed".to_string(),
    };
    
    let closed = SandboxStreamClosed {
        reason: "Stream closed".to_string(),
    };
    
    // Test terminal stream responses
    let terminal_ready = TerminalReady {
        terminal_id: "term-456".to_string(),
    };
    
    let terminal_output = TerminalOutput {
        terminal_id: "term-456".to_string(),
        data: b"terminal output".to_vec(),
    };
    
    let terminal_closed = TerminalClosed {
        terminal_id: "term-456".to_string(),
        exit_code: 0,
    };
    
    let terminal_error = TerminalError {
        terminal_id: "term-456".to_string(),
        error_message: "Terminal error".to_string(),
    };
    
    // Verify types can be constructed
    assert_eq!(ready.stream_id, "stream-123");
    assert_eq!(output.data, b"command output");
    assert_eq!(error.error_message, "Command failed");
    assert_eq!(closed.reason, "Stream closed");
    
    assert_eq!(terminal_ready.terminal_id, "term-456");
    assert_eq!(terminal_output.data, b"terminal output");
    assert_eq!(terminal_closed.exit_code, 0);
    assert_eq!(terminal_error.error_message, "Terminal error");
}

#[tokio::test]
async fn test_file_descriptor_set() {
    // Test that the file descriptor set is available
    use soulbox::FILE_DESCRIPTOR_SET;
    
    // The descriptor set should not be empty
    assert!(!FILE_DESCRIPTOR_SET.is_empty(), "File descriptor set should contain protobuf descriptors");
    
    // It should start with the protobuf file descriptor format
    // File descriptor sets typically start with specific bytes
    assert!(FILE_DESCRIPTOR_SET.len() > 10, "File descriptor set should be substantial in size");
}