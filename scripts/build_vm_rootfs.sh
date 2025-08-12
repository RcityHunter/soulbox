#!/bin/bash

# Script to build rootfs images with SoulBox agent installed
# This creates VM images for different languages with the agent pre-installed

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
ROOTFS_DIR="/opt/firecracker/rootfs"
AGENT_BINARY="$PROJECT_ROOT/target/release/soulbox-agent"

echo "Building SoulBox VM rootfs images..."

# Build the agent binary first
echo "Building soulbox-agent..."
cd "$PROJECT_ROOT"
cargo build --release --bin soulbox-agent

# Function to inject agent into rootfs
inject_agent() {
    local rootfs_file=$1
    local mount_point=/tmp/rootfs_mount_$$
    
    echo "Injecting agent into $rootfs_file..."
    
    # Create mount point
    sudo mkdir -p $mount_point
    
    # Mount rootfs
    sudo mount -o loop $rootfs_file $mount_point
    
    # Copy agent binary
    sudo cp $AGENT_BINARY $mount_point/usr/local/bin/soulbox-agent
    sudo chmod +x $mount_point/usr/local/bin/soulbox-agent
    
    # Create systemd service
    sudo tee $mount_point/etc/systemd/system/soulbox-agent.service > /dev/null << 'EOF'
[Unit]
Description=SoulBox VM Execution Agent
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/soulbox-agent
Restart=always
RestartSec=3
Environment="RUST_LOG=info"
Environment="VSOCK_PATH=/tmp/vsock.sock"

[Install]
WantedBy=multi-user.target
EOF
    
    # Enable the service
    sudo chroot $mount_point systemctl enable soulbox-agent.service 2>/dev/null || true
    
    # Create workspace directory
    sudo mkdir -p $mount_point/workspace
    sudo chmod 777 $mount_point/workspace
    
    # Unmount
    sudo umount $mount_point
    sudo rmdir $mount_point
    
    echo "Agent injected successfully"
}

# Check if rootfs images exist
if [ ! -d "$ROOTFS_DIR" ]; then
    echo "Error: Rootfs directory not found at $ROOTFS_DIR"
    echo "Please run setup_firecracker.sh first"
    exit 1
fi

# Process each rootfs image
for rootfs in $ROOTFS_DIR/*.ext4; do
    if [ -f "$rootfs" ]; then
        inject_agent "$rootfs"
    fi
done

echo ""
echo "VM rootfs images prepared with SoulBox agent!"
echo ""
echo "The agent will:"
echo "1. Start automatically when VM boots"
echo "2. Listen for code execution requests via vsock"
echo "3. Execute code in isolated environment"
echo "4. Return results to host"