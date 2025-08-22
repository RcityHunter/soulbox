fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/soulbox.proto");
    
    // Use prost-build for message generation
    prost_build::compile_protos(&["proto/soulbox.proto"], &["proto"])?;
    
    // Generate file descriptor set for reflection support
    let descriptor_path = "proto/soulbox_descriptor.bin";
    prost_build::Config::new()
        .file_descriptor_set_path(&descriptor_path)
        .compile_protos(&["proto/soulbox.proto"], &["proto"])?;
    
    println!("Generated protobuf code from proto/soulbox.proto");
    println!("Created file descriptor set at {}", descriptor_path);
    println!("Note: gRPC service traits will be manually implemented for compatibility");
    Ok(())
}