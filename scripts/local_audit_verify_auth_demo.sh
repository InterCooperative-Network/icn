#!/usr/bin/env bash
# ============================================================================
# local_audit_verify_auth_demo.sh
# ----------------------------------------------------------------------------
# Demonstrates the `icnctl audit verify --token / ICN_TOKEN` authentication
# support against a live LOCAL gateway, and documents — honestly — the one
# remaining piece a future NYCN v4 demo needs to make `icnctl audit verify`
# PASS on a full economic receipt chain.
#
# WHAT THIS PROVES (real, no faked ICN behaviour):
#   * Before this change, `icnctl audit verify <hash>` sent no auth token, so a
#     JWT-secured gateway answered `401 Unauthorized` on
#     `GET /v1/receipts/chain/{hash}` — the exact wall the NYCN v3 demo hit.
#   * After this change, `icnctl audit verify <hash> --token <JWT>` (or the
#     `ICN_TOKEN` env var) authenticates and REACHES the receipts-chain
#     endpoint. The demo shows both: no-token -> 401, with-token -> reaches the
#     endpoint and returns the real audit result.
#
# WHAT THIS DOES NOT CLAIM (honest boundary):
#   * It does NOT produce the economic receipt chain (AllocationReceipt ->
#     SettlementIntent -> LedgerEntry), so for a decision-registry record the
#     audit still reports the chain as incomplete (a truthful FAILED, not a
#     401). Producing the full chain requires a *voted budget/treasury-execution
#     decision*, which requires *active Member standing* in the domain.
#   * Establishing Member standing over HTTP currently requires the SDIS
#     enrollment ceremony (holder creation + level verification); there is no
#     safe self-service "make me a Member" route, and adding a broad one would
#     weaken security. That standing seed is intentionally NOT scripted here.
#   * The in-process proof that the full chain verifies already exists and runs
#     in CI: icn/crates/icn-core/tests/receipt_chain_vertical_slice.rs
#     (Identity -> Governance proposal+vote -> execution gate -> treasury
#     effects -> AllocationReceipt provenance).
#   * Next smallest safe piece (separate PR): a demo/dev-gated LOCAL standing
#     seed (e.g. an offline `icnctl` helper that completes enrollment / writes a
#     Member affiliation into a local data dir) so a live gateway can produce a
#     verifiable chain that `icnctl audit verify --token` then PASSES.
#
# No production deployment, no live public map, no real participant data, and
# no payment/wallet/balance/currency framing. Fictional data only. The ICN repo
# is used read-only; the gateway runs in an ephemeral temp dir and is torn down
# on exit.
#
# Usage:
#   ./scripts/local_audit_verify_auth_demo.sh
# Env overrides:
#   ICND   - path to the icnd binary   (default: <workspace>/icn/target/release/icnd)
#   ICNCTL - path to the icnctl binary (default: <workspace>/icn/target/{release,debug}/icnctl)
#   ICN_GW_PORT - gateway port (default: 18080)
# ============================================================================
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
find_bin() {
  local name="$1"
  for c in "$ROOT/icn/target/release/$name" "$ROOT/icn/target/debug/$name"; do
    [ -x "$c" ] && { echo "$c"; return 0; }
  done
  return 1
}
ICND="${ICND:-$(find_bin icnd || true)}"
ICNCTL="${ICNCTL:-$(find_bin icnctl || true)}"
PORT="${ICN_GW_PORT:-18080}"
GW="http://127.0.0.1:$PORT"
OUT="$(mktemp -d /tmp/icn-audit-auth-demo-XXXXXX)"
DPID=""

log(){ echo "[$(date -u +%H:%M:%SZ)] $*"; }
fatal(){ echo "FATAL: $*" >&2; exit 1; }
cleanup(){
  if [ -n "$DPID" ]; then
    kill "$DPID" 2>/dev/null
    wait "$DPID" 2>/dev/null
    log "gateway torn down"
  fi
  rm -rf "$OUT" 2>/dev/null
}
trap cleanup EXIT INT TERM

[ -n "$ICND" ] && [ -x "$ICND" ]     || fatal "icnd not found (build it or set ICND=...)"
[ -n "$ICNCTL" ] && [ -x "$ICNCTL" ] || fatal "icnctl not found (build it or set ICNCTL=...)"
command -v curl >/dev/null && command -v python3 >/dev/null && command -v openssl >/dev/null \
  || fatal "curl, python3 and openssl are required"
log "icnd   = $ICND"
log "icnctl = $ICNCTL"

# --- start a JWT-secured local gateway ---------------------------------------
export ICN_KEYSTORE_PASSPHRASE="audit-auth-demo-passphrase-not-secret-0000"
JWT_SECRET="$(openssl rand -hex 32)"
export ICN_GATEWAY_JWT_SECRET="$JWT_SECRET"
# Dev gate (issue #2075): self-asserted /auth/verify coop issuance is fail-closed
# unless ICN_DEV_MODE is set. This LOCAL loopback demo mints a token that way, so
# opt into the explicit dev posture. Never set this on a production gateway.
export ICN_DEV_MODE=1
log "[1/5] init + start JWT-secured local gateway on $GW"
"$ICND" --init --data-dir "$OUT/data" --node-name audit-auth-demo \
  --init-gateway-port "$PORT" --init-gossip-port "$((PORT+1000))" >"$OUT/init.log" 2>&1 \
  || { cat "$OUT/init.log"; fatal "icnd --init failed"; }
