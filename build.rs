fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/soulbox.proto");
    
    // Temporarily disable protobuf compilation for faster builds during development
    // TODO: Re-enable once core compilation errors are fixed
    // tonic_build::compile_protos("proto/soulbox.proto")?;
    
    Ok(())
}