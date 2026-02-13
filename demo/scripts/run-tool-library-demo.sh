#!/bin/bash
# ICN Tool Library Demo - Automated Run Script
# This script starts everything needed for the demo and keeps it running.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ICN_DIR="${REPO_ROOT}/icn"
UI_DIR="${REPO_ROOT}/web/pilot-ui"

# Configuration (override with env vars as needed)
DEMO_NAME="Rochester Tool Library Demo"
GATEWAY_HOST="${ICN_DEMO_GATEWAY_HOST:-0.0.0.0}"
GATEWAY_PORT="${ICN_DEMO_GATEWAY_PORT:-8080}"
# For display/API calls, resolve 0.0.0.0 to a reachable address
if [ "$GATEWAY_HOST" = "0.0.0.0" ]; then
    LAN_IP=$(hostname -I 2>/dev/null | awk '{print $1}')
    GATEWAY_DISPLAY_HOST="${LAN_IP:-127.0.0.1}"
else
    GATEWAY_DISPLAY_HOST="$GATEWAY_HOST"
fi
GATEWAY="http://${GATEWAY_DISPLAY_HOST}:${GATEWAY_PORT}"
UI_PORT="${ICN_DEMO_UI_PORT:-3000}"
COOP_ID="${ICN_DEMO_COOP_ID:-rochester-tool-library}"
DATA_DIR="${ICN_DEMO_DATA_DIR:-${REPO_ROOT}/.demo-data/tool-library}"
RPC_ENDPOINT="${ICN_DEMO_RPC_ENDPOINT:-127.0.0.1:15602}"
JWT_SECRET="${ICN_GATEWAY_JWT_SECRET:-}"
DEFAULT_DID="did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh"
MDNS_ENABLED="${ICN_DEMO_MDNS_ENABLED:-false}"
RUNTIME_CONFIG="$(mktemp /tmp/icn-demo-runtime.XXXXXX.toml)"
RPC_PORT="${RPC_ENDPOINT##*:}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

# PIDs for cleanup
DAEMON_PID=""
UI_PID=""

require_command() {
    local command="$1"
    if ! command -v "$command" >/dev/null 2>&1; then
        echo -e "${RED}✗${NC} Required command not found: $command"
        exit 1
    fi
}

# Cleanup function
cleanup() {
    echo
    echo "========================================="
    echo "Cleaning up..."
    echo "========================================="

    if [ -n "$UI_PID" ] && [[ "$UI_PID" =~ ^[0-9]+$ ]]; then
        echo "Stopping UI (PID: $UI_PID)..."
        kill "$UI_PID" 2>/dev/null || true
    fi

    if [ -n "$DAEMON_PID" ] && [[ "$DAEMON_PID" =~ ^[0-9]+$ ]]; then
        echo "Stopping daemon (PID: $DAEMON_PID)..."
        kill "$DAEMON_PID" 2>/dev/null || true
    fi

    rm -f "$RUNTIME_CONFIG"
    echo "Demo stopped."
}

# Set up trap for cleanup
trap cleanup INT TERM EXIT

# Header
clear
echo "========================================="
echo "$DEMO_NAME"
echo "========================================="
echo
echo "This script will:"
echo "  1. Start the ICN daemon"
echo "  2. Wait for services to be ready"
echo "  3. Start the pilot UI"
echo "  4. Display access information"
echo "  5. Keep everything running until Ctrl+C"
echo
echo "Configuration:"
echo "  Repo root: $REPO_ROOT"
echo "  Data dir:  $DATA_DIR"
echo "  Gateway:   $GATEWAY (bind: $GATEWAY_HOST)"
echo "  UI:        http://${GATEWAY_DISPLAY_HOST}:$UI_PORT"
echo "  mDNS:      $MDNS_ENABLED"
echo
echo "Press Enter to continue or Ctrl+C to cancel..."
read -r

require_command curl
require_command python3
require_command cargo
require_command lsof
require_command openssl

