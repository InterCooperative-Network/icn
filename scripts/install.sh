#!/usr/bin/env bash
#
# ICN Installer Script
#
# Usage:
#   curl -sSL https://icn.coop/install.sh | bash
#
# Or with options:
#   curl -sSL https://icn.coop/install.sh | bash -s -- --no-service --data-dir /custom/path
#
# Options:
#   --no-service      Don't install systemd/launchd service
#   --no-config       Don't create default config
#   --data-dir DIR    Custom data directory (default: ~/.icn)
#   --bin-dir DIR     Custom binary directory (default: ~/.local/bin)
#   --build           Build from source instead of downloading binaries
#   --version VER     Install specific version (default: latest)
#   --help            Show this help
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
DATA_DIR="${HOME}/.icn"
BIN_DIR="${HOME}/.local/bin"
INSTALL_SERVICE=true
CREATE_CONFIG=true
BUILD_FROM_SOURCE=false
VERSION="latest"
REPO_URL="https://github.com/InterCooperative-Network/icn"

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)
            OS="linux"
            ;;
        Darwin)
            OS="darwin"
            ;;
        *)
            echo -e "${RED}Error: Unsupported operating system: $OS${NC}"
            exit 1
            ;;
    esac

    case "$ARCH" in
        x86_64|amd64)
            ARCH="x86_64"
            ;;
        aarch64|arm64)
            ARCH="aarch64"
            ;;
        *)
            echo -e "${RED}Error: Unsupported architecture: $ARCH${NC}"
            exit 1
            ;;
    esac

    PLATFORM="${OS}-${ARCH}"
    echo -e "${BLUE}Detected platform: ${PLATFORM}${NC}"
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --no-service)
                INSTALL_SERVICE=false
                shift
                ;;
            --no-config)
                CREATE_CONFIG=false
                shift
                ;;
            --data-dir)
                DATA_DIR="$2"
                shift 2
                ;;
            --bin-dir)
                BIN_DIR="$2"
                shift 2
                ;;
            --build)
                BUILD_FROM_SOURCE=true
                shift
                ;;
            --version)
                VERSION="$2"
                shift 2
                ;;
            --help)
                show_help
                exit 0
                ;;
            *)
                echo -e "${RED}Error: Unknown option: $1${NC}"
                show_help
                exit 1
                ;;
        esac
    done
}

show_help() {
    head -30 "$0" | tail -25
}

