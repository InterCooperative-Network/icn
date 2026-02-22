#!/usr/bin/env bash
# ICN Pilot Cluster — Live REST Smoke Test
#
# Exercises the governance lifecycle end-to-end against a running gateway:
#   health → create domain → create proposal → open → vote → close → ledger linkage
#
# Usage:
#   HOST=http://10.8.10.40:30080 TOKEN=<jwt> ./scripts/smoke-test.sh
#   HOST=http://10.8.10.40:30080 TOKEN=<jwt> COOP_ID=my-coop ./scripts/smoke-test.sh
#
# Environment:
#   HOST     Gateway base URL (default: http://10.8.10.40:30080)
#   TOKEN    Pre-issued JWT (required for authenticated steps)
#   COOP_ID  Cooperative identifier (default: pilot-coop-1)
#
# Exit codes:
#   0  All steps passed
#   1  One or more steps failed
#   2  Dependency missing (curl, jq)

set -euo pipefail

HOST="${HOST:-http://10.8.10.40:30080}"
TOKEN="${TOKEN:-}"
COOP_ID="${COOP_ID:-pilot-coop-1}"

# Unique suffix so repeated runs don't collide
RUN_ID="smoke-$(date +%s)-${RANDOM}"
DOMAIN_ID="smoke-domain-${RUN_ID}"
PROPOSAL_TITLE="Smoke Test Proposal ${RUN_ID}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0

pass() { echo -e "${GREEN}PASS${NC} $1"; PASS_COUNT=$((PASS_COUNT + 1)); }
fail() { echo -e "${RED}FAIL${NC} $1"; FAIL_COUNT=$((FAIL_COUNT + 1)); }
warn() { echo -e "${YELLOW}WARN${NC} $1"; }
skip() { echo -e "${YELLOW}SKIP${NC} $1"; SKIP_COUNT=$((SKIP_COUNT + 1)); }
step() { echo -e "${CYAN}--->${NC} $1"; }

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "ERROR: required command '$1' not found." >&2
        exit 2
    fi
}

require_cmd curl
require_cmd jq

# Returns: body\nHTTP_CODE
http_get() {
    local url="$1"
    local auth_header="${2:-}"
    if [ -n "$auth_header" ]; then
        curl -s -w "\n%{http_code}" --max-time 15 -H "$auth_header" "$url"
    else
        curl -s -w "\n%{http_code}" --max-time 15 "$url"
    fi
}

http_post() {
    local url="$1"
    local body="$2"
    local auth_header="${3:-}"
    if [ -n "$auth_header" ]; then
        curl -s -w "\n%{http_code}" --max-time 15 \
            -X POST -H "Content-Type: application/json" -H "$auth_header" \
            -d "$body" "$url"
    else
        curl -s -w "\n%{http_code}" --max-time 15 \
            -X POST -H "Content-Type: application/json" \
            -d "$body" "$url"
    fi
}

# Split response into body + code
split_response() {
    local raw="$1"
    RESP_BODY=$(printf '%s' "$raw" | head -n -1)
    RESP_CODE=$(printf '%s' "$raw" | tail -n1)
}

AUTH=""
[ -n "$TOKEN" ] && AUTH="Authorization: Bearer $TOKEN"

echo "=================================="
echo "ICN Pilot Cluster Smoke Test"
echo "Host:    $HOST"
echo "CoopID:  $COOP_ID"
echo "Run ID:  $RUN_ID"
echo "Auth:    $([ -n "$TOKEN" ] && echo "provided" || echo "none (unauthenticated steps only)")"
echo "=================================="
echo ""

# ---------------------------------------------------------------------------
# Step 1: Health check
# ---------------------------------------------------------------------------
step "GET /v1/health"
RAW=$(http_get "$HOST/v1/health")
split_response "$RAW"
if [ "$RESP_CODE" = "200" ]; then
    pass "Gateway health check (200)"
else
    fail "Gateway health check returned $RESP_CODE (expected 200)"
    echo "  Body: $RESP_BODY"
fi

# ---------------------------------------------------------------------------
# Step 2: Unauthenticated endpoints return 401
# ---------------------------------------------------------------------------
step "GET /v1/gov/domains (no auth, expect 401)"
RAW=$(http_get "$HOST/v1/gov/domains")
split_response "$RAW"
if [ "$RESP_CODE" = "401" ]; then
    pass "Unauthenticated request rejected (401)"
else
    warn "Expected 401 without auth, got $RESP_CODE"
fi

