//! Port mapping and allocation management for sandbox containers
//! 
//! This module handles:
//! - Dynamic port allocation for sandbox containers
//! - Port mapping conflict resolution
//! - Port pool management and reservation
//! - Port forwarding configuration for Docker containers

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
// use std::net::TcpListener; // Temporarily unused
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tracing::{debug, error, info};
use rand::seq::SliceRandom;

/// Port allocation and management errors
#[derive(Error, Debug)]
pub enum PortAllocationError {
    #[error("Port {0} is already allocated")]
    PortAlreadyAllocated(u16),
    
    #[error("No available ports in range {0}-{1}")]
    NoAvailablePorts(u16, u16),
    
    #[error("Invalid port range: {0}")]
    InvalidRange(String),
    
    #[error("Port {0} is outside allowed range")]
    PortOutOfRange(u16),
    
    #[error("Port {0} is reserved for system use")]
    PortReserved(u16),
    
    #[error("Failed to bind to port {0}: {1}")]
    BindFailure(u16, String),
    
    #[error("Sandbox {0} not found")]
    SandboxNotFound(String),
    
    #[error("Port mapping conflict: {0}")]
    MappingConflict(String),
}

/// Port allocation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortAllocation {
    /// Host port that was allocated
    pub host_port: u16,
    /// Container port to map to
    pub container_port: u16,
    /// Protocol (tcp/udp)
    pub protocol: String,
    /// Sandbox ID that owns this allocation
    pub sandbox_id: String,
    /// Timestamp when allocated
    pub allocated_at: chrono::DateTime<chrono::Utc>,
    /// Whether the port is currently active
    pub is_active: bool,
}

/// Port range configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRange {
    /// Starting port number
    pub start: u16,
    /// Ending port number
    pub end: u16,
    /// Description of this range
    pub description: String,
}

/// Port mapping manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMappingConfig {
    /// Available port ranges for allocation
    pub available_ranges: Vec<PortRange>,
    /// Reserved ports that should not be allocated
    pub reserved_ports: HashSet<u16>,
    /// Maximum ports per sandbox
    pub max_ports_per_sandbox: u16,
    /// Enable automatic port cleanup
    pub auto_cleanup: bool,
    /// Port allocation timeout in seconds
    pub allocation_timeout_secs: u64,
}

/// Port mapping manager
#[derive(Debug)]
pub struct PortMappingManager {
    /// Configuration
    config: PortMappingConfig,
    /// Currently allocated ports
    allocated_ports: Arc<Mutex<HashMap<u16, PortAllocation>>>,
    /// Ports allocated by sandbox
    sandbox_ports: Arc<Mutex<HashMap<String, Vec<PortAllocation>>>>,
    /// Port allocation statistics
    stats: Arc<Mutex<PortMappingStats>>,
}

/// Port mapping statistics
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PortMappingStats {
    /// Total ports allocated
    pub total_allocated: u64,
    /// Currently active allocations
    pub active_allocations: u32,
    /// Failed allocation attempts
    pub failed_allocations: u64,
    /// Port conflicts encountered
    pub conflicts_resolved: u64,
    /// Average allocation time in milliseconds
    pub avg_allocation_time_ms: f64,
}

impl PortMappingManager {
    /// Create a new port mapping manager with default configuration
    pub fn new() -> Self {
        Self::with_config(PortMappingConfig::default())
    }
    
