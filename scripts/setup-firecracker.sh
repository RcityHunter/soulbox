#!/bin/bash

# SoulBox Firecracker Setup Script
# This script automates the Firecracker VM environment setup

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
FIRECRACKER_DIR="/opt/firecracker"
ARCH=$(uname -m)
OS=$(uname -s)

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_requirements() {
    log_info "Checking system requirements..."
    
    # Check if running on macOS
    if [[ "$OS" == "Darwin" ]]; then
        log_warning "macOS detected. Firecracker requires Linux with KVM support."
        log_warning "Consider using Docker runtime instead."
        log_warning "Continue setup anyway? (y/N)"
        read -r response
        if [[ ! "$response" =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi
    
    # Check if running as root for directory creation
    if [[ $EUID -ne 0 ]] && [[ ! -d "$FIRECRACKER_DIR" ]]; then
        log_error "This script needs sudo privileges to create $FIRECRACKER_DIR"
        log_info "Please run with sudo or create the directory manually"
        exit 1
    fi
}

create_directories() {
    log_info "Creating Firecracker directories..."
    
    if [[ ! -d "$FIRECRACKER_DIR" ]]; then
        sudo mkdir -p "$FIRECRACKER_DIR"
        log_success "Created $FIRECRACKER_DIR"
    else
        log_info "$FIRECRACKER_DIR already exists"
    fi
}

download_firecracker() {
    log_info "Downloading Firecracker binary..."
    
    # Check if firecracker is already installed
    if command -v firecracker &> /dev/null; then
        log_info "Firecracker already installed: $(firecracker --version)"
        return 0
    fi
    
    # Get latest release
    LATEST_RELEASE=$(curl -s https://api.github.com/repos/firecracker-microvm/firecracker/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    
    if [[ -z "$LATEST_RELEASE" ]]; then
        log_error "Failed to get latest Firecracker release"
        exit 1
    fi
    
    log_info "Latest Firecracker release: $LATEST_RELEASE"
    
    # Download and install
    TEMP_DIR=$(mktemp -d)
    cd "$TEMP_DIR"
    
    RELEASE_URL="https://github.com/firecracker-microvm/firecracker/releases"
    DOWNLOAD_URL="${RELEASE_URL}/download/${LATEST_RELEASE}/firecracker-${LATEST_RELEASE}-${ARCH}.tgz"
    
    curl -LO "$DOWNLOAD_URL"
    tar -xzf "firecracker-${LATEST_RELEASE}-${ARCH}.tgz"
    
    sudo cp "release-${LATEST_RELEASE}-${ARCH}/firecracker-${LATEST_RELEASE}-${ARCH}" /usr/local/bin/firecracker
    sudo chmod +x /usr/local/bin/firecracker
    
    cd - > /dev/null
    rm -rf "$TEMP_DIR"
    
    log_success "Firecracker binary installed: $(firecracker --version)"
}

download_kernel() {
    log_info "Downloading Linux kernel for Firecracker..."
    
    KERNEL_PATH="$FIRECRACKER_DIR/vmlinux.bin"
    
    if [[ -f "$KERNEL_PATH" ]]; then
        log_info "Kernel already exists at $KERNEL_PATH"
        return 0
    fi
    
    # Choose kernel based on architecture
    if [[ "$ARCH" == "x86_64" ]]; then
        KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/x86_64/kernels/vmlinux.bin"
    elif [[ "$ARCH" == "aarch64" ]] || [[ "$ARCH" == "arm64" ]]; then
        KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/img/aarch64/kernel/vmlinux.bin"
    else
        log_error "Unsupported architecture: $ARCH"
        exit 1
    fi
    
    log_info "Downloading kernel for $ARCH..."
    sudo curl -fsSL -o "$KERNEL_PATH" "$KERNEL_URL"
    
    if [[ -f "$KERNEL_PATH" ]]; then
        log_success "Kernel downloaded to $KERNEL_PATH"
    else
        log_error "Failed to download kernel"
        exit 1
    fi
}

create_rootfs() {
    log_info "Creating root filesystem..."
    
    ROOTFS_PATH="$FIRECRACKER_DIR/rootfs.ext4"
    
    if [[ -f "$ROOTFS_PATH" ]]; then
        log_info "Root filesystem already exists at $ROOTFS_PATH"
        return 0
    fi
    
    # Create a simple ext4 filesystem
    log_info "Creating 1GB ext4 filesystem..."
    sudo dd if=/dev/zero of="$ROOTFS_PATH" bs=1M count=1024 2>/dev/null
    sudo mkfs.ext4 -F "$ROOTFS_PATH" > /dev/null 2>&1
    
    # Create a minimal directory structure
    MOUNT_POINT="/tmp/soulbox-rootfs-mount"
    sudo mkdir -p "$MOUNT_POINT"
    sudo mount "$ROOTFS_PATH" "$MOUNT_POINT"
    
    # Create basic directory structure
    sudo mkdir -p "$MOUNT_POINT"/{bin,sbin,etc,proc,sys,dev,tmp,var,usr/bin,usr/sbin}
    
    # Copy basic binaries if available
    if command -v busybox &> /dev/null; then
        sudo cp "$(which busybox)" "$MOUNT_POINT/bin/"
        log_info "Added busybox to rootfs"
    fi
    
    sudo umount "$MOUNT_POINT"
    sudo rmdir "$MOUNT_POINT"
    
    log_success "Root filesystem created at $ROOTFS_PATH"
    log_warning "This is a minimal rootfs. Consider using a proper Ubuntu image for production."
}

update_config() {
    log_info "Updating SoulBox configuration..."
    
    CONFIG_FILE="./soulbox.toml"
    
    if [[ ! -f "$CONFIG_FILE" ]]; then
        log_warning "soulbox.toml not found in current directory"
        log_info "Please manually update your configuration to use Firecracker runtime"
        return 0
    fi
    
    # Backup original config
    cp "$CONFIG_FILE" "${CONFIG_FILE}.backup"
    
    # Update runtime configuration
    if grep -q '\[runtime\]' "$CONFIG_FILE"; then
        sed -i.bak 's/default = "docker"/default = "firecracker"/' "$CONFIG_FILE"
        log_success "Updated runtime configuration in $CONFIG_FILE"
        log_info "Backup saved as ${CONFIG_FILE}.backup"
    else
        log_warning "Could not find [runtime] section in $CONFIG_FILE"
        log_info "Please manually update the configuration"
    fi
}

verify_installation() {
    log_info "Verifying Firecracker installation..."
    
    # Check firecracker binary
    if command -v firecracker &> /dev/null; then
        log_success "Firecracker binary: $(which firecracker)"
        log_success "Version: $(firecracker --version)"
    else
        log_error "Firecracker binary not found"
        return 1
    fi
    
    # Check kernel
    if [[ -f "$FIRECRACKER_DIR/vmlinux.bin" ]]; then
        log_success "Kernel: $FIRECRACKER_DIR/vmlinux.bin"
        log_info "Size: $(ls -lh $FIRECRACKER_DIR/vmlinux.bin | awk '{print $5}')"
    else
        log_error "Kernel not found"
        return 1
    fi
    
    # Check rootfs
    if [[ -f "$FIRECRACKER_DIR/rootfs.ext4" ]]; then
        log_success "Rootfs: $FIRECRACKER_DIR/rootfs.ext4"
        log_info "Size: $(ls -lh $FIRECRACKER_DIR/rootfs.ext4 | awk '{print $5}')"
    else
        log_error "Rootfs not found"
        return 1
    fi
    
    log_success "Firecracker environment setup complete!"
}

show_next_steps() {
    log_info "Next steps:"
    echo "1. Start SoulBox with Firecracker runtime:"
    echo "   cargo run --bin soulbox"
    echo ""
    echo "2. Test sandbox creation:"
    echo "   curl -X POST http://localhost:8080/api/v1/sandboxes \\"
    echo "     -H 'Content-Type: application/json' \\"
    echo "     -d '{\"template_id\": \"ubuntu-22.04\", \"runtime\": \"firecracker\"}'"
    echo ""
    echo "3. Check logs for any issues"
    echo ""
    log_warning "Note: Firecracker requires Linux with KVM support."
    log_warning "On macOS/Windows, consider using Docker runtime instead."
}

main() {
    echo "SoulBox Firecracker Setup"
    echo "========================"
    echo ""
    
    check_requirements
    create_directories
    download_firecracker
    download_kernel
    create_rootfs
    update_config
    
    if verify_installation; then
        echo ""
        show_next_steps
    else
        log_error "Installation verification failed"
        exit 1
    fi
}

# Run main function
main "$@"