#!/usr/bin/env bash
# ============================================================================
# local_economic_receipt_chain_demo.sh
# ----------------------------------------------------------------------------
# Faithful LIVE-DAEMON proof of a complete governed coordination receipt chain.
#
# Companion to scripts/local_audit_verify_auth_demo.sh. Where that script proves
# only the `icnctl audit verify --token` AUTH boundary (no-token -> 401,
# with-token -> reaches the verifier) against a registry-only decision, THIS
# script drives the full governed lifecycle over the REAL icnd gateway HTTP
# path so the verifier runs against a complete provenance chain:
#
#   dev-gated member-standing bootstrap (POST /v1/commons/dev/bootstrap-standing)
#     -> create governed allocation/contribution proposal (POST /v1/gov/proposals)
#     -> open  (POST /v1/gov/proposals/{id}/open)
#     -> vote  (POST /v1/gov/proposals/{id}/vote)
#     -> close (POST /v1/gov/proposals/{id}/close)  [captures decision_hash]
#     -> icnctl audit verify --token <decision_hash>  [must report VERIFIED]
#
# The chain the verifier checks is produced by the real daemon wiring, not a
# test reconstruction: governance receipt + allocation/contribution receipt +
# intent are written through GovernanceManager.with_receipt_store on the close
# path; the execution record + journal entry are produced by the daemon
# actor/event-bus/executor path. `icnctl audit verify` (13 checks) then confirms
# every artifact is present and provenance-linked to the same decision hash.
#
# Cautious framing: coordination / allocation / contribution receipts,
# provenance chain, audit trail. No payment / token / bank / money-transmission
# / financial-settlement framing. Fictional data only. The gateway runs in an
# ephemeral temp dir on loopback and is torn down on exit. No external network,
# no NYCN production infrastructure.
#
# Usage:   ./scripts/local_economic_receipt_chain_demo.sh
# Env:
#   ICND        path to icnd   (default: <workspace>/icn/target/{release,debug}/icnd)
#   ICNCTL      path to icnctl (default: <workspace>/icn/target/{release,debug}/icnctl)
#   ICN_GW_PORT gateway port   (default: 18080)
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
OUT="$(mktemp -d /tmp/icn-econ-chain-demo-XXXXXX)"
DPID=""
DOMAIN="demo-econ-receipt-chain-coop"

log()   { echo "[$(date -u +%H:%M:%SZ)] $*"; }
fatal() { echo "FATAL: $*" >&2; exit 1; }
indent(){ sed 's/^/    /'; }
cleanup(){
  if [ -n "$DPID" ]; then
    kill "$DPID" 2>/dev/null
    wait "$DPID" 2>/dev/null
    log "gateway torn down"
  fi
  # KEEP=1 preserves the temp runtime dir (logs) for post-mortem diagnosis.
  if [ "${KEEP:-0}" = "1" ]; then log "runtime/logs preserved at $OUT"; else rm -rf "$OUT" 2>/dev/null; fi
}
trap cleanup EXIT INT TERM