    /// Create a new port mapping manager with custom configuration
    pub fn with_config(config: PortMappingConfig) -> Self {
        Self {
            config,
            allocated_ports: Arc::new(Mutex::new(HashMap::new())),
            sandbox_ports: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(PortMappingStats::default())),
        }
    }
    
    /// Allocate a specific port for a sandbox
    pub fn allocate_port(
        &self,
        sandbox_id: &str,
        container_port: u16,
        preferred_host_port: Option<u16>,
    ) -> Result<u16, PortAllocationError> {
        let start_time = std::time::Instant::now();
        
        // Check sandbox port limit
        self.check_sandbox_port_limit(sandbox_id)?;
        
        let host_port = if let Some(preferred) = preferred_host_port {
            // Try to allocate the preferred port
            self.allocate_specific_port(sandbox_id, container_port, preferred)?
        } else {
            // Find an available port in the allowed ranges
            self.allocate_any_available_port(sandbox_id, container_port)?
        };
        
        // Update statistics
        let allocation_time = start_time.elapsed().as_millis() as f64;
        self.update_allocation_stats(allocation_time);
        
        info!(
            "Allocated port {} -> {} for sandbox {} (protocol: tcp)",
            host_port, container_port, sandbox_id
        );
        
        Ok(host_port)
    }
    
    /// Allocate a specific port with atomic check-and-set operation
    fn allocate_specific_port(
        &self,
        sandbox_id: &str,
        container_port: u16,
        host_port: u16,
    ) -> Result<u16, PortAllocationError> {
        // Validate port is in allowed range
        self.validate_port_in_range(host_port)?;
        
        // Check if port is reserved
        if self.config.reserved_ports.contains(&host_port) {
            return Err(PortAllocationError::PortReserved(host_port));
        }
        
        // Atomic check-and-set operation to prevent race conditions
        {
            let mut allocated = self.allocated_ports.lock().unwrap();
            
            // Check if port is already allocated (inside the lock for atomicity)
            if allocated.contains_key(&host_port) {
                return Err(PortAllocationError::PortAlreadyAllocated(host_port));
            }
            
            // Test port availability while holding the lock
            if let Err(e) = self.test_port_availability(host_port) {
                return Err(e);
            }
            
            // Atomically record the allocation
            let allocation = PortAllocation {
                sandbox_id: sandbox_id.to_string(),
                container_port,
                host_port,
                protocol: "tcp".to_string(), // Default to TCP
                allocated_at: chrono::Utc::now(),
                is_active: true,
            };
            
            allocated.insert(host_port, allocation.clone());
            
            // Update sandbox mappings
            let mut sandbox_mappings = self.sandbox_ports.lock().unwrap();
            sandbox_mappings
                .entry(sandbox_id.to_string())
                .or_insert_with(Vec::new)
                .push(allocation);
            
            info!(
                "Atomically allocated port {} -> {} for sandbox {} (protocol: tcp)",
                host_port, container_port, sandbox_id
            );
        } // Lock is released here
        
        Ok(host_port)
    }
    
    /// Allocate any available port in the allowed ranges with atomic operations
    fn allocate_any_available_port(
        &self,
        sandbox_id: &str,
        container_port: u16,
    ) -> Result<u16, PortAllocationError> {
        // Use a randomized search order to reduce contention between concurrent allocations
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        
        // Collect all possible ports
        let mut candidate_ports = Vec::new();
        for range in &self.config.available_ranges {
            for port in range.start..=range.end {
                // Skip reserved ports
                if !self.config.reserved_ports.contains(&port) {
                    candidate_ports.push(port);
                }
            }
        }
        
        // Randomize the order to reduce thundering herd effects
        candidate_ports.shuffle(&mut rng);
        
        // Try to allocate ports atomically
        for port in candidate_ports {
            // Atomic check-and-set operation
            let allocation_result: Result<u16, PortAllocationError> = {
                let mut allocated = self.allocated_ports.lock().unwrap();
                
                // Check if port is already allocated (inside the lock for atomicity)
                if allocated.contains_key(&port) {
                    continue; // Port already taken, try next one
                }
                
                // Test port availability while holding the lock
                if let Err(_) = self.test_port_availability(port) {
                    continue; // Port not available, try next one
                }
                
                // Atomically record the allocation
                let allocation = PortAllocation {
                    sandbox_id: sandbox_id.to_string(),
                    container_port,
                    host_port: port,
                    protocol: "tcp".to_string(), // Default to TCP
                    allocated_at: chrono::Utc::now(),
                    is_active: true,
                };
                
                allocated.insert(port, allocation.clone());
                
                // Update sandbox mappings
                let mut sandbox_mappings = self.sandbox_ports.lock().unwrap();
                sandbox_mappings
                    .entry(sandbox_id.to_string())
                    .or_insert_with(Vec::new)
                    .push(allocation);
                
                Ok(port)
            }; // Lock is released here
            
            if let Ok(allocated_port) = allocation_result {
                info!(
                    "Atomically allocated random port {} -> {} for sandbox {} (protocol: tcp)",
                    allocated_port, container_port, sandbox_id
                );
                return Ok(allocated_port);
            }
        }
        
        // No available ports found
        let first_range = &self.config.available_ranges[0];
        Err(PortAllocationError::NoAvailablePorts(
            first_range.start,
            first_range.end,
        ))
    }
    
    /// Test if a port is available by attempting a lightweight connection test
    fn test_port_availability(&self, port: u16) -> Result<(), PortAllocationError> {
        use std::net::{TcpStream, SocketAddr};
        use std::time::Duration;
        
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        
        // Try to connect to the port with a very short timeout
        // If connection succeeds, port is likely in use
        // If connection fails, port is likely available
        match TcpStream::connect_timeout(&addr, Duration::from_millis(10)) {
            Ok(_) => {
                // Port is in use
                Err(PortAllocationError::BindFailure(
                    port,
                    "Port already in use".to_string(),
                ))
            },
            Err(e) => {
                // Check the specific error type
                match e.kind() {
                    std::io::ErrorKind::TimedOut |
                    std::io::ErrorKind::ConnectionRefused => {
                        // Port is available (nothing listening or connection refused)
                        Ok(())
                    },
                    _ => {
                        // Other errors might indicate the port is problematic
                        Err(PortAllocationError::BindFailure(
                            port,
                            format!("Port test failed: {}", e),
                        ))
                    }
                }
            }
        }
    }
    
    /// Record a port allocation
    fn record_port_allocation(
        &self,
        sandbox_id: &str,
        container_port: u16,
        host_port: u16,
    ) -> Result<(), PortAllocationError> {
        let allocation = PortAllocation {
            host_port,
            container_port,
            protocol: "tcp".to_string(), // Default to TCP, can be extended
            sandbox_id: sandbox_id.to_string(),
            allocated_at: chrono::Utc::now(),
            is_active: true,
        };
        
        // Record in allocated ports
        {
            let mut allocated = self.allocated_ports.lock().unwrap();
            allocated.insert(host_port, allocation.clone());
        }
        
        // Record in sandbox ports
        {
            let mut sandbox_ports = self.sandbox_ports.lock().unwrap();
            sandbox_ports
                .entry(sandbox_id.to_string())
                .or_insert_with(Vec::new)
                .push(allocation);
        }
        
        // Update stats
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_allocated += 1;
            stats.active_allocations += 1;
        }
        
        Ok(())
    }
    
    /// Release all ports for a sandbox
    pub fn release_ports(&self, sandbox_id: &str) {
        let ports_to_release = {
            let mut sandbox_ports = self.sandbox_ports.lock().unwrap();
            sandbox_ports.remove(sandbox_id).unwrap_or_default()
        };
        
        let mut allocated = self.allocated_ports.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();
        
        for allocation in ports_to_release {
            if allocated.remove(&allocation.host_port).is_some() {
                stats.active_allocations = stats.active_allocations.saturating_sub(1);
                debug!("Released port {} for sandbox {}", allocation.host_port, sandbox_id);
            }
        }
        
        info!("Released all ports for sandbox {}", sandbox_id);
    }
    
    /// Release a specific port
    pub fn release_port(&self, host_port: u16) -> Result<(), PortAllocationError> {
        let allocation = {
            let mut allocated = self.allocated_ports.lock().unwrap();
            allocated.remove(&host_port)
        };
        
        if let Some(allocation) = allocation {
            // Remove from sandbox ports
            {
                let mut sandbox_ports = self.sandbox_ports.lock().unwrap();
                if let Some(ports) = sandbox_ports.get_mut(&allocation.sandbox_id) {
                    ports.retain(|p| p.host_port != host_port);
                    if ports.is_empty() {
                        sandbox_ports.remove(&allocation.sandbox_id);
                    }
                }
            }
            
            // Update stats
            {
                let mut stats = self.stats.lock().unwrap();
                stats.active_allocations = stats.active_allocations.saturating_sub(1);
            }
            
            debug!("Released port {} for sandbox {}", host_port, allocation.sandbox_id);
            Ok(())
        } else {
            Err(PortAllocationError::PortAlreadyAllocated(host_port))
        }
    }
    
    /// Get all ports allocated to a sandbox
    pub fn get_sandbox_ports(&self, sandbox_id: &str) -> Vec<PortAllocation> {
        let sandbox_ports = self.sandbox_ports.lock().unwrap();
        
        sandbox_ports
            .get(sandbox_id)
            .cloned()
            .unwrap_or_default()
    }
    
    /// Get allocation information for a specific port
    pub fn get_port_allocation(&self, host_port: u16) -> Option<PortAllocation> {
        self.allocated_ports.lock().unwrap().get(&host_port).cloned()
    }
    
    /// Get current port mapping statistics
    pub fn get_stats(&self) -> PortMappingStats {
        self.stats.lock().unwrap().clone()
    }
    
    /// Validate that a port is within allowed ranges
    fn validate_port_in_range(&self, port: u16) -> Result<(), PortAllocationError> {
        for range in &self.config.available_ranges {
            if port >= range.start && port <= range.end {
                return Ok(());
            }
        }
        Err(PortAllocationError::PortOutOfRange(port))
    }
    
    /// Check if sandbox has exceeded port limit
    fn check_sandbox_port_limit(&self, sandbox_id: &str) -> Result<(), PortAllocationError> {
        let sandbox_ports = self.sandbox_ports.lock().unwrap();
        let current_count = sandbox_ports
            .get(sandbox_id)
            .map(|ports| ports.len())
            .unwrap_or(0) as u16;
            
        if current_count >= self.config.max_ports_per_sandbox {
            return Err(PortAllocationError::MappingConflict(
                format!(
                    "Sandbox {} has reached maximum port limit of {}",
                    sandbox_id, self.config.max_ports_per_sandbox
                ),
            ));
        }
        
        Ok(())
    }
    
    /// Update allocation statistics
    fn update_allocation_stats(&self, allocation_time_ms: f64) {
        let mut stats = self.stats.lock().unwrap();
        
        // Update average allocation time using exponential moving average
        if stats.avg_allocation_time_ms == 0.0 {
            stats.avg_allocation_time_ms = allocation_time_ms;
        } else {
            stats.avg_allocation_time_ms = 
                0.7 * stats.avg_allocation_time_ms + 0.3 * allocation_time_ms;
        }
    }
    
    /// List all currently allocated ports
    pub fn list_allocated_ports(&self) -> Vec<PortAllocation> {
        self.allocated_ports
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }
    
    /// Clean up inactive port allocations
    pub fn cleanup_inactive_ports(&self) -> usize {
        let mut count = 0;
        let cutoff_time = chrono::Utc::now() - chrono::Duration::hours(1); // 1 hour timeout
        
        let ports_to_remove: Vec<u16> = {
            let allocated = self.allocated_ports.lock().unwrap();
            allocated
                .iter()
                .filter_map(|(&port, allocation)| {
                    if !allocation.is_active && allocation.allocated_at < cutoff_time {
                        Some(port)
                    } else {
                        None
                    }
                })
                .collect()
        };
        
        for port in ports_to_remove {
            if self.release_port(port).is_ok() {
                count += 1;
            }
        }
        
        if count > 0 {
            info!("Cleaned up {} inactive port allocations", count);
        }
        
        count
    }
}

