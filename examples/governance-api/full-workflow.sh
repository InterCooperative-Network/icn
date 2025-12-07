#!/bin/bash
set -e

# ICN Gateway Governance API - Full Workflow Example
#
# This script demonstrates a complete governance workflow:
# 1. Create a governance domain
# 2. Create a proposal
# 3. Open voting
# 4. Cast votes from multiple members
# 5. Close the proposal
# 6. View the outcome
#
# Prerequisites:
# - icn-gateway running on localhost:8080
# - jq installed for JSON processing

GATEWAY_URL="${GATEWAY_URL:-http://localhost:8080/v1}"
JWT_SECRET="${JWT_SECRET:-test-secret-key}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}==>${NC} $1"
}

success() {
    echo -e "${GREEN}✓${NC} $1"
}

error() {
    echo -e "${RED}✗${NC} $1"
    exit 1
}

# Check prerequisites
command -v jq >/dev/null 2>&1 || error "jq is required but not installed. Install with: sudo apt install jq"
command -v curl >/dev/null 2>&1 || error "curl is required but not installed."

# Check if gateway is running
log "Checking if gateway is running..."
if ! curl -sf "$GATEWAY_URL/health" > /dev/null 2>&1; then
    error "Gateway not running at $GATEWAY_URL. Start with: cargo run --bin icn-gateway -- --bind 127.0.0.1:8080 --jwt-secret $JWT_SECRET"
fi
success "Gateway is running"

# Step 1: Get authentication tokens for three test users
log "Step 1: Authenticating three test users (Alice, Bob, Carol)..."

# For demo purposes, we'll create simple DIDs
# In production, these would be real Ed25519-based DIDs
ALICE_DID="did:icn:alice123"
BOB_DID="did:icn:bob456"
CAROL_DID="did:icn:carol789"
COOP_ID="food-coop"

# Get challenge for Alice
log "Getting challenge for Alice..."
ALICE_CHALLENGE=$(curl -sf -X POST "$GATEWAY_URL/auth/challenge" \
    -H "Content-Type: application/json" \
    -d "{\"did\": \"$ALICE_DID\"}" | jq -r '.challenge')
success "Alice challenge: ${ALICE_CHALLENGE:0:20}..."

# In production, sign the challenge with Ed25519 key
# For demo, we'll use a mock signature (gateway won't validate in test mode)
ALICE_SIG="mock_signature_alice"

# Verify and get JWT token for Alice
log "Getting JWT token for Alice..."
ALICE_TOKEN=$(curl -sf -X POST "$GATEWAY_URL/auth/verify" \
    -H "Content-Type: application/json" \
    -d "{
        \"did\": \"$ALICE_DID\",
        \"challenge\": \"$ALICE_CHALLENGE\",
        \"signature\": \"$ALICE_SIG\",
        \"coop_id\": \"$COOP_ID\",
        \"scopes\": [\"gov:read\", \"gov:write\"]
    }" | jq -r '.token')

if [ -z "$ALICE_TOKEN" ] || [ "$ALICE_TOKEN" = "null" ]; then
    error "Failed to get Alice's token. Note: This demo requires a test-mode gateway or valid Ed25519 signatures."
fi
success "Alice authenticated"

# Similarly for Bob and Carol
log "Getting JWT token for Bob..."
BOB_CHALLENGE=$(curl -sf -X POST "$GATEWAY_URL/auth/challenge" \
    -H "Content-Type: application/json" \
    -d "{\"did\": \"$BOB_DID\"}" | jq -r '.challenge')
BOB_TOKEN=$(curl -sf -X POST "$GATEWAY_URL/auth/verify" \
    -H "Content-Type: application/json" \
    -d "{
        \"did\": \"$BOB_DID\",
        \"challenge\": \"$BOB_CHALLENGE\",
        \"signature\": \"mock_signature_bob\",
        \"coop_id\": \"$COOP_ID\",
        \"scopes\": [\"gov:read\", \"gov:write\"]
    }" | jq -r '.token')
success "Bob authenticated"

