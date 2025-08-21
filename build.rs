fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/soulbox.proto");
    
    // For now, use basic prost build. 
    // TODO: Upgrade to proper tonic-build service generation when tonic-build API is stable
    prost_build::compile_protos(&["proto/soulbox.proto"], &["proto"])?;
    
    println!("Generated protobuf code from proto/soulbox.proto");
    println!("Note: Using manual gRPC service implementations for compatibility");
    Ok(())
}