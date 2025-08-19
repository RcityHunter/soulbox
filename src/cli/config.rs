//! CLI configuration management
//! 
//! Handles loading and managing configuration for the SoulBox CLI,
//! supporting both file-based and environment variable configuration.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, warn};

use crate::error::{Result, SoulBoxError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// SoulBox server endpoint URL
    pub server_url: String,
    
    /// API key for authentication
    pub api_key: Option<String>,
    
    /// Default timeout for operations (in seconds)
    pub timeout: u64,
    
    /// Default output format (json, table, yaml)
    pub output_format: String,
    
    /// Enable colored output
    pub colored_output: bool,
    
    /// Default working directory for sandboxes
    pub default_workdir: String,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            server_url: "http://localhost:8080".to_string(),
            api_key: None,
            timeout: 30,
            output_format: "table".to_string(),
            colored_output: true,
            default_workdir: "/workspace".to_string(),
        }
    }
}

impl CliConfig {
    /// Load configuration from file or create default
    pub async fn load(config_path: Option<&str>) -> Result<Self> {
        let config_file = Self::resolve_config_path(config_path)?;
        
        if config_file.exists() {
            debug!("Loading config from: {}", config_file.display());
            let content = fs::read_to_string(&config_file).await?;
            let mut config: CliConfig = toml::from_str(&content)?;
            
            // Override with environment variables if present
            config.apply_env_overrides();
            Ok(config)
        } else {
            debug!("Config file not found, using default configuration");
            let mut config = Self::default();
            config.apply_env_overrides();
            Ok(config)
        }
    }

    /// Save configuration to file
    pub async fn save(&self, config_path: Option<&str>) -> Result<()> {
        let config_file = Self::resolve_config_path(config_path)?;
        
        // Create parent directories if they don't exist
        if let Some(parent) = config_file.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        let content = toml::to_string_pretty(self)?;
        fs::write(config_file, content).await?;
        Ok(())
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        if let Ok(server_url) = std::env::var("SOULBOX_SERVER") {
            self.server_url = server_url;
        }
        
        if let Ok(api_key) = std::env::var("SOULBOX_API_KEY") {
            self.api_key = Some(api_key);
        }
        
        if let Ok(timeout_str) = std::env::var("SOULBOX_TIMEOUT") {
            if let Ok(timeout) = timeout_str.parse::<u64>() {
                self.timeout = timeout;
            } else {
                warn!("Invalid SOULBOX_TIMEOUT value: {}", timeout_str);
            }
        }
        
        if let Ok(output_format) = std::env::var("SOULBOX_OUTPUT_FORMAT") {
            self.output_format = output_format;
        }
    }

    /// Resolve configuration file path
    fn resolve_config_path(config_path: Option<&str>) -> Result<PathBuf> {
        if let Some(path) = config_path {
            Ok(PathBuf::from(path))
        } else {
            // Use default config path: ~/.config/soulbox/config.toml
            let home_dir = dirs::home_dir()
                .ok_or_else(|| SoulBoxError::config("Could not find home directory".to_string()))?;
            
            Ok(home_dir.join(".config").join("soulbox").join("config.toml"))
        }
    }

    /// Create a new config with server URL override
    pub fn with_server_url(mut self, server_url: Option<&str>) -> Self {
        if let Some(url) = server_url {
            self.server_url = url.to_string();
        }
        self
    }

    /// Create a new config with API key override
    pub fn with_api_key(mut self, api_key: Option<&str>) -> Self {
        if let Some(key) = api_key {
            self.api_key = Some(key.to_string());
        }
        self
    }

    /// Get the complete gRPC endpoint URL
    pub fn grpc_endpoint(&self) -> String {
        let base_url = self.server_url.trim_end_matches('/');
        if base_url.starts_with("http://") {
            base_url.replace("http://", "http://").replace(":8080", ":9080")
        } else if base_url.starts_with("https://") {
            base_url.replace("https://", "https://").replace(":8080", ":9080")
        } else {
            format!("http://{}:9080", base_url)
        }
    }

    /// Get the WebSocket endpoint URL
    pub fn ws_endpoint(&self) -> String {
        let base_url = self.server_url.trim_end_matches('/');
        if base_url.starts_with("http://") {
            base_url.replace("http://", "ws://") + "/ws"
        } else if base_url.starts_with("https://") {
            base_url.replace("https://", "wss://") + "/ws"
        } else {
            format!("ws://{}:8080/ws", base_url)
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.server_url.is_empty() {
            return Err(SoulBoxError::config("Server URL cannot be empty".to_string()));
        }

        if self.timeout == 0 {
            return Err(SoulBoxError::config("Timeout must be greater than 0".to_string()));
        }

        match self.output_format.as_str() {
            "json" | "table" | "yaml" => {}
            _ => return Err(SoulBoxError::config(
                format!("Invalid output format: {}. Must be one of: json, table, yaml", self.output_format)
            )),
        }

        Ok(())
    }
}

// Add dirs crate to Cargo.toml dependencies if not already present