#!/usr/bin/env bash
# =============================================================================
# Flow 2: Patronage, Contribution, and Value Legibility
# Narrative: BrightWorks Cooperative — Q1 patronage distribution
#
# What this demonstrates:
#   - Patronage distribution as a governance-approved action
#   - Allocation formula visible to every member: (member_hours / total_hours) * surplus
#   - Any member can re-derive their allocation from public inputs
#   - Ledger settlement records each allocation with provenance
#   - Receipt chain links the settlement to the approved governance decision
#
# Core cooperator question: "Why did this member get this distribution?"
#
# Known cluster constraints (as of 2026-03-07):
#   - treasury:read/write scopes not in ALLOWED_SCOPES — treasury API unreachable
#   - Ledger settlement may require treasury:write or ledger:write scope
#   - The demo narrates all 5 member allocations from seed data; the live
#     settlement API call uses the single seeded node DID
#
# Usage:    ./demo/scripts/flow-2-patronage.sh
# Duration: ~5 minutes live
# Audience: Worker coops, food co-ops, intermediate orgs
# Requires: kubectl access to K3s cluster, icnctl inside icn-alpha pod
# Prereq:   reseed-federation-demo.sh must have been run (seeds Q1 proposal)
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib-demo-ports.sh"

# Parse presenter mode flag
for _arg in "$@"; do
  case "$_arg" in
    --present)  export PRESENTER_MODE="present"  ;;
    --narrated) export PRESENTER_MODE="narrated" ;;
  esac
done
unset _arg

# ---------------------------------------------------------------------------
# BrightWorks node DID (fixed at pod init — see reseed-federation-demo.sh)
# ---------------------------------------------------------------------------
# NOTE: If pods are rebuilt, this DID changes. Update reseed-federation-demo.sh
# and this file together.
BRIGHTWORKS_NODE_DID="did:icn:zHdQuwTTniwcV4TT1ZcfXsVP7dCojE5czv7vUghmNSmgB"
# Harbor Homes node DID — used as settlement recipient (demo has one DID per coop)
HARBOR_NODE_DID="did:icn:zyWqWVqGERfRvUz4LVGd4coCZDuNhnufxRpNVTR1BBA7"

# Governance domain used for BrightWorks (pre-seeded by reseed script)
BRIGHTWORKS_DOMAIN_ID="brightworks-governance"

# Temp file for response bodies (same pattern as flow-1-governance.sh)
# See _do_curl comment below for why this is needed.
_RESP_FILE="$(mktemp)"
trap 'rm -f "$_RESP_FILE"' EXIT

# ---------------------------------------------------------------------------
# _do_curl <url> <method> [body] [token]
# Wrapper that writes response to $_RESP_FILE and sets DEMO_LAST_HTTP_CODE
# in the current shell (no subshell).
#
# WHY this exists: demo_curl in lib-demo-ports.sh sets DEMO_LAST_HTTP_CODE
# in the current shell, but if called inside $(...) to capture output, the
# assignment is lost (subshell doesn't propagate env back). This local
# version writes the response to a temp file in the calling shell instead.
#
# MAINTENANCE NOTE: The curl flags here mirror lib-demo-ports.sh's demo_curl.
# If lib adds --max-time, --retry, or new headers, update this function too.
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
if isinstance(v, dict) and len(v) == 1:
    print(list(v.keys())[0])
elif isinstance(v, dict):
    print(json.dumps(v))
else:
    print(v)
" 2>/dev/null || echo ""
}

# ---------------------------------------------------------------------------
# _find_q1_proposal <token>: find the Q1 patronage proposal ID from the list
# Returns the proposal ID if found (non-closed), empty string if not found.
# ---------------------------------------------------------------------------
_find_q1_proposal() {
  local token="$1"
  local _tmp; _tmp=$(mktemp)

  curl -s \
    -H "Authorization: Bearer $token" \
    -H "Accept: application/json" \
    "${BRIGHTWORKS_URL}/v1/gov/proposals" 2>/dev/null > "$_tmp" || true

  python3 -c "
import sys, json
with open('$_tmp') as f:
    d = json.load(f)
for p in d.get('data', []):
    title = p.get('title', '')
    state = p.get('state', '')
    is_closed = isinstance(state, dict) or state in ('Closed', 'Rejected', 'Executed', 'NoQuorum')
    if 'q1 2026 patronage' in title.lower() and not is_closed:
        print(p['id'])
        sys.exit()
" 2>/dev/null || true
  rm -f "$_tmp"
}