log "Getting JWT token for Carol..."
CAROL_CHALLENGE=$(curl -sf -X POST "$GATEWAY_URL/auth/challenge" \
    -H "Content-Type: application/json" \
    -d "{\"did\": \"$CAROL_DID\"}" | jq -r '.challenge')
CAROL_TOKEN=$(curl -sf -X POST "$GATEWAY_URL/auth/verify" \
    -H "Content-Type: application/json" \
    -d "{
        \"did\": \"$CAROL_DID\",
        \"challenge\": \"$CAROL_CHALLENGE\",
        \"signature\": \"mock_signature_carol\",
        \"coop_id\": \"$COOP_ID\",
        \"scopes\": [\"gov:read\", \"gov:write\"]
    }" | jq -r '.token')
success "Carol authenticated"

echo ""

# Step 2: Create a governance domain
log "Step 2: Creating governance domain 'coop:food'..."
DOMAIN_RESPONSE=$(curl -sf -X POST "$GATEWAY_URL/gov/domains" \
    -H "Authorization: Bearer $ALICE_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
        \"id\": \"coop:food\",
        \"name\": \"Food Cooperative Governance\",
        \"profile\": \"cooperative\",
        \"quorum_percent\": 50,
        \"approval_percent\": 66,
        \"voting_period_days\": 7,
        \"members\": [\"$ALICE_DID\", \"$BOB_DID\", \"$CAROL_DID\"]
    }")

DOMAIN_NAME=$(echo "$DOMAIN_RESPONSE" | jq -r '.name')
success "Domain created: $DOMAIN_NAME"
echo "$DOMAIN_RESPONSE" | jq '.'
echo ""

# Step 3: Create a proposal
log "Step 3: Creating a proposal to approve a new supplier..."
PROPOSAL_RESPONSE=$(curl -sf -X POST "$GATEWAY_URL/gov/proposals" \
    -H "Authorization: Bearer $ALICE_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
        \"domain_id\": \"coop:food\",
        \"title\": \"Approve Local Farms Inc as new supplier\",
        \"description\": \"This proposal seeks approval to add Local Farms Inc as our primary vegetable supplier. They offer organic produce at competitive prices with same-day delivery.\",
        \"payload\": {
            \"Text\": {
                \"body\": \"Contract terms: 2-year agreement, 15% bulk discount, weekly delivery schedule. Estimated annual savings: \$25,000.\"
            }
        }
    }")

PROPOSAL_ID=$(echo "$PROPOSAL_RESPONSE" | jq -r '.id.0')
PROPOSAL_TITLE=$(echo "$PROPOSAL_RESPONSE" | jq -r '.title')
success "Proposal created: $PROPOSAL_ID"
echo "Title: $PROPOSAL_TITLE"
echo "State: $(echo "$PROPOSAL_RESPONSE" | jq -r '.state')"
echo ""

# Step 4: Open the proposal for voting
log "Step 4: Opening proposal for voting (24 hour period)..."
OPEN_RESPONSE=$(curl -sf -X POST "$GATEWAY_URL/gov/proposals/$PROPOSAL_ID/open" \
    -H "Authorization: Bearer $ALICE_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
        \"voting_period_seconds\": 86400
    }")

VOTING_STATE=$(echo "$OPEN_RESPONSE" | jq -r '.state')
success "Proposal opened for voting"
echo "New state: $VOTING_STATE"
echo ""

# Step 5: Cast votes from multiple members
log "Step 5: Casting votes from three members..."

# Alice votes FOR
log "Alice voting FOR..."
curl -sf -X POST "$GATEWAY_URL/gov/proposals/$PROPOSAL_ID/vote" \
    -H "Authorization: Bearer $ALICE_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
        \"choice\": \"for\",
        \"comment\": \"Great pricing and quality. I support this partnership.\"
    }" > /dev/null
success "Alice voted FOR"

# Bob votes FOR
log "Bob voting FOR..."
curl -sf -X POST "$GATEWAY_URL/gov/proposals/$PROPOSAL_ID/vote" \
    -H "Authorization: Bearer $BOB_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
        \"choice\": \"for\",
        \"comment\": \"The savings will help us reduce member costs.\"
    }" > /dev/null
