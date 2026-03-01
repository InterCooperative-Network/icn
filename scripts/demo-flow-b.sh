#!/usr/bin/env bash
# Demo Flow B: Service Discovery
# Tests: announce service → discover services → get service by ID → withdraw
#
# Prerequisites:
#   - ICN devnet running (cd deploy/devnet && make up)
#   - python3 with cryptography library (pip install cryptography)
#   - ICN_TOKEN: JWT from icnctl auth inside a devnet container
#
# Quick token setup:
#   TOKEN=$(docker exec icn-devnet-node-a sh -c \
#     'ICN_KEYSTORE_PASSPHRASE=devnet-insecure icnctl --data-dir /data/node-a auth token \
#      --scopes ledger:read,ledger:write,governance:read')
#   ICN_TOKEN=$TOKEN bash scripts/demo-flow-b.sh
set -euo pipefail

# Gateway binds 8080 by default (see icn-core/src/config/gateway.rs).
GATEWAY="${ICN_GATEWAY:-http://localhost:8080}"
TOKEN="${ICN_TOKEN:-}"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

step() { echo -e "${CYAN}[Flow B] $1${NC}"; }
ok()   { echo -e "${GREEN}  ✓ $1${NC}"; }
fail() { echo -e "${RED}  ✗ $1${NC}"; exit 1; }

AUTH_HEADER=""
if [ -n "$TOKEN" ]; then
  AUTH_HEADER="Authorization: Bearer $TOKEN"
fi

SVC_ID="demo-ledger-$(date +%s)"

# ── Step 1: Generate an ephemeral Ed25519 keypair, build signed announce body ──
step "Generating signed service announcement (provider keypair)..."
ANNOUNCE_BODY=$(SVC_ID="$SVC_ID" python3 - <<'PYEOF'
import struct, time, json, os
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'

def b58enc(data):
    n = int.from_bytes(data, 'big')
    result = []
    while n > 0:
        n, r = divmod(n, 58)
        result.append(ALPHABET[r])
    leading = 0
    for b in data:
        if b == 0: leading += 1
        else: break
    return ALPHABET[0] * leading + ''.join(reversed(result))

def make_did(pub_bytes):
    return 'did:icn:z' + b58enc(pub_bytes)

def lp(buf, s):
    enc = s.encode('utf-8')
    return buf + struct.pack('<I', len(enc)) + enc

# Canonical byte values matching EndpointType::to_canonical_byte()
ENDPOINT_TYPE = {'quic': 0, 'http': 1, 'grpc': 2, 'websocket': 3}
# ScopeLevel numeric values (Local=0, Cell=1, Org=2, Federation=3, Commons=4)
SCOPE_LEVEL = {'local': 0, 'cell': 1, 'org': 2, 'federation': 3, 'commons': 4}

def build_signing_payload(svc_id, provider, endpoint_type, svc_type, svc_ver,
                          endpoints, addresses, capabilities, trust_threshold,
                          scope_visibility, ttl_secs, created_at, updated_at):
    """Build binary signing payload matching ServiceEndpoint::signing_payload() in icn-kernel-api."""
    buf = b''
    buf = lp(buf, svc_id)
    buf = lp(buf, provider)
    buf += bytes([ENDPOINT_TYPE.get(endpoint_type, 1)])  # 1 = Http
    buf = lp(buf, svc_type)
    buf = lp(buf, svc_ver)
    buf += struct.pack('<I', len(endpoints))
    for ep in endpoints:
        buf = lp(buf, ep['protocol'])
        buf = lp(buf, ep['host'])
        buf += struct.pack('<H', ep['port'])
        if ep.get('path'):
            buf += b'\x01'
            buf = lp(buf, ep['path'])
        else:
            buf += b'\x00'
    for lst in [sorted(addresses), sorted(capabilities)]:
        buf += struct.pack('<I', len(lst))
        for item in lst:
            buf = lp(buf, item)
    buf += struct.pack('<d', trust_threshold)
    buf += bytes([SCOPE_LEVEL.get(scope_visibility.lower(), 2)])
    buf += b'\x00'  # no cell_id
    buf += struct.pack('<Q', ttl_secs)
    buf += struct.pack('<Q', created_at)
    buf += struct.pack('<Q', updated_at)
    return buf

svc_id = os.environ['SVC_ID']
now = int(time.time())
endpoint_type = 'http'
svc_type = 'ledger'
svc_ver = '1.0'
endpoints = [{'protocol': 'http', 'host': 'node-a.devnet', 'port': 8080}]
addresses = []
capabilities = ['read', 'write']
trust_threshold = 0.0
scope_visibility = 'org'
ttl_secs = 3600

# Generate ephemeral keypair — each demo run gets a unique provider DID.
priv = Ed25519PrivateKey.generate()
pub_bytes = priv.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
provider = make_did(pub_bytes)

payload = build_signing_payload(svc_id, provider, endpoint_type, svc_type, svc_ver,
                                endpoints, addresses, capabilities, trust_threshold,
                                scope_visibility, ttl_secs, now, now)
sig = priv.sign(payload)

req = {
    'service_id': svc_id,
    'provider': provider,
    'endpoint_type': endpoint_type,
    'service_type': svc_type,
    'service_version': svc_ver,
    'endpoints': endpoints,
    'addresses': addresses,
    'capabilities': capabilities,
    'trust_threshold': trust_threshold,
    'scope_visibility': scope_visibility,
    'ttl_secs': ttl_secs,
    'created_at': now,
    'updated_at': now,
    'signature': sig.hex(),
}
print(json.dumps(req))
PYEOF
) || fail "Failed to build signed payload (python3 + cryptography required)"

