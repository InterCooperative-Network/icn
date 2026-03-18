#!/usr/bin/env bash
# governance-demo.sh — Sprint 10 "Governance Proof of Life"
#
# Proves the full governance lifecycle works end-to-end against a live ICN node.
# Outputs a human-readable transcript suitable for review or CI validation.
#
# Usage:
#   ./scripts/governance-demo.sh                       # defaults: gateway at :30080, auth via pod exec
#   ./scripts/governance-demo.sh --gateway http://localhost:8080 --token <JWT>
#
set -euo pipefail

# ── Defaults ──────────────────────────────────────────────────────────────────
GATEWAY="${GATEWAY:-http://10.8.10.40:30080}"
TOKEN="${TOKEN:-}"
POD_NAMESPACE="${POD_NAMESPACE:-icn}"
DOMAIN_ID="coop:demo-$(date +%s)"
VERBOSE="${VERBOSE:-0}"

# ── Helpers ───────────────────────────────────────────────────────────────────
info()  { printf '\033[1;34m▸\033[0m %s\n' "$*"; }
ok()    { printf '\033[1;32m✓\033[0m %s\n' "$*"; }
fail()  { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }
json()  { python3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps(d,indent=2))"; }

api() {
  local method="$1" path="$2"
  shift 2
  local url="${GATEWAY}/v1${path}"
  local resp
  resp=$(curl -sf -X "$method" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    "$@" "$url" 2>&1) || fail "API call failed: $method $url"
  echo "$resp"
}

# ── Parse args ────────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --gateway) GATEWAY="$2"; shift 2 ;;
    --token)   TOKEN="$2"; shift 2 ;;
    --verbose) VERBOSE=1; shift ;;
    *) fail "Unknown arg: $1" ;;
  esac
done

# ── Obtain token if not provided ──────────────────────────────────────────────
if [[ -z "$TOKEN" ]]; then
  info "No token provided, obtaining via pod exec..."
  POD=$(kubectl get pod -n "$POD_NAMESPACE" -l app=icn -o jsonpath='{.items[0].metadata.name}' 2>/dev/null) \
    || fail "Cannot find ICN pod in namespace $POD_NAMESPACE"
  TOKEN=$(kubectl exec -n "$POD_NAMESPACE" "$POD" -c icnd -- \
    icnctl --data-dir /data auth token \
      --gateway http://127.0.0.1:8080 \
      --coop-id default \
      --scopes "governance:read,governance:write" 2>&1 \
    | grep '^eyJ' | tr -d '[:space:]') \
    || fail "Failed to obtain token from pod"
  [[ -n "$TOKEN" ]] || fail "Token is empty"
  ok "Token obtained from pod $POD"
fi

