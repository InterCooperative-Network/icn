#!/bin/bash
# Generate a test token for ICN beta testing
# Usage: ./generate-test-token.sh [gateway-url] [coop-id]

set -e

GATEWAY_URL="${1:-http://10.8.10.40:30080}"
COOP_ID="${2:-test-coop}"

# Find icnctl
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ICNCTL="$SCRIPT_DIR/../icn/target/release/icnctl"

if [ ! -f "$ICNCTL" ]; then
    ICNCTL="$SCRIPT_DIR/../icn/target/debug/icnctl"
fi

if [ ! -f "$ICNCTL" ]; then
    echo "Error: icnctl not found. Run 'cargo build' in icn/ first."
    exit 1
fi

# Check for passphrase
if [ -z "$ICN_PASSPHRASE" ]; then
    echo "ICN_PASSPHRASE not set. Enter your keystore passphrase:"
    read -s PASS
    export ICN_PASSPHRASE="$PASS"
fi

# Get DID
DID=$($ICNCTL id show 2>/dev/null | grep "DID:" | awk '{print $2}')

if [ -z "$DID" ]; then
    echo "No identity found. Creating one..."
    $ICNCTL id init
    DID=$($ICNCTL id show 2>/dev/null | grep "DID:" | awk '{print $2}')
fi

echo ""
echo "=== ICN Beta Testing Credentials ==="
echo ""
echo "Gateway URL: $GATEWAY_URL"
echo "Cooperative: $COOP_ID"
echo "Your DID:    $DID"
echo ""

# Get token
echo "Requesting authentication token..."
TOKEN=$($ICNCTL auth token --gateway "$GATEWAY_URL" --coop-id "$COOP_ID" 2>/dev/null | grep "Token:" | awk '{print $2}')

if [ -z "$TOKEN" ]; then
    # Try to extract from verbose output
    TOKEN=$($ICNCTL auth token --gateway "$GATEWAY_URL" --coop-id "$COOP_ID" 2>&1 | tail -1)
fi

echo "Token:       $TOKEN"
echo ""
echo "=== Copy these values into the Pilot UI login form ==="
echo ""
echo "Web UI URL: http://10.8.10.40:30030"
