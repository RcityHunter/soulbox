fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/soulbox.proto");
    
    // For now, keep the basic protobuf generation working
    // and improve mock implementation to be more comprehensive
    
    // Generate with basic prost only for now
    prost_build::compile_protos(&["proto/soulbox.proto"], &["proto"])?;
    
    println!("Generated basic protobuf code from proto/soulbox.proto");
    println!("Note: Using enhanced mock implementation for gRPC services");
    Ok(())
}