# jq-free JSON field reader: jget <dotted.key.path> < file.
# Safe traversal only (no eval): splits the path on '.' and walks dict keys.
jget(){ python3 -c 'import sys,json
try:
    d=json.load(sys.stdin)
    for k in sys.argv[1].split("."):
        d=d[k]
    print(d)
except Exception:
    print("")' "$1" 2>/dev/null; }

# Authed JSON POST with a status check: post <label> <url> <outfile> <json>.
# Prints "<label> (<code>):" + the response body and fails closed on HTTP >= 400
# so a bad lifecycle step is named immediately. (AUTH is set after token mint.)
post(){
  local label="$1" url="$2" out="$3" body="$4" code rc
  code="$(curl -s -o "$out" -w '%{http_code}' -X POST "$url" "${AUTH[@]}" -d "$body")"; rc=$?
  echo "  $label (HTTP $code):"; indent <"$out"; echo
  # Fail closed on transport errors: curl's own exit code catches connection
  # failures (refused/reset/DNS), and HTTP 000 is curl's "no response" sentinel.
  # Neither must slip through the "< 400" success check and be read as success.
  [ "$rc" -eq 0 ] || fatal "$label: curl transport failure (exit $rc, code ${code:-none})"
  { [ "${code:-000}" -ge 100 ] && [ "${code:-000}" -lt 400 ]; } 2>/dev/null \
    || fatal "$label returned HTTP ${code:-000}"
}

[ -n "$ICND" ] && [ -x "$ICND" ]     || fatal "icnd not found (build it: cargo build --release -p icnd, or set ICND=...)"
[ -n "$ICNCTL" ] && [ -x "$ICNCTL" ] || fatal "icnctl not found (build it: cargo build --release -p icnctl, or set ICNCTL=...)"
command -v curl >/dev/null && command -v python3 >/dev/null && command -v openssl >/dev/null \
  || fatal "curl, python3 and openssl are required"
log "icnd   = $ICND"
log "icnctl = $ICNCTL"

# --- [1/8] start a JWT-secured, dev-gated local gateway -----------------------
export ICN_KEYSTORE_PASSPHRASE="econ-chain-demo-passphrase-not-secret-0000"
JWT_SECRET="$(openssl rand -hex 32)"
export ICN_GATEWAY_JWT_SECRET="$JWT_SECRET"
# Dev gate (issue #2075): self-asserted /auth/verify coop issuance is fail-closed
# unless ICN_DEV_MODE is set. This LOCAL loopback demo mints tokens that way, so
# opt into the explicit dev posture. Never set this on a production gateway.
export ICN_DEV_MODE=1
# Dev gate for POST /v1/commons/dev/bootstrap-standing: requires admin endpoints
# enabled AND a non-Production posture. Both are set for this LOCAL demo only.
export ICN_ENABLE_ADMIN_ENDPOINTS=true
export ICN_GOVERNANCE_BUILD_MODE=test
# DEV/LOCAL ONLY: seed the node's own DID with genesis self-trust so the ledger
# accepts the node's own governed-settlement journal entry. The ledger trust
# gate (author trust >= 0.1) stays fully enforced — the local operator's node
# legitimately roots its own trust web. Never set this in production.
export ICN_DEV_SELF_TRUST=1
log "[1/8] init + start JWT-secured dev gateway on $GW"
"$ICND" --init --data-dir "$OUT/data" --node-name econ-chain-demo \
  --init-gateway-port "$PORT" --init-gossip-port "$((PORT+1000))" >"$OUT/init.log" 2>&1 \
  || { cat "$OUT/init.log"; fatal "icnd --init failed"; }
"$ICND" --config "$OUT/data/config.toml" --data-dir "$OUT/data" \
  --gateway-enable --gateway-bind "127.0.0.1:$PORT" --gateway-jwt-secret "$JWT_SECRET" \
  >"$OUT/gateway.log" 2>&1 &
DPID=$!
for _ in $(seq 1 60); do
  curl -sf "$GW/v1/healthz" >/dev/null 2>&1 && break
  kill -0 "$DPID" 2>/dev/null || { tail -25 "$OUT/gateway.log"; fatal "gateway died on startup"; }
  sleep 1
done
curl -sf "$GW/v1/healthz" >/dev/null 2>&1 || { tail -25 "$OUT/gateway.log"; fatal "gateway never became healthy"; }
log "  gateway healthy"

# --- [2/8] mint a real JWT for the node DID -----------------------------------
log "[2/8] mint a real JWT (icnctl auth token)"
"$ICNCTL" --data-dir "$OUT/data" auth token --gateway "$GW" --coop-id default \
  --scopes 'governance:read,governance:write,ledger:read,ledger:write,coop:read,coop:write,treasury:read,treasury:write' \
  >"$OUT/auth.log" 2>&1 || { cat "$OUT/auth.log"; fatal "auth token failed"; }
TOKEN="$(grep -Eo 'eyJ[A-Za-z0-9_.-]+' "$OUT/auth.log" | head -1)"
[ -n "$TOKEN" ] || { cat "$OUT/auth.log"; fatal "failed to mint JWT"; }
export ICN_TOKEN="$TOKEN"
MYDID="$(echo "$TOKEN" | cut -d. -f2 | python3 -c 'import sys,base64,json;p=sys.stdin.read().strip();p+="="*((-len(p))%4);print(json.loads(base64.urlsafe_b64decode(p))["sub"])')"
[ -n "$MYDID" ] || fatal "could not decode node DID from token"
log "  member DID: $MYDID"
AUTH=(-H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json')

# --- [3/8] create a governance domain (our DID is the sole member) ------------
# 1% quorum / 1% approval so a single For vote carries the decision.
log "[3/8] create governance domain '$DOMAIN'"
post "domain create" "$GW/v1/gov/domains" "$OUT/domain.json" "{
  \"id\":\"$DOMAIN\",\"name\":\"Demo Coordination Receipt Chain Coop\",
  \"profile\":\"cooperative_default\",\"quorum_percent\":1,\"approval_percent\":1,
  \"voting_period_days\":1,\"members\":[\"$MYDID\"]}"

# --- [4/8] dev-gated member-standing bootstrap (real HTTP path) ---------------
log "[4/8] bootstrap member standing via POST /v1/commons/dev/bootstrap-standing"
BOOT_CODE="$(curl -s -o "$OUT/boot.json" -w '%{http_code}' -X POST \
  "$GW/v1/commons/dev/bootstrap-standing" "${AUTH[@]}" -d "{\"jurisdiction_id\":\"$DOMAIN\"}")"
cat "$OUT/boot.json" | indent; echo
[ "$BOOT_CODE" = "200" ] || fatal "bootstrap-standing returned HTTP $BOOT_CODE (expected 200; check dev gate / posture)"
log "  member standing established (200)"

# --- [5/8] create governed allocation/contribution proposal -------------------
log "[5/8] create governed allocation proposal (budget payload)"
post "proposal create" "$GW/v1/gov/proposals" "$OUT/prop.json" "{
  \"domain_id\":\"$DOMAIN\",\"title\":\"Q1 coordination allocation\",
  \"description\":\"Allocate compute-hours to infrastructure upkeep.\",
  \"payload\":{\"type\":\"budget\",\"amount\":100,\"recipient\":\"$MYDID\",
  \"currency\":\"compute-hours\",\"purpose\":\"infra-upkeep\"}}"
PID="$(jget id <"$OUT/prop.json")"
[ -n "$PID" ] || fatal "no proposal id returned (create rejected — member_checker or validation?)"
log "  proposal id: $PID"

# --- [6/8] open -> vote(for) -> close -----------------------------------------
log "[6/8] open -> vote(for) -> close"
post "open"  "$GW/v1/gov/proposals/$PID/open"  "$OUT/open.json"  '{"voting_period_seconds":3600}'
post "vote"  "$GW/v1/gov/proposals/$PID/vote"  "$OUT/vote.json"  '{"choice":"for"}'
post "close" "$GW/v1/gov/proposals/$PID/close" "$OUT/close.json" '{}'

# --- capture the hex decision_hash --------------------------------------------
# The close response does not carry it; the proof endpoint exposes the governance
# receipt's decision_hash, which is serialized as a 32-byte array (not hex).
# Normalize either form (64-char hex string OR 32-int array) to lowercase hex.
curl -s "$GW/v1/gov/proposals/$PID/proof" "${AUTH[@]}" >"$OUT/proof.json" 2>&1
DHASH="$(python3 -c 'import sys,json
def tohex(v):
    if isinstance(v, str):  return v if len(v) == 64 else ""
    if isinstance(v, list) and len(v) == 32: return "".join("%02x" % (b & 0xff) for b in v)
    return ""
for f in sys.argv[1:]:
    try: d = json.load(open(f))
    except Exception: continue
    if not isinstance(d, dict): continue
    for cand in (d.get("decision_hash"), (d.get("receipt") or {}).get("decision_hash")):
        h = tohex(cand)
        if h: print(h); sys.exit(0)
print("")' "$OUT/close.json" "$OUT/proof.json")"
{ [ -n "$DHASH" ] && [ "${#DHASH}" = 64 ]; } \
  || fatal "no 64-hex decision_hash after close (got '$DHASH'); close outcome may not be Accepted"
log "  decision_hash = $DHASH"

# --- [7/8] authenticated audit verify of the FULL chain -----------------------
# Acceptance dispatch (executor -> execution record + ledger journal) runs on the
# daemon actor/event-bus path asynchronously, so we poll a bounded window for the
# chain to settle before asserting — mirroring the in-process proof's bounded
# wait (demo_member_standing_receipt_chain.rs). Assertions are unchanged: all 13
# checks must still pass; we only allow the async pipeline time to complete.
log "[7/8] icnctl audit verify --token (poll up to ~25s for async settlement)"
VERIFY_RC=1; VERIFIED=""
for attempt in $(seq 1 25); do
  "$ICNCTL" audit verify "$DHASH" --gateway "$GW" --token "$TOKEN" --json \
    >"$OUT/verify.json" 2>"$OUT/verify.err"
  VERIFY_RC=$?
  VERIFIED="$(jget summary.verified <"$OUT/verify.json")"
  { [ "$VERIFY_RC" -eq 0 ] && [ "$VERIFIED" = "True" ]; } && break
  log "  attempt $attempt: $(jget summary.passed <"$OUT/verify.json")/$(jget summary.total <"$OUT/verify.json") passed (awaiting async settlement)"
  sleep 1
done
indent <"$OUT/verify.json"
[ -s "$OUT/verify.err" ] && { echo "  stderr:"; indent <"$OUT/verify.err"; }

# --- [8/8] assert the chain is complete and audit-verified --------------------
log "[8/8] assert all receipt-chain checks pass"
VERIFIED="$(jget summary.verified <"$OUT/verify.json")"
PASSED="$(jget summary.passed <"$OUT/verify.json")"
TOTAL="$(jget summary.total <"$OUT/verify.json")"
if [ "$VERIFY_RC" -eq 0 ] && [ "$VERIFIED" = "True" ]; then
  log "RESULT: PASS — governed allocation produced a complete, audit-verified coordination receipt chain over the live gateway path ($PASSED/$TOTAL checks)."
  echo "RESULT: PASS (full coordination receipt chain audit-verified: $PASSED/$TOTAL)"
  exit 0
fi
echo "  --- diagnostic: governance-app chain view (GET /v1/gov/proposals/{id}/chain) ---"
curl -s "$GW/v1/gov/proposals/$PID/chain" "${AUTH[@]}" | indent; echo
echo "  --- daemon log: governance / receipt-store / executor wiring lines ---"
grep -iE 'governance manager|receipt.?store|install.?backend|install_receipt|executor|effect subscription|standalone|connected to daemon|dispatch' "$OUT/gateway.log" 2>/dev/null | tail -25 | indent
echo "  Failing checks:"
python3 -c 'import sys,json
try:
    d=json.load(open(sys.argv[1]))
    for c in d.get("checks",[]):
        if not c.get("passed"): print("    [FAIL]",c.get("name"),"-",c.get("detail"))
except Exception as e:
    print("    (could not parse verify output:",e,")")' "$OUT/verify.json"
fatal "audit verify did NOT report a complete chain ($PASSED/$TOTAL passed). The failing check(s) above pinpoint the live-path seam (e.g. execution record or journal entry not populated by the daemon close path)."
