#!/bin/bash
# ICN Gateway API Examples
# 
# These examples demonstrate common API operations.
# Replace YOUR_DID and YOUR_TOKEN with actual values.

BASE_URL="${ICN_GATEWAY_URL:-http://localhost:8080}"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}=== ICN Gateway API Examples ===${NC}\n"

# Health Check (Public)
echo -e "${GREEN}1. Health Check${NC}"
echo 'curl -s $BASE_URL/v1/health | jq'
curl -s $BASE_URL/v1/health | jq
echo ""

# Get Authentication Challenge (Public)
echo -e "${GREEN}2. Request Authentication Challenge${NC}"
echo 'curl -s -X POST $BASE_URL/v1/auth/challenge -H "Content-Type: application/json" -d "{\"did\": \"YOUR_DID\"}" | jq'
echo ""

# Note: Steps 3-onwards require a valid JWT token
# Get one using: icnctl auth token --coop my-coop

if [ -z "$TOKEN" ]; then
    echo -e "${BLUE}Set TOKEN environment variable to run authenticated examples:${NC}"
    echo "  export TOKEN=\$(icnctl auth token --coop my-coop)"
    echo ""
    exit 0
fi

# Create Cooperative
echo -e "${GREEN}3. Create Cooperative${NC}"
curl -s -X POST $BASE_URL/v1/coops \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{
        "id": "example-coop",
        "name": "Example Cooperative",
        "description": "A test cooperative"
    }' | jq
echo ""

# Get Cooperative
echo -e "${GREEN}4. Get Cooperative${NC}"
curl -s $BASE_URL/v1/coops/example-coop \
    -H "Authorization: Bearer $TOKEN" | jq
echo ""

# Add Member
echo -e "${GREEN}5. Add Member${NC}"
curl -s -X POST $BASE_URL/v1/coops/example-coop/members \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{
        "did": "did:icn:newmember",
        "role": "member"
    }' | jq
echo ""

# Get Position (net position, derived from signed receipts)
echo -e "${GREEN}6. Get Position${NC}"
curl -s $BASE_URL/v1/ledger/example-coop/position/did:icn:alice \
    -H "Authorization: Bearer $TOKEN" | jq
echo ""

# Record a Settlement
# NOTE: the gateway requires "from" to equal the authenticated DID (the token
# subject). example-coop and the did:icn:* values below are placeholders for the
# request shape; substitute your own coop and DIDs (and a token whose subject
# matches "from") for a runnable call.
echo -e "${GREEN}7. Record a Settlement${NC}"
curl -s -X POST $BASE_URL/v1/ledger/example-coop/settle \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{
        "from": "did:icn:alice",
        "to": "did:icn:bob",
        "amount": 5,
        "unit": "hours",
        "memo": "Garden help"
    }' | jq
echo ""

# Get Transaction History
echo -e "${GREEN}8. Get Transaction History${NC}"
curl -s "$BASE_URL/v1/ledger/example-coop/history?limit=10" \
    -H "Authorization: Bearer $TOKEN" | jq
echo ""

# Create Governance Domain
echo -e "${GREEN}9. Create Governance Domain${NC}"
curl -s -X POST $BASE_URL/v1/gov/domains \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{
        "domain_id": "example-domain",
        "name": "Example Domain",
        "description": "Test governance domain",
        "members": ["did:icn:alice", "did:icn:bob", "did:icn:carol"]
    }' | jq
echo ""

# Create Proposal
echo -e "${GREEN}10. Create Proposal${NC}"
PROPOSAL=$(curl -s -X POST $BASE_URL/v1/gov/proposals \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{
        "domain_id": "example-domain",
        "title": "Test Proposal",
        "description": "A proposal to test the governance system",
        "payload": {
            "type": "text",
            "body": "This is the full text body of the proposal."
        }
    }')
echo "$PROPOSAL" | jq
PROPOSAL_ID=$(echo "$PROPOSAL" | jq -r '.id')
echo ""

# Open Proposal for Voting
echo -e "${GREEN}11. Open Proposal for Voting${NC}"
curl -s -X POST $BASE_URL/v1/gov/proposals/$PROPOSAL_ID/open \
    -H "Authorization: Bearer $TOKEN" | jq
echo ""

# Cast Vote
echo -e "${GREEN}12. Cast Vote${NC}"
curl -s -X POST $BASE_URL/v1/gov/proposals/$PROPOSAL_ID/vote \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"choice": "for"}' | jq
echo ""

# Get Vote Tally
echo -e "${GREEN}13. Get Vote Tally${NC}"
curl -s $BASE_URL/v1/gov/proposals/$PROPOSAL_ID/votes \
    -H "Authorization: Bearer $TOKEN" | jq
echo ""

# Close Proposal
echo -e "${GREEN}14. Close Proposal${NC}"
curl -s -X POST $BASE_URL/v1/gov/proposals/$PROPOSAL_ID/close \
    -H "Authorization: Bearer $TOKEN" | jq
echo ""

echo -e "${BLUE}=== Examples Complete ===${NC}"
