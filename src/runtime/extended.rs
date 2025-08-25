//! Extended runtime support for additional programming languages

use crate::runtime::{RuntimeType, RuntimeConfig};
use crate::error::Result;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Extended runtime types for additional language support
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExtendedRuntimeType {
    // Scripting languages
    Ruby,
    PHP,
    Perl,
    Lua,
    
    // Compiled languages  
    Cpp,
    C,
    Swift,
    Kotlin,
    Scala,
    
    // Web technologies
    TypeScript,
    Deno,
    Bun,
    
    // Data science & ML
    R,
    Julia,
    Octave,
    
    // Functional languages
    Haskell,
    Elixir,
    Erlang,
    Clojure,
    FSharp,
    
    // Other languages
    Dart,
    Nim,
    Crystal,
    Zig,
}

impl ExtendedRuntimeType {
    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            ExtendedRuntimeType::Ruby => "ruby",
            ExtendedRuntimeType::PHP => "php",
            ExtendedRuntimeType::Perl => "perl",
            ExtendedRuntimeType::Lua => "lua",
            ExtendedRuntimeType::Cpp => "cpp",
            ExtendedRuntimeType::C => "c",
            ExtendedRuntimeType::Swift => "swift",
            ExtendedRuntimeType::Kotlin => "kotlin",
            ExtendedRuntimeType::Scala => "scala",
            ExtendedRuntimeType::TypeScript => "typescript",
            ExtendedRuntimeType::Deno => "deno",
            ExtendedRuntimeType::Bun => "bun",
            ExtendedRuntimeType::R => "r",
            ExtendedRuntimeType::Julia => "julia",
            ExtendedRuntimeType::Octave => "octave",
            ExtendedRuntimeType::Haskell => "haskell",
            ExtendedRuntimeType::Elixir => "elixir",
            ExtendedRuntimeType::Erlang => "erlang",
            ExtendedRuntimeType::Clojure => "clojure",
            ExtendedRuntimeType::FSharp => "fsharp",
            ExtendedRuntimeType::Dart => "dart",
            ExtendedRuntimeType::Nim => "nim",
            ExtendedRuntimeType::Crystal => "crystal",
            ExtendedRuntimeType::Zig => "zig",
        }
    }
    
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ruby" | "rb" => Some(ExtendedRuntimeType::Ruby),
            "php" => Some(ExtendedRuntimeType::PHP),
            "perl" | "pl" => Some(ExtendedRuntimeType::Perl),
            "lua" => Some(ExtendedRuntimeType::Lua),
            "cpp" | "c++" | "cxx" => Some(ExtendedRuntimeType::Cpp),
            "c" => Some(ExtendedRuntimeType::C),
            "swift" => Some(ExtendedRuntimeType::Swift),
            "kotlin" | "kt" => Some(ExtendedRuntimeType::Kotlin),
            "scala" => Some(ExtendedRuntimeType::Scala),
            "typescript" | "ts" => Some(ExtendedRuntimeType::TypeScript),
            "deno" => Some(ExtendedRuntimeType::Deno),
            "bun" => Some(ExtendedRuntimeType::Bun),
            "r" => Some(ExtendedRuntimeType::R),
            "julia" | "jl" => Some(ExtendedRuntimeType::Julia),
            "octave" => Some(ExtendedRuntimeType::Octave),
            "haskell" | "hs" => Some(ExtendedRuntimeType::Haskell),
            "elixir" | "ex" => Some(ExtendedRuntimeType::Elixir),
            "erlang" | "erl" => Some(ExtendedRuntimeType::Erlang),
            "clojure" | "clj" => Some(ExtendedRuntimeType::Clojure),
            "fsharp" | "f#" | "fs" => Some(ExtendedRuntimeType::FSharp),
            "dart" => Some(ExtendedRuntimeType::Dart),
            "nim" => Some(ExtendedRuntimeType::Nim),
            "crystal" | "cr" => Some(ExtendedRuntimeType::Crystal),
            "zig" => Some(ExtendedRuntimeType::Zig),
            _ => None,
        }
    }
    
    /// Create runtime configuration for extended language
    pub fn to_runtime_config(&self, version: Option<String>) -> RuntimeConfig {
        let version_str = version.clone().unwrap_or_else(|| "latest".to_string());
        let (image, env, install_cmds, exec_cmd, extensions) = match self {
            ExtendedRuntimeType::Ruby => (
                format!("ruby:{}", version.clone().unwrap_or_else(|| "3.2".to_string())),
                HashMap::new(),
                vec!["gem install bundler".to_string()],
                "ruby {file}".to_string(),
                vec!["rb".to_string()],
            ),
            ExtendedRuntimeType::PHP => (
                format!("php:{}-cli", version.clone().unwrap_or_else(|| "8.2".to_string())),
                HashMap::new(),
                vec!["composer install".to_string()],
                "php {file}".to_string(),
                vec!["php".to_string()],
            ),
            ExtendedRuntimeType::TypeScript => (
                "denoland/deno:latest".to_string(),
                HashMap::new(),
                vec![],
                "deno run --allow-all {file}".to_string(),
                vec!["ts".to_string(), "tsx".to_string()],
            ),
            ExtendedRuntimeType::Cpp => (
                "gcc:latest".to_string(),
                HashMap::new(),
                vec![],
                "g++ -o /tmp/a.out {file} && /tmp/a.out".to_string(),
                vec!["cpp".to_string(), "cc".to_string(), "cxx".to_string()],
            ),
            ExtendedRuntimeType::R => (
                format!("r-base:{}", version.clone().unwrap_or_else(|| "latest".to_string())),
                HashMap::new(),
                vec![],
                "Rscript {file}".to_string(),
                vec!["r".to_string(), "R".to_string()],
            ),
            ExtendedRuntimeType::Julia => (
                format!("julia:{}", version.clone().unwrap_or_else(|| "1.9".to_string())),
                HashMap::new(),
                vec![],
                "julia {file}".to_string(),
                vec!["jl".to_string()],
            ),
            ExtendedRuntimeType::Haskell => (
                "haskell:latest".to_string(),
                HashMap::new(),
                vec!["cabal update".to_string()],
                "runhaskell {file}".to_string(),
                vec!["hs".to_string()],
            ),
            ExtendedRuntimeType::Elixir => (
                format!("elixir:{}", version.clone().unwrap_or_else(|| "1.15".to_string())),
                HashMap::new(),
                vec!["mix deps.get".to_string()],
                "elixir {file}".to_string(),
                vec!["ex".to_string(), "exs".to_string()],
            ),
            ExtendedRuntimeType::Kotlin => (
                "zenika/kotlin:latest".to_string(),
                HashMap::new(),
                vec![],
                "kotlinc {file} -include-runtime -d /tmp/app.jar && java -jar /tmp/app.jar".to_string(),
                vec!["kt".to_string(), "kts".to_string()],
            ),
            ExtendedRuntimeType::Swift => (
                "swift:latest".to_string(),
                HashMap::new(),
                vec![],
                "swift {file}".to_string(),
                vec!["swift".to_string()],
            ),
            ExtendedRuntimeType::Dart => (
                "dart:stable".to_string(),
                HashMap::new(),
                vec!["dart pub get".to_string()],
                "dart run {file}".to_string(),
                vec!["dart".to_string()],
            ),
            ExtendedRuntimeType::Zig => (
                "euantorano/zig:latest".to_string(),
                HashMap::new(),
                vec![],
                "zig run {file}".to_string(),
                vec!["zig".to_string()],
            ),
            _ => {
                // Default fallback for other languages
                ("ubuntu:22.04".to_string(),
                 HashMap::new(),
                 vec![],
                 "echo 'Runtime not fully configured'".to_string(),
                 vec![])
            }
        };
        
        RuntimeConfig {
            runtime_type: RuntimeType::Shell, // Use Shell as base type for extended
            version: version_str,
            docker_image: image,
            working_directory: "/workspace".to_string(),
            environment: env,
            install_commands: install_cmds,
            exec_command: exec_cmd,
            file_extensions: extensions,
            memory_limit: Some(512 * 1024 * 1024), // 512MB default
            cpu_limit: Some(1.0),
            execution_timeout: 30,
        }
    }
}