PROVIDER=$(echo "$ANNOUNCE_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['provider'])")
ok "Provider DID: ${PROVIDER:0:30}…"

# ── Step 2: Announce ──
step "Announcing service endpoint..."
ANNOUNCE_RESP=$(curl -sf -X POST "$GATEWAY/v1/services/announce" \
  -H "Content-Type: application/json" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} \
  -d "$ANNOUNCE_BODY" 2>&1) \
  || fail "Announce failed: $ANNOUNCE_RESP"
ok "Announced: $SVC_ID"

# ── Step 3: Discover ──
step "Discovering services (type=ledger)..."
DISCOVER_RESP=$(curl -sf "$GATEWAY/v1/services/discover?type=ledger&scope=org" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) \
  || fail "Discover failed: $DISCOVER_RESP"

SVC_COUNT=$(echo "$DISCOVER_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['count'])" 2>/dev/null) \
  || fail "Failed to parse discover response: $DISCOVER_RESP"
ok "Found $SVC_COUNT service(s)"

# ── Step 4: Get by ID ──
step "Getting service by ID..."
GET_RESP=$(curl -sf "$GATEWAY/v1/services/$SVC_ID" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) \
  || fail "Get service failed: $GET_RESP"

SVC_PROVIDER=$(echo "$GET_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['provider'])" 2>/dev/null) \
  || fail "Failed to parse service: $GET_RESP"
ok "Service $SVC_ID provider: ${SVC_PROVIDER:0:30}…"

# ── Step 5: Withdraw (provider query param required) ──
step "Withdrawing service..."
WITHDRAW_RESP=$(curl -sf -X DELETE \
  "${GATEWAY}/v1/services/${SVC_ID}?provider=${PROVIDER}" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} 2>&1) \
  || fail "Withdraw failed: $WITHDRAW_RESP"
ok "Withdrawn: $SVC_ID"

# ── Step 6: Verify withdrawal ──
step "Verifying service is gone..."
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
  ${AUTH_HEADER:+-H "$AUTH_HEADER"} \
  "$GATEWAY/v1/services/$SVC_ID")
if [ "$HTTP_CODE" = "404" ]; then
  ok "Service correctly removed (404)"
else
  fail "Expected 404 after withdrawal, got $HTTP_CODE"
fi

echo ""
echo -e "${GREEN}[Flow B] Service Discovery demo completed successfully${NC}"