# ---------------------------------------------------------------------------
# Authenticated steps — skip if no token
# ---------------------------------------------------------------------------
if [ -z "$TOKEN" ]; then
    echo ""
    skip "All authenticated steps (no TOKEN provided)"
    echo ""
    echo "To run the full suite:"
    echo "  1. Obtain a challenge:  curl -s -X POST $HOST/v1/auth/challenge \\"
    echo "       -H 'Content-Type: application/json' \\"
    echo "       -d '{\"did\":\"<your-did>\"}'"
    echo "  2. Sign the challenge with your DID key"
    echo "  3. Verify:              curl -s -X POST $HOST/v1/auth/verify \\"
    echo "       -H 'Content-Type: application/json' \\"
    echo "       -d '{\"did\":\"<did>\",\"challenge\":\"<c>\",\"signature\":\"<sig>\"}'"
    echo "  4. Re-run:              HOST=$HOST TOKEN=<jwt> $0"
    echo ""
    echo "=================================="
    echo "Results: ${PASS_COUNT} passed, ${FAIL_COUNT} failed, ${SKIP_COUNT} skipped"
    echo "=================================="
    [ "$FAIL_COUNT" -eq 0 ] && exit 0 || exit 1
fi

# ---------------------------------------------------------------------------
# Step 3: Create governance domain
# ---------------------------------------------------------------------------
step "POST /v1/gov/domains (create smoke domain)"
DOMAIN_BODY=$(jq -n \
    --arg id "$DOMAIN_ID" \
    --arg name "Smoke Test Domain ${RUN_ID}" \
    '{
        id: $id,
        name: $name,
        profile: "cooperative_default",
        quorum_percent: 1,
        approval_percent: 51,
        voting_period_days: 1,
        members: []
    }')
RAW=$(http_post "$HOST/v1/gov/domains" "$DOMAIN_BODY" "$AUTH")
split_response "$RAW"
if [ "$RESP_CODE" = "201" ] || [ "$RESP_CODE" = "200" ]; then
    pass "Create governance domain ($RESP_CODE)"
elif [ "$RESP_CODE" = "400" ] && echo "$RESP_BODY" | jq -e '.error' 2>/dev/null | grep -qi "already"; then
    warn "Domain already exists (400) — continuing"
else
    fail "Create domain returned $RESP_CODE"
    echo "  Body: $(echo "$RESP_BODY" | jq -c . 2>/dev/null || echo "$RESP_BODY")"
fi

# ---------------------------------------------------------------------------
# Step 4: Create a text proposal
# ---------------------------------------------------------------------------
step "POST /v1/gov/proposals (text proposal)"
PROPOSAL_BODY=$(jq -n \
    --arg domain_id "$DOMAIN_ID" \
    --arg title "$PROPOSAL_TITLE" \
    '{
        domain_id: $domain_id,
        title: $title,
        description: "Automated smoke test proposal. Safe to ignore.",
        payload: {
            type: "text",
            body: "This proposal was created by the ICN pilot smoke test script."
        }
    }')
RAW=$(http_post "$HOST/v1/gov/proposals" "$PROPOSAL_BODY" "$AUTH")
split_response "$RAW"
if [ "$RESP_CODE" = "201" ] || [ "$RESP_CODE" = "200" ]; then
    PROPOSAL_ID=$(echo "$RESP_BODY" | jq -r '.id // .proposal_id // empty' 2>/dev/null)
    if [ -n "$PROPOSAL_ID" ] && [ "$PROPOSAL_ID" != "null" ]; then
        pass "Create proposal ($RESP_CODE) → id=$PROPOSAL_ID"
    else
        fail "Create proposal returned $RESP_CODE but no id in response"
        echo "  Body: $(echo "$RESP_BODY" | jq -c . 2>/dev/null || echo "$RESP_BODY")"
        PROPOSAL_ID=""
    fi
else
    fail "Create proposal returned $RESP_CODE"
    echo "  Body: $(echo "$RESP_BODY" | jq -c . 2>/dev/null || echo "$RESP_BODY")"
    PROPOSAL_ID=""
fi

if [ -z "$PROPOSAL_ID" ]; then
    echo ""
    echo "Cannot proceed without proposal ID. Aborting remaining steps."
    echo ""
    echo "=================================="
    echo "Results: ${PASS_COUNT} passed, ${FAIL_COUNT} failed, ${SKIP_COUNT} skipped"
    echo "=================================="
    exit 1
fi

# ---------------------------------------------------------------------------
# Step 5: Open the proposal for voting
# ---------------------------------------------------------------------------
step "POST /v1/gov/proposals/$PROPOSAL_ID/open"
OPEN_BODY='{"voting_period_seconds": 3600}'
RAW=$(http_post "$HOST/v1/gov/proposals/$PROPOSAL_ID/open" "$OPEN_BODY" "$AUTH")
split_response "$RAW"
if [ "$RESP_CODE" = "200" ]; then
    PROPOSAL_STATE=$(echo "$RESP_BODY" | jq -r '.state // .status // empty' 2>/dev/null)
    pass "Open proposal (200) state=${PROPOSAL_STATE:-unknown}"
else
    fail "Open proposal returned $RESP_CODE"
    echo "  Body: $(echo "$RESP_BODY" | jq -c . 2>/dev/null || echo "$RESP_BODY")"
fi

# ---------------------------------------------------------------------------
# Step 6: Cast a vote
# ---------------------------------------------------------------------------
step "POST /v1/gov/proposals/$PROPOSAL_ID/vote"
VOTE_BODY='{"choice": "for", "comment": "Smoke test vote"}'
RAW=$(http_post "$HOST/v1/gov/proposals/$PROPOSAL_ID/vote" "$VOTE_BODY" "$AUTH")
split_response "$RAW"
if [ "$RESP_CODE" = "200" ] || [ "$RESP_CODE" = "201" ]; then
    pass "Cast vote ($RESP_CODE)"
