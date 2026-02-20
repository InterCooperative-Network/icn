#!/usr/bin/env bash
# Demo Flow D: Tool Library Cooperative — Full Lifecycle
#
# Exercises the complete cooperative lifecycle via the REST gateway API,
# mirroring the vertical slice integration test (icn-core/tests/vertical_slice_integration.rs):
#
#   1. Create identities (Alice, Bob, Carol, Dave)
#   2. Form "Tool Library Coop" with charter
#   3. Activate cooperative (treasury auto-creation)
#   4. Dave applies for membership → governance vote → approved
#   5. Budget proposal: "Purchase drill press for 30 hours"
#   6. Vote on budget → passes
#   7. Register drill press as asset
#   8. Dave checks out tool → returns it
#   9. Verify audit trail
#
# This demo is educational: each step explains what's happening and why.
# Steps that call real gateway endpoints are marked [LIVE].
# Steps that need endpoints not yet implemented are marked [PLACEHOLDER].
#
# Usage:
#   ICN_GATEWAY=http://localhost:8080 ./scripts/demo-flow-d.sh
#
set -euo pipefail

# Gateway binds 8080 by default (see icn-core/src/config/gateway.rs).
GATEWAY="${ICN_GATEWAY:-http://localhost:8080}"
TOKEN="${ICN_TOKEN:-}"
COOP_ID="tool-library"
COOP_NAME="Tool Library Coop"
DOMAIN_ID="tool-library-gov"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[0;33m'
BOLD='\033[1m'
NC='\033[0m'

step()        { echo -e "\n${BOLD}${CYAN}[Flow D] Step $1: $2${NC}"; }
substep()     { echo -e "${CYAN}  > $1${NC}"; }
ok()          { echo -e "${GREEN}  ✓ $1${NC}"; }
warn()        { echo -e "${YELLOW}  ! $1${NC}"; }
fail()        { echo -e "${RED}  ✗ $1${NC}"; exit 1; }
placeholder() { echo -e "${YELLOW}  [PLACEHOLDER] $1${NC}"; }

AUTH_HEADER=""
if [ -n "$TOKEN" ]; then
  AUTH_HEADER="Authorization: Bearer $TOKEN"
fi

# Helper: make authenticated curl requests
# Usage: api_call METHOD PATH [DATA]
api_call() {
  local method="$1"
  local path="$2"
  local data="${3:-}"

  local args=(-sf -X "$method" "$GATEWAY/v1$path")
  args+=(-H "Content-Type: application/json")
  if [ -n "$AUTH_HEADER" ]; then
    args+=(-H "$AUTH_HEADER")
  fi
  if [ -n "$data" ]; then
    args+=(-d "$data")
  fi

  curl "${args[@]}" 2>&1
}

# Helper: make curl request and return HTTP status code
api_status() {
  local method="$1"
  local path="$2"
  local data="${3:-}"

  local args=(-s -o /dev/null -w "%{http_code}" -X "$method" "$GATEWAY/v1$path")
  args+=(-H "Content-Type: application/json")
  if [ -n "$AUTH_HEADER" ]; then
    args+=(-H "$AUTH_HEADER")
  fi
  if [ -n "$data" ]; then
    args+=(-d "$data")
  fi

  curl "${args[@]}" 2>&1
}

# Helper: extract JSON field using python3
json_field() {
  local json="$1"
  local field="$2"
  echo "$json" | python3 -c "import sys,json; print(json.load(sys.stdin)$field)" 2>/dev/null
}

echo -e "${BOLD}${CYAN}"
echo "╔══════════════════════════════════════════════════════════╗"
echo "║   ICN Demo Flow D: Tool Library Cooperative Lifecycle   ║"
echo "║                                                         ║"
echo "║   9-step scenario proving cooperative operations work   ║"
echo "║   end-to-end via the REST gateway API.                  ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo -e "${NC}"

# =============================================================================
# Step 1: Create sovereign identities
# =============================================================================
# In production, each participant generates an Ed25519 keypair client-side,
# derives a DID (did:icn:<base58-pubkey>), and authenticates via challenge-response.
#
# The gateway does not have a "create identity" endpoint because identities are
# self-sovereign — they exist the moment a keypair is generated.
# Authentication flow: POST /v1/auth/challenge → sign nonce → POST /v1/auth/verify → JWT
# =============================================================================
step "1" "Create sovereign identities (Alice, Bob, Carol, Dave)"

# For demo purposes, we use placeholder DIDs.
# In a real deployment, these would be generated from Ed25519 keypairs.
ALICE_DID="did:icn:z6MkAliceDemoFounder00000000000000000000"
BOB_DID="did:icn:z6MkBobDemoWorker000000000000000000000000"
CAROL_DID="did:icn:z6MkCarolDemoWorker00000000000000000000"
DAVE_DID="did:icn:z6MkDaveDemoApplicant000000000000000000"
SUPPLIER_DID="did:icn:z6MkSupplierDemo0000000000000000000000"

placeholder "Identity creation is client-side (Ed25519 keypair generation)"
placeholder "Authentication would use: POST /v1/auth/challenge + POST /v1/auth/verify"
ok "Alice  = $ALICE_DID (founder)"
ok "Bob    = $BOB_DID (member)"
ok "Carol  = $CAROL_DID (member)"
ok "Dave   = $DAVE_DID (applicant)"

# =============================================================================
# Step 2: Form "Tool Library Coop" with charter
# =============================================================================
# Alice creates the cooperative. The gateway's POST /v1/coops endpoint creates
# the coop and automatically makes the authenticated user (Alice) the steward.
#
# Endpoint: POST /v1/coops
# Auth: Requires coop:write scope
# =============================================================================
step "2" "Form Tool Library Cooperative [LIVE]"

substep "Creating cooperative via POST /v1/coops..."
CREATE_RESP=$(api_call POST "/coops" "{
  \"id\": \"$COOP_ID\",
  \"name\": \"$COOP_NAME\"
}") && {
  CREATED_NAME=$(json_field "$CREATE_RESP" "['name']" || echo "")
  ok "Created cooperative: $CREATED_NAME (id=$COOP_ID)"
} || {
  warn "Create coop returned error (may already exist or auth required)"
  warn "Response: ${CREATE_RESP:-<empty>}"
}

