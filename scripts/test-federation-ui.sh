#!/usr/bin/env bash
# test-federation-ui.sh — One-command federation UI test path
#
# Checks health, mints a token, and prints a magic link that auto-logs
# into Pilot UI pointed at the federation tab.
#
# Usage:
#   ./scripts/test-federation-ui.sh
#   COOP=beta ./scripts/test-federation-ui.sh   # login as different coop

set -euo pipefail

# ── Config ──────────────────────────────────────────────────────────────────
K3S_HOST="${K3S_HOST:-ubuntu@10.8.10.40}"
GATEWAY_HOST="${GATEWAY_HOST:-10.8.10.40}"
COOP="${COOP:-alpha}"
UI_PORT="${UI_PORT:-30030}"

declare -A COOP_PORTS=(
  [alpha]=30081
  [beta]=30082
  [gamma]=30083
  [delta]=30084
)
COOPS=(alpha beta gamma delta)

# ── Helpers ─────────────────────────────────────────────────────────────────
info()  { printf '\033[1;34m▸\033[0m %s\n' "$*"; }
ok()    { printf '\033[1;32m✓\033[0m %s\n' "$*"; }
fail()  { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }
warn()  { printf '\033[1;33m⚠\033[0m %s\n' "$*"; }

get_token() {
  local coop="$1" ns="icn-coop-${1}" deploy="icn-${1}"
  ssh "$K3S_HOST" "sudo kubectl -n $ns exec deployment/$deploy -- \
    icnctl -d /data auth token --coop-id $coop --scopes 'federation:read,governance:read,entity:read,coop:read,ledger:read,ledger:write' 2>&1" 2>/dev/null \
    | grep -oP 'eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+'
}

get_did() {
  local coop="$1" ns="icn-coop-${1}" deploy="icn-${1}"
  ssh "$K3S_HOST" "sudo kubectl -n $ns exec deployment/$deploy -- icnctl id show 2>&1" 2>/dev/null \
    | grep -oP 'did:icn:\S+' | head -1
}

urlencode() {
  python3 -c "import urllib.parse,sys; print(urllib.parse.quote(sys.stdin.read().strip(), safe=''))"
}

# ── Step 1: Health checks ──────────────────────────────────────────────────
info "Checking Pilot UI at http://${GATEWAY_HOST}:${UI_PORT}/"
curl -fsS --max-time 5 "http://${GATEWAY_HOST}:${UI_PORT}/" >/dev/null 2>&1 \
  || fail "Pilot UI not reachable at http://${GATEWAY_HOST}:${UI_PORT}/"
ok "Pilot UI reachable"

healthy=0 total=0
for c in "${COOPS[@]}"; do
  total=$((total + 1))
  port="${COOP_PORTS[$c]}"
  if curl -fsS --max-time 3 "http://${GATEWAY_HOST}:${port}/v1/health" >/dev/null 2>&1; then
    ok "$c gateway healthy (:${port})"
    healthy=$((healthy + 1))
  else
    warn "$c gateway unreachable (:${port})"
  fi
done
[[ $healthy -ge 1 ]] || fail "No gateways reachable"
info "${healthy}/${total} coop gateways healthy"

# ── Step 2: Mint token + get DID (needed for authed endpoints) ─────────────
COOP_PORT="${COOP_PORTS[$COOP]}"
info "Minting token on $COOP..."
TOKEN="$(get_token "$COOP")" || fail "Failed to mint token on $COOP"
[[ -n "$TOKEN" ]] || fail "Token mint returned empty"
ok "Token minted (${#TOKEN} chars)"

info "Reading DID from $COOP..."
DID="$(get_did "$COOP")" || fail "Failed to read DID from $COOP"
[[ -n "$DID" ]] || fail "DID read returned empty"
ok "DID: $DID"

# ── Step 3: Federation status (authed) ────────────────────────────────────
info "Checking federation status on $COOP (:${COOP_PORT})"
FED_STATUS="$(curl -fsS --max-time 5 -H "Authorization: Bearer ${TOKEN}" \
  "http://${GATEWAY_HOST}:${COOP_PORT}/v1/federation/status" 2>/dev/null)" \
  || fail "Federation status endpoint failed on $COOP"

if echo "$FED_STATUS" | python3 -c "import sys,json; d=json.load(sys.stdin); sys.exit(0 if d.get('initialized') else 1)" 2>/dev/null; then
  ok "Federation initialized"
  PEER_COUNT="$(echo "$FED_STATUS" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('peers', [])))" 2>/dev/null || echo '?')"
  info "Peers: ${PEER_COUNT}"
else
  warn "Federation not initialized — tab will show 'not initialized' state"
fi

# ── Step 4: Build magic link ──────────────────────────────────────────────
GATEWAY_URL="http://${GATEWAY_HOST}:${COOP_PORT}"
ENC_GW="$(echo "$GATEWAY_URL" | urlencode)"
ENC_COOP="$(echo "$COOP" | urlencode)"
ENC_DID="$(echo "$DID" | urlencode)"
ENC_TOKEN="$(echo "$TOKEN" | urlencode)"

MAGIC_URL="http://${GATEWAY_HOST}:${UI_PORT}/#gateway=${ENC_GW}&coop=${ENC_COOP}&did=${ENC_DID}&token=${ENC_TOKEN}"

echo
printf '\033[1;32m━━━ Open this URL ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\033[0m\n'
echo "$MAGIC_URL"
printf '\033[1;32m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\033[0m\n'
echo
info "Then click the Federation tab."
info "Verify: status card readable, table text legible, 'Last Seen' visible."
info "Toggle dark/light mode — both should be legible."
