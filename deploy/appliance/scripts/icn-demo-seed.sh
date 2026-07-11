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
#   2. Resolves the node operator DID and mints session JWTs by TRUSTED LOCAL
#      issuance: icnctl signs them in-process (the `--local-mint` path) with this
#      VM's own per-instance gateway JWT secret (ICN_GATEWAY_JWT_SECRET from
#      /etc/icn/icnd.env — an INSTANCE-LOCAL secret, mode 0600 owned icn:icn; any
#      process running as the icn service account can read it and exercise gateway
#      signing authority). That is the gateway issuing a JWT for itself to its
#      local operator — it does NOT use the public self-asserted /auth/verify
#      path, which stays fail-closed on the demo's routable 0.0.0.0 bind (issue
#      #2075). No credential is baked into the image; the secret is generated
#      per-VM at first boot and is never printed.
#      TWO least-privilege credentials are minted (issue #2396 hardening):
#        - a SETUP JWT (never printed) that provisions the demo loop, and
#        - a narrow BROWSER JWT (governance:read + governance:action-item:complete)
#          — the ONLY JWT handed to the member shell. governance:action-item:complete
#          is the completion-only capability (#2400): as a member of the seeded
#          fictional domain it lets the operator complete their assigned action
#          item and NOTHING more — it canNOT create action items or meetings,
#          administer a cooperative, read entities, or reach cooperative-admin,
#          treasury, or the broad/other governance write classes.
#   3. Applies the in-tree NYCN institution package (fictional fixture
#      institution — same package the nycn-dogfood rehearsal kit uses).
#   4. Dev-gated standing bootstrap for the operator DID (so the shell's
#      standing pane has something honest to show). Best-effort: if the
#      dev gate is unavailable the seed continues and says so.
#   5. Creates ONE open action item assigned to the operator DID — this is
#      the action card the operator discharges in the member-shell.
#   6. Prints the narrow BROWSER JWT + URLs + honesty labels (or JSON with --json).
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
# Least-privilege scope sets (issue #2396 hardening). Proven against the live
# gateway route guards (icn-http-kit require_scope / require_any_scope_matched):
#   - SETUP: creates the demo action item (POST .../action-items accepts the
#     narrow governance:meeting:write class). Also drives the no-scope dev
#     standing-bootstrap. NEVER emitted to the browser.
#   - BROWSER: the member-shell's live routes — standing / action-cards /
#     pending-publish / completion-receipt (governance:read) and the action-item
#     completion PUT (governance:action-item:complete, the completion-only
#     capability from #2400 — accepted only for the `completed` transition). This
#     is the ONLY JWT printed.
# Neither needs any coop:* scope, and the browser JWT carries NO governance:write,
# NO governance:meeting:write, NO entity:write, and NO coop:admin — so it cannot
# create action items or meetings, administer a cooperative, read entities, or
# reach broad governance-mutation routes.
SETUP_SCOPES="governance:meeting:write"
BROWSER_SCOPES="governance:read,governance:action-item:complete"
JSON_OUT=0
[ "${1:-}" = "--json" ] && JSON_OUT=1