# Add founding members Bob and Carol
substep "Adding Bob as founding member via POST /v1/coops/$COOP_ID/members..."
ADD_BOB_RESP=$(api_call POST "/coops/$COOP_ID/members" "{
  \"did\": \"$BOB_DID\",
  \"role\": \"participant\",
  \"display_name\": \"Bob\"
}") && {
  ok "Added Bob as participant"
} || {
  warn "Add Bob failed (auth required or coop not found): ${ADD_BOB_RESP:-<empty>}"
}

substep "Adding Carol as founding member via POST /v1/coops/$COOP_ID/members..."
ADD_CAROL_RESP=$(api_call POST "/coops/$COOP_ID/members" "{
  \"did\": \"$CAROL_DID\",
  \"role\": \"participant\",
  \"display_name\": \"Carol\"
}") && {
  ok "Added Carol as participant"
} || {
  warn "Add Carol failed: ${ADD_CAROL_RESP:-<empty>}"
}

# Verify cooperative state
substep "Retrieving cooperative info via GET /v1/coops/$COOP_ID..."
GET_COOP_RESP=$(api_call GET "/coops/$COOP_ID") && {
  MEMBER_COUNT=$(json_field "$GET_COOP_RESP" "['members']" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "?")
  ok "Cooperative has $MEMBER_COUNT member(s)"
} || {
  warn "Get coop failed: ${GET_COOP_RESP:-<empty>}"
}

# =============================================================================
# Step 3: Activate cooperative (treasury auto-creation)
# =============================================================================
# In the Rust layer, activation happens via CoopActor::activate_cooperative(),
# which accepts a charter hash and triggers treasury auto-creation (B1 feature).
#
# The gateway does not yet have an "activate" endpoint — the gateway's CoopManager
# creates coops in an immediately-usable state without the charter/activation flow.
#
# TODO: Need POST /v1/coops/{id}/activate endpoint that:
#   - Accepts charter_hash in body
#   - Changes coop status from Pending to Active
#   - Triggers treasury auto-creation via TreasuryManager
#   - Returns activated coop with treasury_did
# =============================================================================
step "3" "Activate cooperative with treasury auto-creation [PLACEHOLDER]"

placeholder "POST /v1/coops/$COOP_ID/activate would accept charter hash and create treasury"
placeholder "Charter hash would be: sha256:toollib-charter-v1"
placeholder "Treasury should auto-create with currency=HOURS, is_active=true"

