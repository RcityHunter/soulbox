//! Dependency installation and management system for sandbox environments
//! 
//! This module provides comprehensive dependency management including:
//! - Multi-language package manager support (pip, npm, cargo, go mod, etc.)
//! - Dependency caching and optimization
//! - Version conflict resolution
//! - Security scanning and vulnerability checks
//! - Installation progress tracking

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, warn, error, info};

pub mod installer;
pub mod cache;

pub use installer::{DependencyInstaller, InstallationResult, InstallationProgress};
pub use cache::{DependencyCache, CacheEntry, CacheError};

/// Dependency management errors
#[derive(Error, Debug)]
pub enum DependencyError {
    #[error("Package manager not supported: {0}")]
    UnsupportedPackageManager(String),
    
    #[error("Installation failed: {0}")]
    InstallationFailed(String),
    
    #[error("Invalid package specification: {0}")]
    InvalidPackageSpec(String),
    
    #[error("Version conflict: {0}")]
    VersionConflict(String),
    
    #[error("Cache error: {0}")]
    Cache(#[from] CacheError),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Security vulnerability detected: {0}")]
    SecurityVulnerability(String),
    
    #[error("Dependency resolution failed: {0}")]
    ResolutionFailed(String),
    
    #[error("Timeout during installation: {0}")]
    Timeout(String),
}

/// Supported package managers
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PackageManager {
    /// Python pip
    Pip,
    /// Python poetry
    Poetry,
    /// Python conda
    Conda,
    /// Node.js npm
    Npm,
    /// Node.js yarn
    Yarn,
    /// Node.js pnpm
    Pnpm,
    /// Rust cargo
    Cargo,
    /// Go modules
    GoMod,
    /// Java maven
    Maven,
    /// Java gradle
    Gradle,
    /// Ruby gems
    Gem,
    /// PHP composer
    Composer,
    /// System packages (apt, yum, etc.)
    System,
}

/// Package specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageSpec {
    /// Package name
    pub name: String,
    /// Version requirement (e.g., ">=1.0.0", "~1.2.0", "latest")
    pub version: Option<String>,
    /// Package manager to use
    pub manager: PackageManager,
    /// Additional options for installation
    pub options: HashMap<String, String>,
    /// Optional source/registry URL
    pub source: Option<String>,
    /// Whether this is a development dependency
    pub dev_dependency: bool,
    /// Whether this package is optional
    pub optional: bool,
}

/// Dependency manifest file types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManifestType {
    /// Python requirements.txt
    PipRequirements,
    /// Python setup.py
    PipSetup,
    /// Python pyproject.toml (Poetry)
    PyProject,
    /// Python environment.yml (Conda)
    CondaEnvironment,
    /// Node.js package.json
    PackageJson,
    /// Node.js yarn.lock
    YarnLock,
    /// Node.js pnpm-lock.yaml
    PnpmLock,
    /// Rust Cargo.toml
    CargoToml,
    /// Go go.mod
    GoMod,
    /// Java pom.xml (Maven)
    MavenPom,
    /// Java build.gradle (Gradle)
    GradleBuild,
    /// Ruby Gemfile
    Gemfile,
    /// PHP composer.json
    ComposerJson,
}

/// Dependency manifest containing package specifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyManifest {
    /// Manifest type
    pub manifest_type: ManifestType,
    /// List of packages to install
    pub packages: Vec<PackageSpec>,
    /// Environment variables required
    pub environment: HashMap<String, String>,
    /// Pre-installation commands
    pub pre_install_commands: Vec<String>,
    /// Post-installation commands
    pub post_install_commands: Vec<String>,
    /// Target directory for installation
    pub target_directory: Option<PathBuf>,
}