impl Default for PortMappingConfig {
    fn default() -> Self {
        Self::new_flexible()
    }
}

impl PortMappingConfig {
    /// Create a new flexible port mapping configuration
    pub fn new_flexible() -> Self {
        // Only reserve truly critical system ports
        let mut reserved_ports = HashSet::new();
        
        // Core system ports (0-1023) - but allow some common development ports
        for port in 0..1024 {
            reserved_ports.insert(port);
        }
        
        // Remove some commonly used development ports from reserved list
        reserved_ports.remove(&80);   // HTTP (often proxied)
        reserved_ports.remove(&443);  // HTTPS (often proxied)
        reserved_ports.remove(&8080); // Common dev port
        reserved_ports.remove(&8443); // Common dev HTTPS port
        
        // Keep critical services reserved
        reserved_ports.insert(22);    // SSH
        reserved_ports.insert(53);    // DNS
        reserved_ports.insert(25);    // SMTP
        reserved_ports.insert(110);   // POP3
        reserved_ports.insert(143);   // IMAP
        reserved_ports.insert(993);   // IMAPS
        reserved_ports.insert(995);   // POP3S
        
        Self {
            available_ranges: vec![
                PortRange {
                    start: 1024,
                    end: 4999,
                    description: "Low registered port range".to_string(),
                },
                PortRange {
                    start: 5000,
                    end: 9999,
                    description: "High registered port range".to_string(),
                },
                PortRange {
                    start: 10000,
                    end: 32767,
                    description: "Dynamic/Private port range".to_string(),
                },
                PortRange {
                    start: 49152,
                    end: 65535,
                    description: "Ephemeral port range".to_string(),
                },
            ],
            reserved_ports,
            max_ports_per_sandbox: 50, // More generous limit
            auto_cleanup: true,
            allocation_timeout_secs: 60, // Longer timeout
        }
    }
    
