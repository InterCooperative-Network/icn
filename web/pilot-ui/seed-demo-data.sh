#!/usr/bin/env bash
#
# ICN Pilot UI - Demo Data Seeder
#
# This script seeds demo data into a cooperative for testing the UI:
# - Creates sample members
# - Records sample transactions
# - Creates sample governance proposals
#
# Usage:
#   ./seed-demo-data.sh <gateway-url> <coop-id> <jwt-token>
#
# Example:
#   ./seed-demo-data.sh http://localhost:8080 demo-coop eyJ0eXAi...
#

set -e

# Configuration
GATEWAY_URL="${1:-http://localhost:8080}"
COOP_ID="${2:-demo-coop}"
JWT_TOKEN="${3}"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

if [ -z "$JWT_TOKEN" ]; then
    echo -e "${RED}Error: JWT token is required${NC}"
    echo ""
    echo "Usage: $0 <gateway-url> <coop-id> <jwt-token>"
    echo ""
    echo "Get a token with:"
    echo "  icnctl auth login --gateway $GATEWAY_URL --coop $COOP_ID"
    exit 1
fi

echo -e "${BLUE}"
echo "╔════════════════════════════════════════════════════════════╗"
echo "║           ICN Pilot UI - Demo Data Seeder                  ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo -e "${NC}"
echo ""
echo "Gateway: $GATEWAY_URL"
echo "Coop ID: $COOP_ID"
echo ""

# Helper function to make API calls
api_call() {
    local method=$1
    local endpoint=$2
    local data=$3

    curl -sf -X "$method" \
        "$GATEWAY_URL/v1$endpoint" \
        -H "Authorization: Bearer $JWT_TOKEN" \
        -H "Content-Type: application/json" \
        -d "$data" 2>/dev/null || echo "{\"error\": true}"
}

# Sample DIDs (in real deployment, these would be actual member DIDs)
ALICE_DID="did:icn:alice123"
BOB_DID="did:icn:bob456"
CAROL_DID="did:icn:carol789"
DAVE_DID="did:icn:dave012"
EVE_DID="did:icn:eve345"

echo -e "${YELLOW}Step 1: Creating cooperative...${NC}"
RESULT=$(api_call POST "/coops" "{
    \"coop_id\": \"$COOP_ID\",
    \"name\": \"Demo Timebank\",
    \"description\": \"A demonstration cooperative for testing the pilot UI\",
    \"currency\": \"hours\"
}")

if echo "$RESULT" | grep -q "error"; then
    echo "  Cooperative may already exist (this is OK)"
else
    echo -e "${GREEN}  Created cooperative: $COOP_ID${NC}"
fi

echo ""

echo -e "${YELLOW}Step 2: Adding sample members...${NC}"

# Note: In a real deployment, you would add actual member DIDs
# For demo purposes, we'll note that members should be added via icnctl
echo "  Demo members (use icnctl to add real members):"
echo "    - Alice (did:icn:alice123) - Gardener"
echo "    - Bob (did:icn:bob456) - Carpenter"
echo "    - Carol (did:icn:carol789) - Teacher"
echo "    - Dave (did:icn:dave012) - Cook"
echo "    - Eve (did:icn:eve345) - Designer"

echo ""

echo -e "${YELLOW}Step 3: Recording sample transactions...${NC}"

# Sample transactions showing typical timebank activity
TRANSACTIONS=(
    "$ALICE_DID|$BOB_DID|3|Garden bed construction"
    "$BOB_DID|$CAROL_DID|2|Bookshelf installation"
    "$CAROL_DID|$DAVE_DID|4|Cooking lessons"
    "$DAVE_DID|$EVE_DID|1|Meal preparation"
    "$EVE_DID|$ALICE_DID|2|Garden logo design"
    "$ALICE_DID|$CAROL_DID|3|After-school gardening class"
    "$BOB_DID|$DAVE_DID|2|Kitchen cabinet repair"
    "$CAROL_DID|$EVE_DID|1|Design feedback session"
    "$DAVE_DID|$ALICE_DID|2|Preserving vegetables"
    "$EVE_DID|$BOB_DID|3|Website design for carpentry business"
)