else
    fail "Cast vote returned $RESP_CODE"
    echo "  Body: $(echo "$RESP_BODY" | jq -c . 2>/dev/null || echo "$RESP_BODY")"
fi

# ---------------------------------------------------------------------------
# Step 7: Close the proposal
# ---------------------------------------------------------------------------
step "POST /v1/gov/proposals/$PROPOSAL_ID/close"
RAW=$(http_post "$HOST/v1/gov/proposals/$PROPOSAL_ID/close" '{}' "$AUTH")
split_response "$RAW"
if [ "$RESP_CODE" = "200" ]; then
    OUTCOME=$(echo "$RESP_BODY" | jq -r '.outcome // .state // empty' 2>/dev/null)
    pass "Close proposal (200) outcome=${OUTCOME:-unknown}"
else
    fail "Close proposal returned $RESP_CODE"
    echo "  Body: $(echo "$RESP_BODY" | jq -c . 2>/dev/null || echo "$RESP_BODY")"
fi

# ---------------------------------------------------------------------------
# Step 8: Verify proposal is readable after close
# ---------------------------------------------------------------------------
step "GET /v1/gov/proposals/$PROPOSAL_ID"
RAW=$(http_get "$HOST/v1/gov/proposals/$PROPOSAL_ID" "$AUTH")
split_response "$RAW"
if [ "$RESP_CODE" = "200" ]; then
    FINAL_STATE=$(echo "$RESP_BODY" | jq -r '.state // empty' 2>/dev/null)
    pass "Get closed proposal (200) state=${FINAL_STATE:-unknown}"
else
    fail "Get proposal post-close returned $RESP_CODE"
    echo "  Body: $(echo "$RESP_BODY" | jq -c . 2>/dev/null || echo "$RESP_BODY")"
fi

# ---------------------------------------------------------------------------
# Step 9: Registry — list decisions (expect 200, may be empty)
# ---------------------------------------------------------------------------
step "GET /v1/registry/decisions?coop_id=$COOP_ID"
RAW=$(http_get "$HOST/v1/registry/decisions?coop_id=$COOP_ID" "$AUTH")
split_response "$RAW"
if [ "$RESP_CODE" = "200" ]; then
    DECISION_COUNT=$(echo "$RESP_BODY" | jq 'if type == "array" then length else (.decisions // [] | length) end' 2>/dev/null || echo "?")
    pass "List registry decisions (200) count=${DECISION_COUNT}"
else
    fail "List registry decisions returned $RESP_CODE"
    echo "  Body: $(echo "$RESP_BODY" | jq -c . 2>/dev/null || echo "$RESP_BODY")"
fi

# ---------------------------------------------------------------------------
# Step 10: Ledger entries by decision (uses a stable fixture hash; 200 + empty is fine)
# ---------------------------------------------------------------------------
step "GET /v1/ledger/$COOP_ID/entries/by-decision (fixture hash)"
# A known-zero hash — expect 200 with empty or 404; both indicate the endpoint is live
FIXTURE_HASH="0000000000000000000000000000000000000000000000000000000000000000"
RAW=$(http_get "$HOST/v1/ledger/$COOP_ID/entries/by-decision?decision_hash=$FIXTURE_HASH" "$AUTH")
split_response "$RAW"
if [ "$RESP_CODE" = "200" ] || [ "$RESP_CODE" = "404" ]; then
    pass "Ledger entries-by-decision endpoint reachable ($RESP_CODE)"
else
    fail "Ledger entries-by-decision returned $RESP_CODE"
    echo "  Body: $(echo "$RESP_BODY" | jq -c . 2>/dev/null || echo "$RESP_BODY")"
fi

# ---------------------------------------------------------------------------
# Step 11: List governance proposals (smoke check list endpoint)
# ---------------------------------------------------------------------------
step "GET /v1/gov/proposals"
RAW=$(http_get "$HOST/v1/gov/proposals" "$AUTH")
split_response "$RAW"
if [ "$RESP_CODE" = "200" ]; then
    pass "List proposals (200)"
else
    fail "List proposals returned $RESP_CODE"
    echo "  Body: $(echo "$RESP_BODY" | jq -c . 2>/dev/null || echo "$RESP_BODY")"
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "=================================="
TOTAL=$((PASS_COUNT + FAIL_COUNT + SKIP_COUNT))
echo "Results: ${PASS_COUNT} passed, ${FAIL_COUNT} failed, ${SKIP_COUNT} skipped (${TOTAL} total)"
if [ "$FAIL_COUNT" -eq 0 ]; then
    echo -e "${GREEN}SMOKE PASS${NC}"
    echo "=================================="
    exit 0
else
    echo -e "${RED}SMOKE FAIL${NC}"
    echo "=================================="
    exit 1
fi
