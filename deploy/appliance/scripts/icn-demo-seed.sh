#!/usr/bin/env bash
# ============================================================================
# icn-demo-seed — seed the appliance DEMO PROFILE with a runnable member loop.
# ----------------------------------------------------------------------------
# DEMO PROFILE ONLY. Installed by build-image.sh when
# ICN_APPLIANCE_DEMO_PROFILE=1. Run as root inside the appliance VM:
#
#     sudo icn-demo-seed [--json]
#
# What it does (all against THIS VM's own gateway on 127.0.0.1:8080):
#   1. Waits for the gateway to be healthy.
#   2. Resolves the node operator DID and mints a dev session JWT
#      (signed with this VM's per-instance JWT secret).
#   3. Applies the in-tree NYCN institution package (fictional fixture
#      institution — same package the nycn-dogfood rehearsal kit uses).
#   4. Dev-gated standing bootstrap for the operator DID (so the shell's
#      standing pane has something honest to show). Best-effort: if the
#      dev gate is unavailable the seed continues and says so.
#   5. Creates ONE open action item assigned to the operator DID — this is
#      the action card the operator discharges in the member-shell.
#   6. Prints the demo JWT + URLs + honesty labels (or JSON with --json).
#
# What it does NOT do:
#   - No production claims. Fictional institution, test posture, local VM.
#   - No external network access. Everything is loopback inside the VM.
#   - The printed JWT is a LOCAL DEV credential for this disposable VM
#     only. It is not a secret worth protecting beyond the VM's lifetime,
#     but it is also never printed to the journal by the units — only by
#     this command, in your terminal, on request.
#
# Idempotency: re-running re-applies the institution package (a no-op when
# already applied) and creates an additional open action item. For a clean
# slate use icn-demo-reset.
# ============================================================================
set -uo pipefail

GW="${ICN_DEMO_GW:-http://127.0.0.1:8080}"
DATA_DIR=/var/lib/icn
ENV_FILE=/etc/icn/icnd.env
PKG=/usr/share/icn/demo/institutions/nycn
COOP_ID="${ICN_DEMO_COOP_ID:-nycn}"
DOMAIN="${ICN_DEMO_DOMAIN:-nycn-federation-gov}"
SCOPES="governance:read,governance:write,coop:read,coop:write,coop:admin"
JSON_OUT=0
[ "${1:-}" = "--json" ] && JSON_OUT=1

