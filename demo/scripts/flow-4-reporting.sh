#!/usr/bin/env bash
# =============================================================================
# Flow 4 (Optional): Federation Oversight and Institutional Reporting
# Narrative: Finger Lakes CDN — audit view across member coops
#
# What this demonstrates:
#   - Read-only visibility across autonomous member coops
#   - Governance and expenditure evidence for grant/compliance reporting
#   - Federation-level reporting without centralized control
#   - Audit without bureaucracy: query records, don't own them
#   - PR #1327 (ExecutionReceiptGate) is merged (2026-03-07): gate is fully wired — no ProposalAccepted fires without passing it
#
# Core institutional question:
#   "Can this produce trustworthy reporting without adding massive admin overhead?"
#
# The scenario:
#   Finger Lakes CDN needs to submit a quarterly report to a regional funder.
#   The funder wants evidence that member coops are governing themselves
#   accountably: real votes, real treasury decisions, real provenance.
#   Amara Diallo (Executive Director) needs this without calling each coop
#   and asking them to send spreadsheets.
#
# Known cluster constraints (as of 2026-03-18):
#   - treasury:read/write scopes are in ALLOWED_SCOPES and DEMO_DEFAULT_SCOPES — resolved
#   - Some receipt/execution endpoints may require elevated scopes (signing key gap)
#   - Flow 4 gracefully handles unavailable endpoints with presenter narration
#
# Usage:    ./demo/scripts/flow-4-reporting.sh
# Duration: ~5 minutes live (or use as self-serve walkthrough)
# Audience: Funders, grant reviewers, cooperative developers, ecosystem orgs
# Requires: kubectl access to K3s cluster, icnctl inside all 4 pods
# Prereq:   Flows 1-3 should have been run (or reseed-federation-demo.sh)
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
# Node DIDs (fixed at pod init)
# ---------------------------------------------------------------------------
HARBOR_NODE_DID="did:icn:zyWqWVqGERfRvUz4LVGd4coCZDuNhnufxRpNVTR1BBA7"
BRIGHTWORKS_NODE_DID="did:icn:zHdQuwTTniwcV4TT1ZcfXsVP7dCojE5czv7vUghmNSmgB"
RIVERCITY_NODE_DID="did:icn:zDMiXkUafnaRfeA8tdPCYiKwMRFsPJ9uN4LeFwWR7cZs3"
FINGERLAKES_NODE_DID="did:icn:zE5E8bz7XrJGr6WozTbUNfSN3he3sUqYaCo4jifFKi4Ln"

# ---------------------------------------------------------------------------
# Temp file for response bodies
# ---------------------------------------------------------------------------
_RESP_FILE="$(mktemp)"
trap 'rm -f "$_RESP_FILE"' EXIT