    /// Create a conservative port mapping configuration (like the old default)
    pub fn new_conservative() -> Self {
        let mut reserved_ports = HashSet::new();
        
        // System ports (0-1023)
        for port in 0..1024 {
            reserved_ports.insert(port);
        }
        
        // Common service ports
        reserved_ports.insert(3000); // Development servers
        reserved_ports.insert(5432); // PostgreSQL
        reserved_ports.insert(6379); // Redis
        reserved_ports.insert(27017); // MongoDB
        
        Self {
            available_ranges: vec![
                PortRange {
                    start: 8000,
                    end: 8999,
                    description: "Primary sandbox port range".to_string(),
                },
                PortRange {
                    start: 9000,
                    end: 9999,
                    description: "Secondary sandbox port range".to_string(),
                },
                PortRange {
                    start: 10000,
                    end: 19999,
                    description: "Extended sandbox port range".to_string(),
                },
            ],
            reserved_ports,
            max_ports_per_sandbox: 10,
            auto_cleanup: true,
            allocation_timeout_secs: 30,
        }
    }
    
    /// Create a permissive configuration for development environments
    pub fn new_permissive() -> Self {
        let mut reserved_ports = HashSet::new();
        
        // Only reserve absolutely critical ports
        reserved_ports.insert(22);  // SSH
        reserved_ports.insert(53);  // DNS
        reserved_ports.insert(25);  // SMTP
        reserved_ports.insert(80);  // HTTP (system)
        reserved_ports.insert(443); // HTTPS (system)
        
        Self {
            available_ranges: vec![
                PortRange {
                    start: 1024,
                    end: 65535,
                    description: "Full non-privileged port range".to_string(),
                },
            ],
            reserved_ports,
            max_ports_per_sandbox: 100, // Very generous for development
            auto_cleanup: true,
            allocation_timeout_secs: 120,
        }
    }
    
