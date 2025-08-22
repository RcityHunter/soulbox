use bollard::Docker;
use bollard::API_DEFAULT_VERSION;
use crate::error::{Result, SoulBoxError};
use tracing::{info, warn, error, debug};

#[derive(Debug)]
pub struct ContainerRuntime {
    docker: Docker,
}

impl ContainerRuntime {
    pub async fn new() -> Result<Self> {
        info!("Initializing Docker client");
        
        // Try to connect to Docker daemon
        let docker = Docker::connect_with_socket_defaults()
            .map_err(|e| SoulBoxError::Internal(format!("Failed to connect to Docker daemon: {}", e)))?;
        
        let runtime = Self { docker };
        
        // Verify Docker connection by pinging
        if let Err(e) = runtime.is_docker_available().await {
            error!("Docker daemon is not available: {}", e);
            return Err(e);
        }
        
        info!("Docker client initialized successfully");
        Ok(runtime)
    }

    pub async fn is_docker_available(&self) -> Result<bool> {
        debug!("Checking Docker daemon availability");
        
        match self.docker.ping().await {
            Ok(_) => {
                debug!("Docker daemon is available");
                Ok(true)
            }
            Err(e) => {
                warn!("Docker daemon ping failed: {}", e);
                Err(SoulBoxError::Internal(format!("Docker daemon is not available: {}", e)))
            }
        }
    }

    pub async fn get_docker_version(&self) -> Result<String> {
        debug!("Getting Docker version information");
        
        match self.docker.version().await {
            Ok(version) => {
                let version_string = version.version.unwrap_or_else(|| "unknown".to_string());
                info!("Docker version: {}", version_string);
                Ok(version_string)
            }
            Err(e) => {
                error!("Failed to get Docker version: {}", e);
                Err(SoulBoxError::Internal(format!("Failed to get Docker version: {}", e)))
            }
        }
    }

    /// Get Docker system information
    pub async fn get_system_info(&self) -> Result<bollard::models::SystemInfo> {
        debug!("Getting Docker system information");
        
        match self.docker.info().await {
            Ok(info) => {
                debug!("Successfully retrieved Docker system info");
                Ok(info)
            }
            Err(e) => {
                error!("Failed to get Docker system info: {}", e);
                Err(SoulBoxError::Internal(format!("Failed to get Docker system info: {}", e)))
            }
        }
    }

    /// List all containers
    pub async fn list_containers(&self, all: bool) -> Result<Vec<bollard::models::ContainerSummary>> {
        debug!("Listing containers (all: {})", all);
        
        use bollard::container::ListContainersOptions;
        
        let options = if all {
            ListContainersOptions::<String> {
                all: true,
                ..Default::default()
            }
        } else {
            ListContainersOptions::<String>::default()
        };

        match self.docker.list_containers(Some(options)).await {
            Ok(containers) => {
                debug!("Found {} containers", containers.len());
                Ok(containers)
            }
            Err(e) => {
                error!("Failed to list containers: {}", e);
                Err(SoulBoxError::Internal(format!("Failed to list containers: {}", e)))
            }
        }
    }

    /// Get container by ID
    pub async fn get_container(&self, id: &str) -> Result<bollard::models::ContainerInspectResponse> {
        debug!("Getting container info for: {}", id);
        
        match self.docker.inspect_container(id, None).await {
            Ok(container) => {
                debug!("Successfully retrieved container info for: {}", id);
                Ok(container)
            }
            Err(e) => {
                warn!("Failed to get container {}: {}", id, e);
                Err(SoulBoxError::Internal(format!("Failed to get container {}: {}", id, e)))
            }
        }
    }

    /// Check if container exists
    pub async fn container_exists(&self, id: &str) -> bool {
        match self.get_container(id).await {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Get the Docker client reference for advanced operations
    pub fn docker(&self) -> &Docker {
        &self.docker
    }

    /// Create a new Docker client with custom configuration
    pub async fn new_with_uri(uri: &str) -> Result<Self> {
        info!("Initializing Docker client with custom URI: {}", uri);
        
        let docker = Docker::connect_with_socket(uri, 120, API_DEFAULT_VERSION)
            .map_err(|e| SoulBoxError::Internal(format!("Failed to connect to Docker at {}: {}", uri, e)))?;
        
        let runtime = Self { docker };
        
        // Verify connection
        if let Err(e) = runtime.is_docker_available().await {
            error!("Docker daemon at {} is not available: {}", uri, e);
            return Err(e);
        }
        
        info!("Docker client connected successfully to: {}", uri);
        Ok(runtime)
    }

    /// Get container logs
    pub async fn get_container_logs(
        &self,
        id: &str,
        follow: bool,
        tail: Option<&str>,
    ) -> crate::error::Result<impl futures::Stream<Item = std::result::Result<bollard::container::LogOutput, bollard::errors::Error>>> {
        debug!("Getting logs for container: {} (follow: {})", id, follow);
        
        use bollard::container::LogsOptions;
        
        let options = LogsOptions {
            follow,
            stdout: true,
            stderr: true,
            tail: tail.unwrap_or("100").to_string(),
            ..Default::default()
        };

        let stream = self.docker.logs(id, Some(options));
        debug!("Successfully created log stream for container: {}", id);
        Ok(stream)
        // Note: logs() returns a stream, not a Result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_container_runtime_creation() {
        // This test will only pass if Docker is actually running
        // In CI/CD environments, you might want to skip this test or use a mock
        match ContainerRuntime::new().await {
            Ok(runtime) => {
                // Test basic functionality
                assert!(runtime.is_docker_available().await.is_ok());
                
                // Test getting version
                let version = runtime.get_docker_version().await;
                assert!(version.is_ok());
                println!("Docker version: {}", version.unwrap());
                
                // Test listing containers
                let containers = runtime.list_containers(false).await;
                assert!(containers.is_ok());
                println!("Found {} running containers", containers.unwrap().len());
            }
            Err(e) => {
                // Docker might not be available in test environment
                println!("Docker not available during test: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_container_runtime_with_custom_uri() {
        // Test with default Unix socket
        match ContainerRuntime::new_with_uri("unix:///var/run/docker.sock").await {
            Ok(runtime) => {
                assert!(runtime.is_docker_available().await.is_ok());
            }
            Err(e) => {
                println!("Custom URI Docker not available during test: {}", e);
            }
        }
    }

    #[test]
    fn test_container_exists_with_invalid_id() {
        // This test just checks the method exists and can be called
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            if let Ok(runtime) = ContainerRuntime::new().await {
                let exists = runtime.container_exists("nonexistent-container-id").await;
                assert!(!exists);
            }
        });
    }
}