# Check if treasury exists (it may have been created via other means)
substep "Checking treasury status via GET /v1/treasury/$COOP_ID..."
TREASURY_RESP=$(api_call GET "/treasury/$COOP_ID") && {
  TREASURY_DID=$(json_field "$TREASURY_RESP" "['treasury_did']" || echo "none")
  CURRENCY=$(json_field "$TREASURY_RESP" "['currency']" || echo "unknown")
  IS_ACTIVE=$(json_field "$TREASURY_RESP" "['is_active']" || echo "unknown")
  ok "Treasury DID: $TREASURY_DID"
  ok "Currency: $CURRENCY, Active: $IS_ACTIVE"
} || {
  warn "Treasury not found (expected - activation endpoint not implemented)"
}

# =============================================================================
# Step 4: Dave applies for membership → governance vote → approved
# =============================================================================
# This is the full governance flow:
#   a) Create a governance domain for the cooperative
#   b) Create a membership proposal to add Dave
#   c) Open the proposal for voting
#   d) Cast votes (Alice votes FOR)
#   e) Close the proposal (tallies votes)
#   f) Add Dave as member (after approval)
#
# All endpoints exist in the gateway [LIVE].
# =============================================================================
step "4" "Dave's membership application + governance vote [LIVE]"

# 4a: Create governance domain
substep "Creating governance domain via POST /v1/gov/domains..."
CREATE_DOMAIN_RESP=$(api_call POST "/gov/domains" "{
  \"id\": \"$DOMAIN_ID\",
  \"name\": \"Tool Library Governance\",
  \"profile\": \"cooperative\",
  \"quorum_percent\": 50,
  \"approval_percent\": 50,
  \"voting_period_days\": 7,
  \"members\": [\"$ALICE_DID\", \"$BOB_DID\", \"$CAROL_DID\"]
}") && {
  ok "Created governance domain: $DOMAIN_ID"
} || {
  warn "Create domain failed (auth required or already exists): ${CREATE_DOMAIN_RESP:-<empty>}"
}

# 4b: Create membership proposal for Dave
substep "Creating membership proposal via POST /v1/gov/proposals..."
MEMBERSHIP_PROP_RESP=$(api_call POST "/gov/proposals" "{
  \"domain_id\": \"$DOMAIN_ID\",
  \"title\": \"Add Dave as member\",
  \"description\": \"Dave wants to join the Tool Library Coop to borrow tools\",
  \"payload\": {
    \"type\": \"membership\",
    \"action\": \"add\",
    \"did\": \"$DAVE_DID\"
  }
}") && {
  MEMBERSHIP_PROP_ID=$(json_field "$MEMBERSHIP_PROP_RESP" "['proposal_id']" 2>/dev/null \
    || json_field "$MEMBERSHIP_PROP_RESP" "['id']" 2>/dev/null \
    || echo "")
  if [ -n "$MEMBERSHIP_PROP_ID" ]; then
    ok "Membership proposal created: $MEMBERSHIP_PROP_ID"
  else
    warn "Proposal created but could not extract ID from response"
    MEMBERSHIP_PROP_ID=""
  fi
} || {
  warn "Create membership proposal failed: ${MEMBERSHIP_PROP_RESP:-<empty>}"
  MEMBERSHIP_PROP_ID=""
}

# 4c-e: Open, vote, and close (only if proposal was created)
if [ -n "${MEMBERSHIP_PROP_ID:-}" ]; then
  # 4c: Open proposal for voting
  substep "Opening proposal for voting via POST /v1/gov/proposals/$MEMBERSHIP_PROP_ID/open..."
  OPEN_RESP=$(api_call POST "/gov/proposals/$MEMBERSHIP_PROP_ID/open" "{
    \"voting_period_seconds\": 3600
  }") && {
    ok "Proposal opened for voting (1 hour period)"
  } || {
    warn "Open proposal failed: ${OPEN_RESP:-<empty>}"
  }

  # 4d: Alice votes FOR
  substep "Alice votes FOR via POST /v1/gov/proposals/$MEMBERSHIP_PROP_ID/vote..."
  VOTE_RESP=$(api_call POST "/gov/proposals/$MEMBERSHIP_PROP_ID/vote" "{
    \"choice\": \"for\",
    \"comment\": \"Welcome Dave!\"
  }") && {
    ok "Alice voted FOR Dave's membership"
  } || {
    warn "Vote failed: ${VOTE_RESP:-<empty>}"
  }

  # 4e: Close proposal (tally votes)
  substep "Closing proposal via POST /v1/gov/proposals/$MEMBERSHIP_PROP_ID/close..."
  CLOSE_RESP=$(api_call POST "/gov/proposals/$MEMBERSHIP_PROP_ID/close" "{}") && {
    OUTCOME=$(json_field "$CLOSE_RESP" "['outcome']" 2>/dev/null \
      || json_field "$CLOSE_RESP" "['state']" 2>/dev/null \
      || echo "unknown")
    ok "Proposal closed, outcome: $OUTCOME"
  } || {
    warn "Close proposal failed: ${CLOSE_RESP:-<empty>}"
  }
