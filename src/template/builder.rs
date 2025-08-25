//! Template builder for easy template creation

use crate::template::models::{CreateTemplateRequest, TemplateFileUpload};
use crate::runtime::RuntimeType;
use crate::error::{Result, SoulBoxError};
use std::collections::HashMap;
use uuid::Uuid;

/// Builder pattern for creating templates with better UX
pub struct TemplateBuilder {
    id: Option<Uuid>,
    name: String,
    description: String,
    runtime: RuntimeType,
    base_image: Option<String>,
    tags: Vec<String>,
    author: String,
    dockerfile_content: Option<String>,
    setup_commands: Vec<String>,
    environment_variables: HashMap<String, String>,
    exposed_ports: Vec<u16>,
    volumes: Vec<String>,
    files: Vec<TemplateFileUpload>,
}

impl TemplateBuilder {
    /// Create a new template builder
    pub fn new(name: impl Into<String>, runtime: RuntimeType) -> Self {
        Self {
            id: None,
            name: name.into(),
            description: String::new(),
            runtime,
            base_image: None,
            tags: Vec::new(),
            author: "system".to_string(),
            dockerfile_content: None,
            setup_commands: Vec::new(),
            environment_variables: HashMap::new(),
            exposed_ports: Vec::new(),
            volumes: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Set template ID (optional, will be auto-generated if not set)
    pub fn with_id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Set template description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set base Docker image
    pub fn with_base_image(mut self, image: impl Into<String>) -> Self {
        self.base_image = Some(image.into());
        self
    }

    /// Add tags for categorization
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Add a single tag
    pub fn add_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set template author
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    /// Set custom Dockerfile content
    pub fn with_dockerfile(mut self, content: impl Into<String>) -> Self {
        self.dockerfile_content = Some(content.into());
        self
    }

    /// Add setup commands to run during container initialization
    pub fn add_setup_command(mut self, command: impl Into<String>) -> Self {
        self.setup_commands.push(command.into());
        self
    }

    /// Add multiple setup commands
    pub fn with_setup_commands(mut self, commands: Vec<String>) -> Self {
        self.setup_commands = commands;
        self
    }

    /// Add environment variable
    pub fn add_env_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment_variables.insert(key.into(), value.into());
        self
    }

    /// Set all environment variables
    pub fn with_env_vars(mut self, vars: HashMap<String, String>) -> Self {
        self.environment_variables = vars;
        self
    }

    /// Expose a port
    pub fn expose_port(mut self, port: u16) -> Self {
        if !self.exposed_ports.contains(&port) {
            self.exposed_ports.push(port);
        }
        self
    }

    /// Expose multiple ports
    pub fn expose_ports(mut self, ports: Vec<u16>) -> Self {
        for port in ports {
            if !self.exposed_ports.contains(&port) {
                self.exposed_ports.push(port);
            }
        }
        self
    }

    /// Add a volume mount point
    pub fn add_volume(mut self, path: impl Into<String>) -> Self {
        self.volumes.push(path.into());
        self
    }

    /// Add a file to the template
    pub fn add_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.files.push(TemplateFileUpload {
            path: path.into(),
            content: content.into(),
            mime_type: Some("text/plain".to_string()),
        });
        self
    }

    /// Add an executable file to the template
    pub fn add_executable(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.files.push(TemplateFileUpload {
            path: path.into(),
            content: content.into(),
            mime_type: Some("application/x-executable".to_string()),
        });
        self
    }

