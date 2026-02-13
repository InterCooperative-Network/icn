#!/bin/bash
# Load Sample Data into ICN Demo
# This script validates demo API access and shows member-loading next steps.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ICN_DIR="${REPO_ROOT}/icn"

GATEWAY_HOST="${ICN_DEMO_GATEWAY_HOST:-127.0.0.1}"
GATEWAY_PORT="${ICN_DEMO_GATEWAY_PORT:-8080}"
GATEWAY="http://${GATEWAY_HOST}:${GATEWAY_PORT}"
COOP_ID="${ICN_DEMO_COOP_ID:-rochester-tool-library}"
DATA_DIR="${ICN_DEMO_DATA_DIR:-${REPO_ROOT}/.demo-data/tool-library}"
RPC_ENDPOINT="${ICN_DEMO_RPC_ENDPOINT:-127.0.0.1:15602}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo "========================================="
echo "ICN Demo - Load Sample Data"
echo "========================================="
echo

# Check if gateway is running
echo "Checking gateway..."
if ! curl -fsS "$GATEWAY/v1/health" >/dev/null 2>&1; then
    echo -e "${RED}✗${NC} Gateway not responding at $GATEWAY"
    echo "Start demo first: ./demo/scripts/run-tool-library-demo.sh"
    exit 1
fi
echo -e "${GREEN}✓${NC} Gateway is running"
echo

# Get JWT token
echo "Getting JWT token..."
TOKEN=$(cd "$ICN_DIR" && ICN_PASSPHRASE=demo123 ./target/release/icnctl \
  -d "$DATA_DIR" \
  -e "$RPC_ENDPOINT" \
  auth token \
  --coop-id "$COOP_ID" \
  --scopes "coop:write,coop:read,ledger:read,ledger:write" \
  2>/dev/null | grep -oE 'eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+' | head -1 || true)

if [ -z "$TOKEN" ]; then
    echo -e "${RED}✗${NC} Failed to get token"
    echo "Try manually:"
    echo "  cd $ICN_DIR"
    echo "  ICN_PASSPHRASE=demo123 ./target/release/icnctl -d $DATA_DIR -e $RPC_ENDPOINT auth token --coop-id $COOP_ID --scopes \"coop:write,coop:read,ledger:read,ledger:write\""
    exit 1
fi
echo -e "${GREEN}✓${NC} Got JWT token"
echo

# Load member data
MEMBERS_FILE="$REPO_ROOT/demo/data/tool-library-members.json"

echo "Loading members from $MEMBERS_FILE..."

CURRENT_DID=$(cd "$ICN_DIR" && ICN_PASSPHRASE=demo123 ./target/release/icnctl -d "$DATA_DIR" id show 2>/dev/null | grep -oE 'did:icn:[A-Za-z0-9]+' | head -1 || true)
if [ -z "$CURRENT_DID" ]; then
    CURRENT_DID="did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh"
fi

echo "========================================="
echo "What this script can validate now"
echo "========================================="
echo

echo "1. Verify cooperative exists:"
COOP_INFO=$(curl -s "$GATEWAY/v1/coops/$COOP_ID" -H "Authorization: Bearer $TOKEN")

if echo "$COOP_INFO" | grep -q "\"id\":\"$COOP_ID\""; then
    echo -e "${GREEN}✓${NC} Cooperative '$COOP_ID' exists"
    if command -v jq >/dev/null 2>&1; then
        echo "$COOP_INFO" | jq .
    else
        echo "$COOP_INFO"
    fi
else
    echo -e "${YELLOW}⚠${NC} Cooperative response: $COOP_INFO"
fi

echo
echo "2. Check current identity balance:"
BALANCE=$(curl -s "$GATEWAY/v1/ledger/$COOP_ID/balance/$CURRENT_DID" -H "Authorization: Bearer $TOKEN")

echo -e "${GREEN}✓${NC} Balance payload:"
if command -v jq >/dev/null 2>&1; then
    echo "$BALANCE" | jq .
else
    echo "$BALANCE"
fi

echo
echo "========================================="
echo "Next Steps for Full Sample Data"
echo "========================================="
echo
echo "The JSON files are present, but full member import still requires identity creation"
echo "for each sample member. Recommended flow:"
echo
echo "1. Start demo: ./demo/scripts/run-tool-library-demo.sh"
echo "2. Create identities (or invite users from UI)"
echo "3. Add members to coop via /v1/coops/{coop_id}/members"
echo "4. Create historical payments via /v1/ledger/{coop_id}/payment"
echo
echo "Gateway: $GATEWAY"
echo "Cooperative: $COOP_ID"
echo "Token preview: $(echo "$TOKEN" | cut -c1-20)..."
echo
echo "========================================="
echo "Script complete"
echo "========================================="