log(){ [ "$JSON_OUT" = 1 ] || echo "[demo-seed] $*"; }
fatal(){ echo "[demo-seed] FATAL: $*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || fatal "run as root: sudo icn-demo-seed"
[ -f "$ENV_FILE" ]   || fatal "$ENV_FILE missing — has icn-appliance-firstboot run?"
[ -d "$PKG" ]        || fatal "demo institution package missing at $PKG (image built without ICN_APPLIANCE_DEMO_PROFILE=1?)"
command -v curl    >/dev/null || fatal "curl missing"
command -v python3 >/dev/null || fatal "python3 missing"

# Per-instance keystore passphrase, generated on this VM by firstboot.
# shellcheck disable=SC1090
. "$ENV_FILE"
[ -n "${ICN_KEYSTORE_PASSPHRASE:-}" ] || fatal "ICN_KEYSTORE_PASSPHRASE not present in $ENV_FILE"
# Instance-local gateway signing secret — the trusted lever for local JWT
# issuance (icnctl --local-mint). Generated per-VM by firstboot; stored mode 0600
# owned icn:icn (readable by the icn service account, not root-exclusive).
[ -n "${ICN_GATEWAY_JWT_SECRET:-}" ] || fatal "ICN_GATEWAY_JWT_SECRET not present in $ENV_FILE — needed for trusted local JWT issuance (icnctl --local-mint)"
# icnctl accepts the keystore passphrase from ICN_PASSPHRASE too; mirror it here so
# as_icn() forwards a single, explicit passphrase pair.
ICN_PASSPHRASE="$ICN_KEYSTORE_PASSPHRASE"

# Allowlist of variables the icn child process may see. Everything else in root's
# environment is stripped before dropping privilege, so no unrelated root state
# (or a stray secret exported by some other tool) reaches icnctl.
ICN_CHILD_ENV_KEEP="ICN_KEYSTORE_PASSPHRASE ICN_PASSPHRASE ICN_GATEWAY_JWT_SECRET LANG LC_ALL LC_CTYPE TERM"

as_icn(){
  # Drop from root to the icn user with `runuser`, NOT `sudo`: sudo records both
  # its command line AND its environment in the auth journal, so any
  # `sudo … ICN_KEYSTORE_PASSPHRASE=… …` (assignment OR --preserve-env) leaks the
  # passphrase into the journal. This script already runs as root, so runuser only
  # drops privilege — it writes neither command nor env to any log.
  #
  # runuser forwards exported variables to the child, so we FIRST strip the
  # inherited environment down to the explicit allowlist in a subshell, then export
  # only the secrets icnctl needs. runuser (no --preserve-environment) resets
  # HOME/USER/PATH for the icn user itself. Secrets travel via the ENVIRONMENT,
  # never a command line — so they never appear in argv/ps or the journal.
  (
    for _name in $(compgen -e); do
      case " $ICN_CHILD_ENV_KEEP " in
        *" $_name "*) : ;;
        *) unset "$_name" 2>/dev/null || true ;;
      esac
    done
    export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    export ICN_KEYSTORE_PASSPHRASE ICN_PASSPHRASE ICN_GATEWAY_JWT_SECRET
    runuser -u icn -- "$@"
  )
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

# Mint a trusted-local JWT for the given scope set ($1) and echo the bare JWT.
# icnctl reads the gateway secret from the environment, never a CLI argument.
mint_local_jwt(){
  local out
  out="$(as_icn /usr/local/bin/icnctl --data-dir "$DATA_DIR" auth token --gateway "$GW" --coop-id "$COOP_ID" -s "$1" --local-mint 2>/dev/null || true)" # vocab-ok: icnctl CLI subcommand name
  grep -oE 'eyJ[A-Za-z0-9_.-]+' <<<"$out" | head -1
}

# SETUP JWT — provisions the demo loop (dev standing-bootstrap + action-item
# creation). Internal to this root-invoked seed; NEVER printed or returned.
SETUP_JWT="$(mint_local_jwt "$SETUP_SCOPES")"
[ -n "$SETUP_JWT" ] || fatal "trusted local SETUP-JWT mint failed (icnctl --local-mint). Check ICN_GATEWAY_JWT_SECRET is present in $ENV_FILE and the keystore unlocks. This path signs with THIS VM's own gateway secret and never uses the public self-asserted /auth/verify flow (issue #2075)."
AUTH_SETUP="Authorization: Bearer $SETUP_JWT"

# BROWSER JWT — the narrow, least-privilege credential handed to the member
# shell (governance:read + the single action-item completion class). The ONLY
# JWT this seed prints.
BROWSER_JWT="$(mint_local_jwt "$BROWSER_SCOPES")"
[ -n "$BROWSER_JWT" ] || fatal "trusted local BROWSER-JWT mint failed (icnctl --local-mint). Check ICN_GATEWAY_JWT_SECRET is present in $ENV_FILE and the keystore unlocks."
AUTH_BROWSER="Authorization: Bearer $BROWSER_JWT"
log "minted two trusted-local JWTs: a setup JWT (internal, never printed) and a narrow browser JWT (printed at the end — local VM only)."