    /// Generate Dockerfile content based on runtime
    fn generate_dockerfile(&self) -> String {
        if let Some(ref content) = self.dockerfile_content {
            return content.clone();
        }

        let default_image = match self.runtime {
            RuntimeType::Python => "python:3.11-slim".to_string(),
            RuntimeType::NodeJS => "node:20-alpine".to_string(),
            RuntimeType::Rust => "rust:1.75-slim".to_string(),
            RuntimeType::Go => "golang:1.21-alpine".to_string(),
            RuntimeType::Java => "openjdk:21-slim".to_string(),
            RuntimeType::Shell => "ubuntu:22.04".to_string(),
            _ => "ubuntu:22.04".to_string(),
        };
        
        let base_image = self.base_image.as_ref().unwrap_or(&default_image);

        let mut dockerfile = vec![
            format!("FROM {}", base_image),
            "WORKDIR /workspace".to_string(),
        ];

        // Add runtime-specific setup
        match self.runtime {
            RuntimeType::Python => {
                dockerfile.push("RUN pip install --upgrade pip".to_string());
                if self.files.iter().any(|f| f.path == "requirements.txt") {
                    dockerfile.push("COPY requirements.txt .".to_string());
                    dockerfile.push("RUN pip install -r requirements.txt".to_string());
                }
            }
            RuntimeType::NodeJS => {
                if self.files.iter().any(|f| f.path == "package.json") {
                    dockerfile.push("COPY package*.json ./".to_string());
                    dockerfile.push("RUN npm ci --only=production".to_string());
                }
            }
            RuntimeType::Go => {
                if self.files.iter().any(|f| f.path == "go.mod") {
                    dockerfile.push("COPY go.mod go.sum ./".to_string());
                    dockerfile.push("RUN go mod download".to_string());
                }
            }
            _ => {}
        }

        // Add custom setup commands
        for cmd in &self.setup_commands {
            dockerfile.push(format!("RUN {}", cmd));
        }

        // Copy application files
        dockerfile.push("COPY . .".to_string());

        // Set environment variables
        for (key, value) in &self.environment_variables {
            dockerfile.push(format!("ENV {}={}", key, value));
        }

        // Expose ports
        for port in &self.exposed_ports {
            dockerfile.push(format!("EXPOSE {}", port));
        }

        dockerfile.join("\n")
    }

    /// Build the template creation request
    pub fn build(self) -> Result<CreateTemplateRequest> {
        // Validate required fields
        if self.name.is_empty() {
            return Err(SoulBoxError::Validation("Template name is required".to_string()));
        }

        // Generate slug from name
        let slug = self.name.to_lowercase().replace(' ', "-").replace('_', "-");

        // Use base image or default based on runtime
        let base_image = self.base_image.unwrap_or_else(|| match self.runtime {
            RuntimeType::Python => "python:3.11-slim".to_string(),
            RuntimeType::NodeJS => "node:20-alpine".to_string(),
            RuntimeType::Rust => "rust:1.75-slim".to_string(),
            RuntimeType::Go => "golang:1.21-alpine".to_string(),
            RuntimeType::Java => "openjdk:21-slim".to_string(),
            RuntimeType::Shell => "ubuntu:22.04".to_string(),
            _ => "ubuntu:22.04".to_string(),
        });

        Ok(CreateTemplateRequest {
            name: self.name,
            slug,
            description: Some(self.description),
            runtime_type: self.runtime,
            base_image,
            default_command: None,
            environment_vars: Some(self.environment_variables),
            resource_limits: None,
            files: Some(self.files),
            setup_commands: Some(self.setup_commands),
            tags: Some(self.tags),
            is_public: Some(false),
        })
    }
}

/// Pre-defined template configurations for common use cases
pub struct TemplatePresets;

