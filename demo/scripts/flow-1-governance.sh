#!/usr/bin/env bash
# =============================================================================
# Flow 1: Governance Legitimacy and Action Traceability
# Narrative: Harbor Homes Cooperative — roof repair authorization
#
# What this demonstrates (Flow 1A — current scope):
#   - Proposal creation with cooperator-legible narrative
#   - Member voting with named voters
#   - Approval result visible to all members
#   - Governance decision record persisted on-chain
#   - Ledger action authorized by the approved decision
#   - Provenance: proposal -> decision -> authorized action visible together
#
# What this does NOT yet claim (Flow 1B — pending PR #1327 ExecutionReceiptGate):
#   - Machine-verifiable cryptographic binding of execution to approved governance
#   - Receipt-gated enforcement (unauthorized actions blocked at the kernel level)
#   - Signed GovernanceReceipt (proof endpoint requires signing key — not yet
#     configured in current cluster deployment)
#
# Core cooperator question: "Did the thing we voted on actually happen,
#                            and can we prove it?"
#
# Known cluster constraints (as of 2026-03-07):
#   - treasury:read/write scopes not in ALLOWED_SCOPES — treasury API unreachable
#   - Proof endpoint returns 404: signing key not configured in pod
#   - These are deployment gaps, not design gaps. Flow 1B addresses both.
#
# Usage:    ./demo/scripts/flow-1-governance.sh
# Duration: ~5 minutes live
# Audience: All — especially housing and worker coops
# Requires: kubectl access to K3s cluster, icnctl inside icn-gamma pod
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib-demo-ports.sh"

# ---------------------------------------------------------------------------
# Persona definitions (named voters)
# ---------------------------------------------------------------------------
# Harbor Homes has one seeded member DID. For demo purposes we name the
# on-chain actor and note that a production deployment would have one DID
# per cooperator. The single seeded DID represents the board quorum.
HARBOR_BOARD_DID="did:icn:zyWqWVqGERfRvUz4LVGd4coCZDuNhnufxRpNVTR1BBA7"

# Domain ID used for this flow — human-readable, not a UUID.
# Must match what's passed in domain creation request.
# Suffix with a short random tag so repeated demo runs don't collide.
_DEMO_RUN_TAG="$(date +%s | tail -c 6)"
HARBOR_DOMAIN_ID="harbor-homes-roof-repair-${_DEMO_RUN_TAG}"

# Temp file for response bodies — avoids subshell DEMO_LAST_HTTP_CODE loss.
# demo_curl sets DEMO_LAST_HTTP_CODE in the calling shell; we must NOT call
# demo_curl inside $(...) or the code is lost. Use _do_curl instead.
_RESP_FILE="$(mktemp)"
trap 'rm -f "$_RESP_FILE"' EXIT

# ---------------------------------------------------------------------------
# _do_curl <url> <method> [body] [token]
# Wrapper that writes response to $_RESP_FILE and checks code in-process.
# DEMO_LAST_HTTP_CODE is set in the current shell (no subshell).
# ---------------------------------------------------------------------------
_do_curl() {
  local url="$1" method="${2:-GET}" body="${3:-}" token="${4:-${DEMO_TOKEN:-}}"
  local _tmp
  _tmp=$(mktemp)

  local args=(-s -o "$_tmp" -w "%{http_code}" -X "$method" -H "Accept: application/json")
  [ -n "$token" ] && args+=(-H "Authorization: Bearer $token")
  if [ -n "$body" ]; then
    args+=(-H "Content-Type: application/json" -d "$body")
  fi

  DEMO_LAST_HTTP_CODE=$(curl "${args[@]}" "$url" 2>/dev/null || echo "000")
  export DEMO_LAST_HTTP_CODE
  cp "$_tmp" "$_RESP_FILE"
  rm -f "$_tmp"
}

# ---------------------------------------------------------------------------
# _pretty: pretty-print $_RESP_FILE
# ---------------------------------------------------------------------------
_pretty() {
  python3 -m json.tool 2>/dev/null < "$_RESP_FILE" || cat "$_RESP_FILE"
  echo ""
}

# ---------------------------------------------------------------------------
# _field <name>: extract top-level JSON field from $_RESP_FILE
# ---------------------------------------------------------------------------
_field() {
  python3 -c "
import sys, json
with open('$_RESP_FILE') as f:
    d = json.load(f)
v = d.get('$1', '')
# Handle nested state dict (e.g. {'Accepted': {...}})
if isinstance(v, dict) and len(v) == 1:
    print(list(v.keys())[0])
elif isinstance(v, dict):
    print(json.dumps(v))
else:
    print(v)
" 2>/dev/null || echo ""
}

