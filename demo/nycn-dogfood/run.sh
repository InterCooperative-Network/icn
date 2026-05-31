#!/usr/bin/env bash
# NYCN dogfood keystone demo -- repeatable.
#
# Proves (core claim):    ICN can turn cooperative work into legible obligations
#                         and verifiable receipts today.
# Receipt claim (narrow): the receipt proves THIS actor discharged THIS obligation
#                         at THIS time. It binds item_id, domain_id, actor_did,
#                         transition, completed_at -- NOT the editable title/description.
# Rehearsal only:         local gateway, test identity, in-tree institutions/nycn
#                         fixture. Not production. Not proposal/vote. Not federation.
#
# Usage:
#   ICN_PASSPHRASE=<pass> ./run.sh [--fresh] [--record] [--force-port-cleanup]
#     --fresh               new run-local data dir + new local gateway (recommended)
#     --record              tee a transcript and render an HTML recording
#     --force-port-cleanup  reclaim the port even if held by a non-script process
#
# Env overrides: GW_PORT GOSSIP_PORT GW ICND ICNCTL PKG COOP_ID DOMAIN SCOPES DOGFOOD_OUT_DIR ICN_PASSPHRASE
set -Eeuo pipefail
trap 'echo "ERROR: failed near line $LINENO" >&2' ERR

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(git -C "$HERE" rev-parse --show-toplevel)"
GW_PORT="${GW_PORT:-8085}"
GOSSIP_PORT="${GOSSIP_PORT:-7799}"   # run-local gossip listener (NOT the default :7777)
GW="${GW:-http://127.0.0.1:${GW_PORT}}"
ICND="${ICND:-$REPO/icn/target/release/icnd}"
ICNCTL="${ICNCTL:-$REPO/icn/target/release/icnctl}"
PKG="${PKG:-$REPO/institutions/nycn}"
COOP_ID="${COOP_ID:-nycn}"
DOMAIN="${DOMAIN:-nycn-federation-gov}"
SCOPES="${SCOPES:-governance:read,governance:write,coop:read,coop:write,coop:admin}"
OUT_BASE="${DOGFOOD_OUT_DIR:-$REPO/demo/nycn-dogfood/runs}"

FRESH=0; RECORD=0; FORCE_PORT=0
for a in "$@"; do case "$a" in
  --fresh) FRESH=1 ;;
  --record) RECORD=1 ;;
  --force-port-cleanup) FORCE_PORT=1 ;;
  -h|--help) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) echo "unknown arg: $a (try --help)" >&2; exit 2 ;;
esac; done

die(){ echo "ERROR: $*" >&2; exit 1; }
say(){ printf '\n== %s ==\n' "$*"; }
note(){ printf '   %s\n' "$*"; }

# ---- optional recording: re-exec once under tee, then render HTML ----
if [ "$RECORD" = 1 ] && [ "${DOGFOOD_TEEING:-0}" != "1" ]; then
  rec_ts="$(date -u +%Y%m%d-%H%M%SZ 2>/dev/null || echo run)"
  REC_DIR="$OUT_BASE/$rec_ts"; mkdir -p "$REC_DIR"
  export DOGFOOD_TEEING=1 DOGFOOD_RUN_DIR="$REC_DIR"
  set +e
  "$0" "$@" 2>&1 | tee "$REC_DIR/transcript.txt"
  rc=${PIPESTATUS[0]}
  set -e
  if [ "$rc" -eq 0 ] && command -v python3 >/dev/null 2>&1; then
    python3 "$HERE/make-recording.py" "$REC_DIR/transcript.txt" "$REC_DIR/recording.html" >/dev/null 2>&1 \
      && echo "recording: $REC_DIR/recording.html"
  fi
  echo "artifacts: $REC_DIR"
  exit "$rc"
fi

