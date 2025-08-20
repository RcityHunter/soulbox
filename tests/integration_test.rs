use soulbox::soulbox::v1::{soul_box_service_client::SoulBoxServiceClient, HealthCheckRequest};

#[tokio::test]
async fn test_server_integration() {
    // Test REST API health check
    let client = reqwest::Client::new();
    let response = client
        .get("http://localhost:8080/health")
        .send()
        .await
        .expect("Failed to connect to REST API");
    
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["service"], "soulbox");
    
    println!("✅ REST API health check passed");
    
    // Test gRPC health check
    let mut grpc_client = SoulBoxServiceClient::connect("http://localhost:9080")
        .await
        .expect("Failed to connect to gRPC service");
    
    let request = tonic::Request::new(HealthCheckRequest {
        service: "soulbox".to_string(),
    });
    
    let response = grpc_client.health_check(request).await.unwrap();
    let health = response.into_inner();
    
    assert_eq!(health.status, 1); // HealthStatus::Serving
    assert_eq!(health.message, "SoulBox service is healthy");
    
    println!("✅ gRPC health check passed");
}

#[tokio::test]
async fn test_create_sandbox() {
    let mut client = SoulBoxServiceClient::connect("http://localhost:9080")
        .await
        .expect("Failed to connect to gRPC service");
    
    use soulbox::soulbox::v1::{CreateSandboxRequest, SandboxConfig, ResourceLimits};
    
    let request = tonic::Request::new(CreateSandboxRequest {
        template_id: "python-3.11".to_string(),
        config: Some(SandboxConfig {
            resource_limits: Some(ResourceLimits {
                memory_mb: 512,
                cpu_cores: 1.0,
                disk_mb: 1024,
                max_processes: 50,
            }),
            network_config: None,
            allowed_domains: vec![],
            enable_internet: true,
        }),
        environment_variables: std::collections::HashMap::new(),
        timeout: None,
    });
    
    let response = client.create_sandbox(request).await.unwrap();
    let sandbox = response.into_inner();
    
    assert!(!sandbox.sandbox_id.is_empty());
    assert_eq!(sandbox.status, "running");
    assert!(sandbox.endpoint_url.contains("soulbox.dev"));
    
    println!("✅ Create sandbox test passed");
    println!("   Sandbox ID: {}", sandbox.sandbox_id);
    println!("   Endpoint: {}", sandbox.endpoint_url);
}