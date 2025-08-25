use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use super::TemplateError;

/// Template runtime types (using runtime module's RuntimeType)
use crate::runtime::RuntimeType;

// RuntimeType Display and FromStr are already implemented in runtime module

/// Resource limits for template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub memory_mb: Option<u64>,
    pub cpu_cores: Option<f64>,
    pub disk_mb: Option<u64>,
    pub network_enabled: bool,
    pub timeout_seconds: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_mb: Some(512),
            cpu_cores: Some(1.0),
            disk_mb: Some(1024),
            network_enabled: true,
            timeout_seconds: Some(300),
        }
    }
}

/// Template metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub runtime_type: RuntimeType,
    pub version: String,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub is_public: bool,
    pub is_verified: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Template version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVersion {
    pub version: String,
    pub changelog: Option<String>,
    pub created_at: DateTime<Utc>,
    pub is_stable: bool,
    pub dependencies: Vec<String>,
}

/// File reference for templates (stores metadata, not content)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateFile {
    pub path: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub checksum: Option<String>, // SHA-256 hash for integrity verification
    pub created_at: DateTime<Utc>,
}

/// Complete template definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub metadata: TemplateMetadata,
    pub base_image: String,
    pub default_command: Option<String>,
    pub environment_vars: HashMap<String, String>,
    pub resource_limits: ResourceLimits,
    pub files: Vec<TemplateFile>, // file references, not content
    pub setup_commands: Vec<String>,
    pub versions: Vec<TemplateVersion>,
    pub owner_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
}

impl Template {
    pub fn new(
        name: String,
        slug: String,
        runtime_type: RuntimeType,
        base_image: String,
    ) -> Self {
        let now = Utc::now();
        let metadata = TemplateMetadata {
            id: Uuid::new_v4(),
            name,
            slug,
            description: None,
            runtime_type,
            version: "1.0.0".to_string(),
            author: None,
            tags: vec![],
            is_public: false,
            is_verified: false,
            created_at: now,
            updated_at: now,
        };

        let version = TemplateVersion {
            version: "1.0.0".to_string(),
            changelog: Some("Initial version".to_string()),
            created_at: now,
            is_stable: true,
            dependencies: vec![],
        };

        Self {
            metadata,
            base_image,
            default_command: None,
            environment_vars: HashMap::new(),
            resource_limits: ResourceLimits::default(),
            files: Vec::new(),
            setup_commands: vec![],
            versions: vec![version],
            owner_id: None,
            tenant_id: None,
        }
    }

    /// Get the current version
    pub fn current_version(&self) -> Option<&TemplateVersion> {
        self.versions
            .iter()
            .find(|v| v.version == self.metadata.version)
            .or_else(|| self.versions.first()) // Fallback to first version if current not found
    }

    /// Get the current version (required), returns error if not found
    pub fn current_version_required(&self) -> Result<&TemplateVersion, TemplateError> {
        self.current_version()
            .ok_or_else(|| TemplateError::VersionConflict(
                format!("Current version '{}' not found and no versions available", self.metadata.version)
            ))
    }

    /// Add a new version
    pub fn add_version(&mut self, version: TemplateVersion) {
        self.versions.push(version.clone());
        self.metadata.version = version.version;
        self.metadata.updated_at = Utc::now();
    }

    /// Validate template configuration
    pub fn validate(&self) -> TemplateValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Validate metadata
        if self.metadata.name.is_empty() {
            errors.push("Template name cannot be empty".to_string());
        }

        if self.metadata.slug.is_empty() {
            errors.push("Template slug cannot be empty".to_string());
        }

        // Validate slug format (URL-friendly)
        if !self.metadata.slug.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            errors.push("Template slug must contain only alphanumeric characters, hyphens, and underscores".to_string());
        }

        // Validate base image
        if self.base_image.is_empty() {
            errors.push("Base image cannot be empty".to_string());
        }

        // Validate resource limits
        if let Some(memory) = self.resource_limits.memory_mb {
            if memory == 0 {
                errors.push("Memory limit must be greater than 0".to_string());
            }
            if memory > 16384 { // 16GB limit
                warnings.push("Memory limit is very high (>16GB)".to_string());
            }
        }

        if let Some(cpu) = self.resource_limits.cpu_cores {
            if cpu <= 0.0 {
                errors.push("CPU cores must be greater than 0".to_string());
            }
            if cpu > 16.0 { // 16 cores limit
                warnings.push("CPU cores limit is very high (>16 cores)".to_string());
            }
        }

        // Validate versions
        if self.versions.is_empty() {
            errors.push("Template must have at least one version".to_string());
        } else {
            // Check if current version exists in versions list
            if self.current_version().is_none() {
                errors.push(format!("Current version '{}' not found in versions list", self.metadata.version));
            }
        }

        TemplateValidationResult {
            is_valid: errors.is_empty(),
            errors,
            warnings,
        }
    }
}

