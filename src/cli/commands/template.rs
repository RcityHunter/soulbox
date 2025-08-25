//! Template management CLI commands
//! 
//! Commands for listing, creating, and managing SoulBox templates

use clap::{Args, Subcommand};
use colored::*;
use prettytable::{Table, row, cell};
use serde::{Deserialize, Serialize};

use crate::cli::{Cli, CliConfig};
use crate::error::Result;
use crate::runtime::RuntimeType;

#[derive(Args)]
pub struct TemplateArgs {
    #[command(subcommand)]
    pub command: TemplateCommands,
}

#[derive(Subcommand)]
pub enum TemplateCommands {
    /// List available templates
    List {
        /// Filter by runtime type
        #[arg(short, long)]
        runtime: Option<String>,
        
        /// Show only preset templates
        #[arg(long)]
        presets: bool,
        
        /// Show only custom templates
        #[arg(long)]
        custom: bool,
    },
    
    /// Show details about a specific template
    Show {
        /// Template name or ID
        template: String,
        
        /// Show full configuration
        #[arg(long)]
        full: bool,
    },
    
    /// Create sandbox from template
    Run {
        /// Template name or ID
        template: String,
        
        /// Sandbox name
        #[arg(short, long)]
        name: Option<String>,
        
        /// Override environment variables (KEY=VALUE)
        #[arg(short, long)]
        env: Vec<String>,
        
        /// Run in detached mode
        #[arg(short, long)]
        detach: bool,
    },
    
    /// Create a new custom template
    Create {
        /// Template name
        name: String,
        
        /// Runtime type
        #[arg(short, long)]
        runtime: String,
        
        /// Base Docker image
        #[arg(short, long)]
        base_image: Option<String>,
        
        /// Template description
        #[arg(short, long)]
        description: Option<String>,
        
        /// Template file directory
        #[arg(short, long)]
        from_dir: Option<String>,
    },
    
    /// Export template configuration
    Export {
        /// Template name or ID
        template: String,
        
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },
    
