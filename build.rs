use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("soulbox_descriptor.bin"))
        .out_dir(&out_dir)
        .compile(&["proto/soulbox.proto"], &["proto"])?;

    Ok(())
}