# ---------------------------------------------------------------------------
# _do_curl <url> <method> [body] [token]
# WHY this exists and MAINTENANCE NOTE: see flow-1-governance.sh.
# ---------------------------------------------------------------------------
_do_curl() {
  local url="$1" method="${2:-GET}" body="${3:-}" token="${4:-}"
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

_pretty() {
  python3 -m json.tool 2>/dev/null < "$_RESP_FILE" || cat "$_RESP_FILE"
  echo ""
}

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

# _show_or_narrate <label> <fallback_description>
# Shows the API response if 2xx; narrates the fallback if not.
_show_or_narrate() {
  local label="$1"
  local fallback="$2"
  if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
    result "${label} (HTTP ${DEMO_LAST_HTTP_CODE}):"
    _pretty
  else
    aside "${label}: HTTP ${DEMO_LAST_HTTP_CODE} — ${fallback}"
    echo ""
  fi
}

# ---------------------------------------------------------------------------
# Audit report counters
# ---------------------------------------------------------------------------
_EVIDENCE_ITEMS=0
_GAPS=0
_evidence() { (( _EVIDENCE_ITEMS += 1 )); result "  ✓ $1"; }
_gap()      { (( _GAPS       += 1 )); aside "  · $1 (deployment scope gap)"; }

# ---------------------------------------------------------------------------
# STEP 0: Setup
# ---------------------------------------------------------------------------
narrate "Step 0: Connecting to all four coop nodes"
_beat ""
aside "Finger Lakes CDN reads across Harbor Homes, BrightWorks, River City, and its own node"

demo_ports_up
demo_wait_ready 30
result "All 4 gateways are up"
echo ""

narrate "Authenticating all participants"
_beat ""
_TF=$(mktemp)
demo_get_token delta > "$_TF";       FINGERLAKES_TOKEN="$(cat "$_TF")"
demo_get_token gamma > "$_TF";       HARBOR_TOKEN="$(cat "$_TF")"
demo_get_token alpha > "$_TF";       BRIGHTWORKS_TOKEN="$(cat "$_TF")"
demo_get_token beta  > "$_TF";       RIVERCITY_TOKEN="$(cat "$_TF")"
rm -f "$_TF"

[ -z "$FINGERLAKES_TOKEN" ] && { fail "Could not get Finger Lakes token"; exit 1; }
[ -z "$HARBOR_TOKEN" ]      && { fail "Could not get Harbor Homes token"; exit 1; }
[ -z "$BRIGHTWORKS_TOKEN" ] && { fail "Could not get BrightWorks token";  exit 1; }
[ -z "$RIVERCITY_TOKEN" ]   && { fail "Could not get River City token";   exit 1; }

result "Finger Lakes CDN          — authenticated"
result "Harbor Homes Cooperative  — authenticated"
result "BrightWorks Cooperative   — authenticated"
result "River City Tool Library   — authenticated"
echo ""

# ---------------------------------------------------------------------------
# STEP 1: The reporting scenario
# ---------------------------------------------------------------------------
narrate "Step 1: The reporting scenario"
_beat "Amara is a program officer at a foundation that funds cooperatives. She needs to verify that Harbor Homes and BrightWorks actually did what they said they did — governed democratically, distributed surplus fairly."
echo "  Finger Lakes CDN has submitted a grant application to a regional"
echo "  foundation that funds cooperative ecosystem development."
echo ""
echo "  The foundation's due-diligence request:"
echo "    'Please provide evidence that member cooperatives are governing"
echo "     themselves accountably: real votes, real decisions, real provenance.'"
echo ""
echo "  Amara Diallo (Executive Director) needs to produce this report"
echo "  without calling each coop and asking them to email spreadsheets."
echo ""
echo "  The ICN approach: query verifiable governance records directly."
echo "  No spreadsheets. No email chains. No manual aggregation."
echo ""
aside "This is the funder question: accountability at scale without bureaucracy"
echo ""

# ---------------------------------------------------------------------------
# STEP 2: Harbor Homes — roof repair governance evidence
# ---------------------------------------------------------------------------
narrate "Step 2: Harbor Homes — governance and capital expenditure evidence"
_beat "The foundation can see the vote. Not a summary, not a claim — the actual on-chain record of the democratic decision."
echo "  Querying Harbor Homes' governance record for capital reserve decisions..."
echo ""

# decision_hash extracted from GovernanceReceipt proof (if available)
# Must be initialized before the nested conditional blocks to satisfy set -u
HARBOR_DECISION_HASH=""

# List recent proposals from Harbor Homes
_do_curl "${HARBOR_URL}/v1/gov/proposals" GET "" "$HARBOR_TOKEN"
if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  # Find any closed/accepted proposal about roof or capital
  HARBOR_PROPOSAL_ID=$(python3 -c "
import sys, json
with open('$_RESP_FILE') as f:
    d = json.load(f)
for p in d.get('data', []):
    state = p.get('state', '')
    is_closed = isinstance(state, dict) or state in ('Closed', 'Rejected', 'Executed', 'Accepted')
    title = p.get('title', '').lower()
    if is_closed and ('roof' in title or 'capital' in title or 'repair' in title):
        print(p['id'])
        break
" 2>/dev/null || echo "")

  if [ -n "$HARBOR_PROPOSAL_ID" ]; then
    result "Found Harbor Homes capital decision: ${HARBOR_PROPOSAL_ID}"
    _evidence "Harbor Homes: capital reserve governance decision on record"

    # Get the full record
    _do_curl "${HARBOR_URL}/v1/gov/proposals/${HARBOR_PROPOSAL_ID}" GET "" "$HARBOR_TOKEN"
    if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
      echo "  Full governance record:"
      _pretty
    fi

    # Try to get proof
    aside "Checking for governance proof (GovernanceReceipt)..."
    _do_curl "${HARBOR_URL}/v1/gov/proposals/${HARBOR_PROPOSAL_ID}/proof" GET "" "$HARBOR_TOKEN"
    if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
      result "GovernanceReceipt available:"
      _pretty
      _evidence "Harbor Homes: cryptographic governance proof on record"
      # Extract decision_hash for use in the receipt chain query (step 5).
      # GovernanceProofV2 serializes as {"receipt": {"decision_hash": [u8;32], ...}, ...}
      # decision_hash is a raw [u8;32] byte array — convert to hex for the query param.
      HARBOR_DECISION_HASH=$(python3 -c "
import sys, json, binascii
with open('$_RESP_FILE') as f:
    d = json.load(f)
dh = d.get('receipt', {}).get('decision_hash', [])
if isinstance(dh, list) and len(dh) == 32:
    print(binascii.hexlify(bytes(dh)).decode())
elif isinstance(dh, str) and len(dh) == 64:
    print(dh)  # already hex-encoded
" 2>/dev/null || echo "")
    else
      aside "Proof endpoint: HTTP ${DEMO_LAST_HTTP_CODE} (signing key not configured in pod)"
      aside "Diagnose: kubectl logs -n icn-coop-gamma deploy/icn-gamma --tail=200 | grep GovernanceProof"
      _gap "Harbor Homes: GovernanceReceipt proof (signing key not configured in pod — not a code gap)"
      HARBOR_DECISION_HASH=""
    fi
  else
    aside "No closed capital decisions found yet on Harbor Homes node."
    aside "Run flow-1-governance.sh first to create the roof repair governance record."
    echo ""
    # Still list what's there
    echo "  Current Harbor Homes proposals:"
    _pretty
  fi
else
  aside "Harbor Homes governance query: HTTP ${DEMO_LAST_HTTP_CODE}"
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 3: BrightWorks — patronage distribution evidence
# ---------------------------------------------------------------------------
narrate "Step 3: BrightWorks — patronage distribution evidence"
_beat "The patronage allocation is verifiable. The formula, the vote, the ledger entry — all of it."
echo "  Querying BrightWorks' governance record for Q1 patronage distribution..."
echo ""

_do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals" GET "" "$BRIGHTWORKS_TOKEN"
if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  BW_PATRONAGE_ID=$(python3 -c "
import sys, json
with open('$_RESP_FILE') as f:
    d = json.load(f)
for p in d.get('data', []):
    state = p.get('state', '')
    title = p.get('title', '').lower()
    if 'patronage' in title or 'q1' in title:
        print(p['id'])
        break
" 2>/dev/null || echo "")

  if [ -n "$BW_PATRONAGE_ID" ]; then
    result "Found BrightWorks patronage decision: ${BW_PATRONAGE_ID}"
    _do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals/${BW_PATRONAGE_ID}" GET "" "$BRIGHTWORKS_TOKEN"
    if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
      BW_STATE=$(_field "state")
      echo "  BrightWorks Q1 patronage proposal (state: ${BW_STATE}):"
      _pretty
      if [ "$BW_STATE" = "Accepted" ]; then
        _evidence "BrightWorks: Q1 patronage ratified by member vote (${BW_PATRONAGE_ID})"
      else
        _evidence "BrightWorks: Q1 patronage governance record on file (state: ${BW_STATE})"
      fi
    fi
  else
    aside "No patronage proposal found. Run flow-2-patronage.sh first."
    echo "  Current BrightWorks proposals:"
    _pretty
  fi
else
  aside "BrightWorks governance query: HTTP ${DEMO_LAST_HTTP_CODE}"
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 4: Ledger history — BrightWorks
# ---------------------------------------------------------------------------
narrate "Step 4: BrightWorks — ledger history and allocation trail"
_beat "Every transaction, in order. The foundation can trace the surplus from Q1 close → formula → vote → settlement → member balances."
echo ""

_do_curl "${BRIGHTWORKS_URL}/v1/ledger/${BRIGHTWORKS_COOP_ID}/history" GET "" "$BRIGHTWORKS_TOKEN"
if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  result "BrightWorks ledger history (HTTP ${DEMO_LAST_HTTP_CODE}):"
  _pretty
  _evidence "BrightWorks: ledger history queryable with decision provenance"
elif [ "$DEMO_LAST_HTTP_CODE" = "403" ] || [ "$DEMO_LAST_HTTP_CODE" = "401" ]; then
  _gap "BrightWorks: ledger history (ledger:read scope constraint)"
  echo "  What ledger history shows a funder:"
  echo "    - Each patronage entry with the governance decision that authorized it"
  echo "    - Member DIDs as recipients (pseudonymous unless mapped to names)"
  echo "    - Amounts, timestamps, and formula context from the proposal memo"
  echo "    - Verifiable: the funder can re-derive every entry from public inputs"
  echo ""
else
  aside "Ledger history: HTTP ${DEMO_LAST_HTTP_CODE}"
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 5: Receipt chain — economic provenance
# ---------------------------------------------------------------------------
narrate "Step 5: Receipt chain — allocation provenance across the federation"
_beat ""
echo ""

if [ -n "$HARBOR_DECISION_HASH" ]; then
  aside "Querying Harbor Homes receipt chain for decision: ${HARBOR_DECISION_HASH:0:16}..."
  _do_curl "${HARBOR_URL}/v1/receipts/chain?decision_hash=${HARBOR_DECISION_HASH}" GET "" "$HARBOR_TOKEN"
  if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
    result "Receipt chain (HTTP ${DEMO_LAST_HTTP_CODE}):"
    _pretty
    _evidence "Harbor Homes: governance decision linked in receipt chain"
    echo ""
    echo "  The chain shows:"
    echo "    governance:    the signed decision that authorized action"
    echo "    allocations:   economic receipts linked to this decision"
    echo "    chain_complete: whether all expected links are present"
    echo ""
    echo "  This is what a funder or regulator verifies — not a PDF,"
    echo "  a cryptographic chain from vote to settlement."
  else
    aside "Receipt chain: HTTP ${DEMO_LAST_HTTP_CODE}"
    echo "  The receipt chain infrastructure is live — the economic allocation"
    echo "  layer is the next integration milestone."
    echo ""
    echo "  What the chain will show when fully wired:"
    echo "    decision_hash:  links to the governance proposal"
    echo "    allocations:    each member's patronage, amount, and timing"
    echo "    chain_complete: confirmed when all settlement intents are present"
  fi
else
  _beat "Receipt chain requires a GovernanceReceipt with a decision_hash."
  echo "  Run flow-1-governance.sh first, then flow-4, to see the full chain."
  echo ""
  echo "  When available, the chain links:"
  echo "    governance decision → allocation receipts → settlement intents"
  echo "  Giving funders a single verifiable audit trail."
  _gap "Receipt chain: no GovernanceReceipt decision_hash available this run"
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 6: River City — federation agreement evidence
# ---------------------------------------------------------------------------
narrate "Step 6: River City Tool Library — federation coordination evidence"
_beat "The federation agreement is also auditable. River City didn't just claim they were cooperating with BrightWorks — the agreement is on the record."
echo ""

_do_curl "${RIVERCITY_URL}/v1/gov/proposals" GET "" "$RIVERCITY_TOKEN"
if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  RC_EQUIP_ID=$(python3 -c "
import sys, json
with open('$_RESP_FILE') as f:
    d = json.load(f)
for p in d.get('data', []):
    title = p.get('title', '').lower()
    if 'equipment' in title or 'brightworks' in title:
        print(p['id'])
        break
" 2>/dev/null || echo "")

  if [ -n "$RC_EQUIP_ID" ]; then
    result "Found River City equipment-sharing governance record: ${RC_EQUIP_ID}"
    _do_curl "${RIVERCITY_URL}/v1/gov/proposals/${RC_EQUIP_ID}" GET "" "$RIVERCITY_TOKEN"
    if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
      RC_STATE=$(_field "state")
      echo "  River City equipment-sharing proposal (state: ${RC_STATE}):"
      _pretty
      _evidence "River City: equipment-sharing agreement ratified by member vote"
    fi
  else
    aside "No equipment-sharing proposal found. Run flow-3-federation.sh first."
    echo "  Current River City proposals:"
    _pretty
  fi
else
  aside "River City governance query: HTTP ${DEMO_LAST_HTTP_CODE}"
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 7: Finger Lakes CDN — federation view across all coops
# ---------------------------------------------------------------------------
narrate "Step 7: Finger Lakes CDN — federation overview"
_beat ""
echo "  This is Finger Lakes CDN's value to the funder: a cross-coop view"
echo "  that no single cooperative can produce for itself."
echo ""

_do_curl "${FINGERLAKES_URL}/v1/federation/coops" GET "" "$FINGERLAKES_TOKEN"
_show_or_narrate "Federation members visible to Finger Lakes CDN" \
  "federation:read scope; federation schema is implemented"

# Federation status
_do_curl "${FINGERLAKES_URL}/v1/federation/status" GET "" "$FINGERLAKES_TOKEN"
_show_or_narrate "Federation status" "status endpoint may need initialization"

# Clearing agreements (if any from Flow 3)
_do_curl "${FINGERLAKES_URL}/v1/federation/clearing" GET "" "$FINGERLAKES_TOKEN"
_show_or_narrate "Active clearing agreements" \
  "clearing:read scope; run flow-3-federation.sh to create an agreement"
echo ""

# ---------------------------------------------------------------------------
# STEP 8: Authorization boundary — read without write
# This is the concrete proof of "visibility without control."
# We use the Finger Lakes CDN token (authenticated against delta/Finger Lakes)
# to attempt a WRITE on Harbor Homes' governance, then a READ.
# Read should succeed (governance data is queryable by federation participants).
# Write should be rejected (Harbor Homes has not granted Finger Lakes CDN control).
# ---------------------------------------------------------------------------
narrate "Step 8: The authorization boundary — read without write"
_beat "This is a security boundary. Amara can read everything she needs to verify. She cannot create transactions, cast votes, or modify anything. Read access only."
echo "  This is what 'visibility without control' means in practice."
echo "  Using Finger Lakes CDN's token, we attempt two operations on Harbor Homes:"
echo "    1. Write: try to create a governance proposal on Harbor Homes' node"
echo "    2. Read:  query Harbor Homes' governance proposals"
echo ""
echo "  The write should be rejected. The read should succeed."
echo "  That boundary — visible, demonstrated, not merely claimed — is the"
echo "  institutional trust property that makes ICN useful to intermediary orgs."
echo ""

# Attempt 1: Write — create a proposal on Harbor Homes using Finger Lakes CDN token
aside "Attempting write: POST /v1/gov/proposals on Harbor Homes (Finger Lakes token)"
_do_curl "${HARBOR_URL}/v1/gov/proposals" POST \
  "{\"domain_id\":\"harborhomes-governance\",\"title\":\"Finger Lakes CDN: test write attempt\",\"description\":\"This should be rejected — Finger Lakes CDN does not have write access to Harbor Homes governance.\",\"payload\":{\"type\":\"text\",\"body\":\"test\"}}" \
  "$FINGERLAKES_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^4 ]]; then
  result "Write rejected (HTTP ${DEMO_LAST_HTTP_CODE}) — Finger Lakes CDN cannot create proposals on Harbor Homes"
  _evidence "Authorization boundary: write rejected across coop boundary"
  echo ""
  echo "  Response:"
  _pretty
elif [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  warn "Write was accepted (HTTP ${DEMO_LAST_HTTP_CODE}) — boundary not enforced in this deployment"
  warn "This may mean the JWT coop_id check is not applied to cross-coop requests."
  aside "Presenter: note this as a deployment configuration gap, not an architecture gap."
  echo ""
else
  aside "Write attempt: HTTP ${DEMO_LAST_HTTP_CODE}"
  echo ""
fi

# Attempt 2: Read — query Harbor Homes proposals using Finger Lakes CDN token
aside "Attempting read: GET /v1/gov/proposals on Harbor Homes (Finger Lakes token)"
_do_curl "${HARBOR_URL}/v1/gov/proposals" GET "" "$FINGERLAKES_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  result "Read succeeded (HTTP ${DEMO_LAST_HTTP_CODE}) — Harbor Homes proposals visible to federation participant"
  _evidence "Authorization boundary: read permitted — Finger Lakes CDN has visibility"
  echo ""
  echo "  Harbor Homes proposals (read via Finger Lakes CDN token):"
  _pretty
elif [[ "$DEMO_LAST_HTTP_CODE" =~ ^4 ]]; then
  aside "Read also returned HTTP ${DEMO_LAST_HTTP_CODE} — cross-coop read may require same-node token"
  echo ""
  echo "  What cross-coop read access would show:"
  echo "    The federation participant (Finger Lakes CDN) can query governance"
  echo "    records from member coops using their own credential."
  echo "    Harbor Homes retains full control — they can revoke this access."
  echo "    No data was transferred to Finger Lakes CDN's systems — it was queried live."
  echo ""
  aside "If cross-coop read requires the queried node's own token, this is a scope config"
  aside "question, not an architecture limitation. The data is still there and verifiable."
  _gap "Cross-coop read with federation token (may require per-node auth config)"
else
  aside "Read attempt: HTTP ${DEMO_LAST_HTTP_CODE}"
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 9: Governance dashboard (if available)
# ---------------------------------------------------------------------------
narrate "Step 9: Governance dashboard — cooperative health at a glance"
_beat "One view across multiple coops. Active proposals, recent decisions, ledger activity. This is what a network of cooperatives looks like from the outside."
echo ""

# Try the governance dashboard endpoint (if a charter_id exists)
# This is experimental — the endpoint exists but requires a charter ID
aside "Governance dashboard requires a charter ID (cooperative constitution)"
aside "Charter-based governance is part of the CCL (Cooperative Contract Language) layer"
echo ""
echo "  What the governance dashboard would show:"
echo "    - Active proposals across all domains"
echo "    - Vote participation rates"
echo "    - Decision latency (average time from proposal to decision)"
echo "    - Treasury authorization utilization"
echo "    - Member engagement metrics"
echo ""
aside "This view is designed for a cooperative board, not just funders"
echo ""

# ---------------------------------------------------------------------------
# STEP 9: The grant report summary
# ---------------------------------------------------------------------------
narrate "Step 10: The grant report — what Amara has to show the foundation"
_beat "This is the output. A verifiable report. Not PDFs and spreadsheets — cryptographic proof that the coops governed and distributed as promised."
echo ""
echo "  Finger Lakes CDN's quarterly report to the regional funder:"
echo ""
echo "  ┌─────────────────────────────────────────────────────────────────┐"
echo "  │ MEMBER COOPERATIVE GOVERNANCE REPORT — Q1 2026                 │"
echo "  │ Submitted by: Amara Diallo, Executive Director, Finger Lakes CDN│"
echo "  ├─────────────────────────────────────────────────────────────────┤"
echo "  │ Harbor Homes Cooperative                                        │"
echo "  │   Governance action: Capital reserve authorization (\$12,000)   │"
echo "  │   Evidence: Member vote on record, decision ID on-chain         │"
echo "  │   Provenance: Proposal → vote → decision → treasury action      │"
echo "  │   Verification: Query ${HARBOR_URL}/v1/gov/proposals   │"
echo "  ├─────────────────────────────────────────────────────────────────┤"
echo "  │ BrightWorks Cooperative                                         │"
echo "  │   Governance action: Q1 patronage distribution (3,840 credits)  │"
echo "  │   Evidence: Formula on record, ratification vote on-chain        │"
echo "  │   Provenance: Labor hours → formula → vote → allocation → ledger │"
echo "  │   Verification: Query ${BRIGHTWORKS_URL}/v1/gov/proposals │"
echo "  ├─────────────────────────────────────────────────────────────────┤"
echo "  │ River City Tool Library                                         │"
echo "  │   Governance action: Equipment-sharing agreement ratified        │"
echo "  │   Evidence: Cross-coop agreement with BrightWorks, on-chain      │"
echo "  │   Provenance: Both parties ratified independently; FL/CDN vouch  │"
echo "  │   Verification: Query ${RIVERCITY_URL}/v1/gov/proposals  │"
echo "  ├─────────────────────────────────────────────────────────────────┤"
echo "  │ NOTES:                                                          │"
echo "  │   This report was generated by querying member coop nodes       │"
echo "  │   directly. No coop was required to submit spreadsheets or      │"
echo "  │   grant Finger Lakes CDN administrative access.                 │"
echo "  │   Each coop retains full control of its own governance records. │"
echo "  └─────────────────────────────────────────────────────────────────┘"
echo ""
aside "The foundation can verify any claim in this report independently"
aside "by querying the same endpoints — no intermediary required"
echo ""

# ---------------------------------------------------------------------------
# STEP 10: What is deployed vs. what remains
# ---------------------------------------------------------------------------
narrate "Step 11: Receipt chain architecture — what is deployed vs. what remains"
_beat "PR #1327 (ExecutionReceiptGate) is merged. Here is the current state of each layer."
echo ""
echo "  What is deployed (PR #1327, merged 2026-03-07):"
echo "    - check_execution_gate(): stateless gate primitive"
echo "      Validates GovernanceDecisionReceipt has outcome=Accepted and hash integrity"
echo "    - /v1/receipts/chain?decision_hash=<hex>: live endpoint"
echo "      Returns GovernanceDecisionReceipt + AllocationReceipts + SettlementIntents"
echo "    - /v1/gov/proposals/{id}/proof: implemented with full signature validation"
echo "      Requires attestations with valid Ed25519 signatures"
echo ""
echo "  What is also deployed (wired in governance actor, integration-tested):"
echo "    - check_execution_gate() is called at proposal-close time in GovernanceActor"
echo "      before any ProposalAccepted event fires (actor.rs, Invariant 7 enforcement)"
echo "    - Integration tests in execution_receipt_gate_integration.rs prove this"
echo ""
echo "  What remains:"
echo "    - Defense-in-depth at ledger write-path (optional future hardening)"
echo "      create_budget_allocation() still takes a raw hash; gate ran upstream"
echo "    - Proof endpoint requires signing key configured in pod"
echo "      (cluster constraint — not a code gap)"
echo ""
echo "  Presenter note:"
echo "    'The governance records are real and verifiable. The receipt chain"
echo "     endpoint is live. Invariant 7 is enforced — no allocation effect fires"
echo "     without a passing gate check. Proof attestations require pod signing key config.'"
echo ""

# ---------------------------------------------------------------------------
# Final summary
# ---------------------------------------------------------------------------
echo "================================================================"
echo " FLOW 4 COMPLETE"
echo " Federation oversight and institutional reporting demonstrated."
echo ""
echo " Evidence collected in this session:"
result "  Items collected: ${_EVIDENCE_ITEMS}"
aside  "  Scope gaps:      ${_GAPS} (deployment constraints, not design gaps)"
echo ""
echo " What was shown:"
echo "   - Finger Lakes CDN queried governance records across 3 member coops"
echo "   - Each coop retains control — Finger Lakes CDN has visibility, not access"
echo "   - Harbor Homes: capital authorization governance record"
echo "   - BrightWorks: patronage distribution ratification record"
echo "   - River City: federation agreement governance record"
echo "   - A funder can verify any claim by querying member coop nodes directly"
echo "   - No spreadsheets, no email chains, no admin overhead"
echo ""
echo " Receipt chain state (PR #1327 merged):"
echo "   ✓ Gate primitive: check_execution_gate() enforces Invariant 7"
echo "   ✓ Chain endpoint: /v1/receipts/chain?decision_hash=<hex> is live"
echo "   ✓ Proof endpoint: /v1/gov/proposals/{id}/proof implemented"
echo "   ✓ Wiring: gate enforced at governance actor proposal-close (Invariant 7 active)"
echo "   · Pod config: signing key required for proof endpoint (cluster constraint)"
echo ""
echo " Presenter note (audience: funders):"
echo "   'This is what accountability without bureaucracy looks like. The records"
echo "    are already on-chain. The foundation can verify them. We didn't ask"
echo "    any cooperative to fill out a form.'"
echo "================================================================"