    /// Add a custom port range
    pub fn add_port_range(&mut self, start: u16, end: u16, description: &str) -> Result<(), PortAllocationError> {
        if start > end {
            return Err(PortAllocationError::InvalidRange(
                format!("Invalid port range: {}-{}", start, end)
            ));
        }
        
        // Check for overlaps with existing ranges
        for existing_range in &self.available_ranges {
            if (start >= existing_range.start && start <= existing_range.end) ||
               (end >= existing_range.start && end <= existing_range.end) ||
               (start < existing_range.start && end > existing_range.end) {
                return Err(PortAllocationError::InvalidRange(
                    format!("Port range {}-{} overlaps with existing range {}-{}", 
                           start, end, existing_range.start, existing_range.end)
                ));
            }
        }
        
        self.available_ranges.push(PortRange {
            start,
            end,
            description: description.to_string(),
        });
        
        // Sort ranges by start port for better allocation efficiency
        self.available_ranges.sort_by(|a, b| a.start.cmp(&b.start));
        
        Ok(())
    }
    
    /// Remove reserved ports to make them available
    pub fn unreserve_ports(&mut self, ports: &[u16]) {
        for &port in ports {
            self.reserved_ports.remove(&port);
        }
    }
    
    /// Add ports to the reserved list
    pub fn reserve_ports(&mut self, ports: &[u16]) {
        for &port in ports {
            self.reserved_ports.insert(port);
        }
    }
    
