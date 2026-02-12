#!/bin/bash
# ICN Demo Reset Script
# Stops all services and cleans demo data.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ICN_DIR="${REPO_ROOT}/icn"

DATA_DIR="${ICN_DEMO_DATA_DIR:-${REPO_ROOT}/.demo-data/tool-library}"
UI_PORT="${ICN_DEMO_UI_PORT:-3000}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo "========================================="
echo "ICN Demo Reset"
echo "========================================="
echo
echo "This will:"
echo "  1. Stop all running services"
echo "  2. Clean demo data"
echo "  3. Prepare for fresh demo run"
echo
echo "  Data dir: $DATA_DIR"
echo
echo -e "${YELLOW}WARNING: This will delete all demo data!${NC}"
echo
read -r -p "Continue? (y/N) " -n 1 REPLY
echo
if [[ ! "$REPLY" =~ ^[Yy]$ ]]; then
    echo "Cancelled."
    exit 0
fi

echo
echo "Step 1: Stopping services..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Stop UI server
UI_PIDS=$(lsof -Pi :"$UI_PORT" -sTCP:LISTEN -t 2>/dev/null || true)
if [ -n "$UI_PIDS" ]; then
    for pid in $UI_PIDS; do
        echo "Stopping UI server (PID: $pid)..."
        kill "$pid" 2>/dev/null || true
    done
    sleep 1
    echo -e "${GREEN}✓${NC} UI server stopped"
else
    echo "UI server not running"
fi

# Stop daemon (check gateway port 8080)
DAEMON_PIDS=$(lsof -Pi :8080 -sTCP:LISTEN -t 2>/dev/null || true)
if [ -n "$DAEMON_PIDS" ]; then
    for pid in $DAEMON_PIDS; do
        echo "Stopping daemon (PID: $pid)..."
        kill "$pid" 2>/dev/null || true
    done
    sleep 2
    echo -e "${GREEN}✓${NC} Daemon stopped"
else
    echo "Daemon not running"
fi

# Check for any remaining icnd processes
ICND_PIDS=$(pgrep icnd || true)
if [ -n "$ICND_PIDS" ]; then
    echo -e "${YELLOW}⚠${NC} Found additional icnd processes:"
    for pid in $ICND_PIDS; do
        ps -p "$pid" -o pid,cmd | grep -v PID
    done
    echo
    read -r -p "Kill these too? (y/N) " -n 1 REPLY
    echo
    if [[ "$REPLY" =~ ^[Yy]$ ]]; then
        for pid in $ICND_PIDS; do
            kill "$pid" 2>/dev/null || true
        done
        sleep 1
        echo -e "${GREEN}✓${NC} Additional processes stopped"
    fi
fi

echo
echo "Step 2: Cleaning data..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ -d "$DATA_DIR" ]; then
    echo "Removing $DATA_DIR..."
    rm -rf "$DATA_DIR"
    echo -e "${GREEN}✓${NC} Data directory removed"
else
    echo "Data directory doesn't exist"
fi

# Clean logs
if [ -f /tmp/icnd-demo.log ]; then
    rm /tmp/icnd-demo.log
    echo -e "${GREEN}✓${NC} Daemon log cleaned"
fi

if [ -f /tmp/pilot-ui-demo.log ]; then
    rm /tmp/pilot-ui-demo.log
    echo -e "${GREEN}✓${NC} UI log cleaned"
fi

echo
echo "Step 3: Recreating identity..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

mkdir -p "$DATA_DIR"
cd "$ICN_DIR"

echo "Creating fresh identity..."
echo "demo123" | ./target/release/icnctl \
    -d "$DATA_DIR" \
    id init \
    --passphrase-stdin > /tmp/icn-reset-init.log 2>&1

if [ $? -eq 0 ]; then
    NEW_DID=$(grep -oE 'did:icn:[a-zA-Z0-9]+' /tmp/icn-reset-init.log | head -1)
    echo -e "${GREEN}✓${NC} Identity created"
    echo "New DID: $NEW_DID"
    echo
    echo -e "${YELLOW}NOTE:${NC} DID has changed! Use this DID for demo login."
else
    echo -e "${RED}✗${NC} Failed to create identity"
    echo "Check /tmp/icn-reset-init.log for details"
fi

echo
echo "========================================="
echo "Reset Complete!"
echo "========================================="
echo
echo "Demo has been reset. To start fresh:"
echo "  ./demo/scripts/run-tool-library-demo.sh"
echo
