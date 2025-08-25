use std::collections::HashMap;
use std::path::Path;
use regex::Regex;
use crate::error::{Result, SoulBoxError};
use super::{RuntimeConfig, RuntimeType};

/// Runtime detection service for automatic language identification
pub struct RuntimeDetector {
    /// File extension to runtime mapping
    extension_mapping: HashMap<String, RuntimeType>,
    /// Shebang patterns for runtime detection
    shebang_patterns: Vec<ShebangPattern>,
    /// Content patterns for runtime detection
    content_patterns: Vec<ContentPattern>,
}

/// Shebang pattern for runtime detection
struct ShebangPattern {
    pattern: Regex,
    runtime_type: RuntimeType,
}

/// Content pattern for runtime detection
struct ContentPattern {
    pattern: Regex,
    runtime_type: RuntimeType,
    confidence: f32,
}

/// Detection result with confidence score
#[derive(Debug, Clone)]
pub struct DetectionResult {
    pub runtime_type: RuntimeType,
    pub confidence: f32,
    pub detection_method: DetectionMethod,
    pub details: String,
}

#[derive(Debug, Clone)]
pub enum DetectionMethod {
    FileExtension,
    Shebang,
    ContentAnalysis,
    ProjectStructure,
    PackageFile,
}

impl RuntimeDetector {
    /// Create a new runtime detector with default patterns
    pub fn new() -> Self {
        let mut extension_mapping = HashMap::new();
        
        // Python extensions
        extension_mapping.insert("py".to_string(), RuntimeType::Python);
        extension_mapping.insert("python".to_string(), RuntimeType::Python);
        extension_mapping.insert("pyw".to_string(), RuntimeType::Python);
        extension_mapping.insert("pyi".to_string(), RuntimeType::Python);
        
        // JavaScript/Node.js extensions
        extension_mapping.insert("js".to_string(), RuntimeType::NodeJS);
        extension_mapping.insert("mjs".to_string(), RuntimeType::NodeJS);
        extension_mapping.insert("jsx".to_string(), RuntimeType::NodeJS);
        extension_mapping.insert("ts".to_string(), RuntimeType::NodeJS);
        extension_mapping.insert("tsx".to_string(), RuntimeType::NodeJS);
        
        // Rust extensions
        extension_mapping.insert("rs".to_string(), RuntimeType::Rust);
        
        // Go extensions
        extension_mapping.insert("go".to_string(), RuntimeType::Go);
        
        // Java extensions
        extension_mapping.insert("java".to_string(), RuntimeType::Java);
        
        // Shell extensions
        extension_mapping.insert("sh".to_string(), RuntimeType::Shell);
        extension_mapping.insert("bash".to_string(), RuntimeType::Shell);
        extension_mapping.insert("zsh".to_string(), RuntimeType::Shell);
        
        let shebang_patterns = vec![
            ShebangPattern {
                pattern: Regex::new(r"^#!/.*python[0-9.]*").unwrap(),
                runtime_type: RuntimeType::Python,
            },
            ShebangPattern {
                pattern: Regex::new(r"^#!/.*node").unwrap(),
                runtime_type: RuntimeType::NodeJS,
            },
            ShebangPattern {
                pattern: Regex::new(r"^#!/.*(bash|sh|zsh)").unwrap(),
                runtime_type: RuntimeType::Shell,
            },
        ];

        let content_patterns = vec![
            // Python patterns
            ContentPattern {
                pattern: Regex::new(r"(?m)^(import|from)\s+\w+").unwrap(),
                runtime_type: RuntimeType::Python,
                confidence: 0.8,
            },
            ContentPattern {
                pattern: Regex::new(r"def\s+\w+\s*\(.*\):").unwrap(),
                runtime_type: RuntimeType::Python,
                confidence: 0.9,
            },
            ContentPattern {
                pattern: Regex::new(r#"if\s+__name__\s*==\s*['"]__main__['"]"#).unwrap(),
                runtime_type: RuntimeType::Python,
                confidence: 0.95,
            },
            
            // JavaScript/Node.js patterns
            ContentPattern {
                pattern: Regex::new(r"(?m)^(const|let|var)\s+\w+").unwrap(),
                runtime_type: RuntimeType::NodeJS,
                confidence: 0.7,
            },
            ContentPattern {
                pattern: Regex::new(r#"require\s*\(['"][^'"]*['"]\)"#).unwrap(),
                runtime_type: RuntimeType::NodeJS,
                confidence: 0.9,
            },
            ContentPattern {
                pattern: Regex::new(r"(?m)^(import|export)\s+").unwrap(),
                runtime_type: RuntimeType::NodeJS,
                confidence: 0.8,
            },
            ContentPattern {
                pattern: Regex::new(r"console\.(log|error|warn|info)").unwrap(),
                runtime_type: RuntimeType::NodeJS,
                confidence: 0.7,
            },
            
            // TypeScript patterns
            ContentPattern {
                pattern: Regex::new(r":\s*(string|number|boolean|any|void)\s*[=;]").unwrap(),
                runtime_type: RuntimeType::NodeJS,
                confidence: 0.85,
            },
            ContentPattern {
                pattern: Regex::new(r"interface\s+\w+\s*\{").unwrap(),
                runtime_type: RuntimeType::NodeJS,
                confidence: 0.9,
            },
            
            // Rust patterns
            ContentPattern {
                pattern: Regex::new(r"fn\s+\w+\s*\(.*\)\s*(\->\s*\w+)?\s*\{").unwrap(),
                runtime_type: RuntimeType::Rust,
                confidence: 0.9,
            },
            ContentPattern {
                pattern: Regex::new(r"use\s+\w+(::\w+)*;").unwrap(),
                runtime_type: RuntimeType::Rust,
                confidence: 0.8,
            },
            ContentPattern {
                pattern: Regex::new(r"let\s+(mut\s+)?\w+\s*=").unwrap(),
                runtime_type: RuntimeType::Rust,
                confidence: 0.7,
            },
            
            // Go patterns
            ContentPattern {
                pattern: Regex::new(r"package\s+\w+").unwrap(),
                runtime_type: RuntimeType::Go,
                confidence: 0.9,
            },
            ContentPattern {
                pattern: Regex::new(r"func\s+\w+\s*\(.*\)").unwrap(),
                runtime_type: RuntimeType::Go,
                confidence: 0.8,
            },
            ContentPattern {
                pattern: Regex::new(r#"import\s+["']\w+["']"#).unwrap(),
                runtime_type: RuntimeType::Go,
                confidence: 0.7,
            },
            
            // Java patterns
            ContentPattern {
                pattern: Regex::new(r"public\s+class\s+\w+").unwrap(),
                runtime_type: RuntimeType::Java,
                confidence: 0.9,
            },
            ContentPattern {
                pattern: Regex::new(r"public\s+static\s+void\s+main").unwrap(),
                runtime_type: RuntimeType::Java,
                confidence: 0.95,
            },
            
            // Shell patterns
            ContentPattern {
                pattern: Regex::new(r"^\s*echo\s+").unwrap(),
                runtime_type: RuntimeType::Shell,
                confidence: 0.6,
            },
            ContentPattern {
                pattern: Regex::new(r"^\s*(if|while|for)\s+\[").unwrap(),
                runtime_type: RuntimeType::Shell,
                confidence: 0.8,
            },
        ];

        Self {
            extension_mapping,
            shebang_patterns,
            content_patterns,
        }
    }

    /// Detect runtime from file path
    pub fn detect_from_path(&self, file_path: &str) -> Option<DetectionResult> {
        let path = Path::new(file_path);
        
        if let Some(extension) = path.extension() {
            if let Some(ext_str) = extension.to_str() {
                if let Some(runtime_type) = self.extension_mapping.get(ext_str) {
                    return Some(DetectionResult {
                        runtime_type: runtime_type.clone(),
                        confidence: 1.0,
                        detection_method: DetectionMethod::FileExtension,
                        details: format!("Detected from file extension: .{}", ext_str),
                    });
                }
            }
        }
        
        None
    }

    /// Detect runtime from file content
    pub fn detect_from_content(&self, content: &str) -> Option<DetectionResult> {
        // First, check shebang
        if let Some(result) = self.detect_from_shebang(content) {
            return Some(result);
        }

        // Then, analyze content patterns
        self.detect_from_patterns(content)
    }

    /// Detect runtime from shebang line
    fn detect_from_shebang(&self, content: &str) -> Option<DetectionResult> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return None;
        }

        let first_line = lines[0];
        if !first_line.starts_with("#!") {
            return None;
        }

        for pattern in &self.shebang_patterns {
            if pattern.pattern.is_match(first_line) {
                return Some(DetectionResult {
                    runtime_type: pattern.runtime_type.clone(),
                    confidence: 0.95,
                    detection_method: DetectionMethod::Shebang,
                    details: format!("Detected from shebang: {}", first_line),
                });
            }
        }

        None
    }

    /// Detect runtime from content patterns
    fn detect_from_patterns(&self, content: &str) -> Option<DetectionResult> {
        let mut scores: HashMap<RuntimeType, f32> = HashMap::new();
        let mut best_match: Option<(RuntimeType, f32)> = None;

        for pattern in &self.content_patterns {
            if pattern.pattern.is_match(content) {
                let current_score = scores.get(&pattern.runtime_type).unwrap_or(&0.0);
                scores.insert(pattern.runtime_type.clone(), current_score + pattern.confidence);
                
                if best_match.is_none() || pattern.confidence > best_match.as_ref().unwrap().1 {
                    best_match = Some((pattern.runtime_type.clone(), pattern.confidence));
                }
            }
        }

        if let Some((runtime_type, _)) = best_match {
            let total_score = scores.get(&runtime_type).unwrap_or(&0.0);
            
            // Implement weighted confidence algorithm
            let weighted_confidence = self.calculate_weighted_confidence(
                &runtime_type,
                total_score,
                &scores,
                content
            );

            if weighted_confidence > 0.4 { // Lower threshold since we're using better calculation
                return Some(DetectionResult {
                    runtime_type,
                    confidence: weighted_confidence,
                    detection_method: DetectionMethod::ContentAnalysis,
                    details: format!("Detected from content analysis (weighted score: {:.3})", weighted_confidence),
                });
            }
        }

        None
    }

    /// Calculate weighted confidence based on multiple factors
    fn calculate_weighted_confidence(
        &self,
        target_runtime: &RuntimeType,
        target_score: &f32,
        all_scores: &HashMap<RuntimeType, f32>,
        content: &str,
    ) -> f32 {
        // Base score from pattern matches
        let mut weighted_score = *target_score;
        
        // Factor 1: Content length bonus (longer content generally more reliable)
        let content_length = content.len() as f32;
        let length_bonus = if content_length > 1000.0 {
            0.1
        } else if content_length > 500.0 {
            0.05
        } else if content_length > 100.0 {
            0.02
        } else {
            0.0
        };
        weighted_score += length_bonus;
        
        // Factor 2: Uniqueness bonus (how much better than other candidates)
        let competing_scores: Vec<f32> = all_scores
            .iter()
            .filter(|(runtime, _)| *runtime != target_runtime)
            .map(|(_, score)| *score)
            .collect();
        
        if !competing_scores.is_empty() {
            let max_competing = competing_scores.iter().fold(0.0f32, |acc, &x| acc.max(x));
            let uniqueness_ratio = if max_competing > 0.0 {
                target_score / max_competing
            } else {
                2.0 // No competition
            };
            
            // Apply uniqueness bonus
            let uniqueness_bonus = match uniqueness_ratio {
                x if x >= 2.0 => 0.15,    // Very unique
                x if x >= 1.5 => 0.1,     // Moderately unique
                x if x >= 1.2 => 0.05,    // Slightly unique
                _ => 0.0,                  // Not unique
            };
            weighted_score += uniqueness_bonus;
        }
        
        // Factor 3: Pattern diversity bonus (multiple different patterns matched)
        let pattern_count = self.count_matching_patterns(target_runtime, content);
        let diversity_bonus = match pattern_count {
            n if n >= 4 => 0.1,
            n if n >= 3 => 0.05,
            n if n >= 2 => 0.02,
            _ => 0.0,
        };
        weighted_score += diversity_bonus;
        
        // Factor 4: High-confidence pattern bonus
        let has_high_confidence_pattern = self.has_high_confidence_pattern(target_runtime, content);
        if has_high_confidence_pattern {
            weighted_score += 0.1;
        }
        
        // Normalize to 0.0-1.0 range
        weighted_score.min(1.0).max(0.0)
    }
    
    /// Count how many different patterns matched for a runtime
    fn count_matching_patterns(&self, runtime: &RuntimeType, content: &str) -> usize {
        self.content_patterns
            .iter()
            .filter(|pattern| &pattern.runtime_type == runtime && pattern.pattern.is_match(content))
            .count()
    }
    
    /// Check if any high-confidence patterns (confidence >= 0.9) matched
    fn has_high_confidence_pattern(&self, runtime: &RuntimeType, content: &str) -> bool {
        self.content_patterns
            .iter()
            .any(|pattern| {
                &pattern.runtime_type == runtime 
                    && pattern.confidence >= 0.9 
                    && pattern.pattern.is_match(content)
            })
    }

    /// Detect runtime from project structure
    pub fn detect_from_project(&self, files: &[String]) -> Option<DetectionResult> {
        // Check for common project files
        for file in files {
            match file.as_str() {
                "package.json" | "yarn.lock" | "pnpm-lock.yaml" => {
                    return Some(DetectionResult {
                        runtime_type: RuntimeType::NodeJS,
                        confidence: 0.9,
                        detection_method: DetectionMethod::PackageFile,
                        details: format!("Found Node.js package file: {}", file),
                    });
                }
                "requirements.txt" | "setup.py" | "pyproject.toml" | "Pipfile" => {
                    return Some(DetectionResult {
                        runtime_type: RuntimeType::Python,
                        confidence: 0.9,
                        detection_method: DetectionMethod::PackageFile,
                        details: format!("Found Python package file: {}", file),
                    });
                }
                "Cargo.toml" | "Cargo.lock" => {
                    return Some(DetectionResult {
                        runtime_type: RuntimeType::Rust,
                        confidence: 0.95,
                        detection_method: DetectionMethod::PackageFile,
                        details: format!("Found Rust package file: {}", file),
                    });
                }
                "go.mod" | "go.sum" => {
                    return Some(DetectionResult {
                        runtime_type: RuntimeType::Go,
                        confidence: 0.95,
                        detection_method: DetectionMethod::PackageFile,
                        details: format!("Found Go package file: {}", file),
                    });
                }
                "pom.xml" | "build.gradle" => {
                    return Some(DetectionResult {
                        runtime_type: RuntimeType::Java,
                        confidence: 0.9,
                        detection_method: DetectionMethod::PackageFile,
                        details: format!("Found Java build file: {}", file),
                    });
                }
                _ => {}
            }
        }

        // Check for common directory structures
        let has_src = files.iter().any(|f| f.starts_with("src/"));
        let has_main_py = files.iter().any(|f| f == "main.py" || f == "__main__.py");
        let has_index_js = files.iter().any(|f| f == "index.js" || f == "app.js");

        if has_main_py && has_src {
            return Some(DetectionResult {
                runtime_type: RuntimeType::Python,
                confidence: 0.8,
                detection_method: DetectionMethod::ProjectStructure,
                details: "Found Python project structure with src/ and main.py".to_string(),
            });
        }

        if has_index_js {
            return Some(DetectionResult {
                runtime_type: RuntimeType::NodeJS,
                confidence: 0.8,
                detection_method: DetectionMethod::ProjectStructure,
                details: "Found Node.js project structure with index.js/app.js".to_string(),
            });
        }

        None
    }

    /// Comprehensive runtime detection
    pub fn detect_runtime(&self, file_path: Option<&str>, content: Option<&str>, project_files: Option<&[String]>) -> Result<DetectionResult> {
        let mut candidates = Vec::new();

        // Try file extension detection
        if let Some(path) = file_path {
            if let Some(result) = self.detect_from_path(path) {
                candidates.push(result);
            }
        }

        // Try content detection
        if let Some(code) = content {
            if let Some(result) = self.detect_from_content(code) {
                candidates.push(result);
            }
        }

        // Try project structure detection
        if let Some(files) = project_files {
            if let Some(result) = self.detect_from_project(files) {
                candidates.push(result);
            }
        }

        if candidates.is_empty() {
            return Err(SoulBoxError::RuntimeError("Could not detect runtime".to_string()));
        }

        // Select the best candidate based on confidence and detection method priority
        candidates.sort_by(|a, b| {
            let method_priority = |method: &DetectionMethod| -> u8 {
                match method {
                    DetectionMethod::PackageFile => 5,
                    DetectionMethod::Shebang => 4,
                    DetectionMethod::FileExtension => 3,
                    DetectionMethod::ProjectStructure => 2,
                    DetectionMethod::ContentAnalysis => 1,
                }
            };

            // First compare by method priority, then by confidence
            let priority_cmp = method_priority(&b.detection_method).cmp(&method_priority(&a.detection_method));
            if priority_cmp == std::cmp::Ordering::Equal {
                b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                priority_cmp
            }
        });

        Ok(candidates.into_iter().next().unwrap())
    }
}

impl Default for RuntimeDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function for runtime detection from content
pub fn detect_runtime_from_content<'a>(content: &str, runtimes: &'a HashMap<String, RuntimeConfig>) -> Option<&'a RuntimeConfig> {
    let detector = RuntimeDetector::new();
    
    if let Some(result) = detector.detect_from_content(content) {
        // Find matching runtime configuration
        for (_name, config) in runtimes {
            if config.runtime_type == result.runtime_type {
                return Some(config);
            }
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_detection() {
        let detector = RuntimeDetector::new();
        
        let result = detector.detect_from_path("script.py").unwrap();
        assert_eq!(result.runtime_type, RuntimeType::Python);
        assert_eq!(result.confidence, 1.0);
        
        let result = detector.detect_from_path("app.js").unwrap();
        assert_eq!(result.runtime_type, RuntimeType::NodeJS);
        
        let result = detector.detect_from_path("main.rs").unwrap();
        assert_eq!(result.runtime_type, RuntimeType::Rust);
    }

    #[test]
    fn test_shebang_detection() {
        let detector = RuntimeDetector::new();
        
        let python_script = "#!/usr/bin/env python3\nprint('hello')";
        let result = detector.detect_from_content(python_script).unwrap();
        assert_eq!(result.runtime_type, RuntimeType::Python);
        assert!(matches!(result.detection_method, DetectionMethod::Shebang));
        
        let node_script = r#"#!/usr/bin/node
console.log('hello')"#;
        let result = detector.detect_from_content(node_script).unwrap();
        assert_eq!(result.runtime_type, RuntimeType::NodeJS);
    }

    #[test]
    fn test_content_patterns() {
        let detector = RuntimeDetector::new();
        
        // Python patterns
        let python_code = r#"import os
def main():
    print('hello')
if __name__ == '__main__':
    main()"#;
        let result = detector.detect_from_content(python_code).unwrap();
        assert_eq!(result.runtime_type, RuntimeType::Python);
        
        // JavaScript patterns
        let js_code = r#"const express = require('express');
console.log('starting server');"#;
        let result = detector.detect_from_content(js_code).unwrap();
        assert_eq!(result.runtime_type, RuntimeType::NodeJS);
        
        // TypeScript patterns
        let ts_code = "interface User { name: string; age: number; }\nfunction greet(user: User): void {}";
        let result = detector.detect_from_content(ts_code).unwrap();
        assert_eq!(result.runtime_type, RuntimeType::NodeJS);
        
        // Rust patterns
        let rust_code = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let result = detector.detect_from_content(rust_code).unwrap();
        assert_eq!(result.runtime_type, RuntimeType::Rust);
    }

    #[test]
    fn test_project_detection() {
        let detector = RuntimeDetector::new();
        
        let node_files = vec!["package.json".to_string(), "index.js".to_string()];
        let result = detector.detect_from_project(&node_files).unwrap();
        assert_eq!(result.runtime_type, RuntimeType::NodeJS);
        
        let python_files = vec!["requirements.txt".to_string(), "main.py".to_string()];
        let result = detector.detect_from_project(&python_files).unwrap();
        assert_eq!(result.runtime_type, RuntimeType::Python);
        
        let rust_files = vec!["Cargo.toml".to_string(), "src/main.rs".to_string()];
        let result = detector.detect_from_project(&rust_files).unwrap();
        assert_eq!(result.runtime_type, RuntimeType::Rust);
    }

    #[test]
    fn test_comprehensive_detection() {
        let detector = RuntimeDetector::new();
        
        let python_code = "import sys\nprint('hello world')";
        let result = detector.detect_runtime(
            Some("script.py"),
            Some(python_code),
            Some(&vec!["requirements.txt".to_string()])
        ).unwrap();
        
        assert_eq!(result.runtime_type, RuntimeType::Python);
        // Package file detection should have highest priority
        assert!(matches!(result.detection_method, DetectionMethod::PackageFile));
    }

    #[test]
    fn test_unknown_runtime() {
        let detector = RuntimeDetector::new();
        
        let result = detector.detect_runtime(
            Some("unknown.xyz"),
            Some("some random content"),
            None
        );
        
        assert!(result.is_err());
    }
}