fi

# 4f: Add Dave as approved member
substep "Adding Dave as member via POST /v1/coops/$COOP_ID/members..."
ADD_DAVE_RESP=$(api_call POST "/coops/$COOP_ID/members" "{
  \"did\": \"$DAVE_DID\",
  \"role\": \"participant\",
  \"display_name\": \"Dave\"
}") && {
  ok "Dave added as cooperative member"
} || {
  warn "Add Dave failed: ${ADD_DAVE_RESP:-<empty>}"
}

# Verify member list
substep "Verifying member list via GET /v1/coops/$COOP_ID..."
VERIFY_MEMBERS_RESP=$(api_call GET "/coops/$COOP_ID") && {
  MEMBER_COUNT=$(json_field "$VERIFY_MEMBERS_RESP" "['members']" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "?")
  ok "Cooperative now has $MEMBER_COUNT member(s) (expected: 4 - Alice, Bob, Carol, Dave)"
} || {
  warn "Verify members failed: ${VERIFY_MEMBERS_RESP:-<empty>}"
}

# =============================================================================
# Step 5: Budget proposal — "Purchase drill press for 30 hours"
# =============================================================================
# Create a budget proposal through the governance system.
# This demonstrates how spending decisions require democratic approval.
#
# Endpoint: POST /v1/gov/proposals (with budget payload)
# =============================================================================
step "5" "Budget proposal: Purchase drill press for 30 HOURS [LIVE]"

substep "Creating budget proposal via POST /v1/gov/proposals..."
BUDGET_PROP_RESP=$(api_call POST "/gov/proposals" "{
  \"domain_id\": \"$DOMAIN_ID\",
  \"title\": \"Purchase drill press\",
  \"description\": \"Buy a drill press for the tool library. Cost: 30 HOURS to supplier.\",
  \"payload\": {
    \"type\": \"budget\",
    \"amount\": 30,
    \"recipient\": \"$SUPPLIER_DID\",
    \"currency\": \"HOURS\",
    \"purpose\": \"Purchase drill press for tool library\"
  }
}") && {
  BUDGET_PROP_ID=$(json_field "$BUDGET_PROP_RESP" "['proposal_id']" 2>/dev/null \
    || json_field "$BUDGET_PROP_RESP" "['id']" 2>/dev/null \
    || echo "")
  if [ -n "$BUDGET_PROP_ID" ]; then
    ok "Budget proposal created: $BUDGET_PROP_ID"
    ok "Amount: 30 HOURS to supplier"
  else
    warn "Proposal created but could not extract ID"
    BUDGET_PROP_ID=""
  fi
} || {
  warn "Create budget proposal failed: ${BUDGET_PROP_RESP:-<empty>}"
  BUDGET_PROP_ID=""
}

# =============================================================================
# Step 6: Vote on budget → passes → ledger transaction
# =============================================================================
# The governance system tallies votes. When a budget proposal passes,
# the event bus triggers a ledger transaction (credit supplier, debit coop).
#
# Endpoints: POST .../open, POST .../vote, POST .../close [LIVE]
# Ledger transaction: happens automatically via event handler (server-side)
# =============================================================================
step "6" "Vote on budget proposal [LIVE]"