log(){ [ "$JSON_OUT" = 1 ] || echo "[demo-seed] $*"; }
fatal(){ echo "[demo-seed] FATAL: $*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || fatal "run as root: sudo icn-demo-seed"
[ -f "$ENV_FILE" ]   || fatal "$ENV_FILE missing — has icn-appliance-firstboot run?"
[ -d "$PKG" ]        || fatal "demo institution package missing at $PKG (image built without ICN_APPLIANCE_DEMO_PROFILE=1?)"
command -v curl >/dev/null || fatal "curl missing"
command -v jq   >/dev/null || fatal "jq missing (demo profile installs it at image build)"

# Per-instance keystore passphrase, generated on this VM by firstboot.
# shellcheck disable=SC1090
. "$ENV_FILE"
[ -n "${ICN_KEYSTORE_PASSPHRASE:-}" ] || fatal "ICN_KEYSTORE_PASSPHRASE not present in $ENV_FILE"

as_icn(){
  sudo -u icn ICN_KEYSTORE_PASSPHRASE="$ICN_KEYSTORE_PASSPHRASE" \
      ICN_PASSPHRASE="$ICN_KEYSTORE_PASSPHRASE" "$@"
}

log "waiting for gateway health at $GW ..."
for i in $(seq 1 30); do
  curl -sf -m 3 "$GW/v1/health" >/dev/null 2>&1 && break
  [ "$i" = 30 ] && fatal "gateway never became healthy at $GW (journalctl -u icnd)"
  sleep 2
done
log "gateway healthy."

id_out="$(as_icn /usr/local/bin/icnctl --data-dir "$DATA_DIR" id show 2>/dev/null || true)"
DID="$(grep -oE 'did:icn:[A-Za-z0-9]+' <<<"$id_out" | head -1)"
[ -n "$DID" ] || fatal "could not resolve node operator DID (icnctl --data-dir $DATA_DIR id show)"
log "operator DID: $DID"

jwt_out="$(as_icn /usr/local/bin/icnctl --data-dir "$DATA_DIR" auth token --gateway "$GW" --coop-id "$COOP_ID" -s "$SCOPES" 2>/dev/null || true)" # vocab-ok: icnctl CLI subcommand name
SESSION_JWT="$(grep -oE 'eyJ[A-Za-z0-9_.-]+' <<<"$jwt_out" | head -1)"
[ -n "$SESSION_JWT" ] || fatal "session JWT mint failed (icnctl auth subcommand)"
AUTH="Authorization: Bearer $SESSION_JWT"
log "dev session JWT minted (printed at the end — local VM only)."

log "applying NYCN institution package (fictional fixture institution)..."
as_icn /usr/local/bin/icnctl --data-dir "$DATA_DIR" institution bootstrap apply \
  -g "$GW" -c "$COOP_ID" --package "$PKG" >/tmp/icn-demo-seed-bootstrap.log 2>&1 \
  || fatal "institution bootstrap apply failed (see /tmp/icn-demo-seed-bootstrap.log)"
log "institution package applied."

# Dev-gated standing bootstrap (best-effort; the action-item loop does not
# strictly require it, but the shell's standing pane is richer with it).
standing_note="bootstrap-standing: ok"
if ! curl -sf -m 5 -X POST -H "$AUTH" -H 'Content-Type: application/json' \
      -d "{\"did\":\"$DID\"}" "$GW/v1/commons/dev/bootstrap-standing" >/dev/null 2>&1; then
  standing_note="bootstrap-standing: unavailable (dev gate off or endpoint shape changed) — standing pane may be sparse; action-item loop unaffected"
  log "WARN: $standing_note"
fi

log "creating one open action item assigned to $DID ..."
item_json="$(curl -s -m 10 -X POST -H "$AUTH" -H 'Content-Type: application/json' \
  -d "{\"title\":\"Confirm Summit 2026 venue booking\",\"description\":\"Demo obligation (fictional): confirm the venue contract for the 2026 Summit\",\"assignee\":\"$DID\",\"priority\":\"high\",\"meeting_context\":\"demo organizing team\"}" \
  "$GW/v1/gov/domains/$DOMAIN/action-items")"
ITEM_ID="$(jq -r '.id // empty' <<<"$item_json" 2>/dev/null)"
[ -n "$ITEM_ID" ] || fatal "action item creation failed: $item_json"
log "action item created: $ITEM_ID"

cards_json="$(curl -s -m 10 -H "$AUTH" "$GW/v1/gov/me/action-cards")"
card_n="$(jq --arg id "$ITEM_ID" '[.cards[]? | select(.source_id==$id)] | length' <<<"$cards_json" 2>/dev/null || echo 0)"
[ "$card_n" = "1" ] || fatal "expected 1 action card for $ITEM_ID, found ${card_n:-0}: $cards_json"
log "action card visible via /v1/gov/me/action-cards."

if [ "$JSON_OUT" = 1 ]; then
  jq -n --arg did "$DID" --arg jwt "$SESSION_JWT" --arg item "$ITEM_ID" \
        --arg domain "$DOMAIN" --arg gw "$GW" --arg standing "$standing_note" \
        '{did:$did, jwt:$jwt, item_id:$item, domain:$domain, gateway:$gw, standing_note:$standing}'
  exit 0
fi

cat <<EOF

============================ ICN DEMO SEEDED =============================
 Gateway (in-VM):    $GW   (host: whatever you hostfwd'd 8080 to)
 Member shell:       http://localhost:8090/member-shell/        (live-local)
                     http://localhost:8090/member-shell/?mode=demo (fixtures)
                     (replace 8090 with your hostfwd port if different)
 Operator DID:       $DID
 Domain:             $DOMAIN
 Open action item:   $ITEM_ID
 $standing_note

 Dev session JWT (LOCAL VM ONLY — paste into the shell's live mode):
 $SESSION_JWT

 Honesty labels:
   live-local : node, gateway, standing/action-card/receipt endpoints,
                action-item completion, completion receipt — all on THIS VM.
   fixture    : ?mode=demo surfaces (self-labeled in the shell).
   NOT real   : no production, no pilot, no federation, no real members —
                fictional institution data on a disposable dev VM.

 Demo loop: open the shell → standing → action card → complete → receipt.
 Evidence:  sudo icn-demo-verify   (receipt re-fetch + 13/13 chain proof)
 Reset:     sudo icn-demo-reset
==========================================================================
EOF
