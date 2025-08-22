//! Tests for dependency management functionality

#[cfg(test)]
mod dependency_tests {
    use super::super::{
        DependencyManager, PackageSpec, VulnerabilitySeverity, SecurityVulnerability,
        DependencyError, ManifestType, InstallationStrategy
    };
    use super::super::cache::{DependencyCache, CachedPackage};
    use super::super::installer::DependencyInstaller;
    use std::collections::HashMap;
    use tempfile::TempDir;
    use serde_json::json;
    use chrono::Utc;

    async fn create_test_dependency_manager() -> (TempDir, DependencyManager) {
        let temp_dir = TempDir::new().unwrap();
        let cache = DependencyCache::new(temp_dir.path().to_path_buf()).await.unwrap();
        let installer = DependencyInstaller::new();
        let dependency_manager = DependencyManager::new(installer, cache);
        (temp_dir, dependency_manager)
    }

    fn create_test_package_spec(name: &str, version: Option<&str>) -> PackageSpec {
        PackageSpec {
            name: name.to_string(),
            version: version.map(|v| v.to_string()),
            source: None,
            extras: Vec::new(),
        }
    }

    fn create_cached_package_with_deps(name: &str, version: &str) -> CachedPackage {
        let mut metadata = HashMap::new();
        
        // Add dependencies metadata
        let dependencies = json!({
            "requests": ">=2.25.0",
            "urllib3": "^1.26.0",
            "certifi": "2021.10.8"
        });
        metadata.insert("dependencies".to_string(), dependencies);
        
        // Add dev dependencies
        let dev_dependencies = json!({
            "pytest": ">=6.0.0",
            "black": "^21.0.0"
        });
        metadata.insert("devDependencies".to_string(), dev_dependencies);

        CachedPackage {
            name: name.to_string(),
            version: version.to_string(),
            manager: super::super::PackageManager::Pip,
            size_bytes: 1024 * 1024, // 1MB
            metadata: Some(metadata),
            cached_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_parse_dependencies_from_cache() {
        let (_temp_dir, dependency_manager) = create_test_dependency_manager().await;
        
        let cached_package = create_cached_package_with_deps("test-package", "1.0.0");
        let dependencies = dependency_manager
            .parse_dependencies_from_cache(&cached_package)
            .await
            .unwrap();

        assert_eq!(dependencies.len(), 5); // 3 regular deps + 2 dev deps

        // Check regular dependencies
        let requests_dep = dependencies.iter().find(|d| d.name == "requests").unwrap();
        assert_eq!(requests_dep.version, Some(">=2.25.0".to_string()));
        assert!(requests_dep.extras.is_empty());

        // Check dev dependencies
        let pytest_dep = dependencies.iter().find(|d| d.name == "pytest").unwrap();
        assert_eq!(pytest_dep.version, Some(">=6.0.0".to_string()));
        assert_eq!(pytest_dep.extras, vec!["dev".to_string()]);
    }

    #[tokio::test]
    async fn test_parse_dependency_string() {
        let (_temp_dir, dependency_manager) = create_test_dependency_manager().await;

        // Test various dependency string formats
        let test_cases = vec![
            ("package>=1.0.0", ("package", Some("1.0.0"))),
            ("numpy~=1.21.0", ("numpy", Some("1.21.0"))),
            ("requests==2.25.1", ("requests", Some("2.25.1"))),
            ("flask^1.1.0", ("flask", Some("1.1.0"))),
            ("simple-package", ("simple-package", None)),
            ("package-with-dashes<=2.0", ("package-with-dashes", Some("2.0"))),
        ];

        for (input, expected) in test_cases {
            let (name, version) = dependency_manager.parse_dependency_string(input);
            assert_eq!(name, expected.0);
            assert_eq!(version, expected.1.map(|v| v.to_string()));
        }
    }

    #[tokio::test]
    async fn test_check_vulnerabilities_demo_package() {
        let (_temp_dir, dependency_manager) = create_test_dependency_manager().await;

        // Test package with "test" in name (should trigger demo vulnerability)
        let vulnerabilities = dependency_manager
            .check_vulnerabilities_for_package("test-package", "1.0.0")
            .await
            .unwrap();

        assert_eq!(vulnerabilities.len(), 1);
        let vuln = &vulnerabilities[0];
        assert_eq!(vuln.id, "DEMO-TEST-PACKAGE-001");
        assert_eq!(vuln.severity, VulnerabilitySeverity::Medium);
        assert!(vuln.description.contains("Demo vulnerability"));
        assert_eq!(vuln.affected_versions, "1.0.0");
        assert!(vuln.fixed_version.is_some());
        assert!(vuln.reference_url.is_some());
    }

    #[tokio::test]
    async fn test_check_vulnerabilities_security_package() {
        let (_temp_dir, dependency_manager) = create_test_dependency_manager().await;

        // Test security-related package with outdated version
        let vulnerabilities = dependency_manager
            .check_vulnerabilities_for_package("crypto-utils", "0.1.0")
            .await
            .unwrap();

        assert_eq!(vulnerabilities.len(), 1);
        let vuln = &vulnerabilities[0];
        assert_eq!(vuln.id, "SECURITY-AUDIT-CRYPTO-UTILS");
        assert_eq!(vuln.severity, VulnerabilitySeverity::High);
        assert!(vuln.description.contains("may be outdated"));
        assert_eq!(vuln.fixed_version, Some("latest".to_string()));
    }

    #[tokio::test]
    async fn test_check_vulnerabilities_safe_package() {
        let (_temp_dir, dependency_manager) = create_test_dependency_manager().await;

        // Test regular package (should have no vulnerabilities)
        let vulnerabilities = dependency_manager
            .check_vulnerabilities_for_package("safe-package", "2.1.0")
            .await
            .unwrap();

        assert_eq!(vulnerabilities.len(), 0);
    }

    #[tokio::test]
    async fn test_suggest_safe_version() {
        let (_temp_dir, dependency_manager) = create_test_dependency_manager().await;

        // Test version suggestions
        assert_eq!(dependency_manager.suggest_safe_version("1.0.0"), "1.0.1");
        assert_eq!(dependency_manager.suggest_safe_version("2.5.3"), "2.5.4");
        assert_eq!(dependency_manager.suggest_safe_version("1.0.9"), "1.0.9"); // Max digit reached
        assert_eq!(dependency_manager.suggest_safe_version("1.0"), "1.0.1");
        assert_eq!(dependency_manager.suggest_safe_version("invalid"), "invalid.1");
    }

    #[tokio::test]
    async fn test_is_outdated_version() {
        let (_temp_dir, dependency_manager) = create_test_dependency_manager().await;

        // Test outdated version detection
        assert!(dependency_manager.is_outdated_version("0.1.0"));
        assert!(dependency_manager.is_outdated_version("0.9.5"));
        assert!(dependency_manager.is_outdated_version("1.0"));
        assert!(dependency_manager.is_outdated_version("1.0.0"));
        assert!(!dependency_manager.is_outdated_version("2.1.0"));
        assert!(!dependency_manager.is_outdated_version("1.5.3"));
        assert!(dependency_manager.is_outdated_version("1")); // Only one component
    }

    #[tokio::test]
    async fn test_parse_dependencies_from_cache_array_format() {
        let (_temp_dir, dependency_manager) = create_test_dependency_manager().await;

        // Create cached package with array-format dependencies
        let mut metadata = HashMap::new();
        let dependencies = json!([
            "requests>=2.25.0",
            "numpy~=1.21.0",
            "flask==1.1.4",
            "simple-package"
        ]);
        metadata.insert("dependencies".to_string(), dependencies);

        let cached_package = CachedPackage {
            name: "test-package".to_string(),
            version: "1.0.0".to_string(),
            manager: super::super::PackageManager::Pip,
            size_bytes: 1024,
            metadata: Some(metadata),
            cached_at: Utc::now(),
        };

        let dependencies = dependency_manager
            .parse_dependencies_from_cache(&cached_package)
            .await
            .unwrap();

        assert_eq!(dependencies.len(), 4);
        
        let requests_dep = dependencies.iter().find(|d| d.name == "requests").unwrap();
        assert_eq!(requests_dep.version, Some("2.25.0".to_string()));
        
        let simple_dep = dependencies.iter().find(|d| d.name == "simple-package").unwrap();
        assert_eq!(simple_dep.version, None);
    }

    #[tokio::test]
    async fn test_parse_dependencies_from_cache_no_metadata() {
        let (_temp_dir, dependency_manager) = create_test_dependency_manager().await;

        let cached_package = CachedPackage {
            name: "test-package".to_string(),
            version: "1.0.0".to_string(),
            manager: super::super::PackageManager::Pip,
            size_bytes: 1024,
            metadata: None, // No metadata
            cached_at: Utc::now(),
        };

        let result = dependency_manager
            .parse_dependencies_from_cache(&cached_package)
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No metadata"));
    }

    #[tokio::test]
    async fn test_parse_dependencies_from_cache_invalid_format() {
        let (_temp_dir, dependency_manager) = create_test_dependency_manager().await;

        // Create cached package with invalid dependency format
        let mut metadata = HashMap::new();
        let dependencies = json!("invalid-string-format");
        metadata.insert("dependencies".to_string(), dependencies);

        let cached_package = CachedPackage {
            name: "test-package".to_string(),
            version: "1.0.0".to_string(),
            manager: super::super::PackageManager::Pip,
            size_bytes: 1024,
            metadata: Some(metadata),
            cached_at: Utc::now(),
        };

        let dependencies = dependency_manager
            .parse_dependencies_from_cache(&cached_package)
            .await
            .unwrap();

        // Should return empty dependencies for unsupported format
        assert_eq!(dependencies.len(), 0);
    }

    #[tokio::test]
    async fn test_vulnerability_severity_levels() {
        // Test vulnerability severity ordering
        assert!(VulnerabilitySeverity::Critical > VulnerabilitySeverity::High);
        assert!(VulnerabilitySeverity::High > VulnerabilitySeverity::Medium);
        assert!(VulnerabilitySeverity::Medium > VulnerabilitySeverity::Low);
    }

    #[tokio::test]
    async fn test_multiple_security_patterns() {
        let (_temp_dir, dependency_manager) = create_test_dependency_manager().await;

        // Test package with multiple security-related patterns
        let vulnerabilities = dependency_manager
            .check_vulnerabilities_for_package("crypto-auth-ssl", "0.5.0")
            .await
            .unwrap();

        // Should trigger security audit for outdated version
        assert_eq!(vulnerabilities.len(), 1);
        assert_eq!(vulnerabilities[0].severity, VulnerabilitySeverity::High);
    }

    #[tokio::test]
    async fn test_dependency_manager_strategy() {
        let (_temp_dir, mut dependency_manager) = create_test_dependency_manager().await;

        // Test default strategy
        let default_strategy = dependency_manager.get_strategy();
        assert!(default_strategy.use_cache);
        assert!(default_strategy.parallel_install);
        assert_eq!(default_strategy.max_parallel, 4);

        // Test setting custom strategy
        let custom_strategy = InstallationStrategy {
            use_cache: false,
            parallel_install: false,
            max_parallel: 2,
            timeout_secs: 600,
            retry_failed: true,
            max_retries: 5,
            skip_vulnerable: true,
            update_packages: true,
            clean_install: true,
        };

        dependency_manager.set_strategy(custom_strategy.clone());
        let updated_strategy = dependency_manager.get_strategy();
        assert!(!updated_strategy.use_cache);
        assert!(!updated_strategy.parallel_install);
        assert_eq!(updated_strategy.max_parallel, 2);
        assert_eq!(updated_strategy.timeout_secs, 600);
        assert!(updated_strategy.skip_vulnerable);
    }
}