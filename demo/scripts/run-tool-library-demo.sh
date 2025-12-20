#!/bin/bash
# ICN Tool Library Demo - Automated Run Script
# This script starts everything needed for the demo and keeps it running

set -e

# Configuration
DEMO_NAME="Rochester Tool Library Demo"
GATEWAY="http://localhost:8080"
UI_PORT=3000
COOP_ID="rochester-tool-library"
DATA_DIR="/home/matt/icn-demo-test/data"
RPC_ENDPOINT="127.0.0.1:15602"
ICN_DIR="/home/matt/projects/icn"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

# PIDs for cleanup
DAEMON_PID=""
UI_PID=""

# Cleanup function
cleanup() {
    echo
    echo "========================================="
    echo "Cleaning up..."
    echo "========================================="
    
    if [ -n "$UI_PID" ]; then
        echo "Stopping UI (PID: $UI_PID)..."
        kill $UI_PID 2>/dev/null || true
    fi
    
    if [ -n "$DAEMON_PID" ]; then
        echo "Stopping daemon (PID: $DAEMON_PID)..."
        kill $DAEMON_PID 2>/dev/null || true
    fi
    
    echo "Demo stopped."
    exit 0
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
echo "Press Enter to continue or Ctrl+C to cancel..."
read

# Step 1: Check if daemon is already running
echo
echo "========================================="
echo "Step 1: Checking daemon status"
echo "========================================="
echo

if curl -s "$GATEWAY/v1/health" > /dev/null 2>&1; then
    echo -e "${YELLOW}⚠${NC} Daemon appears to be already running"
    echo "Using existing daemon at $GATEWAY"
    DAEMON_PID="existing"
else
    echo "Starting ICN daemon..."
    cd "$ICN_DIR/icn"
    
    # Start daemon in background
    ./target/release/icnd \
        -d "$DATA_DIR" \
        -e "$RPC_ENDPOINT" \
        --gateway-enable \
        --gateway-bind "127.0.0.1:8080" \
        --gateway-jwt-secret "demo-secret-key-change-in-production" \
        > /tmp/icnd-demo.log 2>&1 &
    
    DAEMON_PID=$!
    
    echo "Daemon starting (PID: $DAEMON_PID)..."
    echo "Waiting for services to be ready..."
    
    # Wait for gateway to respond (max 30 seconds)
    for i in {1..30}; do
        if curl -s "$GATEWAY/v1/health" > /dev/null 2>&1; then
            echo -e "${GREEN}✓${NC} Gateway ready!"
            break
        fi
        echo -n "."
        sleep 1
    done
    
    if ! curl -s "$GATEWAY/v1/health" > /dev/null 2>&1; then
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

cd "$ICN_DIR/icn"

echo "Generating JWT token..."
TOKEN=$(./target/release/icnctl \
    -d "$DATA_DIR" \
    -e "$RPC_ENDPOINT" \
    auth token \
    --coop-id "$COOP_ID" \
    --scopes "coop:write,coop:read,ledger:read,ledger:write" \
    --passphrase demo123 2>&1 | grep -v "Enter passphrase" | tr -d '\n' || true)

if [ -z "$TOKEN" ]; then
    echo -e "${YELLOW}⚠${NC} Could not auto-generate token"
    echo "You'll need to get it manually when you login"
else
    echo -e "${GREEN}✓${NC} Token generated"
fi

# Step 3: Get cooperative info
echo
echo "========================================="
echo "Step 3: Verifying cooperative"
echo "========================================="
echo

if [ -n "$TOKEN" ]; then
    COOP_INFO=$(curl -s "$GATEWAY/v1/coops/$COOP_ID" \
        -H "Authorization: Bearer $TOKEN" 2>/dev/null || echo "{}")
    
    if echo "$COOP_INFO" | grep -q "\"id\":\"$COOP_ID\""; then
        echo -e "${GREEN}✓${NC} Cooperative '$COOP_ID' verified"
        COOP_NAME=$(echo "$COOP_INFO" | grep -o '"name":"[^"]*"' | cut -d'"' -f4)
        echo "Name: $COOP_NAME"
    else
        echo -e "${YELLOW}⚠${NC} Could not verify cooperative"
    fi
else
    echo -e "${YELLOW}⚠${NC} Skipping verification (no token)"
fi

# Step 4: Start UI
echo
echo "========================================="
echo "Step 4: Starting Pilot UI"
echo "========================================="
echo

cd "$ICN_DIR/web/pilot-ui"

# Check if UI is already running
if lsof -Pi :$UI_PORT -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo -e "${YELLOW}⚠${NC} Port $UI_PORT already in use"
    UI_PID=$(lsof -Pi :$UI_PORT -sTCP:LISTEN -t)
    echo "Using existing UI server"
else
    echo "Starting UI server on port $UI_PORT..."
    python3 -m http.server $UI_PORT > /tmp/pilot-ui-demo.log 2>&1 &
    UI_PID=$!
    
    sleep 2
    
    if ps -p $UI_PID > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC} UI server started (PID: $UI_PID)"
    else
        echo -e "${RED}✗${NC} UI server failed to start"
        exit 1
    fi
fi

# Step 5: Display information
echo
echo "========================================="
echo "🎉 DEMO IS READY!"
echo "========================================="
echo
echo -e "${BLUE}Access Information:${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo "  📱 Pilot UI:"
echo "     http://localhost:$UI_PORT"
echo
echo "  🔌 Gateway API:"
echo "     $GATEWAY"
echo
echo "  🏛️  Cooperative:"
echo "     ID: $COOP_ID"
echo "     Name: ${COOP_NAME:-Rochester Tool Library}"
echo
echo "  👤 Login Credentials:"
echo "     Gateway URL: $GATEWAY"
echo "     Coop ID: $COOP_ID"
echo "     DID: did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh"

if [ -n "$TOKEN" ]; then
    echo "     Token: ${TOKEN:0:40}..."
    echo
    echo "  🔑 Full Token (copy this):"
    echo "     $TOKEN"
fi

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo -e "${BLUE}Quick Start:${NC}"
echo "  1. Open your browser to: http://localhost:$UI_PORT"
echo "  2. Click 'Sign In'"
echo "  3. Fill in the form with the credentials above"
echo "  4. Copy/paste the token"
echo "  5. Click 'Sign In' to access the dashboard"
echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo -e "${BLUE}Demo Scenario:${NC}"
echo "  • View your balance (currently 0.0 hours)"
echo "  • Check transaction history"
echo "  • Log hours for other members"
echo "  • View member directory"
echo "  • Test governance features"
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
