#!/usr/bin/env bash
#
# ICN Pilot UI - Simple Deployment Script
#
# This script deploys ONLY the web UI (assumes icnd is already running)
#
# Usage:
#   ./deploy-ui.sh [port]
#
# Example:
#   ./deploy-ui.sh 3000
#

set -e

# Configuration
PORT="${1:-3000}"
GATEWAY_URL="${ICN_GATEWAY_URL:-http://localhost:8080}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}"
echo "╔════════════════════════════════════════════════════════════╗"
echo "║           ICN Pilot UI - Simple Deploy                     ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo -e "${NC}"
echo ""

# Check if gateway is running
echo -e "${YELLOW}Checking gateway connection...${NC}"
if ! curl -sf "$GATEWAY_URL/v1/health" > /dev/null 2>&1; then
    echo "Warning: Cannot reach gateway at $GATEWAY_URL"
    echo "Make sure icnd is running with --gateway-enable"
    echo ""
    echo "Start icnd with:"
    echo "  icnd --gateway-enable --gateway-bind 127.0.0.1:8080 --gateway-jwt-secret \"your-secret\""
    echo ""
    read -p "Continue anyway? [y/N] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
else
    echo -e "${GREEN}Gateway is reachable at $GATEWAY_URL${NC}"
fi

echo ""

# Method selection
echo "Choose deployment method:"
echo "  1) Python HTTP server (simple, localhost only)"
echo "  2) Node.js serve (requires npm)"
echo "  3) Docker with nginx (production-ready)"
echo ""
read -p "Select method [1-3]: " -n 1 -r METHOD
echo ""
echo ""

case $METHOD in
    1)
        echo -e "${YELLOW}Starting Python HTTP server on port $PORT...${NC}"
        cd "$SCRIPT_DIR"

        # Check Python version
        if command -v python3 &> /dev/null; then
            PYTHON_CMD="python3"
        elif command -v python &> /dev/null; then
            PYTHON_CMD="python"
        else
            echo "Error: Python is not installed"
            exit 1
        fi

        echo -e "${GREEN}Server starting...${NC}"
        echo ""
        echo "Access the UI at: http://localhost:$PORT"
        echo "Gateway URL: $GATEWAY_URL"
        echo ""
        echo "Press Ctrl+C to stop"
        echo ""

        $PYTHON_CMD -m http.server $PORT
        ;;

    2)
        echo -e "${YELLOW}Starting Node.js serve on port $PORT...${NC}"

        if ! command -v npx &> /dev/null; then
            echo "Error: npx (Node.js) is not installed"
            echo "Install Node.js from: https://nodejs.org/"
            exit 1
        fi

        cd "$SCRIPT_DIR"

        echo -e "${GREEN}Server starting...${NC}"
        echo ""
        echo "Access the UI at: http://localhost:$PORT"
        echo "Gateway URL: $GATEWAY_URL"
        echo ""
        echo "Press Ctrl+C to stop"
        echo ""

        npx serve -s . -l $PORT
        ;;

    3)
        echo -e "${YELLOW}Starting Docker container with nginx...${NC}"

        if ! command -v docker &> /dev/null; then
            echo "Error: Docker is not installed"
            echo "Install Docker from: https://docs.docker.com/get-docker/"
            exit 1
        fi

        CONTAINER_NAME="icn-pilot-ui"

        # Stop existing container if running
        if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
            echo "  Stopping existing container..."
            docker stop $CONTAINER_NAME > /dev/null 2>&1 || true
            docker rm $CONTAINER_NAME > /dev/null 2>&1 || true
        fi

        # Create nginx config
        cat > /tmp/nginx-pilot-ui.conf <<EOF
server {
    listen 80;
    server_name localhost;

    location / {
        root /usr/share/nginx/html;
        index index.html;
        try_files \$uri \$uri/ /index.html;
    }

    # Proxy /v1 API requests to gateway
    location /v1 {
        proxy_pass $GATEWAY_URL;
        proxy_http_version 1.1;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host \$host;
    }
}
EOF

        echo "  Starting nginx container..."
        docker run -d \
            --name $CONTAINER_NAME \
            --restart unless-stopped \
            -p $PORT:80 \
            -v "$SCRIPT_DIR:/usr/share/nginx/html:ro" \
            -v /tmp/nginx-pilot-ui.conf:/etc/nginx/conf.d/default.conf:ro \
            nginx:alpine

        sleep 2

        echo -e "${GREEN}Container started${NC}"
        echo ""
        echo "Access the UI at: http://localhost:$PORT"
        echo "Gateway URL: $GATEWAY_URL"
        echo ""
        echo "To stop the container:"
        echo "  docker stop $CONTAINER_NAME"
        echo ""
        echo "To view logs:"
        echo "  docker logs -f $CONTAINER_NAME"
        ;;

    *)
        echo "Invalid selection"
        exit 1
        ;;
esac
