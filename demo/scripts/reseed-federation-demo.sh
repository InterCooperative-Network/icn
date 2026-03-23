#!/usr/bin/env bash
# demo/scripts/reseed-federation-demo.sh
# Reset all four federation demo nodes to canonical state.
#
# Usage: ./demo/scripts/reseed-federation-demo.sh
# Idempotent: safe to run multiple times.
#
# What this does:
#   1. Starts kubectl port-forwards for all 4 coop gateways
#   2. Fetches auth tokens from each pod via icnctl
#   3. Creates the coop record on each node (if missing)
#   4. Creates the canonical governance domain on each node (if missing)
#   5. Clears stale demo proposals (roof-repair, equipment-agreement)
#   6. Creates the Q1 patronage proposal on BrightWorks (Flow 2 pre-seed)
#   7. Updates the node's self-DID display name on each gateway
#   8. Prints a summary of what was seeded vs. what already existed
#
# NOTE: ICN gateway state is in-memory and does not survive pod restarts.
# If pods have been restarted since last reseed, all prior state is gone
# and this script recreates it from scratch (fully idempotent either way).
#
# NOTE: Each pod has a fixed DID assigned at init time. The DIDs are:
#   alpha (BrightWorks):  did:icn:zHdQuwTTniwcV4TT1ZcfXsVP7dCojE5czv7vUghmNSmgB
#   beta  (River City):   did:icn:zDMiXkUafnaRfeA8tdPCYiKwMRFsPJ9uN4LeFwWR7cZs3
#   gamma (Harbor Homes): did:icn:zyWqWVqGERfRvUz4LVGd4coCZDuNhnufxRpNVTR1BBA7
#   delta (Finger Lakes): did:icn:zE5E8bz7XrJGr6WozTbUNfSN3he3sUqYaCo4jifFKi4Ln
# These are NOT configurable — they are fixed in the k8s keystore secrets.
#
# NOTE: The governance domain GET-by-ID endpoint (/v1/gov/domains/{id}) has a
# known routing inconsistency: domains appear in the list response but 404 on
# direct GET. Idempotency is implemented by checking the list response, not
# the direct GET.
#
# NOTE: No API exists to set proposal status to "approved" directly. The Q1
# patronage proposal is seeded as Draft. Flow 2 demonstrates the approval
# workflow live. If a pre-approved patronage proposal is needed, it must be
# opened for voting and then voted through (open + vote + close).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib-demo-ports.sh"

# ---------------------------------------------------------------------------
# Internal counters for the summary
# ---------------------------------------------------------------------------
_SEEDED=0
_SKIPPED=0
_FAILED=0

_count_seeded()  { (( _SEEDED  += 1 )); }
_count_skipped() { (( _SKIPPED += 1 )); }
_count_failed()  { (( _FAILED  += 1 )); }

# ---------------------------------------------------------------------------
# Port-forward management
# We start port-forwards manually here (not via demo_ports_up) because
# demo_ports_up registers an EXIT trap that fires on subshell exit, which
# causes races when run non-interactively. We clean up at the end ourselves.
# ---------------------------------------------------------------------------
_PF_PIDS=()

_start_port_forwards() {
  aside "Starting port-forwards for all 4 gateways..."
  kubectl port-forward -n icn-coop-alpha svc/icn-alpha 18081:8080 \
    >/tmp/pf-18081.log 2>&1 &
  _PF_PIDS+=($!)
  kubectl port-forward -n icn-coop-beta svc/icn-beta 18082:8080 \
    >/tmp/pf-18082.log 2>&1 &
  _PF_PIDS+=($!)
  kubectl port-forward -n icn-coop-gamma svc/icn-gamma 18083:8080 \
    >/tmp/pf-18083.log 2>&1 &
  _PF_PIDS+=($!)
  kubectl port-forward -n icn-coop-delta svc/icn-delta 18084:8080 \
    >/tmp/pf-18084.log 2>&1 &
  _PF_PIDS+=($!)
  sleep 2
  aside "Port-forward PIDs: ${_PF_PIDS[*]}"
}