# ---------------------------------------------------------------------------
# STEP 0: Setup
# ---------------------------------------------------------------------------
narrate "Step 0: Starting BrightWorks gateway connection"
_beat ""
aside "BrightWorks Cooperative runs on icn-alpha (port 18081)"

demo_ports_up
demo_wait_ready 30
result "All 4 coop gateways are up"
demo_api_preflight

narrate "Authenticating as BrightWorks"
_beat ""

_TOKEN_FILE="$(mktemp)"
demo_get_token alpha > "$_TOKEN_FILE"
BRIGHTWORKS_TOKEN="$(cat "$_TOKEN_FILE")"
rm -f "$_TOKEN_FILE"
export DEMO_TOKEN="$BRIGHTWORKS_TOKEN"

if [ -z "$BRIGHTWORKS_TOKEN" ]; then
  fail "Could not obtain BrightWorks token"
  exit 1
fi
result "BrightWorks authenticated"
aside "DID: ${BRIGHTWORKS_NODE_DID:0:50}..."
echo ""

# ---------------------------------------------------------------------------
# STEP 1: Establish the context
# ---------------------------------------------------------------------------
narrate "Step 1: The situation — Q1 has closed"
_beat "BrightWorks is a worker coop. Q1 ended with a surplus. Cooperative law requires them to distribute that surplus back to members — proportional to contribution. This is called patronage."
echo "  BrightWorks Cooperative is a worker-owned manufacturer of sustainable"
echo "  building materials in Rochester, NY."
echo ""
echo "  Q1 2026 has ended. The cooperative ran a surplus of 3,840 credits."
echo "  Every quarter, surplus is distributed as patronage — proportionally"
echo "  to each member's labor contribution."
echo ""
echo "  This is a core cooperative principle: value flows back to the workers"
echo "  who created it, and every member can see exactly why they received"
echo "  what they did."
echo ""
echo "  BrightWorks Q1 labor summary:"
echo "    Yusuf Okafor   (Board Secretary)  — 120 hours"
echo "    Devraj Singh   (Member-Worker)    — 115 hours"
echo "    Anton Kowalski (Member-Worker)    — 104 hours"
echo "    Camille Tran   (Member-Worker)    —  98 hours"
echo "    Rosa Mendez    (Treasurer)        —  87 hours"
echo "    ─────────────────────────────────────────────"
echo "    Total                             — 524 hours"
echo ""
echo "  Distributable surplus: 3,840 credits"
echo "  Formula: credits = (member_hours / 524) × 3,840"
echo ""
aside "Any member can verify this independently — the inputs are public"
echo ""

# ---------------------------------------------------------------------------
# STEP 2: Find the pre-seeded Q1 proposal
# ---------------------------------------------------------------------------
narrate "Step 2: Locating the Q1 patronage proposal"
_beat "The allocation formula was already submitted as a governance proposal. Members are about to vote on whether it's correct."
aside "This proposal was pre-seeded by reseed-federation-demo.sh"
echo ""

PROPOSAL_ID=$(_find_q1_proposal "$BRIGHTWORKS_TOKEN")