/// Installation strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationStrategy {
    /// Use cached packages when available
    pub use_cache: bool,
    /// Parallel installation for compatible packages
    pub parallel_install: bool,
    /// Maximum number of parallel installations
    pub max_parallel: usize,
    /// Installation timeout in seconds
    pub timeout_secs: u64,
    /// Retry failed installations
    pub retry_failed: bool,
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Skip packages with security vulnerabilities
    pub skip_vulnerable: bool,
    /// Update packages to latest compatible versions
    pub update_packages: bool,
    /// Clean install (remove existing packages first)
    pub clean_install: bool,
}

/// Dependency resolution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionResult {
    /// Successfully resolved packages
    pub resolved_packages: Vec<ResolvedPackage>,
    /// Packages with conflicts
    pub conflicts: Vec<PackageConflict>,
    /// Packages that failed to resolve
    pub failed_packages: Vec<String>,
    /// Total installation size estimate
    pub estimated_size_bytes: u64,
    /// Estimated installation time
    pub estimated_time_secs: u64,
}

/// A resolved package with all dependencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPackage {
    /// Original package specification
    pub spec: PackageSpec,
    /// Resolved version
    pub resolved_version: String,
    /// Direct dependencies
    pub dependencies: Vec<PackageSpec>,
    /// Size in bytes
    pub size_bytes: u64,
    /// Whether package is cached
    pub cached: bool,
    /// Security vulnerability information
    pub vulnerabilities: Vec<SecurityVulnerability>,
}

/// Package version conflict information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageConflict {
    /// Package name
    pub package_name: String,
    /// Conflicting version requirements
    pub conflicting_versions: Vec<String>,
    /// Packages that depend on conflicting versions
    pub dependent_packages: Vec<String>,
    /// Suggested resolution
    pub suggested_resolution: Option<String>,
}

/// Security vulnerability information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityVulnerability {
    /// Vulnerability ID (CVE, etc.)
    pub id: String,
    /// Severity level
    pub severity: VulnerabilitySeverity,
    /// Description
    pub description: String,
    /// Affected version range
    pub affected_versions: String,
    /// Fixed version (if available)
    pub fixed_version: Option<String>,
    /// Reference URL
    pub reference_url: Option<String>,
}

/// Vulnerability severity levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum VulnerabilitySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Main dependency manager
pub struct DependencyManager {
    /// Package installer
    installer: DependencyInstaller,
    /// Package cache
    cache: DependencyCache,
    /// Supported package managers
    supported_managers: HashSet<PackageManager>,
    /// Installation strategy
    strategy: InstallationStrategy,
    /// Cache for package manager detection results
    detection_cache: Arc<tokio::sync::RwLock<Option<(HashSet<PackageManager>, std::time::Instant)>>>,
    /// Cache expiry duration for detection results
    detection_cache_duration: std::time::Duration,
}

impl DependencyManager {
    /// Create a new dependency manager
    pub async fn new() -> Result<Self, DependencyError> {
        let installer = DependencyInstaller::new().await?;
        let cache = DependencyCache::new().await?;
        
        let supported_managers = Self::detect_available_package_managers_cached(None).await;
        
        Ok(Self {
            installer,
            cache,
            supported_managers,
            strategy: InstallationStrategy::default(),
            detection_cache: Arc::new(tokio::sync::RwLock::new(None)),
            detection_cache_duration: std::time::Duration::from_secs(300), // 5 minutes
        })
    }
    
    /// Create dependency manager with custom configuration
    pub async fn with_strategy(strategy: InstallationStrategy) -> Result<Self, DependencyError> {
        let mut manager = Self::new().await?;
        manager.strategy = strategy;
        Ok(manager)
    }
    
    /// Detect available package managers with caching
    async fn detect_available_package_managers_cached(
        cache: Option<&Arc<tokio::sync::RwLock<Option<(HashSet<PackageManager>, std::time::Instant)>>>>
    ) -> HashSet<PackageManager> {
        // If we have a cache, check if it's still valid
        if let Some(cache_ref) = cache {
            let cache_read = cache_ref.read().await;
            if let Some((cached_managers, cached_time)) = &*cache_read {
                if cached_time.elapsed() < std::time::Duration::from_secs(300) {
                    debug!("Using cached package manager detection results");
                    return cached_managers.clone();
                }
            }
        }
        
        // Cache miss or expired, perform actual detection
        let detected_managers = Self::detect_available_package_managers().await;
        
        // Update cache if provided
        if let Some(cache_ref) = cache {
            let mut cache_write = cache_ref.write().await;
            *cache_write = Some((detected_managers.clone(), std::time::Instant::now()));
        }
        
        detected_managers
    }
    