success "Bob voted FOR"

# Carol votes AGAINST
log "Carol voting AGAINST..."
curl -sf -X POST "$GATEWAY_URL/gov/proposals/$PROPOSAL_ID/vote" \
    -H "Authorization: Bearer $CAROL_TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
        \"choice\": \"against\",
        \"comment\": \"I prefer to stick with our current supplier who we know well.\"
    }" > /dev/null
success "Carol voted AGAINST"

echo ""
log "Vote tally: 2 FOR, 1 AGAINST (66% approval)"
echo ""

# Step 6: Close the proposal
log "Step 6: Closing the proposal and calculating outcome..."
FINAL_RESPONSE=$(curl -sf -X POST "$GATEWAY_URL/gov/proposals/$PROPOSAL_ID/close" \
    -H "Authorization: Bearer $ALICE_TOKEN" \
    -H "Content-Type: application/json")

FINAL_STATE=$(echo "$FINAL_RESPONSE" | jq -r '.state')
success "Proposal closed"
echo ""

# Step 7: Display the final outcome
log "Step 7: Final Outcome"
echo "=================================="
echo ""
echo "Proposal: $PROPOSAL_TITLE"
echo "ID: $PROPOSAL_ID"
echo ""

# Parse the state to determine outcome
if echo "$FINAL_STATE" | grep -q "Accepted"; then
    echo -e "${GREEN}✓ OUTCOME: ACCEPTED${NC}"
    echo "The proposal has been approved by the members."
    CLOSED_AT=$(echo "$FINAL_RESPONSE" | jq -r '.state.Accepted.closed_at')
    echo "Closed at: $(date -d @$CLOSED_AT 2>/dev/null || echo $CLOSED_AT)"
elif echo "$FINAL_STATE" | grep -q "Rejected"; then
    echo -e "${RED}✗ OUTCOME: REJECTED${NC}"
    echo "The proposal was rejected by the members."
    CLOSED_AT=$(echo "$FINAL_RESPONSE" | jq -r '.state.Rejected.closed_at')
    echo "Closed at: $(date -d @$CLOSED_AT 2>/dev/null || echo $CLOSED_AT)"
else
    echo -e "${YELLOW}○ OUTCOME: NO QUORUM${NC}"
    echo "The proposal did not reach quorum."
fi

echo ""
echo "Full proposal details:"
echo "$FINAL_RESPONSE" | jq '.'
echo ""

# Step 8: List all proposals in the domain
log "Step 8: Listing all proposals in the domain..."
LIST_RESPONSE=$(curl -sf "$GATEWAY_URL/gov/proposals?domain_id=coop:food" \
    -H "Authorization: Bearer $ALICE_TOKEN")

PROPOSAL_COUNT=$(echo "$LIST_RESPONSE" | jq '. | length')
success "Found $PROPOSAL_COUNT proposal(s) in domain"
echo ""

# Step 9: Query closed proposals
log "Step 9: Querying closed proposals..."
CLOSED_RESPONSE=$(curl -sf "$GATEWAY_URL/gov/proposals?state=closed" \
    -H "Authorization: Bearer $ALICE_TOKEN")

CLOSED_COUNT=$(echo "$CLOSED_RESPONSE" | jq '. | length')
success "Found $CLOSED_COUNT closed proposal(s)"
echo ""

echo "=================================="
echo -e "${GREEN}Governance workflow completed successfully!${NC}"
echo ""
echo "This example demonstrated:"
echo "  • Domain creation with 3 members"
echo "  • Proposal creation (text type)"
echo "  • Opening proposal for voting"
echo "  • Multiple members casting votes"
echo "  • Closing proposal with outcome calculation"
echo "  • Querying proposals by domain and state"
echo ""
echo "Next steps:"
echo "  • Try creating budget or membership proposals"
echo "  • Connect a WebSocket client to receive real-time events"
echo "  • Explore proposal filtering and pagination"
echo ""
