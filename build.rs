fn main() -> Result<(), Box<dyn std::error::Error>> {
    // For tonic-build 0.14, temporarily skip protobuf compilation 
    // The grpc module needs to be refactored for the new API
    // This allows the rest of the project to compile
    println!("cargo:rerun-if-changed=proto/soulbox.proto");
    
    // TODO: Re-enable when grpc module is refactored
    // tonic_build API has changed significantly in 0.14
    
    Ok(())
}