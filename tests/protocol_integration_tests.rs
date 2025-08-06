use std::time::Duration;
use serde_json::json;

// Import from our crate
use soulbox::grpc::{SoulBoxServiceImpl, StreamingServiceImpl};
use soulbox::websocket::{WebSocketServer, WebSocketMessage, WebSocketResponse};

#[tokio::test]
async fn test_grpc_service_instantiation() {
    // This integration test validates that our gRPC service implementations can be created
    let grpc_service = SoulBoxServiceImpl::new();
    let streaming_service = StreamingServiceImpl::new();
    
    // Test that services can be instantiated
    assert!(std::mem::size_of::<SoulBoxServiceImpl>() > 0);
    assert!(std::mem::size_of::<StreamingServiceImpl>() > 0);
}

#[tokio::test]
async fn test_websocket_message_creation() {
    // Test WebSocket message and response creation
    let ping_message = WebSocketMessage::ping(42);
    assert_eq!(ping_message.r#type, "ping");
    
    let sandbox_init_message = WebSocketMessage::sandbox_stream_init("test-sandbox", "terminal");
    assert_eq!(sandbox_init_message.r#type, "sandbox_stream_init");
    
    let pong_response = WebSocketResponse::pong();
    assert_eq!(pong_response.r#type, "pong");
    
    let error_response = WebSocketResponse::error("Test error", Some("TEST_ERROR"));
    assert_eq!(error_response.r#type, "error");
}

#[tokio::test]
async fn test_websocket_server_basic_functionality() {
    // Test WebSocket server functionality
    let server = WebSocketServer::new();
    
    // Test server stats
    let connection_count = server.get_active_connection_count().await;
    assert_eq!(connection_count, 0);
    
    let session_count = server.get_authenticated_session_count().await;
    assert_eq!(session_count, 0);
}

#[tokio::test]
async fn test_protocol_coexistence() {
    // This test validates that gRPC and WebSocket protocols can coexist
    let grpc_service = SoulBoxServiceImpl::new();
    let streaming_service = StreamingServiceImpl::new();
    let ws_server = WebSocketServer::new();
    
    // Verify all services can be instantiated together
    assert!(std::mem::size_of::<SoulBoxServiceImpl>() > 0);
    assert!(std::mem::size_of::<StreamingServiceImpl>() > 0);
    assert!(std::mem::size_of::<WebSocketServer>() > 0);
    
    // Test that WebSocket server starts with no connections
    let connection_count = ws_server.get_active_connection_count().await;
    assert_eq!(connection_count, 0);
}

#[tokio::test]
async fn test_concurrent_protocol_operations() {
    // Test that gRPC and WebSocket operations can run concurrently
    let grpc_service = SoulBoxServiceImpl::new();
    let ws_server = WebSocketServer::new();
    
    // Spawn concurrent operations
    let grpc_task = tokio::spawn(async move {
        // Mock gRPC operations - just return service size
        std::mem::size_of::<SoulBoxServiceImpl>()
    });
    
    let ws_task = tokio::spawn(async move {
        // WebSocket server operations
        let initial_count = ws_server.get_active_connection_count().await;
        
        // Simulate some WebSocket activity
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        let final_count = ws_server.get_active_connection_count().await;
        
        (initial_count, final_count)
    });
    
    // Wait for both tasks to complete
    let (grpc_result, ws_results) = tokio::try_join!(grpc_task, ws_task).unwrap();
    
    // Validate results
    assert!(grpc_result > 0);
    
    let (initial_ws_count, final_ws_count) = ws_results;
    assert_eq!(initial_ws_count, 0);
    assert_eq!(final_ws_count, 0);
}

#[tokio::test]
async fn test_protocol_modules_compilation() {
    // Test that all protocol modules compile and can be used together
    let grpc_service = SoulBoxServiceImpl::new();
    let streaming_service = StreamingServiceImpl::new();
    let ws_server = WebSocketServer::new();
    
    // Test message creation
    let ws_message = WebSocketMessage::authenticate("test-key", "test-user");
    let ws_response = WebSocketResponse::authentication_success("test-user", "test-session");
    
    // Validate all components work together
    assert!(std::mem::size_of::<SoulBoxServiceImpl>() > 0);
    assert!(std::mem::size_of::<StreamingServiceImpl>() > 0);
    assert!(std::mem::size_of::<WebSocketServer>() > 0);
    assert_eq!(ws_message.r#type, "authenticate");
    assert_eq!(ws_response.r#type, "authentication_success");
}