_stop_port_forwards() {
  for pid in "${_PF_PIDS[@]:-}"; do
    kill "$pid" 2>/dev/null || true
  done
  for pid in "${_PF_PIDS[@]:-}"; do
    wait "$pid" 2>/dev/null || true
  done
  _PF_PIDS=()
}

_wait_all_ready() {
  local max_wait="${1:-30}"
  local deadline=$(( $(date +%s) + max_wait ))
  local -a urls=("$BRIGHTWORKS_URL" "$RIVERCITY_URL" "$HARBOR_URL" "$FINGERLAKES_URL")
  local -a names=("BrightWorks" "River City" "Harbor Homes" "Finger Lakes")
  local -a ready=(0 0 0 0)

  aside "Waiting up to ${max_wait}s for all 4 gateways..."
  while [ "$(date +%s)" -lt "$deadline" ]; do
    local all=1
    for i in 0 1 2 3; do
      [ "${ready[$i]}" = "1" ] && continue
      local code
      code=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 2 \
        "${urls[$i]}/v1/health" 2>/dev/null || echo "000")
      if [ "$code" = "200" ]; then
        ready[$i]=1
        result "${names[$i]} gateway ready"
      fi
    done
    for i in 0 1 2 3; do [ "${ready[$i]}" != "1" ] && all=0; done
    [ "$all" = "1" ] && return 0
    sleep 1
  done
  for i in 0 1 2 3; do
    [ "${ready[$i]}" != "1" ] && fail "${names[$i]} gateway did not become ready"
  done
  return 1
}