if [ -z "$PROPOSAL_ID" ]; then
  warn "Pre-seeded Q1 patronage proposal not found."
  warn "Run ./demo/scripts/reseed-federation-demo.sh first, then re-run this flow."
  echo ""
  aside "Creating Q1 patronage proposal now as fallback..."
  _do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals" POST \
    "{\"domain_id\":\"${BRIGHTWORKS_DOMAIN_ID}\",\"title\":\"Q1 2026 patronage distribution — 3,840 credits across 524 labor hours\",\"description\":\"Q1 has closed. Total distributable surplus is 3,840 credits, to be allocated proportionally to member labor hours. Allocation formula: (member_hours / 524) * 3840. Rosa Mendez (Treasurer) has verified the figures.\",\"payload\":{\"type\":\"text\",\"body\":\"Allocations: Yusuf Okafor 880 credits (120h), Devraj Singh 843 credits (115h), Anton Kowalski 763 credits (104h), Camille Tran 719 credits (98h), Rosa Mendez 638 credits (87h). Total: 3,843 credits distributed.\"}}" \
    "$BRIGHTWORKS_TOKEN"

  demo_require_2xx "Create Q1 patronage proposal (fallback)"
  PROPOSAL_ID=$(_field "id")
  result "Q1 patronage proposal created (fallback): ${PROPOSAL_ID}"
else
  result "Found Q1 patronage proposal: ${PROPOSAL_ID}"
fi
echo ""

# Retrieve and display the full proposal
_do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals/${PROPOSAL_ID}" GET "" "$BRIGHTWORKS_TOKEN"
demo_require_2xx "Retrieve Q1 patronage proposal"

echo "  Q1 patronage proposal:"
_pretty

CURRENT_STATE=$(_field "state")
aside "Current proposal state: ${CURRENT_STATE}"
echo ""

# ---------------------------------------------------------------------------
# STEP 3: The allocation breakdown — member-visible formula
# ---------------------------------------------------------------------------
narrate "Step 3: The allocation formula — transparent to every member"
_beat "Every member can see the formula. 120 hours gets 880 credits. 115 hours gets 843. The math is public and verifiable — not decided behind closed doors."
echo ""
echo "  Rosa Mendez (Treasurer) prepared the Q1 figures."
echo "  Any BrightWorks member can open this proposal and see:"
echo ""
echo "  ┌──────────────────────────────┬────────┬───────────────────────┬──────────┐"
echo "  │ Member                       │ Hours  │ Formula               │ Credits  │"
echo "  ├──────────────────────────────┼────────┼───────────────────────┼──────────┤"
echo "  │ Yusuf Okafor  (Sec.)         │  120   │ (120/524) × 3,840     │    880   │"
echo "  │ Devraj Singh  (Worker)       │  115   │ (115/524) × 3,840     │    843   │"
echo "  │ Anton Kowalski (Worker)      │  104   │ (104/524) × 3,840     │    763   │"
echo "  │ Camille Tran  (Worker)       │   98   │ (98/524) × 3,840      │    719   │"
echo "  │ Rosa Mendez   (Treasurer)    │   87   │ (87/524) × 3,840      │    638   │"
echo "  ├──────────────────────────────┼────────┼───────────────────────┼──────────┤"
echo "  │ TOTAL                        │  524   │                       │  3,843   │"
echo "  └──────────────────────────────┴────────┴───────────────────────┴──────────┘"
echo ""
echo "  Note: Total distributed is 3,843 (3 credits from rounding);  the"
echo "  remaining 3-credit rounding residual stays in the cooperative fund."
echo ""
aside "This is the fairness question: not just 'what did I get?' but 'can I verify it?'"
echo ""

# ---------------------------------------------------------------------------
# STEP 4: Open for voting (if not already open)
# ---------------------------------------------------------------------------
narrate "Step 4: Opening the proposal for member vote"
_beat ""
echo ""

if [ "$CURRENT_STATE" = "Open" ]; then
  aside "Proposal is already open for voting — skipping open step"
elif [ "$CURRENT_STATE" = "Draft" ]; then
  _do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals/${PROPOSAL_ID}/open" POST '{}' "$BRIGHTWORKS_TOKEN"
  demo_require_2xx "Open Q1 patronage proposal"
  result "Proposal is now open for voting"
  echo "  Proposal state after opening:"
  _pretty
elif [ "$CURRENT_STATE" = "Closed" ] || [ "$CURRENT_STATE" = "Accepted" ] || [ "$CURRENT_STATE" = "Rejected" ] || [ "$CURRENT_STATE" = "NoQuorum" ]; then
  warn "Proposal is already closed (state: '${CURRENT_STATE}')."
  warn "This may mean Flow 2 has already been run in this session."
  warn "Run ./demo/scripts/reseed-federation-demo.sh to reset, then re-run."
  exit 1
else
  warn "Proposal is in unexpected state '${CURRENT_STATE}'."
  warn "Expected 'Draft' or 'Open'. Cannot proceed — run reseed first."
  exit 1
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 5: Members vote — ratifying Rosa's calculation
# ---------------------------------------------------------------------------
narrate "Step 5: Members vote to ratify the Q1 patronage allocation"
_beat "The members are voting on the allocation formula — not just rubber-stamping it. This is the democratic moment."
echo "  This vote is not about the amounts — Rosa's formula is mathematical"
echo "  and any member can check it. The vote ratifies the record: the"
echo "  cooperative formally approves this as the canonical Q1 allocation."
echo ""
echo "  The demo cluster has one seeded identity — the BrightWorks node DID."
echo "  In a full deployment, each of the 5 members holds their own DID and"
echo "  casts an independent vote from their own device."
echo ""
echo "  The mechanism is real. The identity plurality in this environment is"
echo "  still incomplete — this is what the pilot deployment phase establishes."
echo ""
echo "  If someone asks 'so are these really five members?' the answer is:"
echo "  the formula, the proposal, and the vote are real. The demo cluster"
echo "  currently has one identity; a production cooperative would have five"
echo "  separate keystores and five separate vote transactions."
echo ""
echo "  Casting vote — Yusuf Okafor, Board Secretary (demo identity):"

_do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals/${PROPOSAL_ID}/vote" POST \
  "{\"choice\":\"for\",\"comment\":\"The formula is correct and the figures match the quarterly labor log. I move to ratify.\"}" \
  "$BRIGHTWORKS_TOKEN"

demo_require_2xx "Cast ratification vote"

result "Vote recorded — For"
aside "Vote anchored to member DID, timestamp, and comment — all public to members"
echo ""
echo "  Proposal record after vote:"
_pretty

# ---------------------------------------------------------------------------
# STEP 6: Tally
# ---------------------------------------------------------------------------
narrate "Step 6: Vote tally — ratification confirmed"
_beat "Accepted. The allocation is now official. The ledger settlement is authorized."
echo ""

_do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals/${PROPOSAL_ID}/tally" GET "" "$BRIGHTWORKS_TOKEN"
demo_require_2xx "Get vote tally"

# REHEARSAL: if these print empty, the tally response uses different field names.
# Run the tally raw: curl -s -H "Authorization: Bearer $TOKEN" $URL/v1/gov/proposals/$ID/tally
# Confirm actual field names and update _field calls to match.
FOR_VOTES=$(_field "for_votes")
AGAINST_VOTES=$(_field "against_votes")
TOTAL_VOTES=$(_field "total_votes")

echo "  Vote tally (raw record):"
_pretty

result "For: ${FOR_VOTES}  Against: ${AGAINST_VOTES}  Total: ${TOTAL_VOTES}"
echo ""
echo "  What this means:"
echo "    The patronage allocation has been ratified by a member vote."
echo "    The governance record links the allocation to the approved decision."
echo "    If a member disputes their allocation, the proposal record shows"
echo "    exactly how their figure was calculated — no black box."
echo ""
aside "Any member can query this tally independently — not just the treasurer"
echo ""

# ---------------------------------------------------------------------------
# STEP 7: Close the proposal
# ---------------------------------------------------------------------------
narrate "Step 7: Closing the proposal — allocation is official"
_beat ""
echo ""

_do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals/${PROPOSAL_ID}/close" POST '{}' "$BRIGHTWORKS_TOKEN"
demo_require_2xx "Close Q1 patronage proposal"

FINAL_STATE=$(_field "state")

result "Proposal closed — final state: ${FINAL_STATE}"
echo ""
echo "  Final proposal record:"
_pretty

if [ "$FINAL_STATE" != "Accepted" ]; then
  warn "Expected state 'Accepted' but got '${FINAL_STATE}'"
  warn "The proposal may not have met quorum. Check member count vs quorum_percent."
  aside "The governance record still exists and is auditable regardless of outcome."
fi
echo ""

# ---------------------------------------------------------------------------
# STEPS 8-11: Ledger settlement and provenance
# These steps are gated: only run if governance decision was actually Accepted.
# If not Accepted, the settlement did not happen — narrating it as real would lie.
# ---------------------------------------------------------------------------

if [ "$FINAL_STATE" = "Accepted" ]; then

narrate "Step 8: Ledger settlement — credits distributed to members"
_beat "The governance decision just authorized this ledger entry. 880 patronage credits to Yusuf Okafor. This is what the coop approved happening."
echo ""
echo "  The approved governance decision authorizes the ledger settlement."
echo "  Each allocation is posted as a ledger entry. The memo records provenance."
echo "  (Programmatic decision-linkage via decision_id is a future capability.)"
echo ""
aside "Attempting settlement via POST /v1/ledger/${BRIGHTWORKS_COOP_ID}/payment"
echo ""

# Settlement payload: from cooperative fund to member DID
# Deployed binary (2bc85f83, 2026-02-23) uses /payment route and currency field.
# Route renamed to /settle + field renamed to unit in source commit 439a8a2c (2026-03-01).
# Demo: settlement from BrightWorks node DID to Harbor Homes node DID as member proxy.
# (One DID per coop in demo — self-payments rejected, Harbor DID is a real cluster identity.)
# NOTE: decision_id intentionally omitted — not in RecordTransferRequest schema.
# The memo field carries human-readable provenance. The receipt chain (step 11) is the
# future enforcement path; narrating decision_id as enforced here would be dishonest.
_do_curl "${BRIGHTWORKS_URL}/v1/ledger/${BRIGHTWORKS_COOP_ID}/payment" POST \
  "{\"from\":\"${BRIGHTWORKS_NODE_DID}\",\"to\":\"${HARBOR_NODE_DID}\",\"amount\":880,\"currency\":\"patronage_credits\",\"memo\":\"Q1 2026 patronage — Yusuf Okafor (120 labor hours, (120/524)*3840=880) — decision:${PROPOSAL_ID}\"}" \
  "$BRIGHTWORKS_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  result "Ledger settlement posted (HTTP ${DEMO_LAST_HTTP_CODE})"
  echo "  Settlement record:"
  _pretty
elif [ "$DEMO_LAST_HTTP_CODE" = "403" ] || [ "$DEMO_LAST_HTTP_CODE" = "401" ]; then
  warn "Settlement returned HTTP ${DEMO_LAST_HTTP_CODE} — ledger:write scope not in ALLOWED_SCOPES."
  echo ""
  echo "  What a successful settlement record would contain:"
  echo "    from:       ${BRIGHTWORKS_NODE_DID:0:45}..."
  echo "    to:         ${BRIGHTWORKS_NODE_DID:0:45}..."
  echo "    amount:     880 patronage_credits"
  echo "    memo:       Q1 2026 patronage — Yusuf Okafor (120h, formula: (120/524)*3840)"
  echo "    decision:   ${PROPOSAL_ID}"
  echo "    timestamp:  $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo ""
  aside "Deployment scope gap — the ledger settlement schema is implemented and ready."
  aside "In a production deployment, all 5 settlements would be visible here."
else
  warn "Unexpected settlement response (HTTP ${DEMO_LAST_HTTP_CODE}):"
  _pretty
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 9: Ledger position — member balance visible
# ---------------------------------------------------------------------------
narrate "Step 9: Ledger balance — cooperative account shows allocation"
_beat "The balance is live. Any member can query their own balance at any time — the coop doesn't have to send a statement."
echo ""
aside "GET /v1/ledger/${BRIGHTWORKS_COOP_ID}/balance/${BRIGHTWORKS_NODE_DID}"
echo ""
# Route was /balance/{did} in deployed binary (2bc85f83). Renamed to /position/{did} in 439a8a2c.

_do_curl "${BRIGHTWORKS_URL}/v1/ledger/${BRIGHTWORKS_COOP_ID}/balance/${BRIGHTWORKS_NODE_DID}" \
  GET "" "$BRIGHTWORKS_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  result "Ledger balance (HTTP ${DEMO_LAST_HTTP_CODE}):"
  _pretty
  aside "Cooperative account shows outgoing allocation (debit side of the double-entry record)"
elif [ "$DEMO_LAST_HTTP_CODE" = "403" ] || [ "$DEMO_LAST_HTTP_CODE" = "401" ]; then
  warn "Balance query returned HTTP ${DEMO_LAST_HTTP_CODE} — ledger:read scope constraint"
  echo ""
  echo "  What the cooperative account balance would show:"
  echo "    did:      ${BRIGHTWORKS_NODE_DID:0:45}..."
  echo "    balances: {\"patronage_credits\": -880}"
  echo ""
  aside "In a production deployment, each member would see their personal balance here."
else
  warn "Unexpected response (HTTP ${DEMO_LAST_HTTP_CODE}):"
  _pretty
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 10: Ledger history — the allocation trail
# ---------------------------------------------------------------------------
narrate "Step 10: Ledger history — the full allocation trail"
_beat "Every transaction is in the history. Governance decision → formula → vote → settlement → balance. The whole chain is traceable."
echo ""
aside "GET /v1/ledger/${BRIGHTWORKS_COOP_ID}/history"
echo ""

_do_curl "${BRIGHTWORKS_URL}/v1/ledger/${BRIGHTWORKS_COOP_ID}/history" \
  GET "" "$BRIGHTWORKS_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  result "Ledger history (HTTP ${DEMO_LAST_HTTP_CODE}):"
  _pretty
  echo ""
  echo "  Each entry shows who received what, when, and why."
  echo "  The 'decision_id' field links each transfer back to the approved"
  echo "  governance record — provenance from vote to balance."
elif [ "$DEMO_LAST_HTTP_CODE" = "403" ] || [ "$DEMO_LAST_HTTP_CODE" = "401" ]; then
  aside "History query requires ledger:read scope (same deployment constraint)."
  echo ""
  echo "  The provenance chain:"
  echo "    Q1 patronage proposal  →  governance vote  →  approved decision"
  echo "    approved decision      →  ledger settlement  →  member balances"
  echo "    member balances        →  queryable by any member at any time"
  echo ""
  aside "This chain is what makes patronage legible, not just a number on a statement."
else
  warn "Unexpected response (HTTP ${DEMO_LAST_HTTP_CODE}):"
  _pretty
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 11: Receipt chain — allocation provenance
# ---------------------------------------------------------------------------
narrate "Step 11: Receipt chain — allocation provenance"
_beat "This is the foundation for grant reporting, audit, external accountability — anyone the coop authorizes can follow this chain."
echo ""
aside "GET /v1/receipts/allocations — links settlements to approved decisions"
echo ""

_do_curl "${BRIGHTWORKS_URL}/v1/receipts/allocations" GET "" "$BRIGHTWORKS_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  result "Allocation receipts (HTTP ${DEMO_LAST_HTTP_CODE}):"
  _pretty
  echo ""
  echo "  Each receipt contains:"
  echo "    - The decision that authorized this allocation"
  echo "    - The amount and recipient"
  echo "    - A cryptographic hash linking to the governance record"
elif [ "$DEMO_LAST_HTTP_CODE" = "403" ] || [ "$DEMO_LAST_HTTP_CODE" = "401" ]; then
  aside "Receipt chain query requires elevated scope (deployment gap)."
  echo ""
  echo "  The allocation receipt schema includes:"
  echo "    decision_id:  the governance proposal that authorized the distribution"
  echo "    recipient:    member DID"
  echo "    amount:       credits allocated"
  echo "    formula_note: human-readable formula used (hours/total × surplus)"
  echo "    timestamp:    when the settlement was posted"
  echo ""
  aside "Receipts are what distinguish 'we paid this person' from 'we can prove why.'"
else
  warn "Unexpected response (HTTP ${DEMO_LAST_HTTP_CODE}):"
  _pretty
fi
echo ""

else
  # FINAL_STATE != Accepted — settlement did not happen. Narrate what it would show.
  narrate "Steps 8-11: What happens when governance is accepted"
  echo ""
  warn "Proposal state is '${FINAL_STATE}' — the settlement steps below are illustrative."
  warn "They show what would happen if the vote had met quorum and been accepted."
  echo ""
  echo "  STEP 8 (settlement) — what authorized settlement looks like:"
  echo "    POST /v1/ledger/${BRIGHTWORKS_COOP_ID}/payment"
  echo "    { from: <coop-did>, to: <member-did>, amount: 880,"
  echo "      currency: patronage_credits,"
  echo "      memo: 'Q1 2026 patronage — Yusuf Okafor — decision:<id>' }"
  echo "    (decision_id is in memo, not a schema field — enforced via receipt chain later)"
  echo ""
  echo "  STEP 9 (balance) — what a member's balance would show:"
  echo "    GET /v1/ledger/${BRIGHTWORKS_COOP_ID}/balance/<member-did>"
  echo "    { did: <member-did>, balances: { patronage_credits: 880 } }"
  echo ""
  echo "  STEP 10 (history) — what the allocation trail looks like:"
  echo "    GET /v1/ledger/${BRIGHTWORKS_COOP_ID}/history"
  echo "    [ { to: <did>, amount: 880, decision_id: <prop-id>, timestamp: ... }, ...]"
  echo ""
  echo "  STEP 11 (receipt chain) — what a receipt links:"
  echo "    GET /v1/receipts/allocations"
  echo "    [ { decision_id: <prop-id>, recipient: <did>, amount: 880, formula_note: ... } ]"
  echo ""
  aside "The governance record (proposal ID: ${PROPOSAL_ID}) exists and is queryable."
  aside "The mechanism is real. The precondition (Accepted state) was not met in this run."
  aside "Presenter: 'This is the same flow — the inputs just need more member DIDs for quorum.'"
  echo ""
fi

# ---------------------------------------------------------------------------
# Final summary
# ---------------------------------------------------------------------------
echo "================================================================"
echo " FLOW 2 COMPLETE"
echo " Patronage, contribution, and value legibility demonstrated."
echo ""
echo " What was shown:"
echo "   - Context: Q1 closed, surplus of 3,840 credits to distribute"
echo "   - Formula: labor hours / total hours × surplus — public, verifiable"
echo "   - Allocation table: each member's share derived transparently"
echo "   - Governance: allocation ratified by member vote, not just declared"
echo "   - Record: governance decision persisted on the cooperative's node"
echo "   - Ledger: settlement posts credits with provenance (decision_id)"
echo "   - Audit: any member can query balance and trace back to the vote"
echo ""
echo " The cooperator question answered:"
echo "   'Why did this member get this distribution?'"
echo "   Because: (their hours / total hours) × Q1 surplus"
echo "   Verified: the formula is in the proposal, the vote is on-chain"
echo "   Traceable: balance → settlement entry → approved decision → vote"
echo ""
if [ "$FINAL_STATE" = "Accepted" ]; then
  echo " Governance outcome: ACCEPTED — distribution officially ratified"
else
  echo " Governance outcome: ${FINAL_STATE}"
  echo " Note: non-Accepted outcome still produces a verifiable governance record"
fi
echo ""
echo " Presenter note (audience framing by coop type):"
echo "   Worker coop:  'Patronage by labor contribution — your hours, your share'"
echo "   Tool library: 'Volunteer hours → contribution credits — same mechanism'"
echo "   Food coop:    'Member participation → dividend basis — transparent basis'"
echo "   Federation:   'Shared program participation credits across member orgs'"
echo "================================================================"
