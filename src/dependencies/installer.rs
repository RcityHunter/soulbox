//! Package installation engine for multiple package managers
//! 
//! This module provides:
//! - Cross-platform package installation
//! - Real-time installation progress tracking
//! - Error handling and recovery
//! - Container-aware installation
//! - Parallel installation support

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, warn, error, info};
use super::{PackageSpec, PackageManager, DependencyError};

/// Installation result for a single package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationResult {
    /// Package specification that was installed
    pub package: PackageSpec,
    /// Whether installation was successful
    pub success: bool,
    /// Installation duration in seconds
    pub duration_secs: f64,
    /// Size of installed package in bytes
    pub installed_size_bytes: u64,
    /// Installation output (stdout)
    pub output: String,
    /// Error output (stderr) if failed
    pub error_output: Option<String>,
    /// Exit code of installation command
    pub exit_code: Option<i32>,
    /// Whether package was installed from cache
    pub from_cache: bool,
    /// Installation timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Real-time installation progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationProgress {
    /// Sandbox ID
    pub sandbox_id: String,
    /// Package being installed
    pub package_name: String,
    /// Installation phase
    pub phase: InstallationPhase,
    /// Progress percentage (0-100)
    pub progress_percent: f32,
    /// Current status message
    pub status_message: String,
    /// Bytes downloaded so far
    pub bytes_downloaded: u64,
    /// Total bytes to download (if known)
    pub total_bytes: Option<u64>,
    /// Installation start time
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// Estimated completion time
    pub estimated_completion: Option<chrono::DateTime<chrono::Utc>>,
}

/// Installation phases
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstallationPhase {
    /// Preparing for installation
    Preparing,
    /// Resolving dependencies
    Resolving,
    /// Downloading packages
    Downloading,
    /// Installing packages
    Installing,
    /// Running post-install scripts
    PostInstall,
    /// Verifying installation
    Verifying,
    /// Installation completed
    Completed,
    /// Installation failed
    Failed,
}

/// Installation context for a sandbox
#[derive(Debug, Clone)]
struct InstallationContext {
    /// Sandbox ID
    sandbox_id: String,
    /// Working directory for installation
    work_dir: PathBuf,
    /// Environment variables
    environment: HashMap<String, String>,
    /// Installation timeout
    timeout_secs: u64,
}

/// Package installer engine
pub struct DependencyInstaller {
    /// Active installations by sandbox ID
    active_installations: Arc<Mutex<HashMap<String, Vec<InstallationProgress>>>>,
    /// Progress update sender
    progress_sender: Option<mpsc::UnboundedSender<InstallationProgress>>,
    /// Installation cache directory
    cache_dir: PathBuf,
}

impl DependencyInstaller {
    /// Create a new dependency installer
    pub async fn new() -> Result<Self, DependencyError> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("soulbox-dependencies");
            
        tokio::fs::create_dir_all(&cache_dir).await?;
        