# Extract DID from JWT (base64-decode the payload)
MY_DID=$(echo "$TOKEN" | cut -d. -f2 | python3 -c "
import sys, base64, json
payload = sys.stdin.read().strip()
# Add padding
payload += '=' * (4 - len(payload) % 4)
print(json.loads(base64.urlsafe_b64decode(payload))['sub'])
") || fail "Cannot extract DID from token"

# ── Header ────────────────────────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║           ICN Governance — Proof of Life Demo               ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "  Gateway:  $GATEWAY"
echo "  Identity: $MY_DID"
echo "  Domain:   $DOMAIN_ID"
echo "  Time:     $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo ""

# ── Step 0: Health check ──────────────────────────────────────────────────────
info "Step 0: Health check"
HEALTH=$(curl -sf "${GATEWAY}/v1/health") || fail "Gateway unreachable"
VERSION=$(echo "$HEALTH" | python3 -c "import sys,json; h=json.load(sys.stdin); print(f'v{h[\"version\"]} (sha: {h.get(\"git_sha\",\"unknown\")})')")
ok "Gateway healthy — $VERSION"
echo ""

# ── Step 1: Create domain ────────────────────────────────────────────────────
info "Step 1: Create governance domain"
DOMAIN_JSON=$(python3 -c "
import json; print(json.dumps({
  'id': '$DOMAIN_ID', 'name': 'Demo Cooperative', 'profile': 'cooperative',
  'quorum_percent': 51, 'approval_percent': 66, 'voting_period_days': 7,
  'members': ['$MY_DID']
}))
")
DOMAIN=$(api POST "/gov/domains" -d "$DOMAIN_JSON")
ok "Domain created: $DOMAIN_ID"
[[ "$VERBOSE" == "1" ]] && echo "$DOMAIN" | json
echo ""

# ── Step 2: Verify domain in list ────────────────────────────────────────────
info "Step 2: Verify domain appears in list"
DOMAINS=$(api GET "/gov/domains")
COUNT=$(echo "$DOMAINS" | python3 -c "import sys,json; d=json.load(sys.stdin); print(sum(1 for x in d['data'] if x['id']=='$DOMAIN_ID'))")
[[ "$COUNT" == "1" ]] || fail "Domain not found in list"
ok "Domain verified in list (total: $(echo "$DOMAINS" | python3 -c "import sys,json; print(json.load(sys.stdin)['pagination']['total'])"))"
echo ""

# ── Step 3: Create proposal ──────────────────────────────────────────────────
info "Step 3: Create proposal"
PROPOSAL_JSON=$(python3 -c "
import json; print(json.dumps({
  'domain_id': '$DOMAIN_ID',
  'title': 'Adopt cooperative charter v1',
  'description': 'Establish the founding charter for the demo cooperative.',
  'payload': {'type': 'text', 'body': 'All members have equal voting rights. Decisions require 51%% quorum and 66%% approval.'}
}))
")
PROPOSAL=$(api POST "/gov/proposals" -d "$PROPOSAL_JSON")
PROP_ID=$(echo "$PROPOSAL" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
PROP_STATE=$(echo "$PROPOSAL" | python3 -c "import sys,json; print(json.load(sys.stdin)['state'])")
ok "Proposal created: $PROP_ID (state: $PROP_STATE)"
echo ""

# ── Step 4: Open proposal for voting ─────────────────────────────────────────
info "Step 4: Open proposal for voting"
OPENED=$(api POST "/gov/proposals/$PROP_ID/open" -d '{}')
OPEN_STATE=$(echo "$OPENED" | python3 -c "import sys,json; s=json.load(sys.stdin)['state']; print('Open' if isinstance(s,dict) and 'Open' in s else s)")
[[ "$OPEN_STATE" == "Open" ]] || fail "Expected state Open, got $OPEN_STATE"
ok "Proposal opened for voting"
echo ""

# ── Step 5: Cast vote ────────────────────────────────────────────────────────
info "Step 5: Cast vote (for)"
VOTED=$(api POST "/gov/proposals/$PROP_ID/vote" -d '{"choice":"for","comment":"Founding charter approved."}')
ok "Vote cast successfully"
echo ""

# ── Step 6: Check tally ──────────────────────────────────────────────────────
info "Step 6: Check vote tally"
TALLY=$(api GET "/gov/proposals/$PROP_ID/votes")
FOR=$(echo "$TALLY" | python3 -c "import sys,json; print(json.load(sys.stdin)['for_votes'])")
TOTAL=$(echo "$TALLY" | python3 -c "import sys,json; print(json.load(sys.stdin)['total_votes'])")
ok "Tally: $FOR for / $TOTAL total"
echo ""

# ── Step 7: Close proposal ───────────────────────────────────────────────────
info "Step 7: Close proposal"
CLOSED=$(api POST "/gov/proposals/$PROP_ID/close" -d '{}')
FINAL_STATE=$(echo "$CLOSED" | python3 -c "import sys,json; s=json.load(sys.stdin)['state']; print(list(s.keys())[0] if isinstance(s,dict) else s)")
ok "Proposal closed — outcome: $FINAL_STATE"
echo ""

# ── Summary ───────────────────────────────────────────────────────────────────
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                    DEMO COMPLETE                            ║"
echo "╠══════════════════════════════════════════════════════════════╣"
printf "║  %-58s║\n" "Gateway:   $VERSION"
printf "║  %-58s║\n" "Domain:    $DOMAIN_ID"
printf "║  %-58s║\n" "Proposal:  $PROP_ID"
printf "║  %-58s║\n" "Lifecycle: Draft → Open → Voted → $FINAL_STATE"
printf "║  %-58s║\n" "Tally:     $FOR/$TOTAL votes for"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "Transcript generated at $(date -u +%Y-%m-%dT%H:%M:%SZ)"
