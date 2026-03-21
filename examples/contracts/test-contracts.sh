#!/bin/bash
# Test script for CCL contract examples
# This script demonstrates how to deploy and test the example contracts

set -e

ICNCTL="../../icn/target/debug/icnctl"
ENDPOINT="[::1]:5601"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}ICN Contract Testing Script${NC}"
echo "========================================="
echo ""

# Check if icnctl exists
if [ ! -f "$ICNCTL" ]; then
    echo -e "${YELLOW}Building icnctl...${NC}"
    cd ../../icn
    cargo build --bin icnctl
    cd -
fi

# Check if daemon is running
if ! curl -s "http://$ENDPOINT" > /dev/null 2>&1; then
    echo -e "${YELLOW}Warning: icnd does not appear to be running on $ENDPOINT${NC}"
    echo "Start the daemon with: icnd --config config/icn-alpha.toml"
    echo ""
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Test Echo Contract
echo -e "${BLUE}1. Testing Echo Contract${NC}"
echo "-------------------------------------------"

echo "Deploying echo.json..."
ECHO_HASH=$($ICNCTL contract deploy echo.json 2>&1 | grep "Code Hash:" | awk '{print $3}')

if [ -z "$ECHO_HASH" ]; then
    echo -e "${YELLOW}Failed to deploy echo contract${NC}"
else
    echo -e "${GREEN}✓ Deployed with hash: $ECHO_HASH${NC}"
    echo ""

    echo "Calling echo rule..."
    $ICNCTL contract call "$ECHO_HASH" echo "did:icn:test" \
        --args '{"message":"Hello from ICN!"}'
    echo ""

    echo "Calling add rule..."
    $ICNCTL contract call "$ECHO_HASH" add "did:icn:test" \
        --args '{"a":42,"b":58}'
    echo ""
fi

# Test TimeBank Contract
echo -e "${BLUE}2. Testing TimeBank Contract${NC}"
echo "-------------------------------------------"

echo "Deploying timebank.json..."
TIMEBANK_HASH=$($ICNCTL contract deploy timebank.json 2>&1 | grep "Code Hash:" | awk '{print $3}')

if [ -z "$TIMEBANK_HASH" ]; then
    echo -e "${YELLOW}Failed to deploy timebank contract${NC}"
else
    echo -e "${GREEN}✓ Deployed with hash: $TIMEBANK_HASH${NC}"
    echo ""

    echo "Recording service (Alice helps Bob for 5 hours)..."
    $ICNCTL contract call "$TIMEBANK_HASH" record_service "did:icn:alice" \
        --args '{"recipient":"did:icn:bob","hours":5}'
    echo ""

    echo "Checking timebank stats..."
    $ICNCTL contract call "$TIMEBANK_HASH" get_stats "did:icn:alice"
    echo ""
fi

echo -e "${GREEN}Test complete!${NC}"
echo ""
echo "Contract hashes for future reference:"
[ -n "$ECHO_HASH" ] && echo "  Echo:     $ECHO_HASH"
[ -n "$TIMEBANK_HASH" ] && echo "  TimeBank: $TIMEBANK_HASH"
