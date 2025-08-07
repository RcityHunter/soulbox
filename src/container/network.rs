use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub enable_internet: bool,
    pub port_mappings: Vec<PortMapping>,
    pub allowed_domains: Vec<String>,
    pub dns_servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_port: Option<u16>,
    pub container_port: u16,
    pub protocol: String, // "tcp" or "udp"
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            enable_internet: true,
            port_mappings: Vec::new(),
            allowed_domains: Vec::new(),
            dns_servers: vec![
                "8.8.8.8".to_string(),
                "1.1.1.1".to_string(),
            ],
        }
    }
}