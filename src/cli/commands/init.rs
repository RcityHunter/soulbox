//! Initialize command implementation
//! 
//! Creates a new SoulBox project with default configuration and structure.

use clap::Args;
use colored::*;
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info};

use crate::cli::{Cli, CliConfig};
use crate::error::Result;

#[derive(Args)]
pub struct InitArgs {
    /// Project directory (defaults to current directory)
    #[arg(value_name = "DIR")]
    pub directory: Option<PathBuf>,

    /// Project name
    #[arg(short, long)]
    pub name: Option<String>,

    /// Template to use (python, node, rust, etc.)
    #[arg(short, long, default_value = "python")]
    pub template: String,

    /// Force initialization even if directory is not empty
    #[arg(short, long)]
    pub force: bool,
}

pub async fn run(args: InitArgs, _config: &CliConfig) -> Result<()> {
    let project_dir = args.directory.unwrap_or_else(|| PathBuf::from("."));
    let project_name = args.name.unwrap_or_else(|| {
        project_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("soulbox-project")
            .to_string()
    });

    info!("Initializing SoulBox project: {}", project_name);
    
    // Check if directory exists and is empty (unless force is used)
    if project_dir.exists() && !args.force {
        let mut entries = fs::read_dir(&project_dir).await?;
        if entries.next_entry().await?.is_some() {
            Cli::error_exit("Directory is not empty. Use --force to initialize anyway.");
        }
    }

    // Create project directory if it doesn't exist
    fs::create_dir_all(&project_dir).await?;
    debug!("Created project directory: {}", project_dir.display());

    // Create .soulbox directory for project-specific configuration
    let soulbox_dir = project_dir.join(".soulbox");
    fs::create_dir_all(&soulbox_dir).await?;

    // Create project configuration
    let project_config = ProjectConfig {
        name: project_name.clone(),
        template: args.template.clone(),
        version: "1.0.0".to_string(),
        runtime: RuntimeConfig {
            image: get_template_image(&args.template),
            working_directory: "/workspace".to_string(),
            timeout: 300,
            memory_limit: "512M".to_string(),
            cpu_limit: "1.0".to_string(),
        },
        environment: std::collections::HashMap::new(),
        volumes: vec![],
    };

    // Save project configuration
    let config_content = toml::to_string_pretty(&project_config)?;
    let config_path = soulbox_dir.join("project.toml");
    fs::write(&config_path, config_content).await?;
    debug!("Created project config: {}", config_path.display());

    // Create template-specific files
    create_template_files(&project_dir, &args.template).await?;

    // Create .gitignore if it doesn't exist
    let gitignore_path = project_dir.join(".gitignore");
    if !gitignore_path.exists() {
        let gitignore_content = include_str!("../templates/gitignore.txt");
        fs::write(&gitignore_path, gitignore_content).await?;
        debug!("Created .gitignore");
    }

    Cli::success(&format!("Successfully initialized SoulBox project: {}", project_name));
    Cli::info(&format!("Project directory: {}", project_dir.display()));
    Cli::info(&format!("Template: {}", args.template));
    println!();
    println!("{}", "Next steps:".bold());
    println!("  1. {} to enter your project directory", format!("cd {}", project_dir.display()).cyan());
    println!("  2. {} to create a sandbox", "soulbox-cli create".cyan());
    println!("  3. {} to execute code", "soulbox-cli exec --code 'print(\"Hello, SoulBox!\")'".cyan());

    Ok(())
}

fn get_template_image(template: &str) -> String {
    match template {
        "python" => "python:3.11-slim".to_string(),
        "node" => "node:18-alpine".to_string(),
        "rust" => "rust:1.75-slim".to_string(),
        "go" => "golang:1.21-alpine".to_string(),
        "java" => "openjdk:17-jdk-slim".to_string(),
        "cpp" => "gcc:latest".to_string(),
        _ => "ubuntu:22.04".to_string(),
    }
}

async fn create_template_files(project_dir: &PathBuf, template: &str) -> Result<()> {
    match template {
        "python" => {
            // Create main.py
            let main_py = project_dir.join("main.py");
            fs::write(&main_py, "#!/usr/bin/env python3\n\nprint(\"Hello from SoulBox!\")\n").await?;
            
            // Create requirements.txt
            let requirements = project_dir.join("requirements.txt");
            fs::write(&requirements, "# Add your Python dependencies here\n").await?;
            
            debug!("Created Python template files");
        }
        "node" => {
            // Create package.json
            let package_json = project_dir.join("package.json");
            let package_content = serde_json::json!({
                "name": "soulbox-project",
                "version": "1.0.0",
                "description": "SoulBox Node.js project",
                "main": "index.js",
                "scripts": {
                    "start": "node index.js"
                },
                "dependencies": {}
            });
            fs::write(&package_json, serde_json::to_string_pretty(&package_content)?).await?;
            
            // Create index.js
            let index_js = project_dir.join("index.js");
            fs::write(&index_js, "console.log('Hello from SoulBox!');\n").await?;
            
            debug!("Created Node.js template files");
        }
        "rust" => {
            // Create Cargo.toml
            let cargo_toml = project_dir.join("Cargo.toml");
            let cargo_content = r#"[package]
name = "soulbox-project"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
            fs::write(&cargo_toml, cargo_content).await?;
            
            // Create src directory and main.rs
            let src_dir = project_dir.join("src");
            fs::create_dir_all(&src_dir).await?;
            let main_rs = src_dir.join("main.rs");
            fs::write(&main_rs, "fn main() {\n    println!(\"Hello from SoulBox!\");\n}\n").await?;
            
            debug!("Created Rust template files");
        }
        _ => {
            // Generic template - just create a README
            let readme = project_dir.join("README.md");
            let readme_content = format!("# SoulBox Project\n\nThis is a SoulBox project using the `{}` template.\n", template);
            fs::write(&readme, readme_content).await?;
            
            debug!("Created generic template files");
        }
    }
    
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ProjectConfig {
    name: String,
    template: String,
    version: String,
    runtime: RuntimeConfig,
    environment: std::collections::HashMap<String, String>,
    volumes: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RuntimeConfig {
    image: String,
    working_directory: String,
    timeout: u64,
    memory_limit: String,
    cpu_limit: String,
}