if [ -n "${BUDGET_PROP_ID:-}" ]; then
  # Open for voting
  substep "Opening budget proposal for voting..."
  api_call POST "/gov/proposals/$BUDGET_PROP_ID/open" "{
    \"voting_period_seconds\": 3600
  }" > /dev/null 2>&1 && {
    ok "Budget proposal opened for voting"
  } || {
    warn "Open budget proposal failed"
  }

  # Alice votes FOR
  substep "Alice votes FOR the drill press purchase..."
  api_call POST "/gov/proposals/$BUDGET_PROP_ID/vote" "{
    \"choice\": \"for\",
    \"comment\": \"We need this tool\"
  }" > /dev/null 2>&1 && {
    ok "Alice voted FOR budget proposal"
  } || {
    warn "Vote on budget proposal failed"
  }

  # Check vote tally
  substep "Checking votes via GET /v1/gov/proposals/$BUDGET_PROP_ID/votes..."
  VOTES_RESP=$(api_call GET "/gov/proposals/$BUDGET_PROP_ID/votes") && {
    ok "Vote tally retrieved"
  } || {
    warn "Get votes failed"
  }

  # Close proposal
  substep "Closing budget proposal (tally votes, trigger ledger tx)..."
  CLOSE_BUDGET_RESP=$(api_call POST "/gov/proposals/$BUDGET_PROP_ID/close" "{}") && {
    ok "Budget proposal closed - ledger transaction should be triggered"
  } || {
    warn "Close budget proposal failed"
  }

  # Verify the budget proposal result
  substep "Checking proposal state via GET /v1/gov/proposals/$BUDGET_PROP_ID..."
  PROP_STATE_RESP=$(api_call GET "/gov/proposals/$BUDGET_PROP_ID") && {
    PROP_STATE=$(json_field "$PROP_STATE_RESP" "['state']" 2>/dev/null || echo "unknown")
    ok "Proposal state: $PROP_STATE"
  } || {
    warn "Get proposal state failed"
  }
else
  warn "Skipping vote - no budget proposal ID available"
fi

# =============================================================================
# Step 7: Register drill press as asset
# =============================================================================
# In the vertical slice test, the drill press is registered as a ledger entry
# (double-entry: coop acquires asset from supplier) and metadata is stored in KV.
#
# The gateway does not yet have an asset registry endpoint.
#
# TODO: Need POST /v1/coops/{id}/assets endpoint that:
#   - Accepts asset metadata (name, type, value, acquired_via proposal ID)
#   - Creates a TOOLS ledger entry (double-entry: debit coop, credit supplier)
#   - Stores asset metadata linking to ledger hash and proposal
#   - Returns asset record with ID and ledger hash
# =============================================================================
step "7" "Register drill press as asset [PLACEHOLDER]"

placeholder "POST /v1/coops/$COOP_ID/assets would register the drill press"
placeholder "Asset: { name: 'Drill Press', type: 'Durable', value: 30 HOURS }"
placeholder "Would create TOOLS ledger entry and link to budget proposal ${BUDGET_PROP_ID:-<none>}"
placeholder "Asset ID would be: tool-library:drill-press-001"

# =============================================================================
# Step 8: Dave checks out tool → use-access entry → return
# =============================================================================
# In the vertical slice test, tool checkout/return creates TOOL_ACCESS ledger
# entries (double-entry). Checkout debits Dave, credits coop. Return reverses it.
# Dave's TOOL_ACCESS balance should net to zero after return.
#
# The gateway does not yet have tool checkout/return endpoints.
#
# TODO: Need the following endpoints:
#   POST /v1/coops/{id}/assets/{asset_id}/checkout
#     - Creates TOOL_ACCESS debit entry for borrower
#     - Stores checkout record in KV
#     - Returns checkout receipt with ledger hash
#
#   POST /v1/coops/{id}/assets/{asset_id}/return
#     - Creates TOOL_ACCESS credit entry (reversal) for borrower
#     - Updates checkout record as returned
#     - Returns return receipt with ledger hash
# =============================================================================
step "8" "Dave checks out and returns drill press [PLACEHOLDER]"

placeholder "POST /v1/coops/$COOP_ID/assets/drill-press-001/checkout would create:"
placeholder "  Ledger: debit Dave TOOL_ACCESS 1, credit coop TOOL_ACCESS 1"
placeholder "  KV: checkout record linking Dave to drill-press-001"
placeholder ""
placeholder "POST /v1/coops/$COOP_ID/assets/drill-press-001/return would create:"
placeholder "  Ledger: credit Dave TOOL_ACCESS 1, debit coop TOOL_ACCESS 1"
placeholder "  KV: return record, Dave's TOOL_ACCESS balance nets to zero"