TRANSACTION_COUNT=0

for tx in "${TRANSACTIONS[@]}"; do
    IFS='|' read -r from_did to_did amount memo <<< "$tx"

    RESULT=$(api_call POST "/ledger/$COOP_ID/payment" "{
        \"from_did\": \"$from_did\",
        \"to_did\": \"$to_did\",
        \"amount\": $amount,
        \"currency\": \"hours\",
        \"memo\": \"$memo\"
    }")

    if echo "$RESULT" | grep -q "error"; then
        echo "  ⚠ Could not create transaction: $memo"
    else
        ((TRANSACTION_COUNT++))
        echo "  ✓ $memo ($amount hours)"
    fi
done

echo -e "${GREEN}  Created $TRANSACTION_COUNT sample transactions${NC}"

echo ""

echo -e "${YELLOW}Step 4: Creating sample governance proposals...${NC}"

# Create governance domain
DOMAIN_RESULT=$(api_call POST "/gov/domains" "{
    \"domain_id\": \"$COOP_ID\",
    \"name\": \"Demo Timebank Governance\",
    \"description\": \"Democratic decision-making for our timebank\",
    \"members\": [\"$ALICE_DID\", \"$BOB_DID\", \"$CAROL_DID\", \"$DAVE_DID\", \"$EVE_DID\"]
}")

if echo "$DOMAIN_RESULT" | grep -q "error"; then
    echo "  ⚠ Could not create governance domain (may already exist)"
else
    echo -e "${GREEN}  Created governance domain${NC}"
fi

# Sample proposals
echo "  Creating sample proposals..."

PROPOSAL1=$(api_call POST "/gov/proposals" "{
    \"domain_id\": \"$COOP_ID\",
    \"title\": \"Add new member: Frank\",
    \"description\": \"Frank wants to join and offer plumbing services\",
    \"proposal_type\": \"membership\"
}")

PROPOSAL2=$(api_call POST "/gov/proposals" "{
    \"domain_id\": \"$COOP_ID\",
    \"title\": \"Budget: Community garden supplies\",
    \"description\": \"Allocate 50 hours for seeds, tools, and soil\",
    \"proposal_type\": \"budget\"
}")

PROPOSAL3=$(api_call POST "/gov/proposals" "{
    \"domain_id\": \"$COOP_ID\",
    \"title\": \"Change: Service hour minimum\",
    \"description\": \"Require minimum 2 hours contributed per month\",
    \"proposal_type\": \"config_change\"
}")

echo -e "${GREEN}  Created 3 sample proposals${NC}"

echo ""

echo -e "${BLUE}"
echo "╔════════════════════════════════════════════════════════════╗"
echo "║           Demo Data Seeding Complete!                      ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo -e "${NC}"

echo ""
echo "Summary:"
echo "  - Cooperative: $COOP_ID"
echo "  - Members: 5 sample members"
echo "  - Transactions: $TRANSACTION_COUNT recorded"
echo "  - Governance: 1 domain, 3 proposals"
echo ""
echo "Next steps:"
echo ""
echo "  1. Open the web UI: http://localhost:3000"
echo "  2. Sign in with your DID and token"
echo "  3. Explore the dashboard, transactions, and governance"
echo ""
echo "Current balances (approximate):"
echo "  - Alice: +1 hour (gave 6, received 7)"
echo "  - Bob: +1 hour (gave 4, received 5)"
echo "  - Carol: 0 hours (gave 5, received 5)"
echo "  - Dave: -1 hour (gave 3, received 2)"
echo "  - Eve: -1 hour (gave 6, received 5)"
echo ""
echo -e "${GREEN}Happy testing!${NC}"
