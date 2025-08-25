//! Ruby runtime implementation

/// Ruby runtime configuration
pub struct RubyRuntime {
    version: String,
    gemfile_content: Option<String>,
}

impl RubyRuntime {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            gemfile_content: None,
        }
    }
    
    /// Set Gemfile content for dependencies
    pub fn with_gemfile(mut self, content: impl Into<String>) -> Self {
        self.gemfile_content = Some(content.into());
        self
    }
    
    /// Get Docker image for Ruby
    pub fn docker_image(&self) -> String {
        format!("ruby:{}-slim", self.version)
    }
    
    /// Get setup commands
    pub fn setup_commands(&self) -> Vec<String> {
        let mut commands = vec![
            "apt-get update -qq".to_string(),
            "apt-get install -y build-essential".to_string(),
            "gem install bundler".to_string(),
        ];
        
        if self.gemfile_content.is_some() {
            commands.push("bundle install".to_string());
        }
        
        commands
    }
    
    /// Create default Gemfile
    pub fn default_gemfile() -> String {
        r#"source 'https://rubygems.org'

# Add your gems here
gem 'json'
gem 'net-http'
"#.to_string()
    }
    
    /// Get execution command
    pub fn exec_command(filename: &str) -> String {
        format!("ruby {}", filename)
    }
    
    /// Get REPL command
    pub fn repl_command() -> String {
        "irb".to_string()
    }
}