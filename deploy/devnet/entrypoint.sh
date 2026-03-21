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
GENESIS_FILE="${ICN_GENESIS_FILE:-}"

# Determine network_id: read from shared genesis file if present, else fall back to NODE_NAME
NETWORK_ID="$NODE_NAME"
if [ -n "$GENESIS_FILE" ] && [ -f "$GENESIS_FILE" ]; then
  _gid=$(python3 -c "import json; d=json.load(open('$GENESIS_FILE')); print(d.get('network_id',''))" 2>/dev/null || echo "")
  if [ -n "$_gid" ]; then
    NETWORK_ID="$_gid"
  fi
  echo "[$NODE_NAME] Genesis file: $GENESIS_FILE (network_id=$NETWORK_ID)"
fi

# Create data dir
mkdir -p "$DATA_DIR"

# Initialize identity + config if not already done
if [ ! -f "$CONFIG_FILE" ]; then
  echo "[$NODE_NAME] First run — initializing identity and config..."
  echo "$KEYSTORE_PASSPHRASE" | icnd --init --data-dir "$DATA_DIR" --node-name "$NETWORK_ID"
  echo "[$NODE_NAME] Identity initialized."
fi

# Override config for devnet: enable gateway, set ports.
# Patch in-place if [gateway] section already exists (icnd --init writes one);
# append a new section only if it's absent.
if grep -q '^\[gateway\]' "$CONFIG_FILE" 2>/dev/null; then
  # Section already there — patch enabled and bind_addr within that section.
  # sed range: from [gateway] up to (not including) the next section header or EOF.
  sed -i "/^\[gateway\]/,/^\[/{
    s|^enabled *=.*|enabled = true|
    s|^bind_addr *=.*|bind_addr = \"0.0.0.0:${GATEWAY_PORT}\"|
  }" "$CONFIG_FILE"
  # If bind_addr was absent in the section, append it after the [gateway] line.
  if ! grep -A20 '^\[gateway\]' "$CONFIG_FILE" | grep -q '^bind_addr'; then
    sed -i "/^\[gateway\]/a bind_addr = \"0.0.0.0:${GATEWAY_PORT}\"" "$CONFIG_FILE"
  fi
else
  printf '\n[gateway]\nenabled = true\nbind_addr = "0.0.0.0:%s"\n' "$GATEWAY_PORT" >> "$CONFIG_FILE"
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
