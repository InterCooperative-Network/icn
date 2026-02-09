#!/usr/bin/env bash
# ICN Devnet Node Entrypoint
# Initializes identity + config on first run, then starts the daemon.
set -euo pipefail

DATA_DIR="${ICN_DATA_DIR:-/data/node}"
CONFIG_FILE="$DATA_DIR/config.toml"
GATEWAY_PORT="${ICN_GATEWAY_PORT:-8000}"
P2P_PORT="${ICN_P2P_PORT:-9000}"
BOOTSTRAP_PEERS="${ICN_BOOTSTRAP_PEERS:-}"
NODE_NAME="${ICN_NODE_NAME:-node}"
KEYSTORE_PASSPHRASE="${ICN_KEYSTORE_PASSPHRASE:-devnet-insecure}"

# Create data dir
mkdir -p "$DATA_DIR"

# Initialize identity + config if not already done
if [ ! -f "$CONFIG_FILE" ]; then
  echo "[$NODE_NAME] First run — initializing identity and config..."
  echo "$KEYSTORE_PASSPHRASE" | icnd --init --data-dir "$DATA_DIR"
  echo "[$NODE_NAME] Identity initialized."
fi

# Override config for devnet: enable gateway, set ports, disable mDNS
# Use sed for simple config patching
if grep -q 'gateway_enabled' "$CONFIG_FILE" 2>/dev/null; then
  sed -i 's/gateway_enabled.*/gateway_enabled = true/' "$CONFIG_FILE"
else
  echo "" >> "$CONFIG_FILE"
  echo "[gateway]" >> "$CONFIG_FILE"
  echo "enabled = true" >> "$CONFIG_FILE"
  echo "bind_addr = \"0.0.0.0:${GATEWAY_PORT}\"" >> "$CONFIG_FILE"
fi

# Set JWT secret for gateway (deterministic for devnet — NOT for production)
export ICN_GATEWAY_JWT_SECRET="devnet-insecure-jwt-secret-32bytes!"

# Configure bootstrap peers
if [ -n "$BOOTSTRAP_PEERS" ]; then
  echo "[$NODE_NAME] Bootstrap peers: $BOOTSTRAP_PEERS"
fi

# Build bootstrap peer args
BOOTSTRAP_ARGS=""
IFS=',' read -ra PEERS <<< "$BOOTSTRAP_PEERS"
for peer in "${PEERS[@]}"; do
  if [ -n "$peer" ]; then
    BOOTSTRAP_ARGS="$BOOTSTRAP_ARGS --bootstrap-peer $peer"
  fi
done

echo "[$NODE_NAME] Starting ICN daemon..."
echo "  Data dir: $DATA_DIR"
echo "  Gateway:  0.0.0.0:$GATEWAY_PORT"
echo "  P2P:      0.0.0.0:$P2P_PORT"

# Start daemon with gateway enabled
# shellcheck disable=SC2086
exec icnd \
  --config "$CONFIG_FILE" \
  --gateway-enable \
  $BOOTSTRAP_ARGS