/// Template validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Template creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub runtime_type: RuntimeType,
    pub base_image: String,
    pub default_command: Option<String>,
    pub environment_vars: Option<HashMap<String, String>>,
    pub resource_limits: Option<ResourceLimits>,
    pub files: Option<Vec<TemplateFileUpload>>, // File upload requests instead of content
    pub setup_commands: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub is_public: Option<bool>,
}

/// File upload request for templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateFileUpload {
    pub path: String,
    pub content: String, // Base64 encoded content or plain text
    pub mime_type: Option<String>,
}

/// Template update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub base_image: Option<String>,
    pub default_command: Option<String>,
    pub environment_vars: Option<HashMap<String, String>>,
    pub resource_limits: Option<ResourceLimits>,
    pub files: Option<Vec<TemplateFileUpload>>, // File upload requests instead of content
    pub setup_commands: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub is_public: Option<bool>,
}

/// Default templates for common languages
impl Template {
    pub fn default_python() -> Self {
        let mut template = Template::new(
            "Python 3.11".to_string(),
            "python-3-11".to_string(),
            RuntimeType::Python,
            "python:3.11-slim".to_string(),
        );

        template.metadata.description = Some("Python 3.11 environment with common packages".to_string());
        template.metadata.tags = vec!["python".to_string(), "default".to_string()];
        template.metadata.is_public = true;
        template.metadata.is_verified = true;
        template.default_command = Some("python".to_string());
        
        // Add common Python packages
        template.setup_commands = vec![
            "pip install --no-cache-dir numpy pandas matplotlib requests".to_string(),
        ];

        // Add a sample main.py file reference (actual file would be stored separately)
        template.files.push(TemplateFile {
            path: "main.py".to_string(),
            size: "#!/usr/bin/env python3\n\nprint(\"Hello from Python!\")\n".len() as u64,
            mime_type: Some("text/x-python".to_string()),
            checksum: None, // Would be calculated when file is stored
            created_at: template.metadata.created_at,
        });

        template
    }

    pub fn default_node() -> Self {
        let mut template = Template::new(
            "Node.js 18".to_string(),
            "node-18".to_string(),
            RuntimeType::NodeJS,
            "node:18-slim".to_string(),
        );

        template.metadata.description = Some("Node.js 18 environment with npm".to_string());
        template.metadata.tags = vec!["node".to_string(), "javascript".to_string(), "default".to_string()];
        template.metadata.is_public = true;
        template.metadata.is_verified = true;
        template.default_command = Some("node".to_string());

        // Add package.json file reference
        let package_json_content = r#"{
  "name": "sandbox-project",
  "version": "1.0.0",
  "description": "Sandbox Node.js project",
  "main": "index.js",
  "scripts": {
    "start": "node index.js"
  },
  "dependencies": {}
}
"#;
        template.files.push(TemplateFile {
            path: "package.json".to_string(),
            size: package_json_content.len() as u64,
            mime_type: Some("application/json".to_string()),
            checksum: None,
            created_at: template.metadata.created_at,
        });

        // Add sample index.js file reference
        let index_js_content = "console.log('Hello from Node.js!');\n";
        template.files.push(TemplateFile {
            path: "index.js".to_string(),
            size: index_js_content.len() as u64,
            mime_type: Some("application/javascript".to_string()),
            checksum: None,
            created_at: template.metadata.created_at,
        });

        template
    }

    pub fn default_rust() -> Self {
        let mut template = Template::new(
            "Rust Latest".to_string(),
            "rust-latest".to_string(),
            RuntimeType::Rust,
            "rust:latest".to_string(),
        );

        template.metadata.description = Some("Rust environment with Cargo".to_string());
        template.metadata.tags = vec!["rust".to_string(), "default".to_string()];
        template.metadata.is_public = true;
        template.metadata.is_verified = true;
        template.default_command = Some("cargo run".to_string());

        // Add Cargo.toml file reference
        let cargo_toml_content = r#"[package]
name = "sandbox-project"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
        template.files.push(TemplateFile {
            path: "Cargo.toml".to_string(),
            size: cargo_toml_content.len() as u64,
            mime_type: Some("text/x-toml".to_string()),
            checksum: None,
            created_at: template.metadata.created_at,
        });

        // Add main.rs file reference
        let main_rs_content = "fn main() {\n    println!(\"Hello from Rust!\");\n}\n";
        template.files.push(TemplateFile {
            path: "src/main.rs".to_string(),
            size: main_rs_content.len() as u64,
            mime_type: Some("text/x-rust".to_string()),
            checksum: None,
            created_at: template.metadata.created_at,
        });

        template
    }
}