# Check for required tools
check_dependencies() {
    echo -e "${BLUE}Checking dependencies...${NC}"

    local missing=()

    if ! command -v curl &> /dev/null && ! command -v wget &> /dev/null; then
        missing+=("curl or wget")
    fi

    if [ "$BUILD_FROM_SOURCE" = true ]; then
        if ! command -v cargo &> /dev/null; then
            missing+=("cargo (Rust)")
        fi
        if ! command -v git &> /dev/null; then
            missing+=("git")
        fi
    fi

    if [ ${#missing[@]} -ne 0 ]; then
        echo -e "${RED}Error: Missing required dependencies:${NC}"
        for dep in "${missing[@]}"; do
            echo "  - $dep"
        done
        echo ""
        echo "Please install the missing dependencies and try again."

        if [[ " ${missing[*]} " =~ " cargo (Rust) " ]]; then
            echo ""
            echo "To install Rust, run:"
            echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        fi

        exit 1
    fi

    echo -e "${GREEN}All dependencies satisfied.${NC}"
}

# Create directories
create_directories() {
    echo -e "${BLUE}Creating directories...${NC}"

    mkdir -p "$DATA_DIR"
    mkdir -p "$BIN_DIR"
    mkdir -p "$DATA_DIR/logs"

    echo "  Data directory: $DATA_DIR"
    echo "  Binary directory: $BIN_DIR"
}

# Download pre-built binaries
download_binaries() {
    echo -e "${BLUE}Downloading ICN binaries...${NC}"

    local base_url
    if [ "$VERSION" = "latest" ]; then
        base_url="${REPO_URL}/releases/latest/download"
    else
        base_url="${REPO_URL}/releases/download/${VERSION}"
    fi

    local icnd_url="${base_url}/icnd-${PLATFORM}"
    local icnctl_url="${base_url}/icnctl-${PLATFORM}"

    # Download icnd
    echo "  Downloading icnd..."
    if command -v curl &> /dev/null; then
        curl -fsSL "$icnd_url" -o "${BIN_DIR}/icnd" || {
            echo -e "${YELLOW}Warning: Pre-built binaries not available for ${PLATFORM}${NC}"
            echo "Falling back to building from source..."
            BUILD_FROM_SOURCE=true
            return
        }
    else
        wget -q "$icnd_url" -O "${BIN_DIR}/icnd" || {
            echo -e "${YELLOW}Warning: Pre-built binaries not available for ${PLATFORM}${NC}"
            echo "Falling back to building from source..."
            BUILD_FROM_SOURCE=true
            return
        }
    fi

    # Download icnctl
    echo "  Downloading icnctl..."
    if command -v curl &> /dev/null; then
        curl -fsSL "$icnctl_url" -o "${BIN_DIR}/icnctl"
    else
        wget -q "$icnctl_url" -O "${BIN_DIR}/icnctl"
    fi

    # Make executable
    chmod +x "${BIN_DIR}/icnd"
    chmod +x "${BIN_DIR}/icnctl"

    echo -e "${GREEN}Binaries downloaded successfully.${NC}"
}

# Build from source
build_from_source() {
    echo -e "${BLUE}Building ICN from source...${NC}"

    local build_dir="/tmp/icn-build-$$"
    mkdir -p "$build_dir"

    # Clone repository
    echo "  Cloning repository..."
    if [ "$VERSION" = "latest" ]; then
        git clone --depth 1 "$REPO_URL" "$build_dir"
    else
        git clone --depth 1 --branch "$VERSION" "$REPO_URL" "$build_dir"
    fi

    # Build
    echo "  Building (this may take a few minutes)..."
    cd "$build_dir/icn"
    cargo build --release --bin icnd --bin icnctl

    # Copy binaries
    cp target/release/icnd "${BIN_DIR}/icnd"
    cp target/release/icnctl "${BIN_DIR}/icnctl"
    chmod +x "${BIN_DIR}/icnd"
    chmod +x "${BIN_DIR}/icnctl"

    # Cleanup
    rm -rf "$build_dir"

    echo -e "${GREEN}Build completed successfully.${NC}"
}

# Create default configuration
create_config() {
    local config_file="${DATA_DIR}/icn.toml"

    if [ -f "$config_file" ]; then
        echo -e "${YELLOW}Configuration file already exists: ${config_file}${NC}"
        echo "Skipping config creation. Edit manually if needed."
        return
    fi

    echo -e "${BLUE}Creating default configuration...${NC}"

    cat > "$config_file" << 'EOF'
# ICN Configuration
# See https://icn.coop/docs/config for all options

# Data storage directory
data_dir = "~/.icn"

[network]
# Enable local network discovery
mdns_enabled = true

# Port for QUIC connections
listen_addr = "0.0.0.0:7777"

# gRPC API port
rpc_port = 5601

# Bootstrap peers (add known peers here)
# bootstrap_peers = ["icn://did:icn:abc@192.168.1.1:7777"]

# STUN servers for NAT traversal (optional)
# stun_servers = ["stun.l.google.com:19302"]

[observability]
# Prometheus metrics port
metrics_port = 9100

# Health check and dashboard port
health_port = 8080

# Log level: trace, debug, info, warn, error
log_level = "info"

[rate_limiting]
enabled = true
refill_interval_ms = 100

# Rate limits by trust class
[rate_limiting.isolated]
max_messages_per_second = 10
burst_capacity = 2

[rate_limiting.known]
max_messages_per_second = 50
burst_capacity = 10

[rate_limiting.partner]
max_messages_per_second = 100
burst_capacity = 20

[rate_limiting.federated]
max_messages_per_second = 200
burst_capacity = 50

[gateway]
# Enable REST/WebSocket API
enabled = false

# Gateway bind address
bind_addr = "127.0.0.1:8080"

# JWT secret for authentication (CHANGE THIS!)
# jwt_secret = "your-secret-here"

# Token expiry in hours
token_expiry_hours = 24
EOF

    echo "  Configuration created: $config_file"
    echo -e "${YELLOW}  Note: Edit this file to configure your node.${NC}"
}

# Install systemd service (Linux)
install_systemd_service() {
    echo -e "${BLUE}Installing systemd service...${NC}"

    local service_file="/etc/systemd/system/icnd.service"
    local user=$(whoami)

    # Check if we can write to systemd directory
    if [ ! -w "/etc/systemd/system" ]; then
        echo -e "${YELLOW}Warning: Cannot write to /etc/systemd/system${NC}"
        echo "Creating user service instead..."

        mkdir -p "${HOME}/.config/systemd/user"
        service_file="${HOME}/.config/systemd/user/icnd.service"
    fi

    cat > "$service_file" << EOF
[Unit]
Description=ICN Daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${user}
ExecStart=${BIN_DIR}/icnd --config ${DATA_DIR}/icn.toml --data-dir ${DATA_DIR}
Restart=on-failure
RestartSec=10
LimitNOFILE=65535

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=${DATA_DIR}
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

    if [[ "$service_file" == "${HOME}/.config/systemd/user/icnd.service" ]]; then
        systemctl --user daemon-reload
        echo "  User service installed. Enable with: systemctl --user enable icnd"
        echo "  Start with: systemctl --user start icnd"
    else
        sudo systemctl daemon-reload
        echo "  System service installed. Enable with: sudo systemctl enable icnd"
        echo "  Start with: sudo systemctl start icnd"
    fi
}

# Install launchd service (macOS)
install_launchd_service() {
    echo -e "${BLUE}Installing launchd service...${NC}"

    local plist_file="${HOME}/Library/LaunchAgents/coop.icn.icnd.plist"
    mkdir -p "${HOME}/Library/LaunchAgents"

    cat > "$plist_file" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>coop.icn.icnd</string>
    <key>ProgramArguments</key>
    <array>
        <string>${BIN_DIR}/icnd</string>
        <string>--config</string>
        <string>${DATA_DIR}/icn.toml</string>
        <string>--data-dir</string>
        <string>${DATA_DIR}</string>
    </array>
    <key>RunAtLoad</key>
    <false/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>StandardOutPath</key>
    <string>${DATA_DIR}/logs/icnd.log</string>
    <key>StandardErrorPath</key>
    <string>${DATA_DIR}/logs/icnd.err</string>
</dict>
</plist>
EOF

    echo "  LaunchAgent installed: $plist_file"
    echo "  Load with: launchctl load $plist_file"
    echo "  Start with: launchctl start coop.icn.icnd"
}

# Install service based on OS
install_service() {
    case "$OS" in
        linux)
            install_systemd_service
            ;;
        darwin)
            install_launchd_service
            ;;
    esac
}

