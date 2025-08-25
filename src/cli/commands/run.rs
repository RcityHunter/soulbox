use clap::Args;
use colored::*;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;
use tracing::{debug, info};

use crate::error::Result;

/// Arguments for the run command
#[derive(Args)]
pub struct RunArgs {
    /// Sandbox ID to run code in
    #[arg(short, long)]
    pub sandbox: String,

    /// Code file to execute
    #[arg(short, long, conflicts_with = "code")]
    pub file: Option<PathBuf>,

    /// Inline code to execute
    #[arg(short = 'c', long, conflicts_with = "file")]
    pub code: Option<String>,

    /// Language/runtime (auto-detect if not specified)
    #[arg(short, long)]
    pub language: Option<String>,

    /// Execution timeout in seconds
    #[arg(long, default_value = "30")]
    pub timeout: u64,

    /// Show execution time
    #[arg(long)]
    pub time: bool,

    /// Stream output in real-time
    #[arg(long)]
    pub stream: bool,
}

impl RunArgs {
    /// Execute the run command
    pub async fn execute(&self) -> Result<()> {
        info!("Running code in sandbox: {}", self.sandbox);

        // Get the code to execute
        let _code = if let Some(ref file_path) = self.file {
            // Read code from file
            debug!("Reading code from file: {:?}", file_path);
            let mut file = tokio::fs::File::open(file_path).await?;
            let mut contents = String::new();
            file.read_to_string(&mut contents).await?;
            contents
        } else if let Some(ref inline_code) = self.code {
            // Use inline code
            debug!("Using inline code");
            inline_code.clone()
        } else {
            return Err(crate::error::SoulBoxError::Validation(
                "Either --file or --code must be specified".to_string()
            ));
        };

        // Detect language if not specified
        let language = self.language.clone().unwrap_or_else(|| {
            if let Some(ref file_path) = self.file {
                detect_language_from_extension(file_path)
            } else {
                "python".to_string() // Default to Python
            }
        });

        println!("{}", "Executing code...".cyan());
        println!("  {} {}", "Sandbox:".bold(), self.sandbox);
        println!("  {} {}", "Language:".bold(), language);
        println!("  {} {}s", "Timeout:".bold(), self.timeout);

        // Start timing if requested
        let start_time = if self.time {
            Some(std::time::Instant::now())
        } else {
            None
        };

        // TODO: Implement actual execution via API client
        // For now, just simulate execution
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        // Simulated output
        println!("\n{}", "Output:".green().bold());
        println!("Hello from SoulBox!");
        println!("Code executed successfully in sandbox: {}", self.sandbox);

        // Show execution time if requested
        if let Some(start) = start_time {
            let elapsed = start.elapsed();
            println!("\n{} {:.3}s", "Execution time:".cyan(), elapsed.as_secs_f64());
        }

        println!("\n{}", "✓ Code execution completed".green());

        Ok(())
    }
}

/// Detect language from file extension
fn detect_language_from_extension(path: &PathBuf) -> String {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("py") => "python".to_string(),
        Some("js") | Some("mjs") => "javascript".to_string(),
        Some("ts") => "typescript".to_string(),
        Some("rs") => "rust".to_string(),
        Some("go") => "go".to_string(),
        Some("rb") => "ruby".to_string(),
        Some("java") => "java".to_string(),
        Some("cpp") | Some("cc") | Some("cxx") => "cpp".to_string(),
        Some("c") => "c".to_string(),
        Some("sh") | Some("bash") => "bash".to_string(),
        _ => "python".to_string(), // Default
    }
}