log "applying NYCN institution package (fictional fixture institution)..."
as_icn /usr/local/bin/icnctl --data-dir "$DATA_DIR" institution bootstrap apply \
  -g "$GW" -c "$COOP_ID" --package "$PKG" --local-mint >/tmp/icn-demo-seed-bootstrap.log 2>&1 \
  || fatal "institution bootstrap apply failed (see /tmp/icn-demo-seed-bootstrap.log)"
log "institution package applied."

# Dev-gated standing bootstrap (best-effort; the action-item loop does not
# strictly require it, but the shell's standing pane is richer with it).
# The endpoint bootstraps the AUTHENTICATED caller's DID (claims.sub) in the
# given jurisdiction — the body carries jurisdiction_id only, never a DID
# (see icn-gateway api/commons dev_bootstrap_standing).
standing_note="bootstrap-standing: ok"
if ! curl -sf -m 5 -X POST -H "$AUTH_SETUP" -H 'Content-Type: application/json' \
      -d "{\"jurisdiction_id\":\"$DOMAIN\"}" "$GW/v1/commons/dev/bootstrap-standing" >/dev/null 2>&1; then
  standing_note="bootstrap-standing: unavailable (dev gate off or endpoint shape changed) — standing pane may be sparse; action-item loop unaffected"
  log "WARN: $standing_note"
fi

log "creating one open action item assigned to $DID ..."
item_json="$(curl -s -m 10 -X POST -H "$AUTH_SETUP" -H 'Content-Type: application/json' \
  -d "{\"title\":\"Confirm Summit 2026 venue booking\",\"description\":\"Demo obligation (fictional): confirm the venue contract for the 2026 Summit\",\"assignee\":\"$DID\",\"priority\":\"high\",\"meeting_context\":\"demo organizing team\"}" \
  "$GW/v1/gov/domains/$DOMAIN/action-items")"
ITEM_ID="$(python3 -c 'import json,sys
try:
    print(json.load(sys.stdin).get("id", ""))
except Exception:
    print("")' <<<"$item_json")"
[ -n "$ITEM_ID" ] || fatal "action item creation failed: $item_json"
log "action item created: $ITEM_ID"

# Verify with the BROWSER JWT (its governance:read scope) that the card is
# visible — this is exactly the read the member shell performs.
cards_json="$(curl -s -m 10 -H "$AUTH_BROWSER" "$GW/v1/gov/me/action-cards")"
card_n="$(python3 -c 'import json,sys
try:
    d = json.load(sys.stdin)
    print(sum(1 for c in d.get("cards", []) if c.get("source_id") == sys.argv[1]))
except Exception:
    print(0)' "$ITEM_ID" <<<"$cards_json")"
[ "$card_n" = "1" ] || fatal "expected 1 action card for $ITEM_ID, found ${card_n:-0}: $cards_json"
log "action card visible via /v1/gov/me/action-cards."

if [ "$JSON_OUT" = 1 ]; then
  python3 -c 'import json,sys
print(json.dumps({"did": sys.argv[1], "jwt": sys.argv[2], "item_id": sys.argv[3],
                  "domain": sys.argv[4], "gateway": sys.argv[5],
                  "standing_note": sys.argv[6]}))' \
    "$DID" "$BROWSER_JWT" "$ITEM_ID" "$DOMAIN" "$GW" "$standing_note"
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

 Browser session JWT (LOCAL VM ONLY — trusted local issuance signed with this
 VM's own per-instance gateway secret; least-privilege: governance:read +
 action-item completion only; paste into the shell's live mode):
 $BROWSER_JWT

 Honesty labels:
   live-local : node, gateway, standing/action-card/receipt endpoints,
                action-item completion, completion receipt — all on THIS VM.
   fixture    : ?mode=demo surfaces (self-labeled in the shell).
   NOT real   : no production, no pilot, no federation, no real members —
                fictional institution data on a disposable dev VM.

 Demo loop: open the shell → standing → action card → complete → receipt.
 Evidence:  sudo icn-demo-verify $ITEM_ID   (receipt re-fetch + consistency check,
            after you complete the action)
            sudo icn-demo-verify --chain    (full 13/13 governed chain proof)
 Reset:     sudo icn-demo-reset             (then re-run icn-demo-seed)
==========================================================================
EOF