    /// Refresh package manager detection cache
    pub async fn refresh_package_managers(&mut self) -> Result<(), DependencyError> {
        self.supported_managers = Self::detect_available_package_managers_cached(
            Some(&self.detection_cache)
        ).await;
        
        info!("Refreshed package manager detection, found {} managers", 
              self.supported_managers.len());
        Ok(())
    }
    
    /// Detect available package managers on the system with caching and concurrent checks
    async fn detect_available_package_managers() -> HashSet<PackageManager> {
        use futures::future::join_all;
        
        // Define all commands to check with their corresponding package managers
        let commands_to_check = vec![
            ("pip", PackageManager::Pip),
            ("poetry", PackageManager::Poetry),
            ("conda", PackageManager::Conda),
            ("npm", PackageManager::Npm),
            ("yarn", PackageManager::Yarn),
            ("pnpm", PackageManager::Pnpm),
            ("cargo", PackageManager::Cargo),
            ("go", PackageManager::GoMod),
            ("mvn", PackageManager::Maven),
            ("gradle", PackageManager::Gradle),
            ("gem", PackageManager::Gem),
            ("composer", PackageManager::Composer),
            ("apt-get", PackageManager::System),
            ("yum", PackageManager::System),
        ];
        
        // Create concurrent tasks for all command checks
        let check_tasks = commands_to_check.into_iter().map(|(command, manager)| {
            async move {
                let exists = Self::command_exists(command).await;
                (manager, exists)
            }
        });
        
        // Execute all checks concurrently
        let results = join_all(check_tasks).await;
        
        // Collect available package managers
        let mut available = HashSet::new();
        for (manager, exists) in results {
            if exists {
                available.insert(manager);
            }
        }
        
        // Special handling: if either apt-get or yum exists, we have System support
        // (This was already handled above but keeping for clarity)
        
        debug!("Detected {} available package managers", available.len());
        available
    }
    
    /// Check if a command exists in PATH with optimized implementation
    async fn command_exists(command: &str) -> bool {
        // Use a more efficient approach than 'which' command
        // This avoids spawning processes for each check
        if let Ok(path_env) = std::env::var("PATH") {
            let paths = path_env.split(':');
            
            for path_dir in paths {
                let command_path = std::path::PathBuf::from(path_dir).join(command);
                
                // Check multiple possible extensions on Windows
                let extensions = if cfg!(windows) {
                    vec!["", ".exe", ".bat", ".cmd"]
                } else {
                    vec![""]
                };
                
                for ext in extensions {
                    let full_path = if ext.is_empty() {
                        command_path.clone()
                    } else {
                        command_path.with_extension(&ext[1..])
                    };
                    
                    if full_path.is_file() {
                        return true;
                    }
                }
            }
        }
        
        false
    }
    
    /// Parse dependency manifest from file
    pub async fn parse_manifest(&self, file_path: &PathBuf) -> Result<DependencyManifest, DependencyError> {
        let content = tokio::fs::read_to_string(file_path).await?;
        let manifest_type = self.detect_manifest_type(file_path);
        
        match manifest_type {
            ManifestType::PipRequirements => self.parse_pip_requirements(&content).await,
            ManifestType::PackageJson => self.parse_package_json(&content).await,
            ManifestType::CargoToml => self.parse_cargo_toml(&content).await,
            ManifestType::GoMod => self.parse_go_mod(&content).await,
            _ => Err(DependencyError::UnsupportedPackageManager(
                format!("Manifest type {:?} not yet implemented", manifest_type)
            )),
        }
    }
    
