//! E2B Adapter - Converts between E2B and SoulBox models

use crate::e2b::models::*;
use crate::sandbox::provider::{SandboxConfig, SandboxStatus};
use crate::sandbox_manager::SandboxSummary;
use crate::container::resource_limits::{ResourceLimits, CpuLimits, MemoryLimits, DiskLimits, NetworkLimits};
use crate::runtime::RuntimeType;
use crate::template::Template;
use std::time::Duration;
use crate::error::{Result, SoulBoxError};
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

/// Adapter for converting between E2B and SoulBox models
pub struct E2BAdapter;

impl E2BAdapter {
    /// Convert E2B create request to SoulBox config
    pub fn e2b_to_sandbox_config(
        request: &E2BCreateSandboxRequest
    ) -> Result<SandboxConfig> {
        use crate::sandbox::provider::ResourceLimits as ProviderResourceLimits;
        
        let config = SandboxConfig {
            name: request.metadata.as_ref().and_then(|m| m.get("name").and_then(|v| v.as_str().map(|s| s.to_string()))),
            image: "python:3.11-slim".to_string(), // Default image
            working_directory: request.cwd.clone().unwrap_or_else(|| "/workspace".to_string()),
            environment: request.env.clone().unwrap_or_default(),
            resource_limits: ProviderResourceLimits {
                memory: request.resources.as_ref().map(|r| format!("{}M", r.memory_mb)),
                cpu: request.resources.as_ref().map(|r| r.cpu.to_string()),
                disk: request.resources.as_ref().map(|r| format!("{}M", r.disk_mb)),
                network_bandwidth: None,
            },
            network_config: None,
            volume_mounts: Vec::new(),
            timeout: request.timeout.map(Duration::from_secs),
            auto_remove: true,
        };
        
        Ok(config)
    }
    
    /// Convert E2B resources to SoulBox resource limits
    pub fn e2b_to_resource_limits(resources: &E2BResources) -> ResourceLimits {
        ResourceLimits {
            cpu: CpuLimits {
                cores: resources.cpu,
                shares: None,
                cpu_percent: Some(resources.cpu * 100.0),
            },
            memory: MemoryLimits {
                limit_mb: resources.memory_mb,
                swap_limit_mb: None,
                swap_mb: None,
            },
            disk: DiskLimits {
                limit_mb: resources.disk_mb,
                iops_limit: None,
            },
            network: NetworkLimits {
                upload_bps: None,
                download_bps: None,
                max_connections: None,
            },
        }
    }
    
    /// Convert SoulBox sandbox summary to E2B sandbox
    pub fn sandbox_summary_to_e2b(sandbox: &SandboxSummary) -> E2BSandbox {
        E2BSandbox {
            id: sandbox.sandbox_id.clone(),
            template_id: None, // Not available in SandboxSummary
            status: E2BSandboxStatus::Running, // Assume running for summary
            created_at: chrono::Utc::now() - chrono::Duration::from_std(sandbox.uptime).unwrap_or_default(),
            metadata: HashMap::new(),
            resources: E2BResources {
                cpu: 1.0,
                memory_mb: 512,
                disk_mb: 1024,
                network_enabled: true,
            },
            network: E2BNetwork {
                ports: Vec::new(),
                dns: vec!["8.8.8.8".to_string(), "8.8.4.4".to_string()],
                proxy: None,
            },
        }
    }
    
    /// Convert SoulBox status to E2B status
    pub fn status_to_e2b(status: SandboxStatus) -> E2BSandboxStatus {
        match status {
            SandboxStatus::Creating => E2BSandboxStatus::Starting,
            SandboxStatus::Running => E2BSandboxStatus::Running,
            SandboxStatus::Stopped => E2BSandboxStatus::Stopped,
            SandboxStatus::Error | SandboxStatus::Removing => E2BSandboxStatus::Error,
        }
    }
    
