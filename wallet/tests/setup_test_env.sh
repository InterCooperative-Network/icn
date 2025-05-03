#!/bin/bash
set -e

# Create test directories
mkdir -p test-wallet-data/identities
mkdir -p test-wallet-data/queue
mkdir -p test-wallet-data/bundles

# Check if test instances are running
echo "Checking if AgoraNet test instance is running..."
if ! curl -s http://localhost:8080/api/health > /dev/null; then
    echo "AgoraNet test instance is not running. Starting mock AgoraNet..."
    node tests/mock_agoranet.js &
    AGORANET_PID=$!
    echo "Mock AgoraNet started with PID: $AGORANET_PID"
    # Give it time to start
    sleep 2
fi

# Create a test identity
echo "Creating test identity..."
IDENTITY_JSON=$(cargo run --bin icn-wallet-cli -- create --scope personal -m '{"name":"Test User"}')
IDENTITY_ID=$(echo "$IDENTITY_JSON" | grep "ID:" | cut -d' ' -f2)
echo "Created test identity: $IDENTITY_ID"

echo "Test environment setup complete."
echo "To run tests, use: cargo test -- --nocapture"
echo "To run the API server: cargo run --bin icn-wallet-cli -- serve --agoranet-url http://localhost:8080/api"
echo ""
echo "IDENTITY_ID=$IDENTITY_ID" > test-wallet-data/test_env.sh 