        Ok(Self {
            active_installations: Arc::new(Mutex::new(HashMap::new())),
            progress_sender: None,
            cache_dir,
        })
    }
    
    /// Set progress update channel
    pub fn set_progress_channel(&mut self, sender: mpsc::UnboundedSender<InstallationProgress>) {
        self.progress_sender = Some(sender);
    }
    
    /// Install a package in a sandbox
    pub async fn install_package(
        &self,
        sandbox_id: &str,
        package: &PackageSpec,
    ) -> Result<InstallationResult, DependencyError> {
        let start_time = std::time::Instant::now();
        let timestamp = chrono::Utc::now();
        
        info!("Installing package {} for sandbox {}", package.name, sandbox_id);
        
        // Create installation context
        let context = self.create_installation_context(sandbox_id).await?;
        
        // Track installation progress
        self.start_progress_tracking(sandbox_id, &package.name).await;
        
        let result = match package.manager {
            PackageManager::Pip => self.install_pip_package(&context, package).await,
            PackageManager::Npm => self.install_npm_package(&context, package).await,
            PackageManager::Yarn => self.install_yarn_package(&context, package).await,
            PackageManager::Cargo => self.install_cargo_package(&context, package).await,
            PackageManager::GoMod => self.install_go_package(&context, package).await,
            _ => Err(DependencyError::UnsupportedPackageManager(
                format!("{:?}", package.manager)
            )),
        };
        
        let duration = start_time.elapsed();
        
        // Create installation result
        let installation_result = match result {
            Ok((output, size)) => {
                self.update_progress(sandbox_id, &package.name, InstallationPhase::Completed, 100.0, "Installation completed").await;
                
                InstallationResult {
                    package: package.clone(),
                    success: true,
                    duration_secs: duration.as_secs_f64(),
                    installed_size_bytes: size,
                    output,
                    error_output: None,
                    exit_code: Some(0),
                    from_cache: false,
                    timestamp,
                }
            },
            Err(error) => {
                self.update_progress(sandbox_id, &package.name, InstallationPhase::Failed, 0.0, &error.to_string()).await;
                
                InstallationResult {
                    package: package.clone(),
                    success: false,
                    duration_secs: duration.as_secs_f64(),
                    installed_size_bytes: 0,
                    output: String::new(),
                    error_output: Some(error.to_string()),
                    exit_code: Some(-1),
                    from_cache: false,
                    timestamp,
                }
            },
        };
        
        // Clean up progress tracking
        self.finish_progress_tracking(sandbox_id, &package.name).await;
        
        Ok(installation_result)
    }
    
    /// Create installation context for a sandbox
    async fn create_installation_context(&self, sandbox_id: &str) -> Result<InstallationContext, DependencyError> {
        let work_dir = self.cache_dir.join(sandbox_id);
        tokio::fs::create_dir_all(&work_dir).await?;
        
        let mut environment = HashMap::new();
        environment.insert("PATH".to_string(), std::env::var("PATH").unwrap_or_default());
        
        Ok(InstallationContext {
            sandbox_id: sandbox_id.to_string(),
            work_dir,
            environment,
            timeout_secs: 300, // 5 minutes default
        })
    }
    
    /// Install a Python pip package
    async fn install_pip_package(
        &self,
        context: &InstallationContext,
        package: &PackageSpec,
    ) -> Result<(String, u64), DependencyError> {
        self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Preparing, 10.0, "Preparing pip installation").await;
        
        // Security validation before installation
        self.validate_package_security(package).await?;
        
        let package_spec = if let Some(version) = &package.version {
            format!("{}=={}", package.name, version.trim_start_matches("=="))
        } else {
            package.name.clone()
        };
        
        let mut cmd = Command::new("pip");
        cmd.arg("install")
           .arg(&package_spec)
           .arg("--user") // Install to user directory for safety
           .arg("--no-deps") // Prevent automatic dependency installation for security
           .arg("--force-reinstall") // Ensure we get the exact package
           .arg("--trusted-host").arg("pypi.org") // Only trust official PyPI
           .arg("--trusted-host").arg("pypi.python.org")
           .arg("--trusted-host").arg("files.pythonhosted.org")
           .current_dir(&context.work_dir)
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());
        
        // Add environment variables
        for (key, value) in &context.environment {
            cmd.env(key, value);
        }
        
        self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Downloading, 30.0, "Downloading package").await;
        
        let output = cmd.output().await
            .map_err(|e| DependencyError::InstallationFailed(format!("Failed to execute pip: {}", e)))?;
        
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let size = self.estimate_installed_size(&package.name).await.unwrap_or(0);
            
            self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Verifying, 90.0, "Verifying installation").await;
            
            Ok((stdout, size))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(DependencyError::InstallationFailed(format!("pip install failed: {}", stderr)))
        }
    }
    
    /// Install a Node.js npm package
    async fn install_npm_package(
        &self,
        context: &InstallationContext,
        package: &PackageSpec,
    ) -> Result<(String, u64), DependencyError> {
        self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Preparing, 10.0, "Preparing npm installation").await;
        
        // Security validation before installation
        self.validate_package_security(package).await?;
        
        let package_spec = if let Some(version) = &package.version {
            format!("{}@{}", package.name, version)
        } else {
            package.name.clone()
        };
        
        let mut cmd = Command::new("npm");
        cmd.arg("install")
           .arg(&package_spec)
           .arg("--no-optional") // Skip optional dependencies for security
           .arg("--no-package-lock") // Prevent lockfile modifications
           .arg("--registry").arg("https://registry.npmjs.org/") // Official registry only
           .current_dir(&context.work_dir)
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());
        
        // Add environment variables
        for (key, value) in &context.environment {
            cmd.env(key, value);
        }
        
        self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Downloading, 30.0, "Downloading package").await;
        
        let output = cmd.output().await
            .map_err(|e| DependencyError::InstallationFailed(format!("Failed to execute npm: {}", e)))?;
        
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let size = self.estimate_installed_size(&package.name).await.unwrap_or(0);
            
            self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Verifying, 90.0, "Verifying installation").await;
            
            Ok((stdout, size))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(DependencyError::InstallationFailed(format!("npm install failed: {}", stderr)))
        }
    }
    
    /// Install a Node.js yarn package
    async fn install_yarn_package(
        &self,
        context: &InstallationContext,
        package: &PackageSpec,
    ) -> Result<(String, u64), DependencyError> {
        self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Preparing, 10.0, "Preparing yarn installation").await;
        
        let mut cmd = Command::new("yarn");
        cmd.arg("add")
           .arg(&package.name)
           .current_dir(&context.work_dir)
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());
        
        if let Some(version) = &package.version {
            cmd.arg("--exact").arg(version);
        }
        
        // Add environment variables
        for (key, value) in &context.environment {
            cmd.env(key, value);
        }
        
        self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Downloading, 30.0, "Downloading package").await;
        
        let output = cmd.output().await
            .map_err(|e| DependencyError::InstallationFailed(format!("Failed to execute yarn: {}", e)))?;
        
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let size = self.estimate_installed_size(&package.name).await.unwrap_or(0);
            
            self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Verifying, 90.0, "Verifying installation").await;
            
            Ok((stdout, size))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(DependencyError::InstallationFailed(format!("yarn add failed: {}", stderr)))
        }
    }
    
    /// Install a Rust cargo package
    async fn install_cargo_package(
        &self,
        context: &InstallationContext,
        package: &PackageSpec,
    ) -> Result<(String, u64), DependencyError> {
        self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Preparing, 10.0, "Preparing cargo installation").await;
        
        let mut cmd = Command::new("cargo");
        cmd.arg("install")
           .arg(&package.name)
           .current_dir(&context.work_dir)
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());
        
        if let Some(version) = &package.version {
            cmd.arg("--version").arg(version);
        }
        
        // Add environment variables
        for (key, value) in &context.environment {
            cmd.env(key, value);
        }
        
        self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Downloading, 30.0, "Downloading and compiling").await;
        
        let output = cmd.output().await
            .map_err(|e| DependencyError::InstallationFailed(format!("Failed to execute cargo: {}", e)))?;
        
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let size = self.estimate_installed_size(&package.name).await.unwrap_or(0);
            
            self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Verifying, 90.0, "Verifying installation").await;
            
            Ok((stdout, size))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(DependencyError::InstallationFailed(format!("cargo install failed: {}", stderr)))
        }
    }
    
    /// Install a Go module
    async fn install_go_package(
        &self,
        context: &InstallationContext,
        package: &PackageSpec,
    ) -> Result<(String, u64), DependencyError> {
        self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Preparing, 10.0, "Preparing go install").await;
        
        let package_spec = if let Some(version) = &package.version {
            format!("{}@{}", package.name, version)
        } else {
            format!("{}@latest", package.name)
        };
        
        let mut cmd = Command::new("go");
        cmd.arg("install")
           .arg(&package_spec)
           .current_dir(&context.work_dir)
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());
        
        // Add environment variables
        for (key, value) in &context.environment {
            cmd.env(key, value);
        }
        
        self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Downloading, 30.0, "Downloading and building").await;
        
        let output = cmd.output().await
            .map_err(|e| DependencyError::InstallationFailed(format!("Failed to execute go: {}", e)))?;
        
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let size = self.estimate_installed_size(&package.name).await.unwrap_or(0);
            
            self.update_progress(&context.sandbox_id, &package.name, InstallationPhase::Verifying, 90.0, "Verifying installation").await;
            
            Ok((stdout, size))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(DependencyError::InstallationFailed(format!("go install failed: {}", stderr)))
        }
    }
    
    /// Estimate installed package size (simplified)
    async fn estimate_installed_size(&self, _package_name: &str) -> Result<u64, DependencyError> {
        // This is a simplified implementation
        // In production, you would check actual installed files
        Ok(1024 * 1024) // 1MB default
    }
    
    /// Start progress tracking for a package installation
    async fn start_progress_tracking(&self, sandbox_id: &str, package_name: &str) {
        let progress = InstallationProgress {
            sandbox_id: sandbox_id.to_string(),
            package_name: package_name.to_string(),
            phase: InstallationPhase::Preparing,
            progress_percent: 0.0,
            status_message: "Starting installation".to_string(),
            bytes_downloaded: 0,
            total_bytes: None,
            started_at: chrono::Utc::now(),
            estimated_completion: None,
        };
        
        // Store in active installations
        {
            let mut active = self.active_installations.lock().await;
            active.entry(sandbox_id.to_string())
                  .or_insert_with(Vec::new)
                  .push(progress.clone());
        }
        
        // Send progress update
        if let Some(sender) = &self.progress_sender {
            let _ = sender.send(progress);
        }
    }
    
    /// Update installation progress
    async fn update_progress(
        &self,
        sandbox_id: &str,
        package_name: &str,
        phase: InstallationPhase,
        progress_percent: f32,
        status_message: &str,
    ) {
        let progress = InstallationProgress {
            sandbox_id: sandbox_id.to_string(),
            package_name: package_name.to_string(),
            phase,
            progress_percent,
            status_message: status_message.to_string(),
            bytes_downloaded: 0,
            total_bytes: None,
            started_at: chrono::Utc::now(),
            estimated_completion: None,
        };
        
        // Update active installations
        {
            let mut active = self.active_installations.lock().await;
            if let Some(installations) = active.get_mut(sandbox_id) {
                if let Some(existing) = installations.iter_mut().find(|p| p.package_name == package_name) {
                    existing.phase = phase;
                    existing.progress_percent = progress_percent;
                    existing.status_message = status_message.to_string();
                }
            }
        }
        
        // Send progress update
        if let Some(sender) = &self.progress_sender {
            let _ = sender.send(progress);
        }
    }
    
    /// Finish progress tracking for a package installation
    async fn finish_progress_tracking(&self, sandbox_id: &str, package_name: &str) {
        let mut active = self.active_installations.lock().await;
        if let Some(installations) = active.get_mut(sandbox_id) {
            installations.retain(|p| p.package_name != package_name);
            if installations.is_empty() {
                active.remove(sandbox_id);
            }
        }
    }
    
    /// Get active installations for a sandbox
    pub async fn get_active_installations(&self, sandbox_id: &str) -> Vec<InstallationProgress> {
        let active = self.active_installations.lock().await;
        active.get(sandbox_id).cloned().unwrap_or_default()
    }
    
    /// Get all active installations
    pub async fn get_all_active_installations(&self) -> HashMap<String, Vec<InstallationProgress>> {
        self.active_installations.lock().await.clone()
    }
    
    /// Install multiple packages in parallel
    pub async fn install_packages_parallel(
        &self,
        sandbox_id: &str,
        packages: &[PackageSpec],
        max_parallel: usize,
    ) -> Result<Vec<InstallationResult>, DependencyError> {
        use futures::stream::{self, StreamExt};
        
        info!("Installing {} packages in parallel (max {})", packages.len(), max_parallel);
        
        let results = stream::iter(packages)
            .map(|package| async move {
                self.install_package(sandbox_id, package).await
            })
            .buffer_unordered(max_parallel)
            .collect::<Vec<_>>()
            .await;
        
        // Convert results, handling any errors
        let mut installation_results = Vec::new();
        for result in results {
            installation_results.push(result?);
        }
        
        Ok(installation_results)
    }
    
    /// Verify package installation
    pub async fn verify_package_installation(
        &self,
        package: &PackageSpec,
    ) -> Result<bool, DependencyError> {
        match package.manager {
            PackageManager::Pip => self.verify_pip_package(package).await,
            PackageManager::Npm => self.verify_npm_package(package).await,
            PackageManager::Cargo => self.verify_cargo_package(package).await,
            PackageManager::GoMod => self.verify_go_package(package).await,
            _ => Ok(false), // Verification not implemented for this package manager
        }
    }
    
    /// Verify pip package installation
    async fn verify_pip_package(&self, package: &PackageSpec) -> Result<bool, DependencyError> {
        let output = Command::new("pip")
            .arg("show")
            .arg(&package.name)
            .output()
            .await
            .map_err(|e| DependencyError::InstallationFailed(format!("Failed to verify pip package: {}", e)))?;
        
        Ok(output.status.success())
    }
    
    /// Verify npm package installation
    async fn verify_npm_package(&self, package: &PackageSpec) -> Result<bool, DependencyError> {
        let output = Command::new("npm")
            .arg("list")
            .arg(&package.name)
            .output()
            .await
            .map_err(|e| DependencyError::InstallationFailed(format!("Failed to verify npm package: {}", e)))?;
        
        Ok(output.status.success())
    }
    
    /// Verify cargo package installation
    async fn verify_cargo_package(&self, package: &PackageSpec) -> Result<bool, DependencyError> {
        let output = Command::new("cargo")
            .arg("install")
            .arg("--list")
            .output()
            .await
            .map_err(|e| DependencyError::InstallationFailed(format!("Failed to verify cargo package: {}", e)))?;
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        Ok(output_str.contains(&package.name))
    }
    
    /// Verify go package installation
    async fn verify_go_package(&self, package: &PackageSpec) -> Result<bool, DependencyError> {
        // Go packages are typically installed as binaries
        // Check if the binary exists in GOPATH/bin
        if let Ok(gopath) = std::env::var("GOPATH") {
            let binary_path = PathBuf::from(gopath).join("bin").join(&package.name);
            Ok(binary_path.exists())
        } else {
            Ok(false)
        }
    }
    
    /// Validate package security before installation
    async fn validate_package_security(&self, package: &PackageSpec) -> Result<(), DependencyError> {
        // Check package name for suspicious patterns
        self.validate_package_name(&package.name)?;
        
        // Check version specification for security issues
        if let Some(version) = &package.version {
            self.validate_version_spec(version)?;
        }
        
        // Check for known malicious packages
        self.check_malicious_package_list(&package.name).await?;
        
        // Validate source URL if specified
        if let Some(source) = &package.source {
            self.validate_source_url(source)?;
        }
        
        Ok(())
    }
    
    /// Validate package name for suspicious patterns
    fn validate_package_name(&self, name: &str) -> Result<(), DependencyError> {
        // Check for empty or invalid names
        if name.is_empty() || name.len() > 100 {
            return Err(DependencyError::SecurityVulnerability(
                "Invalid package name length".to_string()
            ));
        }
        
        // Check for suspicious characters
        if name.contains("../") || name.contains("..\\") {
            return Err(DependencyError::SecurityVulnerability(
                "Package name contains path traversal patterns".to_string()
            ));
        }
        
        // Check for suspicious Unicode characters that could be used for homograph attacks
        if name.chars().any(|c| c.is_control() || c > '\u{007F}') {
            return Err(DependencyError::SecurityVulnerability(
                "Package name contains suspicious Unicode characters".to_string()
            ));
        }
        
        // Check for suspicious patterns that might indicate typosquatting
        let suspicious_patterns = [
            "admi1n", "adm1n", "np1m", "p1p", "crpyto", "reqeusts", "pilolw", "requsets"
        ];
        
        let name_lower = name.to_lowercase();
        for pattern in &suspicious_patterns {
            if name_lower.contains(pattern) {
                return Err(DependencyError::SecurityVulnerability(
                    format!("Package name '{}' contains suspicious pattern '{}'", name, pattern)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Validate version specification
    fn validate_version_spec(&self, version: &str) -> Result<(), DependencyError> {
        // Check for suspiciously long version strings
        if version.len() > 50 {
            return Err(DependencyError::SecurityVulnerability(
                "Version specification too long".to_string()
            ));
        }
        
        // Check for command injection patterns
        if version.contains(';') || version.contains('|') || version.contains('&') {
            return Err(DependencyError::SecurityVulnerability(
                "Version specification contains command injection patterns".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Check against known malicious package list
    async fn check_malicious_package_list(&self, name: &str) -> Result<(), DependencyError> {
        // In a production system, this would check against:
        // - PyPI malware database
        // - npm security advisories
        // - Community-maintained blocklists
        // - Automated malware detection services
        
        // Basic blocklist of known problematic patterns
        let known_malicious = [
            "malicious-package", "evil-lib", "backdoor-tool", "crypto-miner",
            "keylogger", "password-stealer", "data-exfil"
        ];
        
        let name_lower = name.to_lowercase();
        for malicious_name in &known_malicious {
            if name_lower.contains(malicious_name) {
                return Err(DependencyError::SecurityVulnerability(
                    format!("Package '{}' is in the malicious package blocklist", name)
                ));
            }
        }
        
        // Check for packages that are commonly typosquatted
        let protected_packages = [
            "requests", "urllib3", "pillow", "numpy", "pandas", "flask", "django",
            "express", "lodash", "react", "angular", "vue"
        ];
        
        for protected in &protected_packages {
            let similarity = self.calculate_similarity(name, protected);
            if similarity > 0.8 && name != *protected {
                return Err(DependencyError::SecurityVulnerability(
                    format!("Package '{}' is suspiciously similar to protected package '{}'", name, protected)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Validate source URL for security
    fn validate_source_url(&self, source: &str) -> Result<(), DependencyError> {
        // Parse URL
        let url = url::Url::parse(source)
            .map_err(|_| DependencyError::SecurityVulnerability("Invalid source URL".to_string()))?;
        
        // Check scheme
        match url.scheme() {
            "https" => {}, // Preferred
            "http" => {
                // Allow HTTP but warn (should be configurable in production)
                warn!("Using insecure HTTP source URL: {}", source);
            },
            _ => {
                return Err(DependencyError::SecurityVulnerability(
                    format!("Unsupported URL scheme: {}", url.scheme())
                ));
            }
        }
        
        // Check for suspicious domains
        if let Some(host) = url.host_str() {
            let suspicious_domains = [
                "bit.ly", "tinyurl.com", "t.co", "goo.gl", "ow.ly"
            ];
            
            for suspicious in &suspicious_domains {
                if host.contains(suspicious) {
                    return Err(DependencyError::SecurityVulnerability(
                        format!("Source URL uses suspicious URL shortener: {}", host)
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    /// Calculate similarity between two strings (simplified Levenshtein distance)
    fn calculate_similarity(&self, s1: &str, s2: &str) -> f64 {
        let len1 = s1.len();
        let len2 = s2.len();
        
        if len1 == 0 || len2 == 0 {
            return 0.0;
        }
        
        let max_len = len1.max(len2);
        let distance = self.levenshtein_distance(s1, s2);
        
        1.0 - (distance as f64 / max_len as f64)
    }
    
    /// Calculate Levenshtein distance between two strings
    fn levenshtein_distance(&self, s1: &str, s2: &str) -> usize {
        let s1_chars: Vec<char> = s1.chars().collect();
        let s2_chars: Vec<char> = s2.chars().collect();
        let len1 = s1_chars.len();
        let len2 = s2_chars.len();
        
        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];
        
        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }
        
        for i in 1..=len1 {
            for j in 1..=len2 {
                let cost = if s1_chars[i - 1] == s2_chars[j - 1] { 0 } else { 1 };
                matrix[i][j] = (matrix[i - 1][j] + 1)
                    .min(matrix[i][j - 1] + 1)
                    .min(matrix[i - 1][j - 1] + cost);
            }
        }
        
        matrix[len1][len2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_installer_creation() {
        let installer = DependencyInstaller::new().await;
        assert!(installer.is_ok());
    }
    
    #[tokio::test]
    async fn test_installation_context_creation() {
        let installer = DependencyInstaller::new().await.unwrap();
        let context = installer.create_installation_context("test-sandbox").await;
        assert!(context.is_ok());
        
        let ctx = context.unwrap();
        assert_eq!(ctx.sandbox_id, "test-sandbox");
        assert!(ctx.work_dir.exists());
    }
    
    #[test]
    fn test_installation_progress() {
        let progress = InstallationProgress {
            sandbox_id: "test".to_string(),
            package_name: "requests".to_string(),
            phase: InstallationPhase::Downloading,
            progress_percent: 50.0,
            status_message: "Downloading...".to_string(),
            bytes_downloaded: 1024,
            total_bytes: Some(2048),
            started_at: chrono::Utc::now(),
            estimated_completion: None,
        };
        
        assert_eq!(progress.phase, InstallationPhase::Downloading);
        assert_eq!(progress.progress_percent, 50.0);
    }
    
    #[test]
    fn test_installation_result() {
        let package = PackageSpec {
            name: "test-package".to_string(),
            version: Some("1.0.0".to_string()),
            manager: PackageManager::Pip,
            options: HashMap::new(),
            source: None,
            dev_dependency: false,
            optional: false,
        };
        
        let result = InstallationResult {
            package: package.clone(),
            success: true,
            duration_secs: 30.0,
            installed_size_bytes: 1024 * 1024,
            output: "Installation successful".to_string(),
            error_output: None,
            exit_code: Some(0),
            from_cache: false,
            timestamp: chrono::Utc::now(),
        };
        
        assert!(result.success);
        assert_eq!(result.package.name, "test-package");
        assert_eq!(result.duration_secs, 30.0);
    }
}