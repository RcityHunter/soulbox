fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Temporarily disable proto compilation to fix other issues first
    // TODO: Fix tonic-build 0.14 API 
    println!("cargo:warning=Proto compilation temporarily disabled - fixing tonic-build 0.14 API");
    Ok(())
}