use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Firecracker VM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMConfig {
    pub vm_id: String,
    pub machine_config: MachineConfig,
    pub kernel_config: KernelConfig,
    pub rootfs_config: RootfsConfig,
    pub network_config: NetworkConfig,
    pub vsock_config: Option<VsockConfig>,
}

/// Machine hardware configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineConfig {
    pub vcpu_count: u32,
    pub mem_size_mib: u32,
    pub smt: bool,
    pub track_dirty_pages: bool,
}

/// Kernel boot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelConfig {
    pub kernel_image_path: PathBuf,
    pub boot_args: String,
    pub initrd_path: Option<PathBuf>,
}

/// Root filesystem configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootfsConfig {
    pub drive_id: String,
    pub path_on_host: PathBuf,
    pub is_root_device: bool,
    pub is_read_only: bool,
}

/// Network interface configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub iface_id: String,
    pub host_dev_name: String,
    pub guest_mac: String,
    pub rx_rate_limiter: Option<RateLimiter>,
    pub tx_rate_limiter: Option<RateLimiter>,
}

/// Vsock device configuration for host-guest communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsockConfig {
    pub vsock_id: String,
    pub guest_cid: u32,
    pub uds_path: PathBuf,
}

/// Rate limiter for network traffic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiter {
    pub bandwidth: Option<TokenBucket>,
    pub ops: Option<TokenBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBucket {
    pub size: u64,
    pub one_time_burst: Option<u64>,
    pub refill_time: u64,
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            vcpu_count: 1,
            mem_size_mib: 512,
            smt: false,
            track_dirty_pages: false,
        }
    }
}

impl VMConfig {
    pub fn new(vm_id: String, kernel_path: PathBuf, rootfs_path: PathBuf) -> Self {
        Self {
            vm_id: vm_id.clone(),
            machine_config: MachineConfig::default(),
            kernel_config: KernelConfig {
                kernel_image_path: kernel_path,
                boot_args: "console=ttyS0 reboot=k panic=1 pci=off".to_string(),
                initrd_path: None,
            },
            rootfs_config: RootfsConfig {
                drive_id: "rootfs".to_string(),
                path_on_host: rootfs_path,
                is_root_device: true,
                is_read_only: false,
            },
            network_config: NetworkConfig {
                iface_id: "eth0".to_string(),
                host_dev_name: format!("fc_tap_{}", vm_id),
                guest_mac: generate_mac_address(),
                rx_rate_limiter: None,
                tx_rate_limiter: None,
            },
            vsock_config: Some(VsockConfig {
                vsock_id: "vsock0".to_string(),
                guest_cid: generate_guest_cid(),
                uds_path: PathBuf::from(format!("/tmp/firecracker/{}/vsock.sock", vm_id)),
            }),
        }
    }
}

fn generate_mac_address() -> String {
    // Generate a locally administered MAC address
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!(
        "02:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        rng.gen::<u8>(),
        rng.gen::<u8>(),
        rng.gen::<u8>(),
        rng.gen::<u8>(),
        rng.gen::<u8>()
    )
}

fn generate_guest_cid() -> u32 {
    // CID 3 and above are for guest VMs
    use rand::Rng;
    let mut rng = rand::thread_rng();
    rng.gen_range(3..=u32::MAX)
}