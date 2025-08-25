//! TypeScript runtime implementation using Deno

use serde::{Deserialize, Serialize};

/// TypeScript runtime configuration
pub struct TypeScriptRuntime {
    runtime_engine: TypeScriptEngine,
    version: String,
    permissions: DenoPermissions,
}

/// TypeScript execution engines
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TypeScriptEngine {
    Deno,
    Bun,
    NodeWithTS,
}

/// Deno permissions configuration
#[derive(Debug, Clone, Default)]
pub struct DenoPermissions {
    pub allow_read: bool,
    pub allow_write: bool,
    pub allow_net: bool,
    pub allow_env: bool,
    pub allow_run: bool,
    pub allow_all: bool,
}

impl TypeScriptRuntime {
    pub fn new(engine: TypeScriptEngine) -> Self {
        Self {
            runtime_engine: engine,
            version: "latest".to_string(),
            permissions: DenoPermissions::default(),
        }
    }
    
    /// Create with Deno engine
    pub fn with_deno() -> Self {
        Self::new(TypeScriptEngine::Deno)
    }
    
    /// Create with Bun engine
    pub fn with_bun() -> Self {
        Self::new(TypeScriptEngine::Bun)
    }
    
    /// Set permissions for Deno
    pub fn with_permissions(mut self, perms: DenoPermissions) -> Self {
        self.permissions = perms;
        self
    }
    
    /// Get Docker image
    pub fn docker_image(&self) -> String {
        match self.runtime_engine {
            TypeScriptEngine::Deno => "denoland/deno:latest".to_string(),
            TypeScriptEngine::Bun => "oven/bun:latest".to_string(),
            TypeScriptEngine::NodeWithTS => "node:20-slim".to_string(),
        }
    }
    
    /// Get setup commands
    pub fn setup_commands(&self) -> Vec<String> {
        match self.runtime_engine {
            TypeScriptEngine::Deno => vec![],
            TypeScriptEngine::Bun => vec![],
            TypeScriptEngine::NodeWithTS => vec![
                "npm install -g typescript ts-node".to_string(),
                "npm install @types/node".to_string(),
            ],
        }
    }
    
    /// Get execution command
    pub fn exec_command(&self, filename: &str) -> String {
        match self.runtime_engine {
            TypeScriptEngine::Deno => {
                let mut cmd = "deno run".to_string();
                
                if self.permissions.allow_all {
                    cmd.push_str(" --allow-all");
                } else {
                    if self.permissions.allow_read {
                        cmd.push_str(" --allow-read");
                    }
                    if self.permissions.allow_write {
                        cmd.push_str(" --allow-write");
                    }
                    if self.permissions.allow_net {
                        cmd.push_str(" --allow-net");
                    }
                    if self.permissions.allow_env {
                        cmd.push_str(" --allow-env");
                    }
                    if self.permissions.allow_run {
                        cmd.push_str(" --allow-run");
                    }
                }
                
                format!("{} {}", cmd, filename)
            }
            TypeScriptEngine::Bun => format!("bun run {}", filename),
            TypeScriptEngine::NodeWithTS => format!("ts-node {}", filename),
        }
    }
    
    /// Get REPL command
    pub fn repl_command(&self) -> String {
        match self.runtime_engine {
            TypeScriptEngine::Deno => "deno repl".to_string(),
            TypeScriptEngine::Bun => "bun repl".to_string(),
            TypeScriptEngine::NodeWithTS => "ts-node".to_string(),
        }
    }
    
    /// Create default tsconfig.json
    pub fn default_tsconfig() -> String {
        r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "lib": ["ES2022"],
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "allowSyntheticDefaultImports": true
  }
}"#.to_string()
    }
}