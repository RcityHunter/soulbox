//! Dockerfile parsing and building for template system
//! 
//! This module provides functionality to parse Dockerfiles,
//! validate them, and build custom Docker images for templates.

use crate::error::{Result, SoulBoxError};
use bollard::Docker;
use bollard::image::BuildImageOptions;
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use tar::Builder;
use tracing::{info, warn, error, debug};

/// Dockerfile parser and validator
pub struct DockerfileParser {
    content: String,
    instructions: Vec<DockerfileInstruction>,
}

#[derive(Debug, Clone)]
pub enum DockerfileInstruction {
    From(String),
    Run(String),
    Cmd(Vec<String>),
    Entrypoint(Vec<String>),
    Copy { src: String, dst: String },
    Add { src: String, dst: String },
    Workdir(String),
    Env { key: String, value: String },
    Expose(u16),
    Volume(String),
    User(String),
    Label { key: String, value: String },
    Arg { name: String, default: Option<String> },
}

impl DockerfileParser {
    /// Create a new parser from Dockerfile content
    pub fn new(content: String) -> Self {
        Self {
            content,
            instructions: Vec::new(),
        }
    }

    /// Parse the Dockerfile content
    pub fn parse(&mut self) -> Result<()> {
        self.instructions.clear();
        
        for line in self.content.lines() {
            let line = line.trim();
            
            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse instruction
            if let Some(instruction) = self.parse_instruction(line) {
                self.instructions.push(instruction);
            }
        }

        // Validate that we have at least a FROM instruction
        if !self.instructions.iter().any(|i| matches!(i, DockerfileInstruction::From(_))) {
            return Err(SoulBoxError::Internal(
                "Dockerfile must contain at least one FROM instruction".to_string()
            ));
        }

        Ok(())
    }