    /// Set maximum ports per sandbox
    pub fn set_max_ports_per_sandbox(&mut self, max_ports: u16) {
        self.max_ports_per_sandbox = max_ports;
    }
    
    /// Get total number of available ports across all ranges
    pub fn total_available_ports(&self) -> u32 {
        self.available_ranges.iter()
            .map(|range| (range.end - range.start + 1) as u32)
            .sum::<u32>()
            .saturating_sub(self.reserved_ports.len() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_port_mapping_manager_creation() {
        let manager = PortMappingManager::new();
        let stats = manager.get_stats();
        assert_eq!(stats.active_allocations, 0);
        assert_eq!(stats.total_allocated, 0);
    }
    
    #[test]
    fn test_port_allocation() {
        let manager = PortMappingManager::new();
        
        // Test specific port allocation
        let result = manager.allocate_port("test-sandbox", 3000, Some(8080));
        assert!(result.is_ok());
        
        let port = result.unwrap();
        assert_eq!(port, 8080);
        
        // Test duplicate allocation should fail
        let result2 = manager.allocate_port("test-sandbox-2", 3001, Some(8080));
        assert!(result2.is_err());
    }
    
    #[test]
    fn test_port_release() {
        let manager = PortMappingManager::new();
        
        // Allocate a port
        let port = manager.allocate_port("test-sandbox", 3000, Some(8080)).unwrap();
        assert_eq!(port, 8080);
        
        // Check it's allocated
        let allocation = manager.get_port_allocation(8080);
        assert!(allocation.is_some());
        
        // Release the port
        manager.release_ports("test-sandbox");
        
        // Check it's released
        let allocation = manager.get_port_allocation(8080);
        assert!(allocation.is_none());
    }
    
    #[test]
    fn test_sandbox_port_limit() {
        let mut config = PortMappingConfig::default();
        config.max_ports_per_sandbox = 2;
        
        let manager = PortMappingManager::with_config(config);
        
        // Allocate up to limit
        assert!(manager.allocate_port("test-sandbox", 3000, Some(8080)).is_ok());
        assert!(manager.allocate_port("test-sandbox", 3001, Some(8081)).is_ok());
        
        // Should fail on limit exceeded
        let result = manager.allocate_port("test-sandbox", 3002, Some(8082));
        assert!(result.is_err());
    }
    
    #[test]
    fn test_get_sandbox_ports() {
        let manager = PortMappingManager::new();
        
        // Allocate multiple ports for a sandbox
        manager.allocate_port("test-sandbox", 3000, Some(8080)).unwrap();
        manager.allocate_port("test-sandbox", 3001, Some(8081)).unwrap();
        
        let ports = manager.get_sandbox_ports("test-sandbox");
        assert_eq!(ports.len(), 2);
        
        let host_ports: Vec<u16> = ports.iter().map(|p| p.host_port).collect();
        assert!(host_ports.contains(&8080));
        assert!(host_ports.contains(&8081));
    }
}