/// Extended runtime manager for additional languages
pub struct ExtendedRuntimeManager {
    configs: HashMap<String, RuntimeConfig>,
}

impl ExtendedRuntimeManager {
    /// Create new extended runtime manager
    pub fn new() -> Self {
        let mut configs = HashMap::new();
        
        // Add Ruby
        configs.insert(
            "ruby".to_string(),
            ExtendedRuntimeType::Ruby.to_runtime_config(None),
        );
        
        // Add PHP
        configs.insert(
            "php".to_string(),
            ExtendedRuntimeType::PHP.to_runtime_config(None),
        );
        
        // Add TypeScript
        configs.insert(
            "typescript".to_string(),
            ExtendedRuntimeType::TypeScript.to_runtime_config(None),
        );
        
        // Add C++
        configs.insert(
            "cpp".to_string(),
            ExtendedRuntimeType::Cpp.to_runtime_config(None),
        );
        
        // Add R
        configs.insert(
            "r".to_string(),
            ExtendedRuntimeType::R.to_runtime_config(None),
        );
        
        // Add Julia
        configs.insert(
            "julia".to_string(),
            ExtendedRuntimeType::Julia.to_runtime_config(None),
        );
        
        Self { configs }
    }
    
    /// Get runtime configuration
    pub fn get_config(&self, language: &str) -> Option<&RuntimeConfig> {
        self.configs.get(language)
    }
    
    /// Add custom runtime configuration
    pub fn add_runtime(&mut self, name: String, config: RuntimeConfig) {
        self.configs.insert(name, config);
    }
    
    /// List all available extended runtimes
    pub fn list_runtimes(&self) -> Vec<&str> {
        self.configs.keys().map(|s| s.as_str()).collect()
    }
    
    /// Check if a file extension is supported
    pub fn is_extension_supported(&self, extension: &str) -> Option<String> {
        for (name, config) in &self.configs {
            if config.file_extensions.iter().any(|ext| ext == extension) {
                return Some(name.clone());
            }
        }
        None
    }
}