mkdir -p "$DATA_DIR"

# Generate JWT secret after dependency checks (openssl is now verified)
if [ -z "$JWT_SECRET" ]; then
    JWT_SECRET="$(openssl rand -hex 32)"
fi

if [ "${#JWT_SECRET}" -lt 32 ]; then
    echo -e "${RED}✗${NC} ICN_GATEWAY_JWT_SECRET must be at least 32 bytes"
    exit 1
fi

if [ ! -x "$ICN_DIR/target/release/icnd" ] || [ ! -x "$ICN_DIR/target/release/icnctl" ]; then
    echo "Release binaries missing; building icnd and icnctl..."
    (
        cd "$ICN_DIR"
        cargo build --release -p icnd -p icnctl
    )
fi

if [ ! -f "$DATA_DIR/identity.age" ]; then
    echo "No demo identity found; initializing one in $DATA_DIR..."
    (
        cd "$ICN_DIR"
        ICN_PASSPHRASE=demo123 ./target/release/icnctl -d "$DATA_DIR" id init >/tmp/icn-demo-id-init.log 2>&1
    )
fi

cat > "$RUNTIME_CONFIG" <<EOF
data_dir = "$DATA_DIR"

[network]
listen_addr = "127.0.0.1:7777"
rpc_port = $RPC_PORT
mdns_enabled = $MDNS_ENABLED
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
region = "demo-local"
cluster_id = "demo-node"
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

# Step 1: Check if daemon is already running
echo
echo "========================================="
echo "Step 1: Checking daemon status"
echo "========================================="
echo

if curl -fsS "$GATEWAY/v1/health" >/dev/null 2>&1; then
    echo -e "${YELLOW}⚠${NC} Daemon appears to be already running"
    echo "Using existing daemon at $GATEWAY"
else
    echo "Starting ICN daemon..."
    (
        cd "$ICN_DIR"
        ICN_PASSPHRASE=demo123 \
            ICN_GATEWAY_JWT_SECRET="$JWT_SECRET" \
            ICN_CORS_ORIGINS="http://localhost:$UI_PORT,http://127.0.0.1:$UI_PORT,http://${GATEWAY_DISPLAY_HOST}:$UI_PORT" \
            ./target/release/icnd \
            --config "$RUNTIME_CONFIG" \
            --gateway-enable \
            --gateway-bind "$GATEWAY_HOST:$GATEWAY_PORT" \
            > /tmp/icnd-demo.log 2>&1
    ) &

    DAEMON_PID=$!

    echo "Daemon starting (PID: $DAEMON_PID)..."
    echo "Waiting for services to be ready..."

    # Wait for gateway to respond (max 45 seconds)
    for _ in {1..45}; do
        if curl -fsS "$GATEWAY/v1/health" >/dev/null 2>&1; then
            echo -e "${GREEN}✓${NC} Gateway ready!"
            break
        fi
        echo -n "."
        sleep 1
    done

    if ! curl -fsS "$GATEWAY/v1/health" >/dev/null 2>&1; then
        echo
        echo -e "${RED}✗${NC} Gateway failed to start"
        echo "Check logs: tail -f /tmp/icnd-demo.log"
        exit 1
    fi
fi

# Step 2: Get authentication token
echo
echo "========================================="
echo "Step 2: Getting authentication token"
echo "========================================="
echo

TOKEN=$(cd "$ICN_DIR" && ICN_PASSPHRASE=demo123 ./target/release/icnctl \
    -d "$DATA_DIR" \
    -e "$RPC_ENDPOINT" \
    auth token \
    --coop-id "$COOP_ID" \
    --scopes "coop:write,coop:read,ledger:read,ledger:write" \
    2>/dev/null | grep -oE 'eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+' | head -1 || true)

if [ -z "$TOKEN" ]; then
    echo -e "${YELLOW}⚠${NC} Could not auto-generate token"
    echo "You may need to initialize identity and/or create the cooperative first"
else
    echo -e "${GREEN}✓${NC} Token generated"
