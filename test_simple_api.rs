use soulbox::simple::{SandboxManager, SimpleAPI};
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing SoulBox Simple API");
    
    // Initialize tracing for debugging
    tracing_subscriber::fmt::init();
    
    // Test basic sandbox manager creation
    println!("Creating SandboxManager...");
    let manager = match SandboxManager::new() {
        Ok(m) => {
            println!("✅ SandboxManager created successfully");
            m
        }
        Err(e) => {
            println!("❌ Failed to create SandboxManager: {}", e);
            println!("This is expected if Docker is not running");
            return Ok(());
        }
    };
    
    // Test starting the simple server
    println!("Starting Simple API server...");
    match SimpleAPI::serve(manager).await {
        Ok(_) => println!("✅ Simple API server started"),
        Err(e) => println!("❌ Server error: {}", e),
    }
    
    Ok(())
}