# ---- prechecks (print build command; never auto-build) ----
for dep in curl jq python3; do command -v "$dep" >/dev/null 2>&1 || die "missing dependency: $dep"; done
BUILD_HINT="(cd \"$REPO/icn\" && cargo build --release -p icnd -p icnctl)"
[ -x "$ICND" ]   || die "icnd not found at $ICND -- build it:  $BUILD_HINT"
[ -x "$ICNCTL" ] || die "icnctl not found at $ICNCTL -- build it:  $BUILD_HINT"
[ -d "$PKG" ]    || die "NYCN package not found at $PKG"
: "${ICN_PASSPHRASE:?Set ICN_PASSPHRASE (operator keystore passphrase), e.g. export ICN_PASSPHRASE=demo123}"
export RUST_LOG="${RUST_LOG:-error}"

# ---- run-local output dir ----
TS="$(date -u +%Y%m%d-%H%M%SZ 2>/dev/null || echo run)"
RUN_DIR="${DOGFOOD_RUN_DIR:-$OUT_BASE/$TS}"; mkdir -p "$RUN_DIR"
DATA_DIR="$RUN_DIR/icnd-data"
LOG="$RUN_DIR/gateway.log"
LAST_PID="$OUT_BASE/.last-icnd.pid"

gw_health(){ curl -sf -m3 "$GW/v1/health" >/dev/null 2>&1; }
port_listening(){ local p="${1:-$GW_PORT}"; (exec 3<>"/dev/tcp/127.0.0.1/$p") 2>/dev/null && { exec 3>&- 3<&-; return 0; }; return 1; }
# Gossip is UDP (QUIC), so /dev/tcp cannot see it -- probe with ss by source port.
udp_listening(){ local p="$1"; command -v ss >/dev/null 2>&1 || return 1; [ -n "$(ss -Huln "sport = :$p" 2>/dev/null)" ]; }
free_port(){
  local p="${1:-$GW_PORT}" proto="${2:-tcp}"
  if command -v fuser >/dev/null 2>&1; then fuser -k "${p}/${proto}" 2>/dev/null || true
  elif command -v lsof >/dev/null 2>&1; then
    if [ "$proto" = udp ]; then lsof -tiUDP:"$p" 2>/dev/null | xargs -r kill 2>/dev/null || true
    else lsof -tiTCP:"$p" -sTCP:LISTEN 2>/dev/null | xargs -r kill 2>/dev/null || true; fi
  else die "cannot free :$p/$proto (no fuser/lsof); free it manually"; fi
  sleep 1
}
stop_script_gw(){
  [ -f "$LAST_PID" ] || return 0
  local pid; pid="$(cat "$LAST_PID" 2>/dev/null || true)"
  if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null && \
     ps -p "$pid" -o args= 2>/dev/null | grep -q "gateway-bind 127.0.0.1:${GW_PORT}"; then
    note "stopping prior script-owned gateway (pid $pid)"; kill "$pid" 2>/dev/null || true; sleep 1
  fi
  rm -f "$LAST_PID"
}
start_gw(){
  mkdir -p "$DATA_DIR"; chmod 700 "$DATA_DIR"
  # Init the run-local node identity BEFORE starting the gateway so icnd takes the
  # identity-backed path (actors + event broadcaster) and the gateway opens the
  # PERSISTENT run-local store (Some(data_dir)) instead of the ephemeral
  # GatewayServer::new() fallback (data_dir: None). Without this, --fresh runs the
  # no-identity HTTP path on temporary storage and the receipt would not persist.
  # icnd --init reads ICN_PASSPHRASE; see bins/icnd/src/main.rs,
  # crates/icn-core/src/supervisor/init_gateway.rs, crates/icn-gateway/src/server.rs.
  #
  # Pin gateway + gossip to RUN-LOCAL ports and start with the generated --config, so the
  # identity-backed node never binds the default gossip listener (:7777) and cannot collide
  # with a developer's already-running default node. Gossip is bound to loopback (init writes
  # 0.0.0.0); the demo is single-node, so it needs no externally-reachable gossip.
  "$ICND" --init --data-dir "$DATA_DIR" --init-gateway-port "$GW_PORT" --init-gossip-port "$GOSSIP_PORT" </dev/null >"$RUN_DIR/node-init.log" 2>&1 || die "node identity init failed (see $RUN_DIR/node-init.log)"
  sed -i "s#^listen_addr = \"0.0.0.0:${GOSSIP_PORT}\"#listen_addr = \"127.0.0.1:${GOSSIP_PORT}\"#" "$DATA_DIR/config.toml" 2>/dev/null || true
  local secret; secret="$(openssl rand -hex 32 2>/dev/null || head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  setsid "$ICND" --config "$DATA_DIR/config.toml" --data-dir "$DATA_DIR" --gateway-enable \
    --gateway-bind "127.0.0.1:${GW_PORT}" --gateway-jwt-secret "$secret" \
    --log-level warn >"$LOG" 2>&1 </dev/null &
  local pid=$!
  echo "$pid" > "$LAST_PID"
  note "started identity-backed local gateway (pid $pid, gateway 127.0.0.1:${GW_PORT}, gossip 127.0.0.1:${GOSSIP_PORT}, data: $DATA_DIR)"
}

