//! PHP runtime implementation

use crate::error::Result;
use std::collections::HashMap;

/// PHP runtime configuration
pub struct PHPRuntime {
    version: String,
    composer_json: Option<String>,
    extensions: Vec<String>,
}

impl PHPRuntime {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            composer_json: None,
            extensions: Vec::new(),
        }
    }
    
    /// Add PHP extension
    pub fn with_extension(mut self, extension: impl Into<String>) -> Self {
        self.extensions.push(extension.into());
        self
    }
    
    /// Set composer.json content
    pub fn with_composer_json(mut self, content: impl Into<String>) -> Self {
        self.composer_json = Some(content.into());
        self
    }
    
    /// Get Docker image
    pub fn docker_image(&self) -> String {
        format!("php:{}-cli", self.version)
    }
    
    /// Get setup commands
    pub fn setup_commands(&self) -> Vec<String> {
        let mut commands = vec![];
        
        // Install extensions
        if !self.extensions.is_empty() {
            for ext in &self.extensions {
                commands.push(format!("docker-php-ext-install {}", ext));
            }
        }
        
        // Install Composer
        commands.push("curl -sS https://getcomposer.org/installer | php -- --install-dir=/usr/local/bin --filename=composer".to_string());
        
        // Install dependencies if composer.json exists
        if self.composer_json.is_some() {
            commands.push("composer install".to_string());
        }
        
        commands
    }
    
    /// Create default composer.json
    pub fn default_composer_json() -> String {
        r#"{
    "name": "soulbox/php-sandbox",
    "description": "PHP sandbox environment",
    "require": {
        "php": ">=8.0"
    }
}"#.to_string()
    }
    
    /// Get execution command
    pub fn exec_command(filename: &str) -> String {
        format!("php {}", filename)
    }
    
    /// Get REPL command
    pub fn repl_command() -> String {
        "php -a".to_string()
    }
}