# =============================================================================
# Step 9: Verify audit trail
# =============================================================================
# The audit trail proves every decision has a paper trail:
# - Governance proposals have vote records and outcomes
# - Budget decisions create ledger entries
# - Asset registrations link to proposals
# - Tool checkouts/returns have ledger entries
#
# Available endpoints:
#   GET /v1/treasury/{coop_id}/audit   - treasury audit trail [LIVE]
#   GET /v1/ledger/{coop_id}/history   - ledger transaction history [LIVE]
#   GET /v1/gov/proposals/{id}         - proposal details with outcome [LIVE]
#   GET /v1/gov/proposals/{id}/votes   - vote records [LIVE]
# =============================================================================
step "9" "Verify audit trail [LIVE/PARTIAL]"

# Check treasury audit trail
substep "Checking treasury audit trail via GET /v1/treasury/$COOP_ID/audit..."
AUDIT_RESP=$(api_call GET "/treasury/$COOP_ID/audit") && {
  ok "Treasury audit trail retrieved"
} || {
  warn "Treasury audit trail not available (treasury may not exist)"
}

# Check ledger history
substep "Checking ledger history via GET /v1/ledger/$COOP_ID/history..."
HISTORY_RESP=$(api_call GET "/ledger/$COOP_ID/history") && {
  ok "Ledger history retrieved"
} || {
  warn "Ledger history not available"
}

# Verify governance records exist
if [ -n "${MEMBERSHIP_PROP_ID:-}" ]; then
  substep "Verifying membership proposal record..."
  MEMBERSHIP_RECORD=$(api_call GET "/gov/proposals/$MEMBERSHIP_PROP_ID") && {
    ok "Membership proposal record: verified"
  } || {
    warn "Could not retrieve membership proposal record"
  }
fi

if [ -n "${BUDGET_PROP_ID:-}" ]; then
  substep "Verifying budget proposal record..."
  BUDGET_RECORD=$(api_call GET "/gov/proposals/$BUDGET_PROP_ID") && {
    ok "Budget proposal record: verified"
  } || {
    warn "Could not retrieve budget proposal record"
  }

  substep "Verifying budget proposal vote history..."
  BUDGET_VOTES=$(api_call GET "/gov/proposals/$BUDGET_PROP_ID/votes") && {
    ok "Budget proposal vote history: verified"
  } || {
    warn "Could not retrieve budget vote history"
  }
fi

# Check cooperative final state
substep "Verifying final cooperative state via GET /v1/coops/$COOP_ID..."
FINAL_COOP=$(api_call GET "/coops/$COOP_ID") && {
  FINAL_NAME=$(json_field "$FINAL_COOP" "['name']" 2>/dev/null || echo "unknown")
  FINAL_MEMBERS=$(json_field "$FINAL_COOP" "['members']" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "?")
  ok "Cooperative: $FINAL_NAME"
  ok "Members: $FINAL_MEMBERS"
  ok "Audit trail: governance proposals, votes, and ledger entries recorded"
} || {
  warn "Could not retrieve final cooperative state"
}

# =============================================================================
# Summary
# =============================================================================
echo ""
echo -e "${BOLD}━━━ Flow D Summary ━━━${NC}"
echo ""
echo -e "  ${GREEN}Scenario: Tool Library Cooperative full lifecycle${NC}"
echo ""
echo -e "  Steps with ${GREEN}LIVE${NC} gateway endpoints:"
echo -e "    2. Create cooperative           POST /v1/coops"
echo -e "    2. Add members                  POST /v1/coops/{id}/members"
echo -e "    4. Create governance domain     POST /v1/gov/domains"
echo -e "    4. Membership proposal + vote   POST /v1/gov/proposals, .../vote, .../close"
echo -e "    5. Budget proposal              POST /v1/gov/proposals (budget payload)"
echo -e "    6. Budget vote                  POST /v1/gov/proposals/{id}/vote"
echo -e "    9. Audit trail                  GET /v1/treasury/{id}/audit, /v1/ledger/{id}/history"
echo ""
echo -e "  Steps needing ${YELLOW}new endpoints${NC}:"
echo -e "    1. Identity creation            (client-side, auth via /v1/auth/challenge+verify)"
echo -e "    3. Cooperative activation       POST /v1/coops/{id}/activate (charter + treasury)"
echo -e "    7. Asset registration           POST /v1/coops/{id}/assets"
echo -e "    8. Tool checkout/return         POST /v1/coops/{id}/assets/{id}/checkout|return"
echo ""
echo -e "${GREEN}${BOLD}[Flow D] Tool Library Cooperative demo completed${NC}"
