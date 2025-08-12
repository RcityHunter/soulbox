#!/bin/bash

# Setup script for Firecracker on Linux
# This script downloads Firecracker binary and creates basic rootfs images

set -e

FIRECRACKER_VERSION="v1.4.0"
ARCH=$(uname -m)

echo "Setting up Firecracker for SoulBox..."

# Create directories
sudo mkdir -p /opt/firecracker/{bin,kernels,rootfs,images,snapshots}

# Download Firecracker binary
if [ ! -f /opt/firecracker/bin/firecracker ]; then
    echo "Downloading Firecracker ${FIRECRACKER_VERSION}..."
    curl -fsSL -o /tmp/firecracker \
        "https://github.com/firecracker-microvm/firecracker/releases/download/${FIRECRACKER_VERSION}/firecracker-${FIRECRACKER_VERSION}-${ARCH}"
    sudo mv /tmp/firecracker /opt/firecracker/bin/firecracker
    sudo chmod +x /opt/firecracker/bin/firecracker
fi

# Download kernel (example using Ubuntu kernel)
if [ ! -f /opt/firecracker/vmlinux ]; then
    echo "Downloading kernel..."
    # This is an example - you should build or download a proper microVM kernel
    curl -fsSL -o /tmp/vmlinux.bin \
        "https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/v1.4/x86_64/vmlinux-5.10.186"
    sudo mv /tmp/vmlinux.bin /opt/firecracker/vmlinux
fi

# Create base rootfs images
echo "Creating rootfs templates..."

# Function to create a minimal rootfs
create_minimal_rootfs() {
    local name=$1
    local size_mb=$2
    local packages=$3
    
    echo "Creating ${name} rootfs (${size_mb}MB)..."
    
    # Create image file
    sudo dd if=/dev/zero of=/opt/firecracker/rootfs/${name}-rootfs.ext4 bs=1M count=${size_mb}
    sudo mkfs.ext4 /opt/firecracker/rootfs/${name}-rootfs.ext4
    
    # Mount and setup
    sudo mkdir -p /tmp/rootfs_mount
    sudo mount -o loop /opt/firecracker/rootfs/${name}-rootfs.ext4 /tmp/rootfs_mount
    
    # Install minimal system (this is simplified - use debootstrap or similar in production)
    echo "Note: You need to populate the rootfs with a minimal Linux system"
    echo "For example, using debootstrap:"
    echo "sudo debootstrap --arch=amd64 focal /tmp/rootfs_mount http://archive.ubuntu.com/ubuntu/"
    
    # Unmount
    sudo umount /tmp/rootfs_mount
}

# Create language-specific rootfs templates
create_minimal_rootfs "ubuntu" 1024 "base"
create_minimal_rootfs "node" 1024 "nodejs npm"
create_minimal_rootfs "python" 1024 "python3 python3-pip"
create_minimal_rootfs "rust" 2048 "rustc cargo"
create_minimal_rootfs "go" 1024 "golang"

# Setup network bridge
echo "Setting up network bridge..."
if ! ip link show fc-br0 &>/dev/null; then
    sudo ip link add name fc-br0 type bridge
    sudo ip addr add 172.16.0.1/24 dev fc-br0
    sudo ip link set dev fc-br0 up
    
    # Enable IP forwarding
    sudo sysctl -w net.ipv4.ip_forward=1
    
    # Setup NAT (adjust eth0 to your internet-facing interface)
    sudo iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
    sudo iptables -A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
    sudo iptables -A FORWARD -i fc-br0 -o eth0 -j ACCEPT
fi

# Create TAP interface management script
cat << 'EOF' | sudo tee /opt/firecracker/bin/create_tap.sh
#!/bin/bash
TAP_NAME=$1
sudo ip tuntap add dev $TAP_NAME mode tap
sudo ip link set dev $TAP_NAME master fc-br0
sudo ip link set dev $TAP_NAME up
EOF
sudo chmod +x /opt/firecracker/bin/create_tap.sh

echo "Firecracker setup complete!"
echo ""
echo "Note: This is a basic setup. For production use:"
echo "1. Build proper microVM kernels with minimal config"
echo "2. Create rootfs images with debootstrap or buildroot"
echo "3. Configure proper security policies"
echo "4. Set up vsock for host-guest communication"
echo ""
echo "To test Firecracker:"
echo "sudo /opt/firecracker/bin/firecracker --api-sock /tmp/firecracker.sock"