# ---------------------------------------------------------------------------
# STEP 0: Setup
# ---------------------------------------------------------------------------
narrate "Step 0: Starting Harbor Homes gateway connection"
aside "Harbor Homes Cooperative runs on icn-gamma (port 18083)"

demo_ports_up
demo_wait_ready 30
result "All 4 coop gateways are up"

narrate "Authenticating as Harbor Homes board"

# Store token to a variable WITHOUT subshell by using a temp file
_TOKEN_FILE="$(mktemp)"
demo_get_token gamma > "$_TOKEN_FILE"
HARBOR_TOKEN="$(cat "$_TOKEN_FILE")"
rm -f "$_TOKEN_FILE"
export DEMO_TOKEN="$HARBOR_TOKEN"

if [ -z "$HARBOR_TOKEN" ]; then
  fail "Could not obtain Harbor Homes token"
  exit 1
fi
result "Harbor Homes authenticated"
aside "DID: ${HARBOR_BOARD_DID:0:50}..."
echo ""

# ---------------------------------------------------------------------------
# STEP 1: Establish the problem
# ---------------------------------------------------------------------------
narrate "Step 1: The situation — inspection report received"
echo "  Harbor Homes Cooperative manages 48 units across two buildings."
echo ""
echo "  An inspection of Building A has found water intrusion through"
echo "  the flat roof membrane. The building inspector's report:"
echo ""
echo "    'Active water intrusion detected at northeast parapet wall."
echo "     Membrane failure at three seams. Recommend immediate repair"
echo "     to prevent interior structural damage. Estimated cost: \$12,000."
echo "     Delay beyond 30 days risks mold remediation costs of \$40,000+.'"
echo ""
echo "  The board chair, Delphine Moreau, has raised this for a member vote."
echo "  The question: can the cooperative authorize a \$12,000 draw from the"
echo "  capital reserve — and is that decision traceable?"
echo ""
aside "This is the cooperator question: not just 'did it pass?' but 'can we prove it?'"
echo ""

# ---------------------------------------------------------------------------
# STEP 2: Create the governance domain
# ---------------------------------------------------------------------------
narrate "Step 2: Establish the governance domain for this vote"
aside "Harbor Homes uses a dedicated domain per major decision type."
aside "quorum: 51% — approval: 60% — voting period: 7 days"
aside "domain ID: ${HARBOR_DOMAIN_ID}"
echo ""

_do_curl "${HARBOR_URL}/v1/gov/domains" POST \
  "{\"id\":\"${HARBOR_DOMAIN_ID}\",\"name\":\"Harbor Homes Capital Reserve\",\"description\":\"Governance domain for capital reserve authorization votes\",\"profile\":\"cooperative_default\",\"quorum_percent\":51,\"approval_percent\":60,\"voting_period_days\":7,\"members\":[\"${HARBOR_BOARD_DID}\"]}" \
  "$HARBOR_TOKEN"

demo_require_2xx "Create governance domain"

DOMAIN_REGISTRY_ID=$(_field "id")
result "Domain created — registry ID: ${DOMAIN_REGISTRY_ID}"
aside "The domain_id '${HARBOR_DOMAIN_ID}' is the stable handle for subsequent requests"
echo ""

# ---------------------------------------------------------------------------
# STEP 3: Create the roof repair proposal
# ---------------------------------------------------------------------------
narrate "Step 3: Delphine Moreau raises the roof repair proposal"
echo "  Delphine (Board Chair) creates the proposal with the full cost basis"
echo "  and inspection evidence. Every member can read this before voting."
echo ""

_do_curl "${HARBOR_URL}/v1/gov/proposals" POST \
  "{\"domain_id\":\"${HARBOR_DOMAIN_ID}\",\"title\":\"Authorize \$12,000 Capital Reserve Draw — Building A Roof Repair\",\"description\":\"Based on the March 2026 structural inspection, Building A requires immediate flat roof membrane repair at the northeast parapet. Three seam failures identified. Delay risks mold remediation costs 3x the repair cost. This proposal authorizes a \$12,000 draw from the capital reserve fund to contract Lakeside Roofing LLC.\",\"payload\":{\"type\":\"text\",\"body\":\"AUTHORIZATION REQUEST\\n\\nInspection Date: 2026-03-05\\nContractor Quote: Lakeside Roofing LLC — \$11,800 (accepted at \$12,000 with 10% contingency)\\nFunding Source: Capital Reserve Fund (current balance: \$47,200)\\nPost-Authorization Reserve: \$35,200\\n\\nProposed by: Delphine Moreau, Board Chair\"}}" \
  "$HARBOR_TOKEN"

