#!/bin/bash
# ICN Installation Script
# Installs ICN binaries and sets up systemd service

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "================================"
echo "ICN Installation Script"
echo "================================"
echo ""

# Check root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}Error: Please run as root (sudo $0)${NC}"
    exit 1
fi

# Configuration
INSTALL_DIR="/usr/local/bin"
DATA_DIR="/var/lib/icn"
CONFIG_DIR="/etc/icn"
STATIC_DIR="/usr/share/icn/static"
ICN_USER="icn"

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "Installation directories:"
echo "  Binaries: $INSTALL_DIR"
echo "  Data: $DATA_DIR"
echo "  Config: $CONFIG_DIR"
echo "  Static: $STATIC_DIR"
echo ""

# Create ICN user
if ! id "$ICN_USER" &>/dev/null; then
    echo -e "${YELLOW}Creating user '$ICN_USER'...${NC}"
    useradd --system --home-dir "$DATA_DIR" --shell /bin/false "$ICN_USER"
else
    echo "User '$ICN_USER' already exists"
fi

# Create directories
echo "Creating directories..."
mkdir -p "$DATA_DIR" "$CONFIG_DIR" "$STATIC_DIR"
chown -R "$ICN_USER:$ICN_USER" "$DATA_DIR"

# Check for binaries
BINARY_DIR="$PROJECT_ROOT/icn/target/release"
if [ ! -f "$BINARY_DIR/icnd" ] || [ ! -f "$BINARY_DIR/icnctl" ]; then
    echo -e "${YELLOW}Binaries not found in $BINARY_DIR${NC}"
    echo "Building release binaries..."
    cd "$PROJECT_ROOT/icn"
    cargo build --release
fi

# Install binaries
echo "Installing binaries..."
cp "$BINARY_DIR/icnd" "$INSTALL_DIR/"
cp "$BINARY_DIR/icnctl" "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/icnd" "$INSTALL_DIR/icnctl"

# Install static files
echo "Installing static web files..."
if [ -d "$PROJECT_ROOT/icn/crates/icn-gateway/static" ]; then
    cp -r "$PROJECT_ROOT/icn/crates/icn-gateway/static/"* "$STATIC_DIR/"
elif [ -d "$PROJECT_ROOT/web/pilot-ui" ]; then
    cp "$PROJECT_ROOT/web/pilot-ui/"*.html "$STATIC_DIR/"
    cp "$PROJECT_ROOT/web/pilot-ui/"*.css "$STATIC_DIR/"
    cp "$PROJECT_ROOT/web/pilot-ui/"*.js "$STATIC_DIR/"
fi

# Install systemd service
echo "Installing systemd service..."
cp "$SCRIPT_DIR/icnd.service" /etc/systemd/system/
systemctl daemon-reload

# Install environment template
if [ ! -f "$CONFIG_DIR/icnd.env" ]; then
    echo "Creating environment configuration..."
    cp "$SCRIPT_DIR/icnd.env.example" "$CONFIG_DIR/icnd.env"
    chmod 600 "$CONFIG_DIR/icnd.env"

    # Generate JWT secret
    JWT_SECRET=$(openssl rand -hex 32)
    sed -i "s/your-strong-secret-here/$JWT_SECRET/" "$CONFIG_DIR/icnd.env"
    echo -e "${GREEN}Generated JWT secret${NC}"
else
    echo "Environment config already exists at $CONFIG_DIR/icnd.env"
fi

# Install health check
echo "Installing health check script..."
cp "$SCRIPT_DIR/health-check.sh" "$INSTALL_DIR/icn-health-check"
chmod +x "$INSTALL_DIR/icn-health-check"

echo ""
echo -e "${GREEN}================================${NC}"
echo -e "${GREEN}Installation Complete!${NC}"
echo -e "${GREEN}================================${NC}"
echo ""
echo "Next steps:"
echo ""
echo "1. Initialize identity (as icn user):"
echo "   sudo -u icn $INSTALL_DIR/icnctl --data-dir $DATA_DIR id init"
echo ""
echo "2. Review configuration:"
echo "   sudo nano $CONFIG_DIR/icnd.env"
echo ""
echo "3. Start the service:"
echo "   sudo systemctl start icnd"
echo "   sudo systemctl enable icnd"
echo ""
echo "4. Check status:"
echo "   sudo systemctl status icnd"
echo "   $INSTALL_DIR/icn-health-check"
echo ""
echo "5. Get authentication token:"
echo "   sudo -u icn $INSTALL_DIR/icnctl --data-dir $DATA_DIR auth token --coop your-coop-id"
echo ""
echo "6. Access the web UI:"
echo "   http://localhost:8080"
echo ""