say "0. Local gateway lifecycle"
if [ "$FRESH" = 1 ]; then
  stop_script_gw
  if port_listening "$GW_PORT"; then
    if [ "$FORCE_PORT" = 1 ]; then free_port "$GW_PORT" tcp; else die ":$GW_PORT/tcp (gateway) busy (not script-owned). Free it or pass --force-port-cleanup."; fi
  fi
  if udp_listening "$GOSSIP_PORT"; then
    if [ "$FORCE_PORT" = 1 ]; then free_port "$GOSSIP_PORT" udp; else die ":$GOSSIP_PORT/udp (gossip) busy (not script-owned). Free it or pass --force-port-cleanup."; fi
  fi
  start_gw
else
  if gw_health; then note "reusing healthy gateway on $GW"
  elif port_listening; then
    if [ "$FORCE_PORT" = 1 ]; then free_port; start_gw; else die ":$GW_PORT busy but not a healthy gateway. Use --fresh, or --force-port-cleanup."; fi
  else start_gw; fi
fi
for i in $(seq 1 10); do gw_health && break; [ "$i" = 10 ] && die "gateway not healthy on $GW (see $LOG)"; sleep 1; done

say "1. The cooperative's own node is running (local, on our hardware)"
curl -sf "$GW/v1/health" | jq .
note "No central server. This node belongs to the cooperative."

say "2. An organizer's identity -- a DID they alone control"
id_out="$("$ICNCTL" id show 2>/dev/null || true)"
grep -q 'did:icn:' <<<"$id_out" || { "$ICNCTL" id init >/dev/null 2>&1 || die "id init failed"; id_out="$("$ICNCTL" id show 2>/dev/null || true)"; }
DID="$(grep -oE 'did:icn:[A-Za-z0-9]+' <<<"$id_out" | head -1)"
[ -n "$DID" ] || die "could not resolve organizer DID"
note "Organizer DID: $DID"

say "3. Authenticate to the node"
tok_out="$("$ICNCTL" auth token -g "$GW" -c "$COOP_ID" -s "$SCOPES" 2>/dev/null || true)"
TOKEN="$(grep -oE 'eyJ[A-Za-z0-9_.-]+' <<<"$tok_out" | head -1)"
[ -n "$TOKEN" ] || die "auth token failed (empty token) -- check scopes/gateway"
AUTH="Authorization: Bearer $TOKEN"
note "Session token acquired."

say "4. Stand up the NYCN institution (federation + governance domain)"
"$ICNCTL" institution bootstrap apply -g "$GW" -c "$COOP_ID" --package "$PKG" 2>/dev/null \
  | awk '/Completed steps:/{f=1} f' || die "bootstrap apply failed"