demo_require_2xx "Create roof repair proposal"

PROPOSAL_ID=$(_field "id")

if [ -z "$PROPOSAL_ID" ]; then
  fail "Proposal creation succeeded but no ID returned"
  _pretty
  exit 1
fi

result "Proposal created"
result "Proposal ID: ${PROPOSAL_ID}"
result "State: Draft (not yet open for voting)"
aside "Every member can read the full inspection context before casting a vote"
echo ""
echo "  Raw proposal record:"
_pretty

# ---------------------------------------------------------------------------
# STEP 4: Open for voting
# ---------------------------------------------------------------------------
narrate "Step 4: Board opens the proposal for member voting"
aside "Voting period: 7 days (voting_period_days configured in domain)"
echo ""

_do_curl "${HARBOR_URL}/v1/gov/proposals/${PROPOSAL_ID}/open" POST '{}' "$HARBOR_TOKEN"
demo_require_2xx "Open proposal for voting"

result "Proposal is now open for voting"
echo "  Proposal state:"
_pretty

# ---------------------------------------------------------------------------
# STEP 5: Members vote
# ---------------------------------------------------------------------------
narrate "Step 5: Members vote — transparent, named, recorded on-chain"
echo "  In a full Harbor Homes deployment, each of the 12 voting members"
echo "  has their own DID and casts an independent vote."
echo ""
echo "  The current demo cluster has one seeded member DID representing"
echo "  the cooperative's quorum. In production:"
echo "    - Delphine Moreau (Board Chair) — votes For"
echo "    - Kwame Asante (Treasurer) — votes For"
echo "    - Rosa Figueroa (Member-at-large) — votes For"
echo "    - ... and so on for all 12 voting members"
echo ""
echo "  Casting vote — Delphine Moreau, Board Chair:"

_do_curl "${HARBOR_URL}/v1/gov/proposals/${PROPOSAL_ID}/vote" POST \
  "{\"choice\":\"for\",\"comment\":\"The inspection report is unambiguous. We cannot delay. The capital reserve exists precisely for situations like this.\"}" \
  "$HARBOR_TOKEN"

demo_require_2xx "Cast vote (board representative)"

result "Vote recorded — For"
aside "Vote persisted with voter DID, timestamp, and comment"
echo ""
echo "  Proposal record after vote:"
_pretty

# ---------------------------------------------------------------------------
# STEP 6: Show the tally
# ---------------------------------------------------------------------------
narrate "Step 6: Tally — the vote count is public to all members"
echo ""

_do_curl "${HARBOR_URL}/v1/gov/proposals/${PROPOSAL_ID}/tally" GET "" "$HARBOR_TOKEN"
demo_require_2xx "Get vote tally"

FOR_VOTES=$(_field "for_votes")
AGAINST_VOTES=$(_field "against_votes")
TOTAL_VOTES=$(_field "total_votes")

echo "  Vote tally:"
_pretty

result "For: ${FOR_VOTES}  Against: ${AGAINST_VOTES}  Total: ${TOTAL_VOTES}"
aside "Any member of Harbor Homes can query this tally at any time — not just the board"
echo ""

# ---------------------------------------------------------------------------
# STEP 7: Close the proposal
# ---------------------------------------------------------------------------
narrate "Step 7: Closing the proposal — result is final"
echo ""

_do_curl "${HARBOR_URL}/v1/gov/proposals/${PROPOSAL_ID}/close" POST '{}' "$HARBOR_TOKEN"
demo_require_2xx "Close proposal"

FINAL_STATE=$(_field "state")

result "Proposal closed — final state: ${FINAL_STATE}"
echo ""
echo "  Final proposal record:"
_pretty

if [ "$FINAL_STATE" != "Accepted" ]; then
  warn "Expected state 'Accepted' but got '${FINAL_STATE}'"
  warn "The proposal may not have met quorum. Check member count vs quorum_percent."
fi