# Update PATH if needed
update_path() {
    local shell_rc

    # Determine shell config file
    if [ -n "${ZSH_VERSION:-}" ]; then
        shell_rc="${HOME}/.zshrc"
    elif [ -n "${BASH_VERSION:-}" ]; then
        shell_rc="${HOME}/.bashrc"
    else
        shell_rc="${HOME}/.profile"
    fi

    # Check if BIN_DIR is in PATH
    if [[ ":$PATH:" != *":${BIN_DIR}:"* ]]; then
        echo -e "${BLUE}Adding ${BIN_DIR} to PATH...${NC}"

        echo "" >> "$shell_rc"
        echo "# ICN binaries" >> "$shell_rc"
        echo "export PATH=\"\${PATH}:${BIN_DIR}\"" >> "$shell_rc"

        export PATH="${PATH}:${BIN_DIR}"

        echo "  Added to $shell_rc"
        echo "  Run 'source $shell_rc' or start a new shell to use icnd/icnctl"
    fi
}

# Print completion message
print_completion() {
    echo ""
    echo -e "${GREEN}============================================${NC}"
    echo -e "${GREEN}  ICN installation complete!${NC}"
    echo -e "${GREEN}============================================${NC}"
    echo ""
    echo "Next steps:"
    echo ""
    echo "  1. Create your identity:"
    echo "     ${BIN_DIR}/icnctl id init"
    echo ""
    echo "  2. Edit your configuration:"
    echo "     \$EDITOR ${DATA_DIR}/icn.toml"
    echo ""

    if [ "$INSTALL_SERVICE" = true ]; then
        if [ "$OS" = "linux" ]; then
            echo "  3. Start the daemon:"
            echo "     systemctl --user start icnd"
            echo "     # or: sudo systemctl start icnd"
        else
            echo "  3. Start the daemon:"
            echo "     launchctl start coop.icn.icnd"
        fi
    else
        echo "  3. Start the daemon:"
        echo "     ${BIN_DIR}/icnd --config ${DATA_DIR}/icn.toml"
    fi

    echo ""
    echo "  4. Check status:"
    echo "     ${BIN_DIR}/icnctl status"
    echo ""
    echo "Documentation: https://icn.coop/docs"
    echo "Community: https://icn.coop/community"
    echo ""
}

# Main installation flow
main() {
    echo ""
    echo -e "${BLUE}╔════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║     ICN Installer                      ║${NC}"
    echo -e "${BLUE}║     Intercooperative Network           ║${NC}"
    echo -e "${BLUE}╚════════════════════════════════════════╝${NC}"
    echo ""

    parse_args "$@"
    detect_platform
    check_dependencies
    create_directories

    if [ "$BUILD_FROM_SOURCE" = true ]; then
        build_from_source
    else
        download_binaries
        # May fall back to build_from_source if download fails
        if [ "$BUILD_FROM_SOURCE" = true ]; then
            check_dependencies  # Re-check for build deps
            build_from_source
        fi
    fi

    if [ "$CREATE_CONFIG" = true ]; then
        create_config
    fi

    if [ "$INSTALL_SERVICE" = true ]; then
        install_service
    fi

    update_path
    print_completion
}

main "$@"
