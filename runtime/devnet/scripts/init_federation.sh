#!/bin/bash
# Federation initialization script
# This script initializes a federation with 3 nodes (genesis + 2 validators)

set -e

# Configuration
CONFIG_DIR="$(dirname "$(dirname "$0")")/config"
DATA_DIR="$(dirname "$(dirname "$0")")/data"
FEDERATION_CONFIG="$CONFIG_DIR/federation_icn.toml"
FEDERATION_NAME="ICN Test Federation"
GENESIS_NODE="did:icn:federation:genesis"
NODE1="did:icn:federation:node1"
NODE2="did:icn:federation:node2"
NODE_LIST="$GENESIS_NODE,$NODE1,$NODE2"

# Create required directories
mkdir -p "$DATA_DIR/genesis" "$DATA_DIR/node1" "$DATA_DIR/node2"

echo "=== ICN Federation Initialization ==="
echo "Using configuration from: $FEDERATION_CONFIG"

# Step 1: Initialize federation with the genesis node
echo "Step 1: Initializing federation with genesis node..."
icn-runtime federation init \
  --name "$FEDERATION_NAME" \
  --nodes "$NODE_LIST" \
  --genesis-node "$GENESIS_NODE" \
  --config-file "$FEDERATION_CONFIG" \
  --output-dir "$DATA_DIR/federation" \
  --storage-dir "$DATA_DIR/genesis/storage"

if [ $? -ne 0 ]; then
  echo "Error: Failed to initialize federation"
  exit 1
fi

# Step 2: Copy federation artifacts to each node
echo "Step 2: Copying federation artifacts to each node..."
cp -r "$DATA_DIR/federation/"* "$DATA_DIR/genesis/"
cp -r "$DATA_DIR/federation/"* "$DATA_DIR/node1/"
cp -r "$DATA_DIR/federation/"* "$DATA_DIR/node2/"

# Step 3: Start the genesis node in the background
echo "Step 3: Starting genesis node..."
GENESIS_PID=0
if command -v tmux &> /dev/null; then
  # Use tmux if available
  tmux new-session -d -s "icn-genesis" "icn-runtime daemon --federation --node-id '$GENESIS_NODE' --storage-dir '$DATA_DIR/genesis/storage' --trust-bundle '$DATA_DIR/federation/trust_bundle.json'"
  echo "Genesis node started in tmux session 'icn-genesis'"
else
  # Otherwise use background process
  nohup icn-runtime daemon --federation --node-id "$GENESIS_NODE" --storage-dir "$DATA_DIR/genesis/storage" --trust-bundle "$DATA_DIR/federation/trust_bundle.json" > "$DATA_DIR/genesis/node.log" 2>&1 &
  GENESIS_PID=$!
  echo "Genesis node started with PID $GENESIS_PID"
fi

# Wait for genesis node to start
echo "Waiting for genesis node to start..."
sleep 5

# Step 4: Start node1 and node2
echo "Step 4: Starting additional federation nodes..."
NODE1_PID=0
NODE2_PID=0

if command -v tmux &> /dev/null; then
  # Use tmux if available
  tmux new-session -d -s "icn-node1" "icn-runtime daemon --federation --node-id '$NODE1' --storage-dir '$DATA_DIR/node1/storage' --trust-bundle '$DATA_DIR/federation/trust_bundle.json' --join-federation 'http://localhost:9000'"
  echo "Node 1 started in tmux session 'icn-node1'"
  
  tmux new-session -d -s "icn-node2" "icn-runtime daemon --federation --node-id '$NODE2' --storage-dir '$DATA_DIR/node2/storage' --trust-bundle '$DATA_DIR/federation/trust_bundle.json' --join-federation 'http://localhost:9000'"
  echo "Node 2 started in tmux session 'icn-node2'"
else
  # Otherwise use background processes
  nohup icn-runtime daemon --federation --node-id "$NODE1" --storage-dir "$DATA_DIR/node1/storage" --trust-bundle "$DATA_DIR/federation/trust_bundle.json" --join-federation "http://localhost:9000" > "$DATA_DIR/node1/node.log" 2>&1 &
  NODE1_PID=$!
  echo "Node 1 started with PID $NODE1_PID"
  
  nohup icn-runtime daemon --federation --node-id "$NODE2" --storage-dir "$DATA_DIR/node2/storage" --trust-bundle "$DATA_DIR/federation/trust_bundle.json" --join-federation "http://localhost:9000" > "$DATA_DIR/node2/node.log" 2>&1 &
  NODE2_PID=$!
  echo "Node 2 started with PID $NODE2_PID"
fi

# Step 5: Wait for nodes to connect and synchronize
echo "Step 5: Waiting for nodes to connect and synchronize..."
sleep 10

# Step 6: Check federation status
echo "Step 6: Checking federation status..."
FEDERATION_ID=$(grep "did" "$DATA_DIR/federation/federation_config.toml" | head -1 | cut -d'"' -f2)
icn-runtime federation status --federation "$FEDERATION_ID" --storage-dir "$DATA_DIR/genesis/storage"

echo "=== Federation Initialization Complete ==="
echo "Federation ID: $FEDERATION_ID"
echo ""
echo "To check federation status:"
echo "  icn-runtime federation status --federation $FEDERATION_ID"
echo ""
echo "To verify federation integrity:"
echo "  icn-runtime federation verify --federation $FEDERATION_ID"
echo ""
echo "To stop the federation:"
if [ $GENESIS_PID -ne 0 ]; then
  echo "  kill $GENESIS_PID $NODE1_PID $NODE2_PID"
else
  echo "  tmux kill-session -t icn-genesis"
  echo "  tmux kill-session -t icn-node1"
  echo "  tmux kill-session -t icn-node2"
fi

exit 0 