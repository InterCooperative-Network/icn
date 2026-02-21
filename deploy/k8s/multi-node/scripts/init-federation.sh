#!/usr/bin/env bash
#
# Initialize federation between ICN coop instances on K3s
#
# Prerequisites:
#   - All coop pods running with gateway on ports 30081-30084
#   - icnctl available inside each pod
#
# What this does:
#   1. Gets auth tokens for each coop
#   2. Calls /v1/federation/init on each coop
#   3. Registers each coop as a peer on every other coop
#   4. Creates a governance domain on each coop (for federated proposals)
#
# Usage:
#   ./init-federation.sh
#   ./init-federation.sh --dry-run    # Show what would be done

set -euo pipefail

K3S_HOST="${K3S_HOST:-ubuntu@10.8.10.40}"
GATEWAY_HOST="${GATEWAY_HOST:-10.8.10.40}"
DRY_RUN=false
[ "${1:-}" = "--dry-run" ] && DRY_RUN=true

# Coop definitions: name, namespace, deployment, nodeport
declare -A COOP_PORTS=(
  [alpha]=30081
  [beta]=30082
  [gamma]=30083
  [delta]=30084
)
COOPS=(alpha beta gamma delta)

echo "╔════════════════════════════════════════════════════════════╗"
echo "║       ICN Federation Initialization                       ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Step 1: Collect DIDs and tokens
echo "━━━ Step 1: Collecting identities and auth tokens ━━━"
declare -A DIDS
declare -A TOKENS

for coop in "${COOPS[@]}"; do
  NS="icn-coop-${coop}"
  DEPLOY="icn-${coop}"

  # Get DID
  DID=$(ssh "$K3S_HOST" "sudo kubectl -n $NS exec deployment/$DEPLOY -- icnctl id show 2>&1" 2>/dev/null \
    | grep -oP 'did:icn:\S+' | head -1)
  if [ -z "$DID" ]; then
    echo "ERROR: Could not get DID for $coop"
    exit 1
  fi
  DIDS[$coop]="$DID"
  echo "  $coop DID: $DID"

  # Get auth token with federation + governance scopes
  TOKEN=$(ssh "$K3S_HOST" "sudo kubectl -n $NS exec deployment/$DEPLOY -- \
    icnctl auth token --coop-id $coop \
      --scopes 'federation:read,federation:write,federation:admin,governance:read,governance:write' \
    2>&1" 2>/dev/null | grep -oP 'eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+')
  if [ -z "$TOKEN" ]; then
    echo "ERROR: Could not get token for $coop"
    exit 1
  fi
  TOKENS[$coop]="$TOKEN"
  echo "  $coop token: ${TOKEN:0:20}..."
done
echo ""

# Helper: call gateway API
call_api() {
  local coop="$1" method="$2" path="$3" body="${4:-}"
  local port="${COOP_PORTS[$coop]}"
  local token="${TOKENS[$coop]}"
  local url="http://${GATEWAY_HOST}:${port}${path}"

  if $DRY_RUN; then
    echo "  [DRY-RUN] $method $url"
    [ -n "$body" ] && echo "    Body: $body"
    return 0
  fi

  local args=(-s -w "\n%{http_code}" -X "$method" -H "Authorization: Bearer $token" -H "Content-Type: application/json")
  [ -n "$body" ] && args+=(-d "$body")

  local response
  response=$(curl "${args[@]}" "$url")
  local http_code
  http_code=$(echo "$response" | tail -1)
  local resp_body
  resp_body=$(echo "$response" | sed '$d')

  if [[ "$http_code" =~ ^2 ]]; then
    echo "  ✓ $method $path → $http_code"
    return 0
  else
    echo "  ✗ $method $path → $http_code: $resp_body"
    return 1
  fi
}

# Step 2: Initialize federation on each coop
echo "━━━ Step 2: Initializing federation identity ━━━"
for coop in "${COOPS[@]}"; do
  port="${COOP_PORTS[$coop]}"
  echo "  Initializing $coop..."
  call_api "$coop" POST "/v1/federation/init" \
    "{\"coop_id\":\"${coop}\",\"name\":\"${coop^} Cooperative\",\"gateway_endpoint\":\"http://${GATEWAY_HOST}:${port}\"}" \
    || true  # May already be initialized
done
echo ""

# Step 3: Register peers (each coop knows about every other)
echo "━━━ Step 3: Registering federation peers ━━━"
for src in "${COOPS[@]}"; do
  for dst in "${COOPS[@]}"; do
    [ "$src" = "$dst" ] && continue
    dst_port="${COOP_PORTS[$dst]}"
    echo "  $src → $dst..."
    call_api "$src" POST "/v1/federation/coops" \
      "{\"coop_id\":\"${dst}\",\"name\":\"${dst^} Cooperative\",\"public_did\":\"${DIDS[$dst]}\",\"gateway_endpoints\":[\"http://${GATEWAY_HOST}:${dst_port}\"],\"capabilities\":[\"governance\"]}" \
      || true  # May already be registered
  done
done
echo ""

# Step 4: Create governance domain on each coop (if not exists)
echo "━━━ Step 4: Creating governance domains ━━━"
for coop in "${COOPS[@]}"; do
  echo "  Creating domain on $coop..."
  call_api "$coop" POST "/v1/gov/domains" \
    "{\"id\":\"${coop}-governance\",\"name\":\"${coop^} Governance\",\"profile\":\"cooperative\",\"quorum_percent\":50,\"approval_percent\":66,\"voting_period_days\":7,\"members\":[\"${DIDS[$coop]}\"]}" \
    || true  # May already exist
done
echo ""

# Step 5: Verify federation status
echo "━━━ Step 5: Verifying federation status ━━━"
for coop in "${COOPS[@]}"; do
  port="${COOP_PORTS[$coop]}"
  if ! $DRY_RUN; then
    status=$(curl -s -H "Authorization: Bearer ${TOKENS[$coop]}" \
      "http://${GATEWAY_HOST}:${port}/v1/federation/status" 2>/dev/null)
    echo "  $coop: $status"
  else
    echo "  [DRY-RUN] GET /v1/federation/status"
  fi
done
echo ""

echo "╔════════════════════════════════════════════════════════════╗"
echo "║       Federation Initialized!                              ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Coop DIDs:"
for coop in "${COOPS[@]}"; do
  echo "  $coop: ${DIDS[$coop]}"
done
echo ""
echo "Next steps:"
echo "  1. Create a federation-scoped proposal via API"
echo "  2. Vote on it from other coops"
echo "  3. Verify propagation via gossip"