    /// Parse a single instruction line
    fn parse_instruction(&self, line: &str) -> Option<DockerfileInstruction> {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.is_empty() {
            return None;
        }

        let instruction = parts[0].to_uppercase();
        let args = parts.get(1).map(|s| s.trim());

        match instruction.as_str() {
            "FROM" => args.map(|a| DockerfileInstruction::From(a.to_string())),
            "RUN" => args.map(|a| DockerfileInstruction::Run(a.to_string())),
            "WORKDIR" => args.map(|a| DockerfileInstruction::Workdir(a.to_string())),
            "USER" => args.map(|a| DockerfileInstruction::User(a.to_string())),
            "VOLUME" => args.map(|a| DockerfileInstruction::Volume(a.to_string())),
            "CMD" => args.map(|a| DockerfileInstruction::Cmd(self.parse_shell_args(a))),
            "ENTRYPOINT" => args.map(|a| DockerfileInstruction::Entrypoint(self.parse_shell_args(a))),
            "EXPOSE" => args.and_then(|a| a.parse().ok()).map(DockerfileInstruction::Expose),
            "ENV" => {
                if let Some(args) = args {
                    let parts: Vec<&str> = args.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        Some(DockerfileInstruction::Env {
                            key: parts[0].trim().to_string(),
                            value: parts[1].trim().to_string(),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "COPY" | "ADD" => {
                if let Some(args) = args {
                    let parts: Vec<&str> = args.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let src = parts[0].to_string();
                        let dst = parts[parts.len() - 1].to_string();
                        if instruction == "COPY" {
                            Some(DockerfileInstruction::Copy { src, dst })
                        } else {
                            Some(DockerfileInstruction::Add { src, dst })
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "LABEL" => {
                if let Some(args) = args {
                    let parts: Vec<&str> = args.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        Some(DockerfileInstruction::Label {
                            key: parts[0].trim().to_string(),
                            value: parts[1].trim().trim_matches('"').to_string(),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "ARG" => {
                if let Some(args) = args {
                    let parts: Vec<&str> = args.splitn(2, '=').collect();
                    Some(DockerfileInstruction::Arg {
                        name: parts[0].trim().to_string(),
                        default: parts.get(1).map(|s| s.trim().to_string()),
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Parse shell-style arguments (handles both exec and shell forms)
    fn parse_shell_args(&self, args: &str) -> Vec<String> {
        // Handle JSON array format ["cmd", "arg1", "arg2"]
        if args.starts_with('[') && args.ends_with(']') {
            let json_str = args.trim_start_matches('[').trim_end_matches(']');
            json_str
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .collect()
        } else {
            // Shell format
            vec![args.to_string()]
        }
    }

    /// Validate the parsed Dockerfile
    pub fn validate(&self) -> Result<()> {
        // Check for FROM instruction
        let has_from = self.instructions.iter().any(|i| matches!(i, DockerfileInstruction::From(_)));
        if !has_from {
            return Err(SoulBoxError::Internal("Dockerfile must start with FROM instruction".to_string()));
        }

        // Check for potentially dangerous instructions
        for instruction in &self.instructions {
            if let DockerfileInstruction::Run(cmd) = instruction {
                if self.is_dangerous_command(cmd) {
                    return Err(SoulBoxError::Internal(format!("Potentially dangerous command: {}", cmd)));
                }
            }
        }

        Ok(())
    }

    /// Check if a command is potentially dangerous
    fn is_dangerous_command(&self, cmd: &str) -> bool {
        let dangerous_patterns = [
            "rm -rf /",
            "rm -fr /",
            "dd if=/dev/zero",
            "mkfs",
            "format",
            ":(){:|:&};:",  // Fork bomb
        ];

        let cmd_lower = cmd.to_lowercase();
        dangerous_patterns.iter().any(|pattern| cmd_lower.contains(pattern))
    }

    /// Get the base image from the Dockerfile
    pub fn get_base_image(&self) -> Option<String> {
        self.instructions.iter().find_map(|i| {
            if let DockerfileInstruction::From(image) = i {
                Some(image.clone())
            } else {
                None
            }
        })
    }

    /// Get all exposed ports
    pub fn get_exposed_ports(&self) -> Vec<u16> {
        self.instructions.iter().filter_map(|i| {
            if let DockerfileInstruction::Expose(port) = i {
                Some(*port)
            } else {
                None
            }
        }).collect()
    }

    /// Get environment variables
    pub fn get_env_vars(&self) -> HashMap<String, String> {
        let mut env_vars = HashMap::new();
        for instruction in &self.instructions {
            if let DockerfileInstruction::Env { key, value } = instruction {
                env_vars.insert(key.clone(), value.clone());
            }
        }
        env_vars
    }
}

/// Docker image builder for templates
pub struct DockerImageBuilder {
    docker: Docker,
}

impl DockerImageBuilder {
    /// Create a new Docker image builder
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    /// Build a Docker image from a Dockerfile
    pub async fn build_image(
        &self,
        dockerfile_content: &str,
        image_name: &str,
        build_args: HashMap<String, String>,
    ) -> Result<String> {
        info!("Building Docker image: {}", image_name);

        // Create a tar archive with the Dockerfile
        let dockerfile_tar = self.create_dockerfile_tar(dockerfile_content)?;

        // Build options
        let options = BuildImageOptions {
            dockerfile: "Dockerfile".to_string(),
            t: image_name.to_string(),
            buildargs: build_args,
            forcerm: true,
            ..Default::default()
        };

        // Build the image
        let mut build_stream = self.docker.build_image(options, None, Some(dockerfile_tar.into()));

        // Process build output
        let mut image_id = String::new();
        while let Some(msg) = build_stream.next().await {
            match msg {
                Ok(output) => {
                    if let Some(stream) = output.stream {
                        debug!("Build output: {}", stream);
                    }
                    // Note: aux field contains build metadata but structure varies
                    // We'll use the image name as the ID
                }
                Err(e) => {
                    error!("Error building image: {}", e);
                    return Err(SoulBoxError::Internal(format!("Failed to build image: {}", e)));
                }
            }
        }

        if image_id.is_empty() {
            // If we didn't get an image ID from aux, try to get it from the image list
            image_id = image_name.to_string();
        }

        info!("Successfully built Docker image: {}", image_id);
        Ok(image_id)
    }

    /// Create a tar archive containing the Dockerfile
    fn create_dockerfile_tar(&self, dockerfile_content: &str) -> Result<Vec<u8>> {
        let mut tar_data = Vec::new();
        {
            let mut tar = Builder::new(&mut tar_data);
            
            // Add Dockerfile to the tar archive
            let dockerfile_bytes = dockerfile_content.as_bytes();
            let mut header = tar::Header::new_gnu();
            header.set_path("Dockerfile")?;
            header.set_size(dockerfile_bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            
            tar.append(&header, dockerfile_bytes)?;
            tar.finish()?;
        }
        
        Ok(tar_data)
    }

    /// Validate that an image exists
    pub async fn image_exists(&self, image_name: &str) -> Result<bool> {
        match self.docker.inspect_image(image_name).await {
            Ok(_) => Ok(true),
            Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => Ok(false),
            Err(e) => Err(SoulBoxError::Internal(format!("Failed to inspect image: {}", e))),
        }
    }

    /// Remove a Docker image
    pub async fn remove_image(&self, image_name: &str) -> Result<()> {
        info!("Removing Docker image: {}", image_name);
        
        self.docker
            .remove_image(image_name, None, None)
            .await
            .map_err(|e| SoulBoxError::Internal(format!("Failed to remove image: {}", e)))?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dockerfile_parser() {
        let dockerfile = r#"
FROM python:3.11-slim
WORKDIR /app
COPY requirements.txt .
RUN pip install -r requirements.txt
ENV PYTHONUNBUFFERED=1
EXPOSE 8000
CMD ["python", "app.py"]
"#;

        let mut parser = DockerfileParser::new(dockerfile.to_string());
        assert!(parser.parse().is_ok());
        assert!(parser.validate().is_ok());
        
        assert_eq!(parser.get_base_image(), Some("python:3.11-slim".to_string()));
        assert_eq!(parser.get_exposed_ports(), vec![8000]);
        
        let env_vars = parser.get_env_vars();
        assert_eq!(env_vars.get("PYTHONUNBUFFERED"), Some(&"1".to_string()));
    }

    #[test]
    fn test_dangerous_command_detection() {
        let dockerfile = r#"
FROM ubuntu:22.04
RUN rm -rf /
"#;

        let mut parser = DockerfileParser::new(dockerfile.to_string());
        assert!(parser.parse().is_ok());
        assert!(parser.validate().is_err());
    }

    #[test]
    fn test_dockerfile_without_from() {
        let dockerfile = r#"
RUN echo "Hello"
"#;

        let mut parser = DockerfileParser::new(dockerfile.to_string());
        assert!(parser.parse().is_err());
    }
}