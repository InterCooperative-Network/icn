#!/bin/bash
# Two-Node ICN Demo with Trust-Gated Rate Limiting
#
# This script demonstrates:
# - Two ICN nodes connecting via mDNS
# - Trust-gated rate limiting in action
# - Prometheus metrics showing rate limit enforcement
# - Different rate limits for different trust classes

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔══════════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║       ICN Two-Node Demo - Trust-Gated Rate Limiting             ║${NC}"
echo -e "${BLUE}╚══════════════════════════════════════════════════════════════════╝${NC}"
echo

# Get the script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE_ROOT="$PROJECT_ROOT/icn"

# Change to workspace directory for cargo commands
cd "$WORKSPACE_ROOT"

# Check if icnd binary exists
if [ ! -f "target/release/icnd" ]; then
    echo -e "${YELLOW}Building release binary...${NC}"
    cargo build --release
fi

# Clean up any existing data
echo -e "${YELLOW}Cleaning up existing test data...${NC}"
rm -rf /tmp/icn-alpha /tmp/icn-beta

# Initialize identities for both nodes
echo -e "${YELLOW}Initializing identities...${NC}"
mkdir -p /tmp/icn-alpha /tmp/icn-beta

# Alpha identity
echo -e "${GREEN}Creating Alpha node identity...${NC}"
if [ ! -f /tmp/icn-alpha/identity.age ]; then
    echo "testpass123" | ./target/release/icnctl --data-dir /tmp/icn-alpha id init || {
        echo -e "${RED}Failed to create Alpha identity${NC}"
        exit 1
    }
fi

# Beta identity
echo -e "${GREEN}Creating Beta node identity...${NC}"
if [ ! -f /tmp/icn-beta/identity.age ]; then
    echo "testpass123" | ./target/release/icnctl --data-dir /tmp/icn-beta id init || {
        echo -e "${RED}Failed to create Beta identity${NC}"
        exit 1
    }
fi

# Trap to cleanup on exit
cleanup() {
    echo
    echo -e "${YELLOW}Shutting down nodes...${NC}"
    kill $ALPHA_PID $BETA_PID 2>/dev/null || true
    wait $ALPHA_PID $BETA_PID 2>/dev/null || true
    echo -e "${GREEN}Demo complete!${NC}"
}
trap cleanup EXIT INT TERM

# Start Alpha node
echo -e "${GREEN}Starting Alpha node (port 7777, metrics 9100)...${NC}"
ICN_PASSPHRASE="testpass123" ./target/release/icnd \
    --config "$PROJECT_ROOT/config/icn-alpha.toml" > /tmp/icn-alpha.log 2>&1 &
ALPHA_PID=$!
echo "  PID: $ALPHA_PID"

# Wait for alpha to initialize
sleep 3

# Start Beta node
echo -e "${GREEN}Starting Beta node (port 7778, metrics 9101)...${NC}"
ICN_PASSPHRASE="testpass123" ./target/release/icnd \
    --config "$PROJECT_ROOT/config/icn-beta.toml" > /tmp/icn-beta.log 2>&1 &
BETA_PID=$!
echo "  PID: $BETA_PID"

# Wait for nodes to discover each other
echo
echo -e "${YELLOW}Waiting for nodes to discover each other via mDNS...${NC}"
sleep 5

echo
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}✓ Both nodes are running!${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo
echo "Alpha node:"
echo "  - Data: /tmp/icn-alpha"
echo "  - Network: 0.0.0.0:7777"
echo "  - RPC: http://localhost:5601"
echo "  - Metrics: http://localhost:9100/metrics"
echo "  - Logs: /tmp/icn-alpha.log"
echo
echo "Beta node:"
echo "  - Data: /tmp/icn-beta"
echo "  - Network: 0.0.0.0:7778"
echo "  - RPC: http://localhost:5602"
echo "  - Metrics: http://localhost:9101/metrics"
echo "  - Logs: /tmp/icn-beta.log"
echo
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${YELLOW}Trust-Gated Rate Limiting Status:${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo
echo "Rate Limits by Trust Class:"
echo "  Isolated   (0.0-0.1): 10 msg/sec, burst 2"
echo "  Known      (0.1-0.4): 50 msg/sec, burst 10"
echo "  Partner    (0.4-0.7): 100 msg/sec, burst 20"
echo "  Federated  (0.7-1.0): 200 msg/sec, burst 50"
echo
echo -e "${YELLOW}Checking metrics...${NC}"
sleep 2

# Check Alpha metrics
echo
echo -e "${GREEN}Alpha Node Metrics (port 9100):${NC}"
curl -s http://localhost:9100/metrics | grep -E "(icn_network_connections|icn_network_messages_rate_limited|icn_trust)" | head -10 || echo "  (metrics not yet available)"

# Check Beta metrics
echo
echo -e "${GREEN}Beta Node Metrics (port 9101):${NC}"
curl -s http://localhost:9101/metrics | grep -E "(icn_network_connections|icn_network_messages_rate_limited|icn_trust)" | head -10 || echo "  (metrics not yet available)"

echo
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}Demo is running!${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo
echo "Press Ctrl+C to stop the demo"
echo
echo "Useful commands:"
echo "  # Watch Alpha logs"
echo "  tail -f /tmp/icn-alpha.log"
echo
echo "  # Watch Beta logs"
echo "  tail -f /tmp/icn-beta.log"
echo
echo "  # Query Alpha metrics"
echo "  curl http://localhost:9100/metrics | grep rate_limited"
echo
echo "  # Query Beta metrics"
echo "  curl http://localhost:9101/metrics | grep rate_limited"
echo

# Keep running until interrupted
wait