say "5. Turn a piece of organizing work into a legible obligation"
note 'Obligation: "Confirm Summit 2026 venue booking"'
item_json="$(curl -s -X POST -H "$AUTH" -H 'Content-Type: application/json' \
  -d "{\"title\":\"Confirm Summit 2026 venue booking\",\"description\":\"NYCN organizing: confirm the venue contract for the 2026 Summit\",\"assignee\":\"$DID\",\"priority\":\"high\",\"meeting_context\":\"NYCN organizing team\"}" \
  "$GW/v1/gov/domains/$DOMAIN/action-items")"
ITEM_ID="$(jq -r '.id // empty' <<<"$item_json")"
[ -n "$ITEM_ID" ] || die "action item creation failed: $item_json"
note "Recorded as a tracked obligation. id: $ITEM_ID"

say "6. The organizer sees it as a plain-language action card (what they owe)"
cards_json="$(curl -s -H "$AUTH" "$GW/v1/gov/me/action-cards")"
card_n="$(jq --arg id "$ITEM_ID" '[.cards[] | select(.source_id==$id)] | length' <<<"$cards_json")"
[ "$card_n" = "1" ] || die "expected exactly one action card for $ITEM_ID before completion, found $card_n: $cards_json"
jq --arg id "$ITEM_ID" '.cards[] | select(.source_id==$id) | {title, summary, receipt_expected}' <<<"$cards_json"

say "7. The organizer does the work and marks it done"
curl -s -X PUT -H "$AUTH" -H 'Content-Type: application/json' -d '{"status":"completed"}' \
  "$GW/v1/gov/domains/$DOMAIN/action-items/$ITEM_ID" | jq '{title, status}'

say "8. The node issues a verifiable completion receipt"
receipt="$(curl -s -H "$AUTH" "$GW/v1/gov/domains/$DOMAIN/action-items/$ITEM_ID/completion-receipt")"
jq . <<<"$receipt"
# Verify the receipt is well-formed and binds THIS obligation: a 32-byte BLAKE3
# record_hash over exactly {item_id, domain_id, actor_did, transition, completed_at}
# (canonical hasher ActionItemCompletionReceipt::compute_record_hash, domain tag
# "icn:gov:action_item_completion:v1", in crates/icn-governance/src/proof.rs). This
# kit checks the binding + 32-byte shape; it does not itself re-derive the BLAKE3.
jq -e --arg id "$ITEM_ID" --arg dom "$DOMAIN" --arg did "$DID" '(.record_hash | type == "array" and length == 32 and all(.[]; type == "number" and . >= 0 and . <= 255)) and .item_id == $id and .domain_id == $dom and .actor_did == $did and .transition == "completed" and (.completed_at | type == "number" and . > 0)' <<<"$receipt" >/dev/null || die "receipt failed verification (need 32-byte record_hash bound to this item/domain/actor/transition): $receipt"
note "Verified: 32-byte BLAKE3 record_hash binding item_id, domain_id, actor_did, transition=completed, completed_at."
note "It proves THIS actor discharged THIS obligation at THIS time (re-derive via proof.rs canonical hasher)."

say "9. The obligation is discharged -- the card clears, the proof remains"
open_n="$(curl -s -H "$AUTH" "$GW/v1/gov/me/action-cards" | jq --arg id "$ITEM_ID" '[.cards[] | select(.source_id==$id)] | length')"
[ "$open_n" = "0" ] || die "obligation card did not clear (still $open_n open)"
note "card cleared -- obligation discharged, proof retained."

printf '\n* ICN turned cooperative work into a legible obligation and a verifiable receipt --\n'
printf '  on a node the group controls. The receipt proves THIS actor discharged THIS\n'
printf '  obligation at THIS time. (Rehearsal: local gateway, test identity, in-tree NYCN fixture.)\n'
