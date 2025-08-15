# Firecracker VM Environment Setup

This guide explains how to configure the Firecracker VM environment for SoulBox.

## Prerequisites

1. **Firecracker Binary**: Download from [Firecracker Releases](https://github.com/firecracker-microvm/firecracker/releases)
2. **Kernel Image**: Compatible Linux kernel for Firecracker
3. **Root Filesystem**: Base filesystem image for VMs
4. **Root Privileges**: Required for VM management

## Quick Setup

### 1. Create Firecracker Directory

```bash
sudo mkdir -p /opt/firecracker
cd /opt/firecracker
```

### 2. Download Firecracker Binary

```bash
# For ARM64 (Apple Silicon)
ARCH=$(uname -m)
RELEASE_URL="https://github.com/firecracker-microvm/firecracker/releases"
LATEST_RELEASE=$(curl -s https://api.github.com/repos/firecracker-microvm/firecracker/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

sudo curl -LO ${RELEASE_URL}/download/${LATEST_RELEASE}/firecracker-${LATEST_RELEASE}-${ARCH}.tgz
sudo tar -xzf firecracker-${LATEST_RELEASE}-${ARCH}.tgz
sudo mv release-${LATEST_RELEASE}-${ARCH}/firecracker-${LATEST_RELEASE}-${ARCH} /usr/local/bin/firecracker
sudo chmod +x /usr/local/bin/firecracker
```

### 3. Download Kernel

```bash
# Download a compatible kernel
sudo curl -fsSL -o vmlinux.bin https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/x86_64/kernels/vmlinux.bin

# For ARM64, use:
# sudo curl -fsSL -o vmlinux.bin https://s3.amazonaws.com/spec.ccfc.min/img/aarch64/kernel/vmlinux.bin
```

### 4. Create Root Filesystem

```bash
# Download base Ubuntu rootfs
sudo curl -fsSL -o ubuntu-22.04.ext4 https://cloud-images.ubuntu.com/minimal/releases/jammy/release/ubuntu-22.04-minimal-cloudimg-amd64-root.tar.xz

# Or create custom rootfs
sudo dd if=/dev/zero of=rootfs.ext4 bs=1M count=1024
sudo mkfs.ext4 rootfs.ext4
sudo mkdir -p /tmp/mount
sudo mount rootfs.ext4 /tmp/mount

# Install base system (example with debootstrap)
sudo debootstrap --arch amd64 jammy /tmp/mount http://archive.ubuntu.com/ubuntu/
sudo umount /tmp/mount
```

### 5. Update SoulBox Configuration

Edit `soulbox.toml`:

```toml
[runtime]
default = "firecracker"

[runtime.firecracker]
kernel_path = "/opt/firecracker/vmlinux.bin"
rootfs_path = "/opt/firecracker/rootfs.ext4"
```

## Configuration Options

### Resource Limits

```toml
[resources]
cpu_cores = 2
memory_mb = 1024
disk_mb = 5120
```

### Network Configuration

```toml
[network]
allow_internet = true
allow_localhost = true
```

### Security Settings

```toml
[security]
allow_sudo = false
read_only = false
```

## Verification

Test Firecracker installation:

```bash
firecracker --version
```

Verify files exist:

```bash
ls -la /opt/firecracker/vmlinux.bin
ls -la /opt/firecracker/rootfs.ext4
```

## Troubleshooting

### Permission Issues

Ensure the firecracker binary has proper permissions:

```bash
sudo chmod +x /usr/local/bin/firecracker
```

### KVM Support

Firecracker requires KVM. On macOS, this is not available, so Docker runtime should be used instead.

### Network Configuration

Firecracker requires TAP interfaces for networking. Ensure your system supports this or use Docker runtime for simpler setup.

## Production Considerations

1. **Security**: Run Firecracker in a sandboxed environment
2. **Networking**: Configure proper firewall rules
3. **Storage**: Use fast storage for VM images
4. **Monitoring**: Set up VM monitoring and resource tracking

## Alternative: Docker Runtime

For development and testing, Docker runtime is recommended as it's easier to set up:

```toml
[runtime]
default = "docker"
```

This avoids the complexity of Firecracker setup while providing similar isolation capabilities.