    /// Detect manifest type from file path
    fn detect_manifest_type(&self, file_path: &PathBuf) -> ManifestType {
        if let Some(filename) = file_path.file_name().and_then(|n| n.to_str()) {
            match filename {
                "requirements.txt" => ManifestType::PipRequirements,
                "setup.py" => ManifestType::PipSetup,
                "pyproject.toml" => ManifestType::PyProject,
                "environment.yml" | "environment.yaml" => ManifestType::CondaEnvironment,
                "package.json" => ManifestType::PackageJson,
                "yarn.lock" => ManifestType::YarnLock,
                "pnpm-lock.yaml" => ManifestType::PnpmLock,
                "Cargo.toml" => ManifestType::CargoToml,
                "go.mod" => ManifestType::GoMod,
                "pom.xml" => ManifestType::MavenPom,
                "build.gradle" => ManifestType::GradleBuild,
                "Gemfile" => ManifestType::Gemfile,
                "composer.json" => ManifestType::ComposerJson,
                _ => ManifestType::PipRequirements, // Default fallback
            }
        } else {
            ManifestType::PipRequirements
        }
    }
    
    /// Parse pip requirements.txt file
    async fn parse_pip_requirements(&self, content: &str) -> Result<DependencyManifest, DependencyError> {
        let mut packages = Vec::new();
        
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            // Parse package specification (simplified)
            let parts: Vec<&str> = line.split("==").collect();
            if parts.len() >= 1 {
                let name = parts[0].trim().to_string();
                let version = if parts.len() > 1 {
                    Some(format!("=={}", parts[1].trim()))
                } else {
                    None
                };
                
                packages.push(PackageSpec {
                    name,
                    version,
                    manager: PackageManager::Pip,
                    options: HashMap::new(),
                    source: None,
                    dev_dependency: false,
                    optional: false,
                });
            }
        }
        
