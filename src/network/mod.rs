//! Network management module for SoulBox sandbox environments
//! 
//! This module provides comprehensive networking capabilities including:
//! - Port mapping and forwarding configuration
//! - Network isolation and security
//! - HTTP/HTTPS proxy support
//! - DNS settings management
//! - Network monitoring and diagnostics

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use thiserror::Error;

pub mod port_mapping;
pub mod proxy;

pub use crate::container::network::{NetworkConfig, PortMapping};
pub use port_mapping::{PortMappingManager, PortAllocationError};
pub use proxy::{ProxyConfig, ProxyManager, ProxyError};

/// Comprehensive network configuration for a sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxNetworkConfig {
    /// Basic network configuration from container module
    pub base_config: NetworkConfig,
    
    /// Network isolation level
    pub isolation_level: NetworkIsolationLevel,
    
    /// Proxy configuration
    pub proxy_config: Option<ProxyConfig>,
    
    /// Custom network name for the sandbox
    pub network_name: Option<String>,
    
    /// Bridge network configuration
    pub bridge_config: Option<BridgeConfig>,
    
    /// Bandwidth limitations
    pub bandwidth_limits: Option<BandwidthLimits>,
    
    /// Custom firewall rules
    pub firewall_rules: Vec<FirewallRule>,
}

/// Network isolation levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NetworkIsolationLevel {
    /// No network access (completely isolated)
    None,
    /// Limited network access (only whitelisted domains)
    Limited,
    /// Full network access with monitoring
    Full,
    /// Custom isolation rules
    Custom,
}

/// Bridge network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    /// Bridge name
    pub name: String,
    /// Subnet CIDR
    pub subnet: String,
    /// Gateway IP
    pub gateway: IpAddr,
    /// Enable IP masquerading
    pub enable_masquerade: bool,
}

/// Bandwidth limitation settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthLimits {
    /// Upload limit in KB/s
    pub upload_limit_kbps: Option<u64>,
    /// Download limit in KB/s  
    pub download_limit_kbps: Option<u64>,
    /// Total bandwidth limit in KB/s
    pub total_limit_kbps: Option<u64>,
}

/// Firewall rule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    /// Rule action (allow, deny)
    pub action: FirewallAction,
    /// Direction (ingress, egress)
    pub direction: TrafficDirection,
    /// Source IP/CIDR
    pub source: Option<String>,
    /// Destination IP/CIDR
    pub destination: Option<String>,
    /// Port range
    pub port_range: Option<PortRange>,
    /// Protocol (tcp, udp, icmp)
    pub protocol: Option<String>,
}

/// Firewall actions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FirewallAction {
    Allow,
    Deny,
    Log,
}

/// Traffic direction
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrafficDirection {
    Ingress,
    Egress,
    Both,
}

/// Port range specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRange {
    pub start: u16,
    pub end: u16,
}