impl TemplatePresets {
    /// Create a Python FastAPI template
    pub fn python_fastapi() -> TemplateBuilder {
        TemplateBuilder::new("Python FastAPI", RuntimeType::Python)
            .with_description("FastAPI web application with async support")
            .with_base_image("python:3.11-slim")
            .add_tag("web")
            .add_tag("api")
            .add_tag("async")
            .add_file("requirements.txt", "fastapi==0.104.1\nuvicorn==0.24.0\npydantic==2.5.0")
            .add_file("main.py", r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/")
async def root():
    return {"message": "Hello from SoulBox!"}

@app.get("/health")
async def health():
    return {"status": "healthy"}
"#)
            .add_setup_command("pip install --no-cache-dir -r requirements.txt")
            .expose_port(8000)
            .add_env_var("PYTHONUNBUFFERED", "1")
    }

    /// Create a Node.js Express template
    pub fn nodejs_express() -> TemplateBuilder {
        TemplateBuilder::new("Node.js Express", RuntimeType::NodeJS)
            .with_description("Express.js web application")
            .with_base_image("node:20-alpine")
            .add_tag("web")
            .add_tag("api")
            .add_file("package.json", r#"{
  "name": "express-app",
  "version": "1.0.0",
  "main": "index.js",
  "scripts": {
    "start": "node index.js"
  },
  "dependencies": {
    "express": "^4.18.2"
  }
}"#)
            .add_file("index.js", r#"
const express = require('express');
const app = express();
const port = process.env.PORT || 3000;

app.get('/', (req, res) => {
  res.json({ message: 'Hello from SoulBox!' });
});

app.get('/health', (req, res) => {
  res.json({ status: 'healthy' });
});

app.listen(port, () => {
  console.log(`Server running on port ${port}`);
});
"#)
            .expose_port(3000)
    }

    /// Create a Rust Actix-web template
    pub fn rust_actix() -> TemplateBuilder {
        TemplateBuilder::new("Rust Actix-web", RuntimeType::Rust)
            .with_description("Actix-web HTTP server")
            .with_base_image("rust:1.75-slim")
            .add_tag("web")
            .add_tag("api")
            .add_tag("performance")
            .add_file("Cargo.toml", r#"[package]
name = "actix-app"
version = "0.1.0"
edition = "2021"

[dependencies]
actix-web = "4"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
"#)
            .add_file("src/main.rs", r#"
use actix_web::{web, App, HttpServer, Result};
use serde::Serialize;

#[derive(Serialize)]
struct Message {
    message: String,
}

async fn index() -> Result<web::Json<Message>> {
    Ok(web::Json(Message {
        message: "Hello from SoulBox!".to_string(),
    }))
}

async fn health() -> Result<web::Json<Message>> {
    Ok(web::Json(Message {
        message: "healthy".to_string(),
    }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(index))
            .route("/health", web::get().to(health))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
"#)
            .add_setup_command("cargo build --release")
            .expose_port(8080)
    }

    /// Create a data science Python template
    pub fn python_data_science() -> TemplateBuilder {
        TemplateBuilder::new("Python Data Science", RuntimeType::Python)
            .with_description("Jupyter notebook with common data science libraries")
            .with_base_image("python:3.11")
            .add_tag("data-science")
            .add_tag("ml")
            .add_tag("jupyter")
            .add_file("requirements.txt", "jupyter==1.0.0\nnumpy==1.24.3\npandas==2.0.3\nmatplotlib==3.7.2\nscikit-learn==1.3.0\nseaborn==0.12.2")
            .add_setup_command("pip install --no-cache-dir -r requirements.txt")
            .expose_port(8888)
            .add_env_var("JUPYTER_ENABLE_LAB", "yes")
    }

    /// Create a Go Gin template
    pub fn go_gin() -> TemplateBuilder {
        TemplateBuilder::new("Go Gin", RuntimeType::Go)
            .with_description("Gin web framework application")
            .with_base_image("golang:1.21-alpine")
            .add_tag("web")
            .add_tag("api")
            .add_file("go.mod", r#"module gin-app

go 1.21

require github.com/gin-gonic/gin v1.9.1
"#)
            .add_file("main.go", r#"
package main

import (
    "github.com/gin-gonic/gin"
    "net/http"
)

func main() {
    r := gin.Default()
    
    r.GET("/", func(c *gin.Context) {
        c.JSON(http.StatusOK, gin.H{
            "message": "Hello from SoulBox!",
        })
    })
    
    r.GET("/health", func(c *gin.Context) {
        c.JSON(http.StatusOK, gin.H{
            "status": "healthy",
        })
    })
    
    r.Run(":8080")
}
"#)
            .add_setup_command("go mod download")
            .add_setup_command("go build -o app")
            .expose_port(8080)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_builder() {
        let template = TemplateBuilder::new("test-template", RuntimeType::Python)
            .with_description("Test template")
            .with_author("tester")
            .add_tag("test")
            .add_env_var("TEST", "true")
            .expose_port(8080)
            .build()
            .unwrap();

        assert_eq!(template.name, "test-template");
        assert_eq!(template.slug, "test-template");
        assert_eq!(template.runtime_type, RuntimeType::Python);
        assert_eq!(template.description, Some("Test template".to_string()));
        
        // Check environment variables
        let env_vars = template.environment_vars.unwrap_or_default();
        assert!(env_vars.contains_key("TEST"));
        
        // Check tags
        let tags = template.tags.unwrap_or_default();
        assert!(tags.contains(&"test".to_string()));
    }

    #[test]
    fn test_preset_templates() {
        let fastapi = TemplatePresets::python_fastapi().build().unwrap();
        assert_eq!(fastapi.name, "Python FastAPI");
        assert_eq!(fastapi.runtime_type, RuntimeType::Python);
        
        // Check environment variables for PYTHONUNBUFFERED
        let env_vars = fastapi.environment_vars.unwrap_or_default();
        assert!(env_vars.contains_key("PYTHONUNBUFFERED"));

        let express = TemplatePresets::nodejs_express().build().unwrap();
        assert_eq!(express.name, "Node.js Express");
        assert_eq!(express.runtime_type, RuntimeType::NodeJS);
        
        // Check files were added
        let files = express.files.unwrap_or_default();
        assert!(files.iter().any(|f| f.path == "package.json"));
        assert!(files.iter().any(|f| f.path == "index.js"));
    }
}