        Ok(DependencyManifest {
            manifest_type: ManifestType::PipRequirements,
            packages,
            environment: HashMap::new(),
            pre_install_commands: Vec::new(),
            post_install_commands: Vec::new(),
            target_directory: None,
        })
    }
    
    /// Parse package.json file (simplified)
    async fn parse_package_json(&self, content: &str) -> Result<DependencyManifest, DependencyError> {
        // This is a simplified implementation - in production you'd use a proper JSON parser
        let mut packages = Vec::new();
        
        // For now, just create a placeholder manifest
        Ok(DependencyManifest {
            manifest_type: ManifestType::PackageJson,
            packages,
            environment: HashMap::new(),
            pre_install_commands: Vec::new(),
            post_install_commands: Vec::new(),
            target_directory: None,
        })
    }
    
    /// Parse Cargo.toml file (simplified)
    async fn parse_cargo_toml(&self, content: &str) -> Result<DependencyManifest, DependencyError> {
        let mut packages = Vec::new();
        
        // Simplified parser - would use toml crate in production
        Ok(DependencyManifest {
            manifest_type: ManifestType::CargoToml,
            packages,
            environment: HashMap::new(),
            pre_install_commands: Vec::new(),
            post_install_commands: Vec::new(),
            target_directory: None,
        })
    }
    
    /// Parse go.mod file (simplified)
    async fn parse_go_mod(&self, content: &str) -> Result<DependencyManifest, DependencyError> {
        let mut packages = Vec::new();
        
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("require ") {
                // Parse Go module requirements
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[1].to_string();
                    let version = if parts.len() > 2 {
                        Some(parts[2].to_string())
                    } else {
                        None
                    };
                    
                    packages.push(PackageSpec {
                        name,
                        version,
                        manager: PackageManager::GoMod,
                        options: HashMap::new(),
                        source: None,
                        dev_dependency: false,
                        optional: false,
                    });
                }
            }
        }
        
        Ok(DependencyManifest {
            manifest_type: ManifestType::GoMod,
            packages,
            environment: HashMap::new(),
            pre_install_commands: Vec::new(),
            post_install_commands: Vec::new(),
            target_directory: None,
        })
    }
    
    /// Resolve dependencies from manifest
    pub async fn resolve_dependencies(
        &self,
        manifest: &DependencyManifest,
    ) -> Result<ResolutionResult, DependencyError> {
        info!("Resolving dependencies for {} packages", manifest.packages.len());
        
        let mut resolved_packages = Vec::new();
        let mut conflicts = Vec::new();
        let mut failed_packages = Vec::new();
        let mut total_size = 0u64;
        
        for package in &manifest.packages {
            // Check if package manager is supported
            if !self.supported_managers.contains(&package.manager) {
                warn!("Package manager {:?} not supported for package {}", package.manager, package.name);
                failed_packages.push(package.name.clone());
                continue;
            }
            
            match self.resolve_single_package(package).await {
                Ok(resolved) => {
                    total_size += resolved.size_bytes;
                    resolved_packages.push(resolved);
                },
                Err(e) => {
                    error!("Failed to resolve package {}: {}", package.name, e);
                    failed_packages.push(package.name.clone());
                },
            }
        }
        
        // Estimate installation time (simplified)
        let estimated_time_secs = (total_size / 1_000_000).max(60); // At least 1 minute
        
        Ok(ResolutionResult {
            resolved_packages,
            conflicts,
            failed_packages,
            estimated_size_bytes: total_size,
            estimated_time_secs,
        })
    }
    
    /// Resolve a single package
    async fn resolve_single_package(&self, package: &PackageSpec) -> Result<ResolvedPackage, DependencyError> {
        // Check cache first
        if self.strategy.use_cache {
            if let Ok(cached_entry) = self.cache.get_package(&package.name, package.version.as_deref()).await {
                debug!("Found cached package: {}", package.name);
                return Ok(ResolvedPackage {
                    spec: package.clone(),
                    resolved_version: cached_entry.version,
                    dependencies: Vec::new(), // TODO: Parse from cache
                    size_bytes: cached_entry.size_bytes,
                    cached: true,
                    vulnerabilities: Vec::new(), // TODO: Check vulnerabilities
                });
            }
        }
        
        // Resolve version and dependencies
        let resolved_version = self.resolve_version(package).await?;
        let dependencies = self.resolve_dependencies_for_package(package).await.unwrap_or_default();
        let size_bytes = self.estimate_package_size(package).await.unwrap_or(1024 * 1024); // 1MB default
        
        Ok(ResolvedPackage {
            spec: package.clone(),
            resolved_version,
            dependencies,
            size_bytes,
            cached: false,
            vulnerabilities: Vec::new(),
        })
    }
    
    /// Resolve package version
    async fn resolve_version(&self, package: &PackageSpec) -> Result<String, DependencyError> {
        match &package.version {
            Some(version) => Ok(version.clone()),
            None => Ok("latest".to_string()),
        }
    }
    
    /// Resolve dependencies for a package
    async fn resolve_dependencies_for_package(&self, _package: &PackageSpec) -> Result<Vec<PackageSpec>, DependencyError> {
        // Simplified implementation - would query package registries in production
        Ok(Vec::new())
    }
    
    /// Estimate package size
    async fn estimate_package_size(&self, _package: &PackageSpec) -> Result<u64, DependencyError> {
        // Simplified implementation - would query package registries
        Ok(1024 * 1024) // 1MB default
    }
    
    /// Install dependencies from resolution result
    pub async fn install_dependencies(
        &mut self,
        sandbox_id: &str,
        resolution: &ResolutionResult,
    ) -> Result<Vec<InstallationResult>, DependencyError> {
        info!("Installing {} packages for sandbox {}", resolution.resolved_packages.len(), sandbox_id);
        
        let mut results = Vec::new();
        
        for package in &resolution.resolved_packages {
            let result = self.installer.install_package(sandbox_id, &package.spec).await?;
            results.push(result);
        }
        
        Ok(results)
    }
    
    /// Get supported package managers
    pub fn get_supported_managers(&self) -> &HashSet<PackageManager> {
        &self.supported_managers
    }
    
    /// Update installation strategy
    pub fn set_strategy(&mut self, strategy: InstallationStrategy) {
        self.strategy = strategy;
    }
    
    /// Get current installation strategy
    pub fn get_strategy(&self) -> &InstallationStrategy {
        &self.strategy
    }
}