/// Network statistics and monitoring data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    /// Bytes sent
    pub bytes_sent: u64,
    /// Bytes received
    pub bytes_received: u64,
    /// Packets sent
    pub packets_sent: u64,
    /// Packets received
    pub packets_received: u64,
    /// Active connections count
    pub active_connections: u32,
    /// Bandwidth usage (bytes/sec)
    pub bandwidth_usage: f64,
    /// Last update timestamp
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Network management errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Port allocation error: {0}")]
    PortAllocation(#[from] PortAllocationError),
    
    #[error("Proxy configuration error: {0}")]
    Proxy(#[from] ProxyError),
    
    #[error("Network isolation setup failed: {0}")]
    IsolationSetup(String),
    
    #[error("Firewall rule configuration failed: {0}")]
    FirewallConfig(String),
    
    #[error("Bridge network creation failed: {0}")]
    BridgeCreation(String),
    
    #[error("Bandwidth limit configuration failed: {0}")]
    BandwidthLimit(String),
    
    #[error("Docker network error: {0}")]
    Docker(String),
    
    #[error("Network monitoring error: {0}")]
    Monitoring(String),
}

/// Port service for managing port allocations
#[derive(Debug)]
pub struct PortService {
    port_manager: PortMappingManager,
}

impl PortService {
    pub fn new() -> Self {
        Self {
            port_manager: PortMappingManager::new(),
        }
    }
    
    pub fn allocate_port(&self, sandbox_id: &str, container_port: u16, host_port: Option<u16>) 
        -> Result<u16, PortAllocationError> {
        self.port_manager.allocate_port(sandbox_id, container_port, host_port)
    }
    
    pub fn release_ports(&self, sandbox_id: &str) {
        self.port_manager.release_ports(sandbox_id)
    }
    
    pub fn get_sandbox_ports(&self, sandbox_id: &str) -> Vec<port_mapping::PortAllocation> {
        self.port_manager.get_sandbox_ports(sandbox_id)
    }
}

/// Proxy service for managing HTTP/HTTPS proxies
#[derive(Debug)]
pub struct ProxyService {
    proxy_manager: ProxyManager,
}

impl ProxyService {
    pub fn new() -> Self {
        Self {
            proxy_manager: ProxyManager::new(),
        }
    }
    
    pub async fn configure_proxy(&mut self, sandbox_id: &str, config: ProxyConfig) 
        -> Result<(), ProxyError> {
        self.proxy_manager.configure_proxy(sandbox_id, config).await
    }
    
    pub async fn remove_proxy(&mut self, sandbox_id: &str) -> Result<(), ProxyError> {
        self.proxy_manager.remove_proxy(sandbox_id).await
    }
    
    pub fn check_request_allowed(&self, sandbox_id: &str, url: &str, method: &str, 
                                headers: &HashMap<String, String>) -> (bool, Option<String>) {
        self.proxy_manager.check_request_allowed(sandbox_id, url, method, headers)
    }
}

/// Statistics collector for network monitoring
#[derive(Debug)]
pub struct StatsCollector {
    network_stats: HashMap<String, NetworkStats>,
}

impl StatsCollector {
    pub fn new() -> Self {
        Self {
            network_stats: HashMap::new(),
        }
    }
    
    pub fn get_stats(&self, sandbox_id: &str) -> Option<&NetworkStats> {
        self.network_stats.get(sandbox_id)
    }
    
    pub fn update_stats(&mut self, sandbox_id: &str, stats: NetworkStats) {
        self.network_stats.insert(sandbox_id.to_string(), stats);
    }
    
    pub fn remove_stats(&mut self, sandbox_id: &str) {
        self.network_stats.remove(sandbox_id);
    }
}

/// Main network manager that coordinates separate services
#[derive(Debug)]
pub struct NetworkManager {
    port_service: PortService,
    proxy_service: ProxyService,
    stats_collector: StatsCollector,
    active_networks: HashMap<String, SandboxNetworkConfig>,
}

impl NetworkManager {
    /// Create a new network manager
    pub fn new() -> Self {
        Self {
            port_service: PortService::new(),
            proxy_service: ProxyService::new(),
            stats_collector: StatsCollector::new(),
            active_networks: HashMap::new(),
        }
    }
    
    /// Configure network for a sandbox
    pub async fn configure_sandbox_network(
        &mut self,
        sandbox_id: &str,
        config: SandboxNetworkConfig,
    ) -> Result<String, NetworkError> {
        // Allocate ports if needed
        if !config.base_config.port_mappings.is_empty() {
            for port_mapping in &config.base_config.port_mappings {
                self.port_service.allocate_port(
                    sandbox_id,
                    port_mapping.container_port,
                    port_mapping.host_port,
                )?;
            }
        }
        
        // Configure proxy if needed
        if let Some(proxy_config) = &config.proxy_config {
            self.proxy_service.configure_proxy(sandbox_id, proxy_config.clone()).await?;
        }
        
        // Create custom network if specified
        let network_name = if let Some(name) = &config.network_name {
            name.clone()
        } else {
            format!("soulbox-{}", sandbox_id)
        };
        
        // Store configuration
        self.active_networks.insert(sandbox_id.to_string(), config);
        
        Ok(network_name)
    }
    
    /// Remove network configuration for a sandbox
    pub async fn cleanup_sandbox_network(&mut self, sandbox_id: &str) -> Result<(), NetworkError> {
        // Release allocated ports
        self.port_service.release_ports(sandbox_id);
        
        // Remove proxy configuration
        self.proxy_service.remove_proxy(sandbox_id).await?;
        
        // Remove network configuration
        self.active_networks.remove(sandbox_id);
        self.stats_collector.remove_stats(sandbox_id);
        
        Ok(())
    }
    
    /// Get network statistics for a sandbox
    pub fn get_network_stats(&self, sandbox_id: &str) -> Option<&NetworkStats> {
        self.stats_collector.get_stats(sandbox_id)
    }
    
    /// Update network statistics for a sandbox
    pub fn update_network_stats(&mut self, sandbox_id: &str, stats: NetworkStats) {
        self.stats_collector.update_stats(sandbox_id, stats);
    }
    
    /// Get port service for direct access
    pub fn port_service(&self) -> &PortService {
        &self.port_service
    }
    
    /// Get proxy service for direct access
    pub fn proxy_service(&self) -> &ProxyService {
        &self.proxy_service
    }
    
    /// Get stats collector for direct access
    pub fn stats_collector(&self) -> &StatsCollector {
        &self.stats_collector
    }
    
    /// Get active network configuration
    pub fn get_network_config(&self, sandbox_id: &str) -> Option<&SandboxNetworkConfig> {
        self.active_networks.get(sandbox_id)
    }
    
    /// List all active networks
    pub fn list_active_networks(&self) -> Vec<&str> {
        self.active_networks.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for SandboxNetworkConfig {
    fn default() -> Self {
        Self {
            base_config: NetworkConfig::default(),
            isolation_level: NetworkIsolationLevel::Limited,
            proxy_config: None,
            network_name: None,
            bridge_config: None,
            bandwidth_limits: None,
            firewall_rules: Vec::new(),
        }
    }
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            name: "soulbox-bridge".to_string(),
            subnet: "172.20.0.0/16".to_string(),
            gateway: IpAddr::V4(Ipv4Addr::new(172, 20, 0, 1)),
            enable_masquerade: true,
        }
    }
}

impl PortRange {
    /// Create a new port range
    pub fn new(start: u16, end: u16) -> Result<Self, NetworkError> {
        if start > end {
            return Err(NetworkError::PortAllocation(
                PortAllocationError::InvalidRange(format!("Invalid port range: {}-{}", start, end))
            ));
        }
        Ok(Self { start, end })
    }
    
    /// Check if a port is within this range
    pub fn contains(&self, port: u16) -> bool {
        port >= self.start && port <= self.end
    }
    
    /// Get the number of ports in this range
    pub fn size(&self) -> u16 {
        self.end - self.start + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_port_range_creation() {
        let range = PortRange::new(8000, 8100).unwrap();
        assert_eq!(range.start, 8000);
        assert_eq!(range.end, 8100);
        assert_eq!(range.size(), 101);
        
        // Test invalid range
        assert!(PortRange::new(8100, 8000).is_err());
    }
    
    #[test]
    fn test_port_range_contains() {
        let range = PortRange::new(8000, 8100).unwrap();
        assert!(range.contains(8050));
        assert!(range.contains(8000));
        assert!(range.contains(8100));
        assert!(!range.contains(7999));
        assert!(!range.contains(8101));
    }
    
    #[test]
    fn test_default_sandbox_network_config() {
        let config = SandboxNetworkConfig::default();
        assert_eq!(config.isolation_level, NetworkIsolationLevel::Limited);
        assert!(config.proxy_config.is_none());
        assert!(config.network_name.is_none());
    }
    
    #[tokio::test]
    async fn test_network_manager_creation() {
        let manager = NetworkManager::new();
        assert_eq!(manager.list_active_networks().len(), 0);
    }
}