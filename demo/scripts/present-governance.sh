#!/usr/bin/env bash
# present-governance.sh — Interactive governance demo for live presentations
#
# Builds the gateway, starts it on a clean data directory, runs the
# demo script in presenter mode, and cleans up on exit.
#
# Usage: bash present-governance.sh [--port PORT] [data-dir]
#
# Options:
#   --port PORT   Gateway port (default: 8080). Use if 8080 is occupied
#                 by another service (e.g. a running devnet container).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ICN_ROOT="$(cd "$SCRIPT_DIR/../../icn" && pwd)"
JWT_SECRET="demo-secret-key-for-testing-only-32bytes"
GATEWAY_PID=""
PORT=8080
DATA_DIR=""

# Parse args
while [[ $# -gt 0 ]]; do
  case "$1" in
    --port) PORT="$2"; shift 2 ;;
    --port=*) PORT="${1#--port=}"; shift ;;
    *) DATA_DIR="$1"; shift ;;
  esac
done
DATA_DIR="${DATA_DIR:-/tmp/icn-demo}"
BIND="0.0.0.0:${PORT}"

cleanup() {
  if [ -n "$GATEWAY_PID" ] && kill -0 "$GATEWAY_PID" 2>/dev/null; then
    echo ""
    echo "  Stopping gateway (PID $GATEWAY_PID)..."
    kill "$GATEWAY_PID" 2>/dev/null || true
    wait "$GATEWAY_PID" 2>/dev/null || true
  fi
  echo "  Done."
}
trap cleanup EXIT INT TERM

echo ""
echo "  ┌──────────────────────────────────────────────────────┐"
echo "  │  ICN Governance Demo — Presenter Mode                │"
echo "  └──────────────────────────────────────────────────────┘"
echo ""

# Check for port conflict before doing anything else
if ss -tlnp 2>/dev/null | grep -q ":${PORT} " || \
   ss -tlnp 2>/dev/null | grep -q ":${PORT}$"; then
  OCCUPANT="$(ss -tlnp 2>/dev/null | grep ":${PORT}" | awk '{print $NF}' | head -1)"
  echo "  ERROR: Port ${PORT} is already in use."
  echo "  Occupant: ${OCCUPANT:-unknown process}"
  echo ""
  echo "  Options:"
  echo "    1. Stop the process holding port ${PORT}:"
  echo "         pkill -f icnd  (if another icnd is running)"
  echo "         docker stop icn-daemon  (if the devnet container is running)"
  echo "    2. Use a different port:"
  echo "         bash present-governance.sh --port 9080"
  echo ""
  exit 1
fi

# 1. Build (skip if binary is fresh)
ICND="$ICN_ROOT/target/debug/icnd"
if [ -f "$ICND" ] && [ "$ICND" -nt "$ICN_ROOT/Cargo.lock" ]; then
  echo "  [1/4] Binary is fresh — skipping build"
else
  echo "  [1/4] Building icnd..."
  cd "$ICN_ROOT"
  cargo build -p icnd --quiet 2>&1
fi
echo "  Binary: $ICND"

# 2. Kill any existing instance
if pgrep -f "icnd.*--gateway-enable" >/dev/null 2>&1; then
  echo "  [2/4] Stopping existing gateway..."
  pkill -f "icnd.*--gateway-enable" || true
  sleep 2
else
  echo "  [2/4] No existing gateway found"
fi

# 3. Initialize clean data directory
echo "  [3/4] Initializing node in $DATA_DIR..."
if [ -d "$DATA_DIR" ]; then
  rm -rf "$DATA_DIR"
fi
mkdir -p "$DATA_DIR"
ICN_KEYSTORE_PASSPHRASE=demo "$ICND" --data-dir "$DATA_DIR" --init >/dev/null 2>&1

# 4. Start gateway
echo "  [4/4] Starting gateway on port ${PORT}..."
ICN_KEYSTORE_PASSPHRASE=demo \
ICN_GATEWAY_JWT_SECRET="$JWT_SECRET" \
ICN_DEV_MODE=1 \
"$ICND" \
  --gateway-enable \
  --gateway-bind "$BIND" \
  --gateway-jwt-secret "$JWT_SECRET" \
  --data-dir "$DATA_DIR" \
  > "$DATA_DIR/gateway.log" 2>&1 &

GATEWAY_PID=$!

# Wait for health
echo ""
echo "  Waiting for gateway..."
for i in $(seq 1 15); do
  if curl -sf "http://localhost:${PORT}/v1/health" >/dev/null 2>&1; then
    echo "  Gateway is healthy!"
    echo ""
    echo "  ┌──────────────────────────────────────────────────────────┐"
    printf  "  │  Browser demo: http://localhost:%-4s/static/demo.html  │\n" "${PORT}"
    echo "  └──────────────────────────────────────────────────────────┘"
    echo ""

    # Run the demo script in presenter mode
    python3 "$SCRIPT_DIR/demo-governance.py" "http://localhost:${PORT}" --presenter

    echo ""
    echo "  Gateway is still running at http://localhost:${PORT}"
    echo "  Browser demo: http://localhost:${PORT}/static/demo.html"
    echo ""
    echo "  Press Ctrl+C to stop."
    echo ""

    # Keep running until Ctrl+C
    wait "$GATEWAY_PID" 2>/dev/null || true
    exit 0
  fi
  sleep 1
done

echo "  ERROR: Gateway did not start within 15 seconds"
echo "  Check $DATA_DIR/gateway.log for details"
exit 1