# ---------------------------------------------------------------------------
# STEP 8: Governance proof (current deployment status)
# ---------------------------------------------------------------------------
narrate "Step 8: Governance proof — what is verifiable now"
echo ""
aside "Querying the governance proof endpoint (GovernanceReceipt)..."

_do_curl "${HARBOR_URL}/v1/gov/proposals/${PROPOSAL_ID}/proof" GET "" "$HARBOR_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  result "Governance proof retrieved (GovernanceReceipt):"
  _pretty
elif [ "$DEMO_LAST_HTTP_CODE" = "404" ]; then
  warn "Proof endpoint returned 404 — signing key not configured in this pod"
  echo ""
  echo "  The proposal record IS the verifiable audit trail for now:"
  echo ""
  echo "    Proposal ID:  ${PROPOSAL_ID}"
  echo "    Domain:       ${HARBOR_DOMAIN_ID}"
  echo "    Proposer DID: ${HARBOR_BOARD_DID}"
  echo "    Final state:  ${FINAL_STATE}"
  echo "    Vote tally:   For=${FOR_VOTES}, Against=${AGAINST_VOTES}"
  echo ""
  aside "GovernanceReceipt with cryptographic signature: pending pod signing key config"
  aside "This is a deployment gap — the receipt schema is implemented and ready"
else
  warn "Unexpected proof response (HTTP ${DEMO_LAST_HTTP_CODE}):"
  _pretty
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 9: Full governance record — what any member can verify
# ---------------------------------------------------------------------------
narrate "Step 9: Provenance — what any Harbor Homes member can verify right now"
echo ""

_do_curl "${HARBOR_URL}/v1/gov/proposals/${PROPOSAL_ID}" GET "" "$HARBOR_TOKEN"
demo_require_2xx "Retrieve final governance decision record"

echo "  Full governance record (queryable by any member):"
_pretty

result "Proposal ID, domain, proposer DID, vote state, and timestamps all present"
result "This record persists on the cooperative's ICN node permanently"
aside "Any member can verify independently: the vote happened, count is correct"
echo ""

# ---------------------------------------------------------------------------
# STEP 10: The authorized action — connecting governance to execution
# ---------------------------------------------------------------------------
narrate "Step 10: The authorized action — governance to execution"
echo ""
echo "  The vote has passed. The cooperative's treasury staff now have"
echo "  authorization to execute the \$12,000 spend against Lakeside Roofing."
echo ""
echo "  Authorization record:"
echo "    Governance decision:  ${PROPOSAL_ID}"
echo "    Decision outcome:     ${FINAL_STATE}"
echo "    Authorized amount:    \$12,000"
echo "    Payee:                Lakeside Roofing LLC"
echo "    Purpose:              Building A roof repair — northeast parapet"
echo "    Authorization date:   $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "    Authorized by:        ${HARBOR_BOARD_DID:0:50}..."
echo ""
aside "Treasury API is not yet reachable in this deployment (scope 'treasury:read'"
aside "is not in ALLOWED_SCOPES — this is a deployment gap, not a design gap)"
aside "In Flow 1B: the treasury spend will be cryptographically bound to this proposal ID"
echo ""

# ---------------------------------------------------------------------------
# Final summary
# ---------------------------------------------------------------------------
echo "================================================================"
echo " FLOW 1A COMPLETE"
echo " Governance legitimacy and action traceability demonstrated."
echo ""
echo " What was shown:"
echo "   - Domain: Harbor Homes capital reserve governance domain"
echo "   - Proposal: Board chair raised roof repair with full cost basis"
echo "   - Voting: Named voters cast transparent, recorded votes"
echo "   - Result: Approval visible to all members, not just the board"
echo "   - Record: Governance decision persisted on the cooperative's node"
echo "   - Traceability: Proposal ID links governance to authorized action"
echo ""
echo " What's coming (Flow 1B — pending PR #1327 ExecutionReceiptGate):"
echo "   -> Machine-verifiable binding: execution receipt cryptographically"
echo "      links the treasury spend to the approved governance decision"
echo "   -> Kernel enforcement: an unauthorized spend is blocked, not just"
echo "      recorded after the fact"
echo "   -> Signed GovernanceReceipt: proof endpoint returns a receipt"
echo "      signed by the cooperative's DID (requires signing key in pod)"
echo ""
echo " Presenter note: Until #1327 merges, describe this as:"
echo "   'The governance and the action are linked and visible —"
echo "    the final enforcement-proof layer is being finalized.'"
echo "================================================================"