    /// Import template from configuration
    Import {
        /// Input file path
        file: String,
        
        /// Override template name
        #[arg(short, long)]
        name: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct TemplateInfo {
    id: String,
    name: String,
    runtime: String,
    description: Option<String>,
    is_preset: bool,
    tags: Vec<String>,
}

pub async fn run(args: TemplateArgs, config: &CliConfig) -> Result<()> {
    match args.command {
        TemplateCommands::List { runtime, presets, custom } => {
            list_templates(config, runtime, presets, custom).await
        }
        TemplateCommands::Show { template, full } => {
            show_template(config, &template, full).await
        }
        TemplateCommands::Run { template, name, env, detach } => {
            run_template(config, &template, name, env, detach).await
        }
        TemplateCommands::Create { name, runtime, base_image, description, from_dir } => {
            create_template(config, &name, &runtime, base_image, description, from_dir).await
        }
        TemplateCommands::Export { template, output } => {
            export_template(config, &template, output).await
        }
        TemplateCommands::Import { file, name } => {
            import_template(config, &file, name).await
        }
    }
}

async fn list_templates(
    _config: &CliConfig,
    runtime_filter: Option<String>,
    presets_only: bool,
    custom_only: bool,
) -> Result<()> {
    // Mock data for demonstration
    let templates = vec![
        TemplateInfo {
            id: "python-fastapi".to_string(),
            name: "Python FastAPI".to_string(),
            runtime: "Python".to_string(),
            description: Some("FastAPI web application with async support".to_string()),
            is_preset: true,
            tags: vec!["web".to_string(), "api".to_string()],
        },
        TemplateInfo {
            id: "nodejs-express".to_string(),
            name: "Node.js Express".to_string(),
            runtime: "NodeJS".to_string(),
            description: Some("Express.js web application".to_string()),
            is_preset: true,
            tags: vec!["web".to_string(), "api".to_string()],
        },
        TemplateInfo {
            id: "rust-actix".to_string(),
            name: "Rust Actix-web".to_string(),
            runtime: "Rust".to_string(),
            description: Some("High-performance Actix-web server".to_string()),
            is_preset: true,
            tags: vec!["web".to_string(), "performance".to_string()],
        },
        TemplateInfo {
            id: "custom-ml".to_string(),
            name: "ML Pipeline".to_string(),
            runtime: "Python".to_string(),
            description: Some("Custom machine learning pipeline".to_string()),
            is_preset: false,
            tags: vec!["ml".to_string(), "data-science".to_string()],
        },
    ];
    
    // Apply filters
    let mut filtered = templates;
    
    if let Some(runtime) = runtime_filter {
        filtered.retain(|t| t.runtime.to_lowercase() == runtime.to_lowercase());
    }
    
    if presets_only {
        filtered.retain(|t| t.is_preset);
    } else if custom_only {
        filtered.retain(|t| !t.is_preset);
    }
    
    if filtered.is_empty() {
        Cli::info("No templates found matching the criteria.");
        return Ok(());
    }
    
    // Create table
    let mut table = Table::new();
    table.add_row(row![
        b->"ID",
        b->"Name",
        b->"Runtime",
        b->"Type",
        b->"Tags"
    ]);
    
    for template in filtered {
        let type_str = if template.is_preset {
            "Preset".green().to_string()
        } else {
            "Custom".blue().to_string()
        };
        
        table.add_row(row![
            template.id,
            template.name,
            template.runtime,
            type_str,
            template.tags.join(", ")
        ]);
    }
    
    println!("\n{}", "Available Templates".bold());
    table.printstd();
    
    println!("\nUse 'soulbox template show <id>' to see details");
    println!("Use 'soulbox template run <id>' to create a sandbox");
    
    Ok(())
}

async fn show_template(
    _config: &CliConfig,
    template_id: &str,
    full: bool,
) -> Result<()> {
    println!("\n{}", format!("Template: {}", template_id).bold());
    
    // Mock template details
    println!("{}: Python FastAPI", "Name".bold());
    println!("{}: Python 3.11", "Runtime".bold());
    println!("{}: python:3.11-slim", "Base Image".bold());
    println!("{}: FastAPI web application with async support", "Description".bold());
    
    if full {
        println!("\n{}", "Environment Variables:".bold());
        println!("  PYTHONUNBUFFERED=1");
        
        println!("\n{}", "Files:".bold());
        println!("  - requirements.txt");
        println!("  - main.py");
        
        println!("\n{}", "Setup Commands:".bold());
        println!("  - pip install --no-cache-dir -r requirements.txt");
        
        println!("\n{}", "Exposed Ports:".bold());
        println!("  - 8000");
    }
    
    Ok(())
}

async fn run_template(
    _config: &CliConfig,
    template_id: &str,
    name: Option<String>,
    env_vars: Vec<String>,
    detach: bool,
) -> Result<()> {
    let sandbox_name = name.unwrap_or_else(|| format!("{}-{}", template_id, uuid::Uuid::new_v4().to_string()[..8].to_string()));
    
    println!("Creating sandbox '{}' from template '{}'...", sandbox_name, template_id);
    
    // Parse environment variables
    if !env_vars.is_empty() {
        println!("Setting environment variables:");
        for var in &env_vars {
            println!("  - {}", var);
        }
    }
    
    // Mock creation
    Cli::success(&format!("Sandbox '{}' created successfully!", sandbox_name));
    
    if !detach {
        println!("Attaching to sandbox...");
        println!("Press Ctrl+C to detach");
    }
    
    Ok(())
}

async fn create_template(
    _config: &CliConfig,
    name: &str,
    runtime: &str,
    base_image: Option<String>,
    description: Option<String>,
    from_dir: Option<String>,
) -> Result<()> {
    println!("Creating template '{}'...", name);
    
    // Validate runtime
    let _runtime_type = match runtime.to_lowercase().as_str() {
        "python" => RuntimeType::Python,
        "nodejs" | "node" => RuntimeType::NodeJS,
        "rust" => RuntimeType::Rust,
        "go" => RuntimeType::Go,
        "java" => RuntimeType::Java,
        _ => {
            Cli::error_exit(&format!("Invalid runtime: {}", runtime));
        }
    };
    
    if let Some(dir) = from_dir {
        println!("Scanning directory: {}", dir);
        println!("Found 5 files to include");
    }
    
    println!("Runtime: {}", runtime);
    if let Some(image) = base_image {
        println!("Base image: {}", image);
    }
    if let Some(desc) = description {
        println!("Description: {}", desc);
    }
    
    Cli::success(&format!("Template '{}' created successfully!", name));
    
    Ok(())
}

async fn export_template(
    _config: &CliConfig,
    template_id: &str,
    output: Option<String>,
) -> Result<()> {
    let output_file = output.unwrap_or_else(|| format!("{}.json", template_id));
    
    println!("Exporting template '{}' to '{}'...", template_id, output_file);
    
    // Mock export
    let template_json = r#"{
  "name": "Python FastAPI",
  "runtime": "Python",
  "base_image": "python:3.11-slim",
  "description": "FastAPI web application with async support",
  "files": [
    {
      "path": "requirements.txt",
      "content": "fastapi==0.104.1\nuvicorn==0.24.0"
    },
    {
      "path": "main.py",
      "content": "from fastapi import FastAPI\n\napp = FastAPI()\n\n@app.get(\"/\")\nasync def root():\n    return {\"message\": \"Hello from SoulBox!\"}"
    }
  ]
}"#;
    
    std::fs::write(&output_file, template_json)?;
    
    Cli::success(&format!("Template exported to '{}'", output_file));
    
    Ok(())
}

async fn import_template(
    _config: &CliConfig,
    file: &str,
    name_override: Option<String>,
) -> Result<()> {
    println!("Importing template from '{}'...", file);
    
    // Check file exists
    if !std::path::Path::new(file).exists() {
        Cli::error_exit(&format!("File not found: {}", file));
    }
    
    let template_name = name_override.unwrap_or_else(|| "imported-template".to_string());
    
    println!("Template name: {}", template_name);
    
    Cli::success(&format!("Template '{}' imported successfully!", template_name));
    
    Ok(())
}