impl Default for InstallationStrategy {
    fn default() -> Self {
        Self {
            use_cache: true,
            parallel_install: true,
            max_parallel: 4,
            timeout_secs: 300, // 5 minutes
            retry_failed: true,
            max_retries: 3,
            skip_vulnerable: false,
            update_packages: false,
            clean_install: false,
        }
    }
}

impl PackageManager {
    /// Get the command name for this package manager
    pub fn command(&self) -> &'static str {
        match self {
            PackageManager::Pip => "pip",
            PackageManager::Poetry => "poetry",
            PackageManager::Conda => "conda",
            PackageManager::Npm => "npm",
            PackageManager::Yarn => "yarn",
            PackageManager::Pnpm => "pnpm",
            PackageManager::Cargo => "cargo",
            PackageManager::GoMod => "go",
            PackageManager::Maven => "mvn",
            PackageManager::Gradle => "gradle",
            PackageManager::Gem => "gem",
            PackageManager::Composer => "composer",
            PackageManager::System => "apt-get", // Default to apt
        }
    }
    
    /// Get typical manifest file names for this package manager
    pub fn manifest_files(&self) -> Vec<&'static str> {
        match self {
            PackageManager::Pip => vec!["requirements.txt", "setup.py", "pyproject.toml"],
            PackageManager::Poetry => vec!["pyproject.toml"],
            PackageManager::Conda => vec!["environment.yml", "environment.yaml"],
            PackageManager::Npm | PackageManager::Yarn | PackageManager::Pnpm => vec!["package.json"],
            PackageManager::Cargo => vec!["Cargo.toml"],
            PackageManager::GoMod => vec!["go.mod"],
            PackageManager::Maven => vec!["pom.xml"],
            PackageManager::Gradle => vec!["build.gradle", "build.gradle.kts"],
            PackageManager::Gem => vec!["Gemfile"],
            PackageManager::Composer => vec!["composer.json"],
            PackageManager::System => vec![], // No standard manifest
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_dependency_manager_creation() {
        let manager = DependencyManager::new().await;
        assert!(manager.is_ok());
    }
    
    #[test]
    fn test_package_manager_command() {
        assert_eq!(PackageManager::Pip.command(), "pip");
        assert_eq!(PackageManager::Npm.command(), "npm");
        assert_eq!(PackageManager::Cargo.command(), "cargo");
    }
    
    #[test]
    fn test_package_manager_manifest_files() {
        let pip_files = PackageManager::Pip.manifest_files();
        assert!(pip_files.contains(&"requirements.txt"));
        assert!(pip_files.contains(&"setup.py"));
        
        let npm_files = PackageManager::Npm.manifest_files();
        assert!(npm_files.contains(&"package.json"));
    }
    
    #[tokio::test]
    async fn test_parse_pip_requirements() {
        let manager = DependencyManager::new().await.unwrap();
        let content = r#"
            requests==2.25.1
            flask>=1.1.0
            # This is a comment
            numpy
        "#;
        
        let manifest = manager.parse_pip_requirements(content).await.unwrap();
        assert_eq!(manifest.packages.len(), 3);
        assert_eq!(manifest.packages[0].name, "requests");
        assert_eq!(manifest.packages[0].version, Some("==2.25.1".to_string()));
    }
    
    #[test]
    fn test_installation_strategy_default() {
        let strategy = InstallationStrategy::default();
        assert!(strategy.use_cache);
        assert!(strategy.parallel_install);
        assert_eq!(strategy.max_parallel, 4);
        assert_eq!(strategy.timeout_secs, 300);
    }
}