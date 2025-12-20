#!/bin/bash
# Load Sample Data into ICN Demo
# This script adds the 12 members and creates historical transactions

set -e

GATEWAY="http://localhost:8080"
COOP_ID="rochester-tool-library"
DATA_DIR="/home/matt/icn-demo-test/data"
RPC_ENDPOINT="127.0.0.1:15602"

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
if ! curl -s "$GATEWAY/v1/health" > /dev/null 2>&1; then
    echo -e "${RED}✗${NC} Gateway not responding at $GATEWAY"
    echo "Please start the daemon first:"
    echo "  cd icn && ./target/release/icnd -d $DATA_DIR -e $RPC_ENDPOINT --gateway-enable --gateway-bind \"127.0.0.1:8080\""
    exit 1
fi
echo -e "${GREEN}✓${NC} Gateway is running"
echo

# Get JWT token
echo "Getting JWT token..."
cd /home/matt/projects/icn/icn

TOKEN=$(./target/release/icnctl \
  -d "$DATA_DIR" \
  -e "$RPC_ENDPOINT" \
  auth token \
  --coop-id "$COOP_ID" \
  --scopes "coop:write,coop:read,ledger:read,ledger:write" \
  --passphrase demo123 2>/dev/null | grep -v "Enter passphrase" | tr -d '\n')

if [ -z "$TOKEN" ]; then
    echo -e "${RED}✗${NC} Failed to get token"
    echo "Try manually:"
    echo "  ./target/release/icnctl -d $DATA_DIR -e $RPC_ENDPOINT auth token --coop-id $COOP_ID --scopes \"coop:write,coop:read,ledger:read,ledger:write\""
    exit 1
fi
echo -e "${GREEN}✓${NC} Got JWT token"
echo

# Load member data
cd /home/matt/projects/icn
MEMBERS_FILE="demo/data/tool-library-members.json"

echo "Loading members from $MEMBERS_FILE..."

# Parse JSON and add each member
# For now, we'll create a simple version that adds members one by one
# In a real implementation, this would parse the JSON properly

# Get founder DID (the existing member)
FOUNDER_DID="did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh"

echo "Sample members to add:"
echo "  1. Alice Chen - Tool Coordinator"
echo "  2. Bob Martinez - Member"
echo "  3. Carol Johnson - Member"
echo "  4. David Lee - Treasurer"
echo "  5. Elena Rodriguez - Member"
echo "  6. Frank Wilson - Member"
echo "  7. Grace Park - Board Member"
echo "  8. Henry Brown - Member"
echo "  9. Isabel Garcia - Member"
echo "  10. Jack Thompson - Member"
echo "  11. Kelly O'Brien - Member"
echo "  12. Luis Sanchez - Member"
echo

echo -e "${YELLOW}NOTE:${NC} To add members, we need their DIDs."
echo "Each member needs to:"
echo "  1. Create their own identity (icnctl id init)"
echo "  2. Share their DID with the admin"
echo "  3. Admin adds them to the cooperative"
echo

echo "For demo purposes, you can:"
echo

echo "Option A: Create DIDs for all 12 members"
echo "  - Run icnctl id init for each member"
echo "  - Add each to the cooperative via API"
echo

echo "Option B: Use the pilot UI's 'Invite' feature"
echo "  - Generate invite links"
echo "  - Members join via invite"
echo

echo "Option C: Simulate members for demo"
echo "  - Create identities programmatically"
echo "  - Add via API in batch"
echo

echo "========================================="
echo "What this script CAN do right now:"
echo "========================================="
echo

echo "1. Verify cooperative exists:"
COOP_INFO=$(curl -s "$GATEWAY/v1/coops/$COOP_ID" \
  -H "Authorization: Bearer $TOKEN")

if echo "$COOP_INFO" | grep -q "\"id\":\"$COOP_ID\""; then
    echo -e "${GREEN}✓${NC} Cooperative '$COOP_ID' exists"
    echo "$COOP_INFO" | jq . 2>/dev/null || echo "$COOP_INFO"
else
    echo -e "${YELLOW}⚠${NC} Cooperative response: $COOP_INFO"
fi

echo

echo "2. Check founder balance:"
BALANCE=$(curl -s "$GATEWAY/v1/ledger/coops/$COOP_ID/balances/$FOUNDER_DID" \
  -H "Authorization: Bearer $TOKEN")

echo -e "${GREEN}✓${NC} Founder balance:"
echo "$BALANCE" | jq . 2>/dev/null || echo "$BALANCE"

echo

echo "========================================="
echo "Next Steps"
echo "========================================="
echo

echo "To complete member loading:"
echo

echo "1. Create member identities:"
echo "   for i in {1..12}; do"
echo "     icnctl id init --data-dir /tmp/member\$i"
echo "   done"
echo

echo "2. Get their DIDs and add to cooperative"
echo

echo "3. OR use the UI invite system:"
echo "   - Open http://localhost:3000"
echo "   - Go to Members → Invite"
echo "   - Generate invite links"
echo "   - Share with members"
echo

echo "4. OR use a full automation script that:"
echo "   - Creates identities"
echo "   - Extracts DIDs"
echo "   - Calls API to add members"
echo "   - Creates sample transactions"
echo

echo "Gateway: $GATEWAY"
echo "Cooperative: $COOP_ID"
echo "Token valid: $(echo $TOKEN | cut -c1-20)..."
echo

echo "========================================="
echo "Script complete!"
echo "========================================="
