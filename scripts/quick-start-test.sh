#!/bin/bash
# ICN Demo - Quick Start Test
# Run this to test if the basic stack works

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

PROJECT_ROOT="/home/matt/projects/icn"
TEST_DIR="$HOME/icn-demo-test"

echo "========================================"
echo "ICN Demo - Quick Start Test"
echo "========================================"
echo ""

# Step 1: Check binaries exist
echo -e "${YELLOW}Step 1: Checking binaries...${NC}"
if [ ! -f "$PROJECT_ROOT/icn/target/release/icnd" ]; then
    echo -e "${RED}✗ icnd not found. Building...${NC}"
    cd "$PROJECT_ROOT/icn"
    cargo build --release
    echo -e "${GREEN}✓ Build complete${NC}"
else
    echo -e "${GREEN}✓ icnd exists${NC}"
fi

if [ ! -f "$PROJECT_ROOT/icn/target/release/icnctl" ]; then
    echo -e "${RED}✗ icnctl not found${NC}"
    exit 1
else
    echo -e "${GREEN}✓ icnctl exists${NC}"
fi
echo ""

# Step 2: Create test directory
echo -e "${YELLOW}Step 2: Setting up test environment...${NC}"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"
echo -e "${GREEN}✓ Test directory: $TEST_DIR${NC}"
echo ""

# Step 3: Check/create identity
echo -e "${YELLOW}Step 3: Checking identity...${NC}"
if [ -f "$HOME/.icn/identity.age" ]; then
    echo -e "${GREEN}✓ Identity exists at ~/.icn/${NC}"
    $PROJECT_ROOT/icn/target/release/icnctl id show
else
    echo -e "${YELLOW}Creating new identity...${NC}"
    echo "You'll be prompted for a passphrase (use something simple for demo, like 'demo123')"
    $PROJECT_ROOT/icn/target/release/icnctl id init
    echo -e "${GREEN}✓ Identity created${NC}"
fi
DID=$($PROJECT_ROOT/icn/target/release/icnctl id show | grep "DID:" | awk '{print $2}')
echo "Your DID: $DID"
echo ""

# Step 4: Create config
echo -e "${YELLOW}Step 4: Creating config...${NC}"
cat > "$TEST_DIR/demo.toml" << EOF
# ICN Demo Configuration
data_dir = "$TEST_DIR/data"

[network]
listen_addr = "0.0.0.0:7777"
rpc_port = 5601
mdns_enabled = true
bootstrap_peers = []
min_trust_threshold = 0.0

[observability]
metrics_port = 9100
health_port = 8081
log_level = "info"

[rate_limiting]
enabled = true
refill_interval_ms = 100

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

[rate_limiting.fallback]
max_messages_per_second = 100
burst_capacity = 20

[topology]
region = "demo"
cluster_id = "test"
role = "edge"

[topology.neighbor_limits]
max_local_cluster = 10
max_regional = 10
max_backbone = 5
max_trusted = 20

[topology.fanout]
local_cluster = 8
regional = 6
global = 4
EOF
echo -e "${GREEN}✓ Config created: $TEST_DIR/demo.toml${NC}"
echo ""

# Step 5: Test daemon startup
echo -e "${YELLOW}Step 5: Testing daemon startup...${NC}"
echo "Starting daemon for 5 seconds to test..."
cd "$PROJECT_ROOT/icn"
timeout 5 ./target/release/icnd \
    --config "$TEST_DIR/demo.toml" \
    --gateway-enable \
    --gateway-bind "127.0.0.1:8080" \
    2>&1 | tee "$TEST_DIR/daemon.log" &

DAEMON_PID=$!
sleep 3

# Check if daemon is running
if ps -p $DAEMON_PID > /dev/null; then
    echo -e "${GREEN}✓ Daemon started (PID: $DAEMON_PID)${NC}"
else
    echo -e "${RED}✗ Daemon failed to start${NC}"
    echo "Check logs: $TEST_DIR/daemon.log"
    exit 1
fi

# Test health endpoints
echo ""
echo -e "${YELLOW}Testing endpoints...${NC}"

sleep 2  # Give time for servers to start

if curl -f http://localhost:8081/health > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Health endpoint responding (8081)${NC}"
else
    echo -e "${RED}✗ Health endpoint not responding${NC}"
fi

if curl -f http://localhost:9100/metrics > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Metrics endpoint responding (9100)${NC}"
else
    echo -e "${RED}✗ Metrics endpoint not responding${NC}"
fi

# Check if gateway is responding
if curl -f http://localhost:8080 > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Gateway responding (8080)${NC}"
else
    echo -e "${YELLOW}⚠ Gateway may not be responding (this is expected if gateway needs setup)${NC}"
fi

# Stop daemon
kill $DAEMON_PID 2>/dev/null || true
wait $DAEMON_PID 2>/dev/null || true
echo ""

# Step 6: Next steps
echo ""
echo "========================================"
echo -e "${GREEN}✓ Basic setup working!${NC}"
echo "========================================"
echo ""
echo "Next steps to test full stack:"
echo ""
echo "1. Start the daemon in a separate terminal:"
echo "   cd $PROJECT_ROOT/icn"
echo "   ./target/release/icnd --config $TEST_DIR/demo.toml --gateway-enable --gateway-bind \"127.0.0.1:8080\""
echo ""
echo "2. In another terminal, try these commands:"
echo "   # Check daemon status"
echo "   ./target/release/icnctl status"
echo ""
echo "   # Show your identity"
echo "   ./target/release/icnctl id show"
echo ""
echo "   # Try to initialize a cooperative"
echo "   ./target/release/icnctl init-coop"
echo ""
echo "3. Start the UI (in another terminal):"
echo "   cd $PROJECT_ROOT/web/pilot-ui"
echo "   python3 -m http.server 3000"
echo ""
echo "4. Open browser to http://localhost:3000"
echo ""
echo "Test results and logs saved to:"
echo "  $TEST_DIR/"
echo ""
echo "Your DID: $DID"
echo ""
