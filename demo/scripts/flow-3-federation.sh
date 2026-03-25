#!/usr/bin/env bash
# =============================================================================
# Flow 3: Federation Coordination Across Autonomous Coops
# Narrative: River City Tool Library ↔ BrightWorks Cooperative equipment agreement
#            Facilitated by Finger Lakes Cooperative Development Network
#
# What this demonstrates:
#   - Three autonomous nodes with separate identities and governance
#   - Cross-coop coordination without a central authority
#   - Each coop ratifies the agreement independently (self-governing)
#   - Finger Lakes CDN facilitates but never owns or controls
#   - Federation clearing tracks value exchange across organizational boundaries
#   - DelegationManager + gossip sync visible in federation status
#
# Core cooperator question:
#   "How do independent co-ops work together without creating another bureaucracy?"
#
# The agreement:
#   River City Tool Library  → provides metalworking equipment access (off-peak hours)
#   BrightWorks Cooperative  → provides 20 maintenance labor hours/quarter
#   Finger Lakes CDN         → facilitates, tracks, reports — does NOT control
#   Term: 90 days, renewable by mutual consent
#
# Known cluster constraints (as of 2026-03-18):
#   - treasury:read/write scopes are in ALLOWED_SCOPES and DEMO_DEFAULT_SCOPES — resolved
#   - federation:write scope included in DEMO_DEFAULT_SCOPES
#   - gossip sync between nodes is eventual — don't expect instant propagation
#
# Usage:    ./demo/scripts/flow-3-federation.sh
# Duration: ~7 minutes live
# Audience: Federations, cooperative developers, ecosystem builders
# Requires: kubectl access to K3s cluster, icnctl inside all 3 pods
# Prereq:   reseed-federation-demo.sh must have been run
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
# Node DIDs (fixed at pod init — see reseed-federation-demo.sh for provenance)
# ---------------------------------------------------------------------------
# NOTE: If pods are rebuilt, these DIDs change. Update both files together.
RIVERCITY_NODE_DID="did:icn:zDMiXkUafnaRfeA8tdPCYiKwMRFsPJ9uN4LeFwWR7cZs3"
BRIGHTWORKS_NODE_DID="did:icn:zHdQuwTTniwcV4TT1ZcfXsVP7dCojE5czv7vUghmNSmgB"
FINGERLAKES_NODE_DID="did:icn:zE5E8bz7XrJGr6WozTbUNfSN3he3sUqYaCo4jifFKi4Ln"

# Governance domains (pre-seeded by reseed script)
RIVERCITY_DOMAIN_ID="rivercity-governance"
BRIGHTWORKS_DOMAIN_ID="brightworks-governance"
FINGERLAKES_DOMAIN_ID="fingerlakes-governance"

# Run tag to avoid collision across demo runs
_DEMO_RUN_TAG="$(date +%s | tail -c 6)"

# Temp file for response bodies
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
  local url="$1" method="${2:-GET}" body="${3:-}" token="${4:-}"
  # NOTE: token defaults to empty (not DEMO_TOKEN) because flow-3 uses three
  # different tokens and always passes them explicitly. Do not add calls here
  # without an explicit token argument.
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

