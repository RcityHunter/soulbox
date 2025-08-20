fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/soulbox.proto");
    
    // For now, let's temporarily disable protobuf compilation
    // but keep the gRPC module enabled for development
    // The module will use mock implementations until this is resolved
    
    // TODO: Implement proper tonic-build 0.14 integration
    // The API has changed significantly and requires careful adaptation
    
    Ok(())
}