"$ICND" --config "$OUT/data/config.toml" --data-dir "$OUT/data" \
  --gateway-enable --gateway-bind "127.0.0.1:$PORT" --gateway-jwt-secret "$JWT_SECRET" \
  >"$OUT/gateway.log" 2>&1 &
DPID=$!
for i in $(seq 1 45); do
  curl -sf "$GW/v1/healthz" >/dev/null 2>&1 && break
  kill -0 "$DPID" 2>/dev/null || { tail -20 "$OUT/gateway.log"; fatal "gateway died on startup"; }
  sleep 1
done
curl -sf "$GW/v1/healthz" >/dev/null 2>&1 || fatal "gateway never became healthy"
log "  gateway healthy"

# --- mint a real JWT ----------------------------------------------------------
log "[2/5] mint a real JWT (icnctl auth token)"
"$ICNCTL" --data-dir "$OUT/data" auth token --gateway "$GW" --coop-id default \
  --scopes 'governance:read,governance:write,ledger:read,ledger:write,coop:read,coop:write,treasury:read,treasury:write' \
  >"$OUT/auth.log" 2>&1
TOKEN="$(grep -Eo 'eyJ[A-Za-z0-9_.-]+' "$OUT/auth.log" | head -1)"
[ -n "$TOKEN" ] || { cat "$OUT/auth.log"; fatal "failed to mint JWT"; }
MYDID="$(echo "$TOKEN" | cut -d. -f2 | python3 -c 'import sys,base64,json;p=sys.stdin.read().strip();p+="="*((-len(p))%4);print(json.loads(base64.urlsafe_b64decode(p))["sub"])')"
log "  node DID: $MYDID"

# --- record a real decision so there is a real 64-hex decisionHash to verify --
log "[3/5] record a real decision (authenticated) to get a decisionHash"
AUTH=(-H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json')
DEC="$(curl -s -X POST "$GW/v1/registry/decisions" "${AUTH[@]}" \
  -d "{\"coopId\":\"$MYDID\",\"meetingId\":\"audit-auth-demo\",\"title\":\"audit verify auth demo\",\"tags\":[\"demo\"],\"status\":\"approved\",\"proposalType\":\"text\"}")"
HASH="$(echo "$DEC" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("decisionHash",""))')"
{ [ -n "$HASH" ] && [ "${#HASH}" = 64 ]; } || { echo "$DEC"; fatal "no decisionHash"; }
log "  decisionHash = $HASH"

# --- THE POINT: audit verify WITHOUT vs WITH a token --------------------------
log "[4/5] icnctl audit verify WITHOUT a token (expected: 401 Unauthorized)"
# Clear any ambient ICN_TOKEN so this leg is genuinely unauthenticated even when
# the demo is run from a shell that already exports ICN_TOKEN (audit verify now
# falls back to ICN_TOKEN). Without this, the "no-token" call could silently
# authenticate and defeat the boundary it proves.
NOAUTH_OUT="$(env -u ICN_TOKEN "$ICNCTL" audit verify "$HASH" --gateway "$GW" 2>&1 || true)"
echo "$NOAUTH_OUT" | sed 's/^/    /'
# This script exists to PROVE the auth boundary, so fail closed if the
# unauthenticated request is NOT rejected with 401.
echo "$NOAUTH_OUT" | grep -q '401' \
  || fatal "no-token request did NOT return 401 - the auth boundary is not being enforced; refusing to claim the auth proof"
log "  CONFIRMED: no-token request is rejected with 401 (the v3 boundary)"

log "[5/5] icnctl audit verify WITH --token (expected: reaches endpoint, NOT 401)"
AUTH_OUT="$("$ICNCTL" audit verify "$HASH" --gateway "$GW" --token "$TOKEN" 2>&1 || true)"
echo "$AUTH_OUT" | sed 's/^/    /'
# Fail closed on ANY transport/auth error (401/403/404/5xx/connect/parse) — none
# of those prove the tokened request reached and exercised the verifier.
if echo "$AUTH_OUT" | grep -qiE 'Gateway returned error|Failed to connect|No receipt chain found|schema mismatch'; then
  fatal "with --token the request did not cleanly reach the receipts endpoint: $AUTH_OUT"
fi
# Positively assert the tokened request reached and ran the receipt-chain verifier.
echo "$AUTH_OUT" | grep -q 'Receipt Chain Audit' \
  || fatal "with --token the receipt-chain verifier output was not produced — the request did not reach the verifier: $AUTH_OUT"
log "  CONFIRMED: with --token the request authenticates, reaches the endpoint, and runs the receipt-chain verifier."
log ""
log "Honest boundary: for this decision-registry record there is no economic"
log "receipt chain yet, so audit verify reports an incomplete chain (a truthful"
log "result, not a 401). Producing a chain that PASSES needs a voted"
log "budget/treasury decision with active Member standing — see this script's"
log "header and icn/crates/icn-core/tests/receipt_chain_vertical_slice.rs."
log ""
log "RESULT: audit-verify AUTHENTICATION works (no-token -> 401; with-token -> reaches the receipt-chain verifier)."
log "Note: this proves the AUTH boundary only; it does not assert that the economic chain itself verifies."
echo "RESULT: PASS (audit-verify authentication only)"
