#!/usr/bin/env bash
# Demo Flow B: Name Registration and Service Discovery
# Tests: register name → resolve name → assert round-trip → list by DID → assert membership
set -euo pipefail

GATEWAY="${ICN_GATEWAY:-http://localhost:8000}"
TOKEN="${ICN_TOKEN:-}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

step()  { echo -e "${YELLOW}[Flow B] $1${NC}"; }
ok()    { echo -e "${GREEN}  ✓ $1${NC}"; }
fail()  { echo -e "${RED}  ✗ $1${NC}"; exit 1; }

AUTH_HEADER=""
if [ -n "$TOKEN" ]; then
  AUTH_HEADER="Authorization: Bearer $TOKEN"
fi

TIMESTAMP=$(date +%s)
TEST_NAME="demo-service-${TIMESTAMP}"
TEST_DID="did:icn:demo-service-${TIMESTAMP}"

# Step 1: Register a name to a DID
step "Registering name '$TEST_NAME' to DID '$TEST_DID'..."
REG_RESP=$(curl -s -w "\n%{http_code}" -X POST "$GATEWAY/v1/names/register" \
  -H "Content-Type: application/json" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} \
  -d "{\"name\": \"$TEST_NAME\", \"did\": \"$TEST_DID\"}" 2>&1)
HTTP_CODE=$(echo "$REG_RESP" | tail -1)
REG_BODY=$(echo "$REG_RESP" | head -n -1)
if [ "$HTTP_CODE" = "200" ]; then
  ok "Registered name: $TEST_NAME → $TEST_DID"
else
  fail "Register returned HTTP $HTTP_CODE (expected 200): $REG_BODY"
fi

# Step 2: Resolve name back to DID
step "Resolving name '$TEST_NAME'..."
RESOLVE_RESP=$(curl -sf "$GATEWAY/v1/names/$TEST_NAME" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) \
  || fail "Resolve failed: $RESOLVE_RESP"

RESOLVED_DID=$(echo "$RESOLVE_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['did'])" 2>/dev/null) \
  || fail "Failed to parse resolve response: $RESOLVE_RESP"
ok "Resolved '$TEST_NAME' → $RESOLVED_DID"

# Step 3: Assert resolved DID matches registered DID
step "Asserting resolved DID matches registered DID..."
if [ "$RESOLVED_DID" = "$TEST_DID" ]; then
  ok "Round-trip assertion passed: $RESOLVED_DID"
else
  fail "DID mismatch: expected '$TEST_DID', got '$RESOLVED_DID'"
fi

# Step 4: List names for the DID
step "Listing names owned by '$TEST_DID'..."
LIST_RESP=$(curl -sf "$GATEWAY/v1/names?owner=$TEST_DID" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) \
  || fail "List failed: $LIST_RESP"

NAME_COUNT=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null) \
  || fail "Failed to parse list response: $LIST_RESP"
ok "Found $NAME_COUNT name(s) for DID"

# Step 5: Assert "demo-service" appears in the list
step "Asserting '$TEST_NAME' appears in DID's name list..."
FOUND=$(echo "$LIST_RESP" | python3 -c "
import sys, json
names = json.load(sys.stdin)
target = '$TEST_NAME'
matches = [n for n in names if n.get('name') == target]
print('yes' if matches else 'no')
" 2>/dev/null) || fail "Failed to search list response: $LIST_RESP"

if [ "$FOUND" = "yes" ]; then
  ok "'$TEST_NAME' found in DID's name list"
else
  fail "'$TEST_NAME' not found in name list for $TEST_DID"
fi

echo ""
echo -e "${GREEN}[Flow B] Name Registration and Service Discovery demo completed successfully${NC}"
