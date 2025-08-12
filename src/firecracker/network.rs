use std::process::Command;
use tracing::{info, error};
use crate::error::Result;

/// Network manager for Firecracker VMs
pub struct NetworkManager {
    bridge_name: String,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            bridge_name: "fc-br0".to_string(),
        }
    }

    /// Setup network bridge for Firecracker VMs
    pub fn setup_bridge(&self) -> Result<()> {
        // Create bridge if it doesn't exist
        let output = Command::new("ip")
            .args(&["link", "show", &self.bridge_name])
            .output()?;

        if !output.status.success() {
            info!("Creating network bridge: {}", self.bridge_name);
            
            // Create bridge
            Command::new("ip")
                .args(&["link", "add", "name", &self.bridge_name, "type", "bridge"])
                .status()?;

            // Set bridge up
            Command::new("ip")
                .args(&["link", "set", "dev", &self.bridge_name, "up"])
                .status()?;

            // Add IP to bridge
            Command::new("ip")
                .args(&["addr", "add", "172.16.0.1/24", "dev", &self.bridge_name])
                .status()?;

            // Enable IP forwarding
            Command::new("sysctl")
                .args(&["-w", "net.ipv4.ip_forward=1"])
                .status()?;

            // Setup NAT
            Command::new("iptables")
                .args(&["-t", "nat", "-A", "POSTROUTING", "-o", "eth0", "-j", "MASQUERADE"])
                .status()?;

            Command::new("iptables")
                .args(&["-A", "FORWARD", "-m", "conntrack", "--ctstate", "RELATED,ESTABLISHED", "-j", "ACCEPT"])
                .status()?;

            Command::new("iptables")
                .args(&["-A", "FORWARD", "-i", &self.bridge_name, "-o", "eth0", "-j", "ACCEPT"])
                .status()?;
        }

        Ok(())
    }

    /// Create TAP interface for a VM
    pub fn create_tap(&self, tap_name: &str) -> Result<()> {
        info!("Creating TAP interface: {}", tap_name);

        // Create TAP interface
        Command::new("ip")
            .args(&["tuntap", "add", "dev", tap_name, "mode", "tap"])
            .status()?;

        // Add TAP to bridge
        Command::new("ip")
            .args(&["link", "set", "dev", tap_name, "master", &self.bridge_name])
            .status()?;

        // Set TAP up
        Command::new("ip")
            .args(&["link", "set", "dev", tap_name, "up"])
            .status()?;

        Ok(())
    }

    /// Delete TAP interface
    pub fn delete_tap(&self, tap_name: &str) -> Result<()> {
        info!("Deleting TAP interface: {}", tap_name);

        let _ = Command::new("ip")
            .args(&["link", "delete", tap_name])
            .status();

        Ok(())
    }

    /// Setup rate limiting on TAP interface
    pub fn setup_rate_limit(&self, tap_name: &str, rate_mbps: u32) -> Result<()> {
        // Use tc (traffic control) to limit bandwidth
        Command::new("tc")
            .args(&[
                "qdisc", "add", "dev", tap_name, "root", "tbf",
                "rate", &format!("{}mbit", rate_mbps),
                "burst", "10kb", "latency", "70ms"
            ])
            .status()?;

        Ok(())
    }
}

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new()
    }
}