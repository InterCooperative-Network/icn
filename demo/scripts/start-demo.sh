#!/usr/bin/env bash
# ICN Governance Demo — Cold Start Script
# Usage: ./start-demo.sh [data-dir]
#
# Builds the gateway, initializes a fresh node, starts the daemon,
# and opens the demo UI in a browser.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ICN_ROOT="$(cd "$SCRIPT_DIR/../../icn" && pwd)"
DATA_DIR="${1:-/tmp/icn-demo}"
BIND="0.0.0.0:8080"
JWT_SECRET="demo-secret-key-for-testing-only-32bytes"

echo "=== ICN Governance Demo ==="
echo "  Workspace: $ICN_ROOT"
echo "  Data dir:  $DATA_DIR"
echo ""

# 1. Build
echo "[1/4] Building icnd..."
cd "$ICN_ROOT"
cargo build -p icnd --quiet 2>&1
ICND="$ICN_ROOT/target/debug/icnd"
echo "  Built: $ICND"

# 2. Kill any existing instance
if pgrep -f "icnd.*--gateway-enable" >/dev/null 2>&1; then
  echo "[2/4] Stopping existing gateway..."
  pkill -f "icnd.*--gateway-enable" || true
  sleep 2
else
  echo "[2/4] No existing gateway found"
fi

# 3. Initialize and start
echo "[3/4] Initializing node..."
mkdir -p "$DATA_DIR"
if [ ! -f "$DATA_DIR/identity.age" ]; then
  ICN_KEYSTORE_PASSPHRASE=demo "$ICND" --data-dir "$DATA_DIR" --init 2>&1 | sed 's/^/  /'
fi

echo "[4/4] Starting gateway on port 8080..."
ICN_KEYSTORE_PASSPHRASE=demo \
ICN_GATEWAY_JWT_SECRET="$JWT_SECRET" \
nohup "$ICND" \
  --gateway-enable \
  --gateway-bind "$BIND" \
  --gateway-jwt-secret "$JWT_SECRET" \
  --data-dir "$DATA_DIR" \
  > "$DATA_DIR/gateway.log" 2>&1 &

GATEWAY_PID=$!
echo "  PID: $GATEWAY_PID"
echo "  Log: $DATA_DIR/gateway.log"

# Wait for health
echo ""
echo "Waiting for gateway..."
for i in $(seq 1 15); do
  if curl -sf http://localhost:8080/v1/health >/dev/null 2>&1; then
    echo "  Gateway is healthy!"
    echo ""
    echo "=== Demo Ready ==="
    echo "  API:     http://localhost:8080/v1/"
    echo "  Demo UI: http://localhost:8080/static/demo.html"
    echo "  Script:  python3 $SCRIPT_DIR/demo-governance.py"
    echo ""
    echo "  To stop: kill $GATEWAY_PID"
    exit 0
  fi
  sleep 1
done

echo "  ERROR: Gateway did not start within 15 seconds"
echo "  Check $DATA_DIR/gateway.log for details"
exit 1