# _require_2xx <context>: exit if DEMO_LAST_HTTP_CODE is not 2xx
_require_2xx() {
  if [[ ! "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
    fail "HTTP ${DEMO_LAST_HTTP_CODE} on: $1"
    _pretty
    exit 1
  fi
}

# ---------------------------------------------------------------------------
# STEP 0: Setup — authenticate all three nodes
# ---------------------------------------------------------------------------
# ---------------------------------------------------------------------------
# PRE-STEP: Reseed federation state
# ---------------------------------------------------------------------------
# flow-3 creates governance proposals and federation registration records.
# On a second run, prior-run proposals and coop registrations persist and
# cause conflicts. reseed-federation-demo.sh is idempotent -- safe on first
# run and cleans stale state from prior runs.
# ---------------------------------------------------------------------------
narrate "Pre-step: Reseeding federation demo state (idempotent)"
aside "Clears stale proposals from prior runs; re-seeds coop records and governance domains"
_reseed_out=$(bash "${SCRIPT_DIR}/reseed-federation-demo.sh" 2>&1)
_reseed_rc=$?
echo "$_reseed_out" | grep -E "(RESULT|WARN|FAIL|===|Seeded|Skipped|Failed)" | head -20
if [ "$_reseed_rc" -eq 0 ]; then
  result "Reseed complete"
else
  warn "Reseed returned non-zero -- proceeding anyway"
fi
echo ""

narrate "Step 0: Connecting to all three federation nodes"
_beat ""
aside "River City (beta / 18082), BrightWorks (alpha / 18081), Finger Lakes CDN (delta / 18084)"

demo_ports_up
demo_wait_ready 30
result "All 4 coop gateways are up"
echo ""

narrate "Authenticating all three participants"
_beat ""

_TF=$(mktemp)
demo_get_token beta > "$_TF";       RIVERCITY_TOKEN="$(cat "$_TF")"
demo_get_token alpha > "$_TF";      BRIGHTWORKS_TOKEN="$(cat "$_TF")"
demo_get_token delta > "$_TF";      FINGERLAKES_TOKEN="$(cat "$_TF")"
rm -f "$_TF"

[ -z "$RIVERCITY_TOKEN" ]   && { fail "Could not get River City token";   exit 1; }
[ -z "$BRIGHTWORKS_TOKEN" ] && { fail "Could not get BrightWorks token";  exit 1; }
[ -z "$FINGERLAKES_TOKEN" ] && { fail "Could not get Finger Lakes token"; exit 1; }

result "River City Tool Library   — authenticated"
result "BrightWorks Cooperative   — authenticated"
result "Finger Lakes CDN          — authenticated"
echo ""

# ---------------------------------------------------------------------------
# STEP 1: Establish the situation
# ---------------------------------------------------------------------------
narrate "Step 1: The situation — two coops with complementary needs"
_beat "River City has metalworking equipment. BrightWorks needs it occasionally. They're going to form an agreement — not through a central platform, but directly, with Finger Lakes CDN as a neutral facilitator."
echo "  These two cooperatives don't share a database, a server, or a landlord."
echo "  They're distinct organizations with separate governance, separate members,"
echo "  and separate ICN nodes."
echo ""
echo "  The problem they share:"
echo ""
echo "  River City Tool Library has metalworking equipment — a lathe, drill"
echo "  press, and metal bandsaw — that sits idle during off-peak hours."
echo "  That unused capacity costs them maintenance overhead without returns."
echo ""
echo "  BrightWorks Cooperative makes sustainable building materials."
echo "  They need occasional metalworking access but can't justify buying"
echo "  their own equipment. They have skilled maintenance workers."
echo ""
echo "  The question: how do these two organizations work together without"
echo "  one becoming dependent on the other — and without creating a"
echo "  separate bureaucracy to manage the relationship?"
echo ""
aside "This is the federation problem: coordination without centralization"
echo ""

# ---------------------------------------------------------------------------
# STEP 2: Each coop's current federation status (independent views)
# ---------------------------------------------------------------------------
narrate "Step 2: Current federation status — three independent views"
_beat "Three coops, three independent nodes. None of them controls the others. The federation is voluntary."
echo ""
echo "  ── River City Tool Library (its own node) ──"

_do_curl "${RIVERCITY_URL}/v1/federation/status" GET "" "$RIVERCITY_TOKEN"
if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  _pretty
else
  aside "Federation status: HTTP ${DEMO_LAST_HTTP_CODE} (module may need initialization)"
  echo ""
fi

echo "  ── BrightWorks Cooperative (its own node) ──"

_do_curl "${BRIGHTWORKS_URL}/v1/federation/status" GET "" "$BRIGHTWORKS_TOKEN"
if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  _pretty
else
  aside "Federation status: HTTP ${DEMO_LAST_HTTP_CODE} (module may need initialization)"
  echo ""
fi

echo "  ── Finger Lakes CDN (its own node) ──"

_do_curl "${FINGERLAKES_URL}/v1/federation/status" GET "" "$FINGERLAKES_TOKEN"
if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  _pretty
else
  aside "Federation status: HTTP ${DEMO_LAST_HTTP_CODE} (module may need initialization)"
  echo ""
fi

aside "Each query above went to a different gateway URL — separate nodes, separate views."
aside "Whether the federation module returns data or 404 (uninitialized), the key point is:"
aside "no node holds the authoritative state for another. Each maintains its own view."
echo ""

# ---------------------------------------------------------------------------
# STEP 3: River City governs the equipment sharing decision
# ---------------------------------------------------------------------------
narrate "Step 3: River City Tool Library — internal governance"
_beat "River City governs itself. Their governance decisions don't require approval from BrightWorks or Finger Lakes."
echo "  Priya Sharma (Coordinator) brings the equipment sharing proposal"
echo "  to River City's members. This is River City's decision, made"
echo "  by River City's members, on River City's node."
echo ""
echo "  The proposal: authorize off-peak access to metalworking equipment"
echo "  in exchange for 20 maintenance hours/quarter from BrightWorks."
echo ""

_RCPROP_TITLE="Authorize equipment-sharing: metalworking access ↔ BrightWorks maintenance (${_DEMO_RUN_TAG})"

_do_curl "${RIVERCITY_URL}/v1/gov/proposals" POST \
  "{\"domain_id\":\"${RIVERCITY_DOMAIN_ID}\",\"title\":\"${_RCPROP_TITLE}\",\"description\":\"River City Tool Library authorizes off-peak access to metalworking equipment (lathe, drill press, metal bandsaw) to BrightWorks Cooperative members in exchange for 20 maintenance labor hours per quarter. Term: 90 days renewable. Facilitated by Finger Lakes CDN. Priya Sharma (Coordinator) recommends approval.\",\"payload\":{\"type\":\"text\",\"body\":\"EQUIPMENT SHARING AGREEMENT — RIVER CITY SIDE\\n\\nEquipment provided: Lathe (Grizzly G4003G), Drill Press (JET JDP-17), Metal Bandsaw (Milwaukee Deep Cut)\\nAvailability: Off-peak hours (weekday mornings, weekend afternoons)\\nBrightWorks obligation: 20 maintenance hours/quarter\\nFacilitator: Finger Lakes CDN (no control rights, coordination only)\\nTerm: 90 days, renewable by mutual governance vote\\n\\nProposed by: Priya Sharma, Coordinator\"}}" \
  "$RIVERCITY_TOKEN"

_require_2xx "Create River City equipment-sharing proposal"
RC_PROPOSAL_ID=$(_field "id")
result "River City proposal created: ${RC_PROPOSAL_ID}"
echo ""

# Open for voting
_do_curl "${RIVERCITY_URL}/v1/gov/proposals/${RC_PROPOSAL_ID}/open" POST '{}' "$RIVERCITY_TOKEN"
_require_2xx "Open River City proposal"

# Vote
echo "  River City members vote — Priya Sharma casts first:"
_do_curl "${RIVERCITY_URL}/v1/gov/proposals/${RC_PROPOSAL_ID}/vote" POST \
  "{\"choice\":\"for\",\"comment\":\"Turning idle equipment into community value. BrightWorks' maintenance contribution covers what we spend on upkeep. I recommend approval.\"}" \
  "$RIVERCITY_TOKEN"
_require_2xx "Cast River City vote"

result "Vote recorded — For"

# Close
_do_curl "${RIVERCITY_URL}/v1/gov/proposals/${RC_PROPOSAL_ID}/close" POST '{}' "$RIVERCITY_TOKEN"
_require_2xx "Close River City proposal"

RC_FINAL_STATE=$(_field "state")
result "River City proposal closed — state: ${RC_FINAL_STATE}"
aside "This decision was made by River City, on River City's node, by River City's members"
aside "Finger Lakes CDN didn't vote. BrightWorks didn't vote. This was autonomous governance."
echo ""
_beat "River City's internal decision is on the record. Moving to BrightWorks."

# ---------------------------------------------------------------------------
# STEP 4: BrightWorks governs its side of the agreement
# ---------------------------------------------------------------------------
narrate "Step 4: BrightWorks Cooperative — independent internal governance"
_beat "Same for BrightWorks. Independent. Sovereign."
echo "  Now BrightWorks votes on their side. This is a completely separate"
echo "  governance event on a completely separate node."
echo ""
echo "  The proposal: authorize committing 20 maintenance hours/quarter"
echo "  in exchange for metalworking equipment access from River City."
echo ""

_BWPROP_TITLE="Authorize equipment-sharing: maintenance contribution ↔ River City access (${_DEMO_RUN_TAG})"

_do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals" POST \
  "{\"domain_id\":\"${BRIGHTWORKS_DOMAIN_ID}\",\"title\":\"${_BWPROP_TITLE}\",\"description\":\"BrightWorks Cooperative authorizes 20 maintenance labor hours per quarter to River City Tool Library in exchange for off-peak access to metalworking equipment (lathe, drill press, metal bandsaw). Term: 90 days renewable. Facilitated by Finger Lakes CDN. Devraj Singh (Metal Fabrication) and Anton Kowalski (Finishing) recommend approval.\",\"payload\":{\"type\":\"text\",\"body\":\"EQUIPMENT SHARING AGREEMENT — BRIGHTWORKS SIDE\\n\\nBrightWorks contribution: 20 maintenance hours/quarter\\nContributors: Devraj Singh (welding/fabrication), Anton Kowalski (assembly/finishing)\\nEquipment access received: Lathe, Drill Press, Metal Bandsaw (off-peak hours)\\nFacilitator: Finger Lakes CDN (no control rights, coordination only)\\nTerm: 90 days, renewable by mutual governance vote\\n\\nProposed by: Yusuf Okafor, Board Secretary\"}}" \
  "$BRIGHTWORKS_TOKEN"

_require_2xx "Create BrightWorks equipment-sharing proposal"
BW_PROPOSAL_ID=$(_field "id")
result "BrightWorks proposal created: ${BW_PROPOSAL_ID}"
echo ""

# Open → Vote → Close
_do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals/${BW_PROPOSAL_ID}/open" POST '{}' "$BRIGHTWORKS_TOKEN"
_require_2xx "Open BrightWorks proposal"

echo "  BrightWorks members vote — Yusuf Okafor:"
_do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals/${BW_PROPOSAL_ID}/vote" POST \
  "{\"choice\":\"for\",\"comment\":\"Access to this equipment expands our production capacity without capital outlay. Twenty maintenance hours is good value.\"}" \
  "$BRIGHTWORKS_TOKEN"
_require_2xx "Cast BrightWorks vote"

result "Vote recorded — For"

_do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals/${BW_PROPOSAL_ID}/close" POST '{}' "$BRIGHTWORKS_TOKEN"
_require_2xx "Close BrightWorks proposal"

BW_FINAL_STATE=$(_field "state")
result "BrightWorks proposal closed — state: ${BW_FINAL_STATE}"
aside "Two coops. Two separate votes. Two separate governance decisions. One agreement."
echo ""
_beat "BrightWorks' internal decision is on the record. Now: Finger Lakes facilitates."

# ---------------------------------------------------------------------------
# STEP 5: Finger Lakes CDN registers both coops
# ---------------------------------------------------------------------------
narrate "Step 5: Finger Lakes CDN — facilitating without controlling"
_beat "Finger Lakes is a coordination layer — a facilitator. They don't own the agreement. They help enforce the terms both parties agreed to."
echo "  Rebecca Nwosu (Federation Liaison) registers both coops in the"
echo "  federation's coordination layer. This gives Finger Lakes CDN"
echo "  visibility — but not control — over the agreement."
echo ""
aside "Finger Lakes CDN is NOT the owner of the agreement — it facilitates it"
echo ""

# Register River City in federation (from Finger Lakes CDN's view)
echo "  Registering River City Tool Library..."
_do_curl "${FINGERLAKES_URL}/v1/federation/coops" POST \
  "{\"coop_id\":\"${RIVERCITY_COOP_ID}\",\"name\":\"River City Tool Library\",\"public_did\":\"${RIVERCITY_NODE_DID}\",\"gateway_endpoints\":[]}" \
  "$FINGERLAKES_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  result "River City registered in federation (HTTP ${DEMO_LAST_HTTP_CODE})"
elif [[ "$DEMO_LAST_HTTP_CODE" =~ ^4 ]]; then
  aside "River City registration: HTTP ${DEMO_LAST_HTTP_CODE} (may already be registered or scope limited)"
else
  warn "River City registration: HTTP ${DEMO_LAST_HTTP_CODE}"
fi

# Register BrightWorks in federation
echo "  Registering BrightWorks Cooperative..."
_do_curl "${FINGERLAKES_URL}/v1/federation/coops" POST \
  "{\"coop_id\":\"${BRIGHTWORKS_COOP_ID}\",\"name\":\"BrightWorks Cooperative\",\"public_did\":\"${BRIGHTWORKS_NODE_DID}\",\"gateway_endpoints\":[]}" \
  "$FINGERLAKES_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  result "BrightWorks registered in federation (HTTP ${DEMO_LAST_HTTP_CODE})"
elif [[ "$DEMO_LAST_HTTP_CODE" =~ ^4 ]]; then
  aside "BrightWorks registration: HTTP ${DEMO_LAST_HTTP_CODE} (may already be registered or scope limited)"
else
  warn "BrightWorks registration: HTTP ${DEMO_LAST_HTTP_CODE}"
fi
echo ""

# Show the federation view
echo "  Federation view from Finger Lakes CDN:"
_do_curl "${FINGERLAKES_URL}/v1/federation/coops" GET "" "$FINGERLAKES_TOKEN"
if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  _pretty
else
  aside "Federation coops list: HTTP ${DEMO_LAST_HTTP_CODE}"
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 6: Vouch — Finger Lakes CDN attests to both coops
# ---------------------------------------------------------------------------
narrate "Step 6: Federation vouching — trust attestation without ownership"
_beat "This is the key idea: trust is attestable without ownership. Finger Lakes says 'we vouch for this agreement' — but River City and BrightWorks remain independent."
echo "  Finger Lakes CDN issues trust attestations for both coops."
echo "  A vouch says: 'we know this organization, we support this agreement.'"
echo "  It is NOT a grant of authority."
echo ""

# Vouch for River City (target_coop_id in body; path param is also used by the handler)
_do_curl "${FINGERLAKES_URL}/v1/federation/coops/${RIVERCITY_COOP_ID}/vouch" POST \
  "{\"target_coop_id\":\"${RIVERCITY_COOP_ID}\",\"trust_score\":0.85,\"expires_in_days\":365}" \
  "$FINGERLAKES_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  result "River City vouch issued (HTTP ${DEMO_LAST_HTTP_CODE})"
elif [[ "$DEMO_LAST_HTTP_CODE" =~ ^4 ]]; then
  aside "River City vouch: HTTP ${DEMO_LAST_HTTP_CODE} (scope limited in this deployment)"
else
  warn "River City vouch: HTTP ${DEMO_LAST_HTTP_CODE}"
fi

# Vouch for BrightWorks
_do_curl "${FINGERLAKES_URL}/v1/federation/coops/${BRIGHTWORKS_COOP_ID}/vouch" POST \
  "{\"target_coop_id\":\"${BRIGHTWORKS_COOP_ID}\",\"trust_score\":0.85,\"expires_in_days\":365}" \
  "$FINGERLAKES_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  result "BrightWorks vouch issued (HTTP ${DEMO_LAST_HTTP_CODE})"
elif [[ "$DEMO_LAST_HTTP_CODE" =~ ^4 ]]; then
  aside "BrightWorks vouch: HTTP ${DEMO_LAST_HTTP_CODE} (scope limited in this deployment)"
else
  warn "BrightWorks vouch: HTTP ${DEMO_LAST_HTTP_CODE}"
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 7: Create clearing agreement — cross-coop value tracking
# ---------------------------------------------------------------------------
narrate "Step 7: Clearing agreement — tracking value across boundaries"
_beat "The agreement is now on the network. Every exchange of off-peak equipment access for maintenance hours gets recorded here — not in a spreadsheet, not in someone's email."
echo "  River City establishes a bilateral clearing agreement with BrightWorks."
echo "  The clearing layer tracks what's owed between them over the 90-day term."
echo ""
echo "  River City provides equipment access; BrightWorks provides maintenance labor."
echo "  Max imbalance: 100 exchange-hours. Settlement: monthly."
echo ""

# Client-generated agreement ID (deterministic for idempotency across demo runs)
_AGREEMENT_ID="rc-bw-equipment-${_DEMO_RUN_TAG}"

_do_curl "${RIVERCITY_URL}/v1/federation/clearing" POST \
  "{\"agreement_id\":\"${_AGREEMENT_ID}\",\"partner_coop_id\":\"${BRIGHTWORKS_COOP_ID}\",\"partner_did\":\"${BRIGHTWORKS_NODE_DID}\",\"max_imbalance\":100,\"settlement\":\"monthly\"}" \
  "$RIVERCITY_TOKEN"

if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  CLEARING_ID=$(_field "agreement_id")
  [ -z "$CLEARING_ID" ] && CLEARING_ID=$(_field "id")
  result "Clearing agreement created: ${CLEARING_ID}"
  echo ""
  echo "  Clearing agreement record:"
  _pretty
elif [ "$DEMO_LAST_HTTP_CODE" = "403" ] || [ "$DEMO_LAST_HTTP_CODE" = "401" ]; then
  warn "Clearing creation: HTTP ${DEMO_LAST_HTTP_CODE} — federation:write scope may be limited"
  echo ""
  echo "  What the clearing agreement tracks:"
  echo "    From:        River City Tool Library (equipment provider)"
  echo "    To:          BrightWorks Cooperative (maintenance provider)"
  echo "    Max balance: 100 exchange-hours"
  echo "    Settlement:  monthly"
  echo ""
  aside "The clearing schema is implemented — this is a scope constraint in the demo cluster."
  CLEARING_ID="(pending scope fix)"
else
  warn "Clearing creation: HTTP ${DEMO_LAST_HTTP_CODE}"
  _pretty
  CLEARING_ID="(failed)"
fi
echo ""

# ---------------------------------------------------------------------------
# STEP 8: Show clearing position if we have an ID
# ---------------------------------------------------------------------------
if [[ "$CLEARING_ID" =~ ^[0-9a-f-]{20,} ]] 2>/dev/null || \
   python3 -c "import sys; s='$CLEARING_ID'; exit(0 if len(s) > 10 and '-' in s else 1)" 2>/dev/null; then
  narrate "Step 8: Clearing position — current net balance"
  _beat ""
  echo ""

  _do_curl "${RIVERCITY_URL}/v1/federation/clearing/${CLEARING_ID}/position" GET "" "$RIVERCITY_TOKEN"
  if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
    result "Clearing position (HTTP ${DEMO_LAST_HTTP_CODE}):"
    _pretty
    aside "As maintenance hours accumulate, this position updates — visible to both parties"
  else
    aside "Clearing position: HTTP ${DEMO_LAST_HTTP_CODE}"
    echo "  (Position tracking available once first exchanges are recorded)"
  fi
  echo ""
else
  warn "Step 8 (clearing position) skipped — clearing agreement pending full scope deployment."
  aside "CLEARING_ID=${CLEARING_ID}"
fi

# ---------------------------------------------------------------------------
# STEP 9: The key demo moment — three independent nodes, one shared outcome
# ---------------------------------------------------------------------------
narrate "Step 9: The key moment — three nodes, three views, one agreement"
_beat "Three independent nodes. Each sees the same agreement from their own perspective. No central server. No single point of failure or control."
echo ""
echo "  Let's query the governance record from each node independently."
echo "  Three separate API calls to three separate nodes:"
echo ""

echo "  ── River City's view of its own governance decision ──"
_do_curl "${RIVERCITY_URL}/v1/gov/proposals/${RC_PROPOSAL_ID}" GET "" "$RIVERCITY_TOKEN"
if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  echo "  River City proposal ${RC_PROPOSAL_ID}:"
  _pretty
else
  aside "  River City proposal query: HTTP ${DEMO_LAST_HTTP_CODE}"
fi

echo "  ── BrightWorks' view of its own governance decision ──"
_do_curl "${BRIGHTWORKS_URL}/v1/gov/proposals/${BW_PROPOSAL_ID}" GET "" "$BRIGHTWORKS_TOKEN"
if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  echo "  BrightWorks proposal ${BW_PROPOSAL_ID}:"
  _pretty
else
  aside "  BrightWorks proposal query: HTTP ${DEMO_LAST_HTTP_CODE}"
fi

echo "  ── Finger Lakes CDN's federation view (both coops visible) ──"
_do_curl "${FINGERLAKES_URL}/v1/federation/coops" GET "" "$FINGERLAKES_TOKEN"
if [[ "$DEMO_LAST_HTTP_CODE" =~ ^2 ]]; then
  echo "  Federation coops (from Finger Lakes CDN's view):"
  _pretty
else
  aside "  Federation view: HTTP ${DEMO_LAST_HTTP_CODE}"
fi

aside "Same story told three ways — each coop has its own record, Finger Lakes has the overview"
aside "No single point of failure. No central database. No authority that can be captured."
echo ""

# ---------------------------------------------------------------------------
# STEP 10: What this proves about federation
# ---------------------------------------------------------------------------
narrate "Step 10: What federation without centralization looks like"
_beat "This is the whole pitch. Cooperatives coordinating with each other — as equals — without surrendering sovereignty to a platform."
echo ""
echo "  Traditional approach:"
echo "    River City and BrightWorks sign a paper contract."
echo "    Finger Lakes CDN stores copies. One party keeps the 'official' version."
echo "    Disputes go to whoever controls the records."
echo ""
echo "  ICN federation approach:"
echo "    River City ratified this on its own node — queryable from ${RIVERCITY_URL}"
echo "    BrightWorks ratified this on its own node — queryable from ${BRIGHTWORKS_URL}"
echo "    Finger Lakes CDN has a coordination view — queryable from ${FINGERLAKES_URL}"
echo "    No single node has authority over the agreement."
echo "    If Finger Lakes CDN disappears, the two coops' records remain."
echo ""
aside "This is what 'autonomous coordination' means: the federation adds value"
aside "without adding dependency. Coops can leave; their governance records stay."
echo ""

# ---------------------------------------------------------------------------
# Final summary
# ---------------------------------------------------------------------------
echo "================================================================"
echo " FLOW 3 COMPLETE"
echo " Federation coordination across autonomous coops demonstrated."
echo ""
echo " What was shown:"
echo "   - Problem: two coops, complementary needs, no shared infrastructure"
echo "   - River City: internal governance vote authorizing equipment access"
echo "   - BrightWorks: independent governance vote authorizing maintenance"
echo "   - Finger Lakes CDN: registration + vouching (facilitator, not owner)"
echo "   - Clearing agreement: cross-coop value tracking without central ledger"
echo "   - Three nodes: three separate API endpoints, three separate governance records"
echo "   - No central authority was created — the federation is the coops"
echo ""

if [ "$RC_FINAL_STATE" = "Accepted" ] && [ "$BW_FINAL_STATE" = "Accepted" ]; then
  echo " Both sides ratified: River City (${RC_FINAL_STATE}) and BrightWorks (${BW_FINAL_STATE})"
else
  echo " Governance outcomes:"
  echo "   River City:  ${RC_FINAL_STATE}"
  echo "   BrightWorks: ${BW_FINAL_STATE}"
  if [ "$RC_FINAL_STATE" != "Accepted" ] || [ "$BW_FINAL_STATE" != "Accepted" ]; then
    aside "Presenter: a non-Accepted outcome still shows the governance record and"
    aside "multi-node architecture — the coordination mechanism is the demo, not the outcome."
  fi
fi

echo ""
echo " Audience framing:"
echo "   Federations:     'This is what member-controlled coordination looks like'"
echo "   Co-op developers:'Deploy this for your member network without owning them'"
echo "   Technical:       'Three nodes, gossip sync, federation clearing — all live'"
echo "   Funders:         'Supporting member coops without becoming their authority'"
echo ""
echo " Clearing agreement ID: ${CLEARING_ID}"
echo " Use this ID in Flow 4 (reporting) to show the audit trail."
echo "================================================================"