# ---------------------------------------------------------------------------
# Token fetching (delegates to demo_get_token from lib-demo-ports.sh)
# ---------------------------------------------------------------------------
_get_token() {
  local coop="$1"
  demo_get_token "$coop"
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# _ensure_federation_init <url> <coop_id> <name> <token>
# Initializes the federation module if not already initialized.
# Requires federation:admin scope in token.
_ensure_federation_init() {
  local url="$1" coop_id="$2" name="$3" token="$4"
  local status
  status=$(curl -s -w "\n%{http_code}" -o /dev/null \
    -H "Authorization: Bearer $token" \
    "${url}/v1/federation/status" 2>/dev/null | tail -1 || echo "000")
  if [ "$status" = "200" ]; then
    local initialized
    initialized=$(curl -s -H "Authorization: Bearer $token" \
      "${url}/v1/federation/status" 2>/dev/null | \
      python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('initialized','false'))" 2>/dev/null || echo "false")
    if [ "$initialized" = "True" ] || [ "$initialized" = "true" ]; then
      aside "  Federation already initialized on ${coop_id} — skipping"
      _count_skipped
      return 0
    fi
  fi
  local resp code
  resp=$(curl -s -w "\n%{http_code}" \
    -X POST "${url}/v1/federation/init" \
    -H "Authorization: Bearer $token" \
    -H "Content-Type: application/json" \
    -d "{\"coop_id\":\"${coop_id}\",\"name\":\"${name}\"}" 2>/dev/null)
  code=$(echo "$resp" | tail -1)
  if [[ "$code" =~ ^2 ]]; then
    result "  Federation initialized: ${coop_id} (HTTP ${code})"
    _count_seeded
  else
    warn "  Federation init: HTTP ${code} — ${coop_id}"
  fi
}

# _coop_exists <url> <coop_id> <token> → 0 if exists, 1 if not
_coop_exists() {
  local url="$1" coop_id="$2" token="$3"
  local code
  code=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer $token" \
    "${url}/v1/coops/${coop_id}" 2>/dev/null || echo "000")
  [ "$code" = "200" ]
}

# _ensure_coop <url> <coop_id> <name> <token>
_ensure_coop() {
  local url="$1" coop_id="$2" name="$3" token="$4"
  if _coop_exists "$url" "$coop_id" "$token"; then
    aside "  Coop '${coop_id}': already exists"
    _count_skipped
    return 0
  fi
  local code
  code=$(curl -s -o /tmp/reseed-coop.json -w "%{http_code}" -X POST \
    -H "Authorization: Bearer $token" \
    -H "Content-Type: application/json" \
    -d "{\"id\":\"${coop_id}\",\"name\":\"${name}\"}" \
    "${url}/v1/coops" 2>/dev/null || echo "000")
  if [[ "$code" =~ ^2 ]]; then
    result "  Coop '${coop_id}': created (${code})"
    _count_seeded
  else
    warn "  Coop '${coop_id}': create failed (HTTP ${code}) — $(cat /tmp/reseed-coop.json 2>/dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("error","?"))' 2>/dev/null || echo '?')"
    _count_failed
  fi
}

# _domain_exists <url> <domain_id> <token> → 0 if found in list (by name), 1 if not
#
# NOTE: The governance domain list endpoint returns UUIDs in the `id` field, not
# the string ID we pass on create. This is a known routing inconsistency: the
# create handler stores the domain under our string key, but the list/get handler
# serialises with a UUID. Proposal creation uses our string key and works correctly.
# We detect existence by checking whether the `name` field matches — names are
# unique per domain within a node.
_domain_exists_by_name() {
  local url="$1" name="$2" token="$3"
  local found
  found=$(curl -s \
    -H "Authorization: Bearer $token" \
    "${url}/v1/gov/domains" 2>/dev/null \
    | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    names = [x.get('name','') for x in d.get('data', [])]
    print('yes' if '${name}' in names else 'no')
except:
    print('no')
" 2>/dev/null || echo "no")
  [ "$found" = "yes" ]
}

# _ensure_domain <url> <domain_id> <name> <node_did> <token>
# NOTE: domain_id must match the id field sent on create. Proposal creation uses
# this string key, not the UUID that appears in the list response.
# Idempotency: if create returns "already exists" we treat it as a skip (not a
# failure), because the domain was created in a previous run.
_ensure_domain() {
  local url="$1" domain_id="$2" name="$3" node_did="$4" token="$5"
  if _domain_exists_by_name "$url" "$name" "$token"; then
    aside "  Domain '${domain_id}': already exists"
    _count_skipped
    return 0
  fi
  local code
  code=$(curl -s -o /tmp/reseed-domain.json -w "%{http_code}" -X POST \
    -H "Authorization: Bearer $token" \
    -H "Content-Type: application/json" \
    -d "{
      \"id\": \"${domain_id}\",
      \"name\": \"${name}\",
      \"profile\": \"cooperative_default\",
      \"quorum_percent\": 60,
      \"approval_percent\": 67,
      \"voting_period_days\": 7,
      \"members\": [\"${node_did}\"]
    }" \
    "${url}/v1/gov/domains" 2>/dev/null || echo "000")
  if [[ "$code" =~ ^2 ]]; then
    result "  Domain '${domain_id}': created (${code})"
    _count_seeded
  else
    # Treat "already exists" as idempotent (domain was created in a prior run)
    local errmsg
    errmsg=$(cat /tmp/reseed-domain.json 2>/dev/null \
      | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("error","?"))' 2>/dev/null || echo "?")
    if echo "$errmsg" | grep -qi "already exists"; then
      aside "  Domain '${domain_id}': already exists (confirmed via error)"
      _count_skipped
    else
      warn "  Domain '${domain_id}': create failed (HTTP ${code}) — ${errmsg}"
      _count_failed
    fi
  fi
}

# _list_proposals <url> <token> → JSON array of {id, title, state}
_list_proposals() {
  local url="$1" token="$2"
  curl -s \
    -H "Authorization: Bearer $token" \
    "${url}/v1/gov/proposals" 2>/dev/null \
    | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    for p in d.get('data', []):
        print(p['id'], '|', p.get('title','?'), '|', p.get('state','?'))
except:
    pass
" 2>/dev/null || true
}

# _close_stale_proposals <url> <token> <title_pattern> [<exact_title_to_keep>]
# Closes any non-closed proposals whose title matches the pattern (case-insensitive),
# EXCEPT for proposals whose title exactly matches the optional keep_title argument.
# NOTE: There is no DELETE /v1/gov/proposals/{id} endpoint in the gateway.
# The only lifecycle transitions are: Draft→Open→Closed.
# We close stale Draft proposals by opening them immediately and then closing them.
# NOTE: title strings are written to a temp file to safely handle non-ASCII chars.
_close_stale_proposals() {
  local url="$1" token="$2" pattern="$3" keep_title="${4:-}"
  # Write filter script to temp file to avoid heredoc-in-subshell issues
  local _py; _py=$(mktemp /tmp/reseed-filter.XXXXXX.py)
  cat > "$_py" << 'PYEOF'
import sys, json
pattern = sys.argv[1].lower()
keep_title = sys.argv[2] if len(sys.argv) > 2 else ""
try:
    d = json.load(sys.stdin)
    for p in d.get('data', []):
        title = p.get('title', '')
        state = p.get('state', '')
        is_closed = isinstance(state, dict) or state in ('Closed', 'Executed', 'Rejected', 'NoQuorum')
        matches = pattern in title.lower()
        is_kept = bool(keep_title) and title == keep_title
        if matches and not is_closed and not is_kept:
            print(p['id'])
except Exception:
    pass
PYEOF
  local ids
  ids=$(curl -s \
    -H "Authorization: Bearer $token" \
    "${url}/v1/gov/proposals" 2>/dev/null \
    | python3 "$_py" "$pattern" "$keep_title" 2>/dev/null || true)
  rm -f "$_py"

  if [ -z "$ids" ]; then
    return 0
  fi

  for prop_id in $ids; do
    # Open it (required before closing)
    curl -s -o /dev/null -X POST \
      -H "Authorization: Bearer $token" \
      -H "Content-Type: application/json" \
      -d '{}' \
      "${url}/v1/gov/proposals/${prop_id}/open" 2>/dev/null || true
    # Close it
    local code
    code=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
      -H "Authorization: Bearer $token" \
      -H "Content-Type: application/json" \
      -d '{}' \
      "${url}/v1/gov/proposals/${prop_id}/close" 2>/dev/null || echo "000")
    if [[ "$code" =~ ^2 ]]; then
      warn "  Proposal '${prop_id}': closed stale '${pattern}' proposal (${code})"
    else
      aside "  Proposal '${prop_id}': could not close stale (HTTP ${code}) — continuing"
    fi
  done
}

# _proposal_exists_with_title <url> <token> <title_substring> → 0 if found (non-closed), 1 if not
# Matches case-insensitively on the title substring.
# Uses a temp file to safely handle non-ASCII title characters (em-dashes, etc.).
_proposal_exists_with_title() {
  local url="$1" token="$2" pattern="$3"
  local _py; _py=$(mktemp /tmp/reseed-check.XXXXXX.py)
  cat > "$_py" << 'PYEOF'
import sys, json
pattern = sys.argv[1].lower()
try:
    d = json.load(sys.stdin)
    for p in d.get('data', []):
        state = p.get('state', '')
        is_closed = isinstance(state, dict) or state in ('Closed', 'Rejected', 'Executed', 'NoQuorum')
        if pattern in p.get('title', '').lower() and not is_closed:
            print('yes'); sys.exit()
    print('no')
except Exception:
    print('no')
PYEOF
  local found
  found=$(curl -s \
    -H "Authorization: Bearer $token" \
    "${url}/v1/gov/proposals" 2>/dev/null \
    | python3 "$_py" "$pattern" 2>/dev/null || echo "no")
  rm -f "$_py"
  [ "$found" = "yes" ]
}

# _ensure_proposal <url> <domain_id> <title> <description> <payload_json> <token>
_ensure_proposal() {
  local url="$1" domain_id="$2" title="$3" description="$4" payload="$5" token="$6"
  local title_short="${title:0:40}"
  if _proposal_exists_with_title "$url" "$token" "${title:0:20}"; then
    aside "  Proposal '${title_short}...': already exists"
    _count_skipped
    return 0
  fi
  local body
  body=$(python3 -c "
import json, sys
print(json.dumps({
    'domain_id': '${domain_id}',
    'title': sys.argv[1],
    'description': sys.argv[2],
    'payload': json.loads(sys.argv[3])
}))" "$title" "$description" "$payload" 2>/dev/null)

  local code
  code=$(curl -s -o /tmp/reseed-prop.json -w "%{http_code}" -X POST \
    -H "Authorization: Bearer $token" \
    -H "Content-Type: application/json" \
    -d "$body" \
    "${url}/v1/gov/proposals" 2>/dev/null || echo "000")
  if [[ "$code" =~ ^2 ]]; then
    local prop_id
    prop_id=$(cat /tmp/reseed-prop.json 2>/dev/null \
      | python3 -c "import sys,json; print(json.load(sys.stdin).get('id','?'))" 2>/dev/null || echo "?")
    result "  Proposal '${title_short}...': created as ${prop_id} (${code})"
    _count_seeded
  else
    warn "  Proposal '${title_short}...': create failed (HTTP ${code}) — $(cat /tmp/reseed-prop.json 2>/dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("error","?"))' 2>/dev/null || echo '?')"
    _count_failed
  fi
}

# _update_display_name <url> <coop_id> <node_did> <display_name> <token>
# NOTE: PUT /v1/members/{coop_id}/{did}/profile is self-service only —
# the JWT subject must match the DID being edited. This updates the node
# identity's display name, not individual human members.
_update_display_name() {
  local url="$1" coop_id="$2" node_did="$3" display_name="$4" token="$5"
  local code
  code=$(curl -s -o /tmp/reseed-profile.json -w "%{http_code}" -X PUT \
    -H "Authorization: Bearer $token" \
    -H "Content-Type: application/json" \
    -d "{\"display_name\":\"${display_name}\"}" \
    "${url}/v1/members/${coop_id}/${node_did}/profile" 2>/dev/null || echo "000")
  if [[ "$code" =~ ^2 ]]; then
    result "  Display name set to '${display_name}' (${code})"
    _count_seeded
  else
    warn "  Display name update failed (HTTP ${code}) — $(cat /tmp/reseed-profile.json 2>/dev/null | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("error","?"))' 2>/dev/null || echo '?')"
    _count_failed
  fi
}

# ---------------------------------------------------------------------------
# Per-coop seed functions
# ---------------------------------------------------------------------------

# NOTE: Node DIDs are fixed at pod init time. Confirmed from live cluster
# on 2026-03-07. If pods are rebuilt, these DIDs will change and this
# script must be updated.
ALPHA_DID="did:icn:zHdQuwTTniwcV4TT1ZcfXsVP7dCojE5czv7vUghmNSmgB"
BETA_DID="did:icn:zDMiXkUafnaRfeA8tdPCYiKwMRFsPJ9uN4LeFwWR7cZs3"
GAMMA_DID="did:icn:zyWqWVqGERfRvUz4LVGd4coCZDuNhnufxRpNVTR1BBA7"
DELTA_DID="did:icn:zE5E8bz7XrJGr6WozTbUNfSN3he3sUqYaCo4jifFKi4Ln"

seed_brightworks() {
  narrate "Seeding BrightWorks Cooperative (alpha / port 18081)"
  local token
  token=$(_get_token alpha) || { fail "  Could not get alpha token"; _count_failed; return 1; }

  _ensure_federation_init "$BRIGHTWORKS_URL" "$BRIGHTWORKS_COOP_ID" "BrightWorks Cooperative" "$token"
  _ensure_coop "$BRIGHTWORKS_URL" "$BRIGHTWORKS_COOP_ID" "BrightWorks Cooperative" "$token"
  _update_display_name "$BRIGHTWORKS_URL" "$BRIGHTWORKS_COOP_ID" "$ALPHA_DID" "BrightWorks Cooperative" "$token"
  _ensure_domain "$BRIGHTWORKS_URL" "brightworks-governance" "BrightWorks Governance" "$ALPHA_DID" "$token"

  # Flow 2: Q1 patronage proposal (pre-seeded as Draft, voted through live during Flow 2)
  # Close any stale/test Q1 proposals, keeping only the canonical one if it exists.
  _close_stale_proposals "$BRIGHTWORKS_URL" "$token" "Q1 2026 patronage" \
    "Q1 2026 patronage distribution — 3,840 credits across 524 labor hours"

  # Canonical title must be matched exactly (prefix ≥ 30 chars) to avoid false positives.
  _ensure_proposal \
    "$BRIGHTWORKS_URL" \
    "brightworks-governance" \
    "Q1 2026 patronage distribution — 3,840 credits across 524 labor hours" \
    "Q1 has closed. Total distributable surplus is 3,840 credits, to be allocated proportionally to member labor hours. Allocation formula: (member_hours / 524) * 3840. Rosa Mendez (Treasurer) has verified the figures." \
    '{"type":"text","body":"Allocations: Yusuf Okafor 880 credits (120h), Devraj Singh 843 credits (115h), Anton Kowalski 763 credits (104h), Camille Tran 719 credits (98h), Rosa Mendez 638 credits (87h). Total: 3843 credits distributed."}' \
    "$token"

  # Print current proposal state for diagnostics
  aside "  Current proposals:"
  _list_proposals "$BRIGHTWORKS_URL" "$token" | sed 's/^/    /'
}

seed_rivercity() {
  narrate "Seeding River City Tool Library (beta / port 18082)"
  local token
  token=$(_get_token beta) || { fail "  Could not get beta token"; _count_failed; return 1; }

  _ensure_federation_init "$RIVERCITY_URL" "$RIVERCITY_COOP_ID" "River City Tool Library" "$token"
  _ensure_coop "$RIVERCITY_URL" "$RIVERCITY_COOP_ID" "River City Tool Library" "$token"
  _update_display_name "$RIVERCITY_URL" "$RIVERCITY_COOP_ID" "$BETA_DID" "River City Tool Library" "$token"
  _ensure_domain "$RIVERCITY_URL" "rivercity-governance" "River City Governance" "$BETA_DID" "$token"

  # Flow 3: clear any stale equipment-agreement proposals
  # (the equipment agreement is created live during Flow 3)
  _close_stale_proposals "$RIVERCITY_URL" "$token" "equipment"
  _close_stale_proposals "$RIVERCITY_URL" "$token" "river city"

  aside "  Current proposals:"
  _list_proposals "$RIVERCITY_URL" "$token" | sed 's/^/    /'
}

seed_harborhomes() {
  narrate "Seeding Harbor Homes Cooperative (gamma / port 18083)"
  local token
  token=$(_get_token gamma) || { fail "  Could not get gamma token"; _count_failed; return 1; }

  _ensure_federation_init "$HARBOR_URL" "$HARBOR_COOP_ID" "Harbor Homes Cooperative" "$token"
  _ensure_coop "$HARBOR_URL" "$HARBOR_COOP_ID" "Harbor Homes Cooperative" "$token"
  _update_display_name "$HARBOR_URL" "$HARBOR_COOP_ID" "$GAMMA_DID" "Harbor Homes Cooperative" "$token"
  _ensure_domain "$HARBOR_URL" "harborhomes-governance" "Harbor Homes Governance" "$GAMMA_DID" "$token"

  # Flow 1: clear any stale roof-repair proposals
  # (the roof-repair proposal is created LIVE during Flow 1 to demonstrate governance)
  _close_stale_proposals "$HARBOR_URL" "$token" "roof"
  _close_stale_proposals "$HARBOR_URL" "$token" "harbor homes"

  aside "  Current proposals:"
  _list_proposals "$HARBOR_URL" "$token" | sed 's/^/    /'
}

seed_fingerlakes() {
  narrate "Seeding Finger Lakes CDN (delta / port 18084)"
  local token
  token=$(_get_token delta) || { fail "  Could not get delta token"; _count_failed; return 1; }

  _ensure_federation_init "$FINGERLAKES_URL" "$FINGERLAKES_COOP_ID" "Finger Lakes Cooperative Development Network" "$token"
  _ensure_coop "$FINGERLAKES_URL" "$FINGERLAKES_COOP_ID" "Finger Lakes Cooperative Development Network" "$token"
  _update_display_name "$FINGERLAKES_URL" "$FINGERLAKES_COOP_ID" "$DELTA_DID" "Finger Lakes CDN" "$token"
  _ensure_domain "$FINGERLAKES_URL" "fingerlakes-governance" "Finger Lakes Governance" "$DELTA_DID" "$token"

  # Flow 3: clear any stale equipment-agreement / federation proposals
  _close_stale_proposals "$FINGERLAKES_URL" "$token" "equipment"
  _close_stale_proposals "$FINGERLAKES_URL" "$token" "finger lakes"

  # NOTE: federation history transactions (maintenance contributions, dues)
  # from federation-history.json are narrative context only — there is no
  # POST endpoint to seed ledger history without a live settlement flow.
  # The federation-history.json is consumed by presenter scripts for narration.

  aside "  Current proposals:"
  _list_proposals "$FINGERLAKES_URL" "$token" | sed 's/^/    /'
}

# ---------------------------------------------------------------------------
# seed_fingerlakes_compute
#
# Flow 5 (commons compute) requires a trust edge in the Delta node's in-memory
# TrustGraph. The compute actor calls trust_service.trust_score(submitter_did)
# on admission; MIN_TRUST_SUBMIT = 0.1.
#
# Trust state lives in-memory and resets on pod restart. The HTTP trust API
# (/v1/trust/attest) was added after the current K3s binary was deployed
# (2026-02-28). We use icnctl trust add via gRPC (port 5655 inside pod) as
# the supported path for this binary vintage.
#
# Idempotent: adding the same trust edge multiple times is safe.
#
# NOTE: Port 5655 is the internal gRPC port inside the Delta pod. It is NOT
# the Kubernetes NodePort (30655). Use kubectl exec to reach it.
# ---------------------------------------------------------------------------
_DELTA_GRPC_INTERNAL="[::1]:5655"

seed_fingerlakes_compute() {
  narrate "Seeding Finger Lakes CDN compute trust (Flow 5)"

  aside "  Seeding trust edge: $DELTA_DID → score 0.85 (MIN_TRUST_SUBMIT = 0.1)"
  aside "  via gRPC at $_DELTA_GRPC_INTERNAL inside pod"

  if kubectl exec -n "$FINGERLAKES_NS" deploy/icn-delta -- \
      icnctl --endpoint "$_DELTA_GRPC_INTERNAL" trust add \
      "$DELTA_DID" 0.85 --label "compute-demo" \
      >/dev/null 2>&1; then
    result "  Compute trust seeded: $DELTA_DID score=0.85"
    _count_seeded
  else
    warn "  Compute trust seed failed — Flow 5 may reject task submission"
    warn "  Retry: kubectl exec -n icn-coop-delta deploy/icn-delta -- icnctl --endpoint $_DELTA_GRPC_INTERNAL trust add $DELTA_DID 0.85"
    _count_failed
  fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

echo "=== ICN Federation Demo — Reseed to Canonical State ==="
echo "    $(date)"
echo

_start_port_forwards
_wait_all_ready 30

echo ""
seed_brightworks
seed_rivercity
seed_harborhomes
seed_fingerlakes
seed_fingerlakes_compute

_stop_port_forwards

echo ""
echo "=== Reseed summary ==="
result "  Seeded:  ${_SEEDED}"
aside  "  Skipped: ${_SKIPPED} (already in place)"
if [ "$_FAILED" -gt 0 ]; then
  fail "  Failed:  ${_FAILED} (see warnings above)"
else
  result "  Failed:  0"
fi

echo ""
echo "Canonical state:"
echo "  brightworks-cooperative  — coop + governance domain + Q1 patronage proposal (Draft)"
echo "  river-city-tool-library  — coop + governance domain (no pending proposals)"
echo "  harbor-homes-cooperative — coop + governance domain (no pending proposals)"
echo "  fingerlakes-cdn          — coop + governance domain + compute trust seeded (score=0.85)"
echo ""
echo "Run flows:"
echo "  Flow 1 (Harbor Homes governance):  bash demo/scripts/flow-1-governance.sh"
echo "  Flow 2 (BrightWorks patronage):    bash demo/scripts/flow-2-patronage.sh"
echo "  Flow 3 (federation agreement):     bash demo/scripts/flow-3-federation.sh"
echo "  Flow 5 (commons compute):          bash demo/scripts/flow-5-compute.sh"
echo ""
if [ "$_FAILED" -gt 0 ]; then
  echo "WARNING: ${_FAILED} operation(s) failed. Demo may be in partial state."
  echo "Re-run this script or check port-forwards manually."
  exit 1
fi
