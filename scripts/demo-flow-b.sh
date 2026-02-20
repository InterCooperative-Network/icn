#!/usr/bin/env bash
# Demo Flow B: Service Discovery
# Tests: announce service → discover services → get service by ID → withdraw
set -euo pipefail

# Gateway binds 8080 by default (see icn-core/src/config/gateway.rs).
# NOTE: Other demo scripts (flow-a, flow-c, flow-d, runner) still use 8000 and should be migrated.
GATEWAY="${ICN_GATEWAY:-http://localhost:8080}"
TOKEN="${ICN_TOKEN:-}"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

step() { echo -e "${CYAN}[Flow B] $1${NC}"; }
ok()   { echo -e "${GREEN}  ✓ $1${NC}"; }
fail() { echo -e "${RED}  ✗ $1${NC}"; exit 1; }

AUTH_HEADER=""
if [ -n "$TOKEN" ]; then
  AUTH_HEADER="Authorization: Bearer $TOKEN"
fi

SVC_ID="demo-ledger-$(date +%s)"

# Step 1: Announce a service endpoint
step "Announcing service endpoint..."
ANNOUNCE_RESP=$(curl -sf -X POST "$GATEWAY/v1/services/announce" \
  -H "Content-Type: application/json" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} \
  -d "{
    \"service_id\": \"$SVC_ID\",
    \"service_type\": {\"name\": \"ledger\", \"version\": \"1.0\"},
    \"addresses\": [{\"protocol\": \"https\", \"host\": \"node-a.local\", \"port\": 8080}],
    \"capabilities\": [\"read\", \"write\"],
    \"scope\": \"org\",
    \"ttl_secs\": 3600
  }" 2>&1) \
  || fail "Announce failed: $ANNOUNCE_RESP"
ok "Announced service: $SVC_ID"

# Step 2: Discover services
step "Discovering services..."
DISCOVER_RESP=$(curl -sf "$GATEWAY/v1/services?service_type=ledger&scope=org" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) \
  || fail "Discover failed: $DISCOVER_RESP"

SVC_COUNT=$(echo "$DISCOVER_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['total'])" 2>/dev/null) \
  || fail "Failed to parse discover response: $DISCOVER_RESP"
ok "Found $SVC_COUNT service(s)"

# Step 3: Get specific service by ID
step "Getting service by ID..."
GET_RESP=$(curl -sf "$GATEWAY/v1/services/$SVC_ID" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) \
  || fail "Get service failed: $GET_RESP"

SVC_PROVIDER=$(echo "$GET_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['provider'])" 2>/dev/null) \
  || fail "Failed to parse service: $GET_RESP"
ok "Service $SVC_ID provider: $SVC_PROVIDER"

# Step 4: Withdraw service
step "Withdrawing service..."
WITHDRAW_RESP=$(curl -sf -X DELETE "$GATEWAY/v1/services/$SVC_ID" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) \
  || fail "Withdraw failed: $WITHDRAW_RESP"
ok "Withdrawn service: $SVC_ID"

# Step 5: Verify withdrawal
step "Verifying service is gone..."
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "$GATEWAY/v1/services/$SVC_ID" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1)
if [ "$HTTP_CODE" = "404" ]; then
  ok "Service correctly removed (404)"
else
  fail "Expected 404, got $HTTP_CODE"
fi

echo ""
echo -e "${GREEN}[Flow B] Service Discovery demo completed successfully${NC}"