    /// Convert SoulBox resource limits to E2B resources
    pub fn resource_limits_to_e2b(limits: &ResourceLimits) -> E2BResources {
        E2BResources {
            cpu: limits.cpu.cores,
            memory_mb: limits.memory.limit_mb,
            disk_mb: limits.disk.limit_mb,
            network_enabled: true,
        }
    }
    
    /// Convert SoulBox template to E2B template
    pub fn template_to_e2b(template: &Template) -> E2BTemplate {
        E2BTemplate {
            id: template.metadata.id.to_string(),
            name: template.metadata.name.clone(),
            description: template.metadata.description.clone(),
            runtime: template.metadata.runtime_type.to_string(),
            version: template.metadata.version.clone(),
            public: template.metadata.is_public,
            created_at: template.metadata.created_at,
            updated_at: template.metadata.updated_at,
        }
    }
    
    /// Convert E2B run process request to command and args
    pub fn parse_e2b_command(request: &E2BRunProcessRequest) -> (String, Vec<String>) {
        let args = request.args.clone().unwrap_or_default();
        (request.command.clone(), args)
    }
    
    /// Create E2B error response
    pub fn create_error_response(error: &SoulBoxError) -> E2BErrorResponse {
        let (code, message) = match error {
            SoulBoxError::NotFound { message, .. } => ("NOT_FOUND", message.clone()),
            SoulBoxError::ValidationError { reason, .. } => ("VALIDATION_ERROR", reason.clone()),
            SoulBoxError::ResourceExhausted { resource, .. } => ("RESOURCE_EXHAUSTED", format!("Resource {} exhausted", resource)),
            SoulBoxError::Timeout(_) => ("TIMEOUT", "Operation timed out".to_string()),
            _ => ("INTERNAL_ERROR", error.to_string()),
        };
        
        E2BErrorResponse {
            error: E2BErrorDetail::new(code, message),
        }
    }
    
    /// Convert E2B file operation to filesystem operations
    /// Note: Returns operation type as string since FileOperation enum is not exported
    pub fn convert_file_operation(op: &E2BFileOperation) -> Result<(String, HashMap<String, String>)> {
        let (op_type, params) = match op {
            E2BFileOperation::Read { path } => {
                ("read", vec![("path", path.clone())])
            }
            E2BFileOperation::Write { path, content } => {
                ("write", vec![("path", path.clone()), ("content", content.clone())])
            }
            E2BFileOperation::Delete { path } => {
                ("delete", vec![("path", path.clone())])
            }
            E2BFileOperation::List { path } => {
                ("list", vec![("path", path.clone())])
            }
            E2BFileOperation::Mkdir { path } => {
                ("mkdir", vec![("path", path.clone())])
            }
            E2BFileOperation::Move { from, to } => {
                ("move", vec![("from", from.clone()), ("to", to.clone())])
            }
            E2BFileOperation::Copy { from, to } => {
                ("copy", vec![("from", from.clone()), ("to", to.clone())])
            }
        };
        
        Ok((op_type.to_string(), params.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))
    }
    
    /// Convert process output to E2B format
    pub fn format_process_output(
        stdout: String,
        stderr: String,
        exit_code: Option<i32>,
        duration_ms: u64,
    ) -> E2BRunProcessResponse {
        E2BRunProcessResponse {
            id: Uuid::new_v4().to_string(),
            stdout,
            stderr,
            exit_code,
            duration_ms,
        }
    }
    
    /// Parse E2B template ID to runtime type
    pub fn parse_template_id(template_id: &str) -> Result<RuntimeType> {
        // E2B uses template IDs like "python", "nodejs", etc.
        RuntimeType::from_str(template_id)
            .or_else(|_| {
                // Try to extract runtime from longer template IDs
                let parts: Vec<&str> = template_id.split('-').collect();
                if let Some(first) = parts.first() {
                    RuntimeType::from_str(first)
                } else {
                    Err(SoulBoxError::ValidationError {
                        field: "template_id".to_string(),
                        reason: format!("Invalid template ID: {}", template_id),
                        provided_value: Some(template_id.to_string()),
                        security_context: None,
                    })
                }
            })
    }
}