fi

CURRENT_DID=$(cd "$ICN_DIR" && ICN_PASSPHRASE=demo123 ./target/release/icnctl -d "$DATA_DIR" id show 2>/dev/null | grep -oE 'did:icn:[A-Za-z0-9]+' | head -1 || true)
if [ -z "$CURRENT_DID" ]; then
    CURRENT_DID="$DEFAULT_DID"
fi

# Step 3: Get cooperative info
echo
echo "========================================="
echo "Step 3: Verifying cooperative"
echo "========================================="
echo

COOP_NAME="Rochester Tool Library"
if [ -n "$TOKEN" ]; then
    COOP_INFO=$(curl -s "$GATEWAY/v1/coops/$COOP_ID" \
        -H "Authorization: Bearer $TOKEN" 2>/dev/null || echo "{}")

    if echo "$COOP_INFO" | grep -q "\"id\":\"$COOP_ID\""; then
        echo -e "${GREEN}✓${NC} Cooperative '$COOP_ID' verified"
        COOP_NAME=$(echo "$COOP_INFO" | grep -o '"name":"[^"]*"' | cut -d'"' -f4)
        echo "Name: $COOP_NAME"
    else
        echo -e "${YELLOW}⚠${NC} Cooperative '$COOP_ID' not found yet"
        echo "Create it first, then rerun the script for fully automated login"
    fi
else
    echo -e "${YELLOW}⚠${NC} Skipping cooperative verification (no token)"
fi

# Step 4: Start UI
echo
echo "========================================="
echo "Step 4: Starting Pilot UI"
echo "========================================="
echo

cd "$UI_DIR"

# Check if UI is already running
if lsof -Pi :"$UI_PORT" -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo -e "${YELLOW}⚠${NC} Port $UI_PORT already in use"
    UI_PID=$(lsof -Pi :"$UI_PORT" -sTCP:LISTEN -t | head -1)
    echo "Using existing UI server (PID: $UI_PID)"
else
    echo "Starting UI server on port $UI_PORT..."
    python3 -m http.server "$UI_PORT" > /tmp/pilot-ui-demo.log 2>&1 &
    UI_PID=$!

    sleep 2

    if ps -p "$UI_PID" >/dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} UI server started (PID: $UI_PID)"
    else
        echo -e "${RED}✗${NC} UI server failed to start"
        exit 1
    fi
fi

# Step 5: Display information
echo
echo "========================================="
echo "DEMO IS READY"
echo "========================================="
echo
echo -e "${BLUE}Access Information:${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo "  Pilot UI:"
echo "     http://${GATEWAY_DISPLAY_HOST}:$UI_PORT"
echo
echo "  Gateway API:"
echo "     $GATEWAY"
echo
echo "  Cooperative:"
echo "     ID: $COOP_ID"
echo "     Name: ${COOP_NAME}"
echo
echo "  Login Credentials:"
echo "     Gateway URL: $GATEWAY"
echo "     Coop ID: $COOP_ID"
echo "     DID: $CURRENT_DID"

if [ -n "$TOKEN" ]; then
    echo "     Token: ${TOKEN:0:40}..."
    echo
    echo "  Full Token (copy this):"
    echo "     $TOKEN"
else
    echo "     Token: Not generated automatically"
fi

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo -e "${BLUE}Quick Start:${NC}"
echo "  1. Open your browser to: http://${GATEWAY_DISPLAY_HOST}:$UI_PORT"
echo "  2. Click 'Sign In'"
echo "  3. Fill in the form with the credentials above"
echo "  4. Copy/paste the token"
echo "  5. Click 'Sign In' to access the dashboard"
echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo -e "${BLUE}Logs:${NC}"
echo "  Daemon:  tail -f /tmp/icnd-demo.log"
echo "  UI:      tail -f /tmp/pilot-ui-demo.log"
echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo -e "${GREEN}Press Ctrl+C to stop the demo${NC}"
echo

# Keep running
wait
