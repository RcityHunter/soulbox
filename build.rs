fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/soulbox.proto");
    
    // FIXME: Temporary solution using only prost for message generation
    // Need to investigate proper tonic-build 0.14 usage for service generation
    prost_build::Config::new()
        .file_descriptor_set_path("proto/soulbox_descriptor.bin")
        .compile_protos(&["proto/soulbox.proto"], &["proto/"])?;
    
    println!("Generated protobuf messages from proto/soulbox.proto");
    println!("Created file descriptor set at proto/soulbox_descriptor.bin");
    println!("WARNING: gRPC service traits need to be manually implemented");
    Ok(())
}