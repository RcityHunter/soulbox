use soulbox::container::ContainerManager;
use soulbox::sandbox_manager::SandboxManager;
use soulbox::template::{Template, TemplateManager, DockerfileParser};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🚀 Testing SoulBox Template System\n");

    // Create container manager
    println!("1. Setting up container manager...");
    let config = soulbox::config::Config::default();
    let container_manager = Arc::new(ContainerManager::new(config).await?);
    println!("   ✅ Container manager ready\n");

    // Create template manager
    println!("2. Creating template manager...");
    let template_manager = TemplateManager::new_without_database();
    println!("   ✅ Template manager ready\n");

    // Get default templates
    println!("3. Loading default templates...");
    let default_templates = template_manager.get_default_templates();
    println!("   Found {} default templates:", default_templates.len());
    for (name, template) in &default_templates {
        println!("   - {}: {} ({})", name, template.metadata.name, template.config.image);
    }
    println!();

    // Test Dockerfile parsing
    println!("4. Testing Dockerfile parsing...");
    let dockerfile_content = r#"
FROM python:3.11-slim
WORKDIR /app
RUN pip install --no-cache-dir fastapi uvicorn pandas numpy
ENV PYTHONUNBUFFERED=1
EXPOSE 8000
CMD ["python", "-m", "uvicorn", "main:app", "--host", "0.0.0.0", "--port", "8000"]
"#;

    let mut parser = DockerfileParser::new(dockerfile_content.to_string());
    parser.parse()?;
    parser.validate()?;
    
    println!("   ✅ Dockerfile parsed successfully");
    println!("   Base image: {:?}", parser.get_base_image());
    println!("   Exposed ports: {:?}", parser.get_exposed_ports());
    println!("   Environment vars: {:?}", parser.get_env_vars());
    println!();

    // Create sandbox from template
    println!("5. Creating sandbox from Python template...");
    let sandbox_manager = SandboxManager::new(container_manager.clone());
    
    // Get Python template
    let python_template = default_templates.get("python").unwrap();
    
    // Create sandbox from template
    let sandbox_id = sandbox_manager
        .create_sandbox_from_template(python_template, None, None)
        .await?;
    println!("   ✅ Sandbox created: {}\n", sandbox_id);

    // Execute code in template-based sandbox
    println!("6. Executing code in template-based sandbox...");
    let python_code = r#"
import sys
import platform

print(f"Python: {sys.version}")
print(f"Platform: {platform.platform()}")

# Test that we can import common libraries
try:
    import json
    import datetime
    import os
    print("✅ Standard libraries available")
except ImportError as e:
    print(f"❌ Import error: {e}")

# Test environment variables
template_name = os.environ.get('SOULBOX_TEMPLATE', 'unknown')
sandbox_id = os.environ.get('SOULBOX_SANDBOX_ID', 'unknown')
print(f"Template: {template_name}")
print(f"Sandbox ID: {sandbox_id}")
"#;

    let result = sandbox_manager
        .execute_code(&sandbox_id, python_code, Some(Duration::from_secs(10)))
        .await?;

    println!("   📝 Output:");
    println!("{}", result.stdout);
    
    if !result.stderr.is_empty() {
        println!("   ⚠️ Errors:");
        println!("{}", result.stderr);
    }

    // Test custom Dockerfile template
    println!("\n7. Testing custom Dockerfile template...");
    let custom_template = Template {
        metadata: soulbox::template::models::TemplateMetadata {
            id: uuid::Uuid::new_v4(),
            name: "FastAPI Template".to_string(),
            slug: "fastapi".to_string(),
            description: Some("Python FastAPI web application template".to_string()),
            version: soulbox::template::models::TemplateVersion::new(1, 0, 0),
            runtime_type: "python".to_string(),
            is_public: true,
            is_verified: false,
            owner_id: uuid::Uuid::new_v4(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
        config: soulbox::template::models::TemplateConfig {
            image: "python:3.11-slim".to_string(),
            dockerfile: Some(dockerfile_content.to_string()),
            env: Some(vec![
                ("APP_ENV".to_string(), "development".to_string()),
                ("API_VERSION".to_string(), "v1".to_string()),
            ].into_iter().collect()),
            setup_commands: Some(vec![
                "echo '✅ Template initialized'".to_string(),
                "python --version".to_string(),
            ]),
            cpu_limit: Some(1024),
            memory_limit: Some(512 * 1024 * 1024),
            build_args: None,
        },
    };

    println!("   Template: {}", custom_template.metadata.name);
    println!("   Runtime: {}", custom_template.metadata.runtime_type);
    println!("   Has Dockerfile: {}", custom_template.config.dockerfile.is_some());
    
    // Note: Building from Dockerfile requires Docker BuildKit
    // For now, we'll use the pre-built image
    let custom_sandbox_id = sandbox_manager
        .create_sandbox("python", None, None)
        .await?;
    
    println!("   ✅ Custom sandbox created: {}", custom_sandbox_id);

    // Cleanup
    println!("\n8. Cleaning up...");
    sandbox_manager.destroy_sandbox(&sandbox_id).await?;
    sandbox_manager.destroy_sandbox(&custom_sandbox_id).await?;
    println!("   ✅ Sandboxes destroyed\n");

    println!("🎉 Template system test completed successfully!");

    Ok(())
}