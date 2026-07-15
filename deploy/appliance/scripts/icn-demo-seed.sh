#!/usr/bin/env bash
# ============================================================================
# icn-demo-seed — seed the appliance DEMO PROFILE with a runnable member loop.
# ----------------------------------------------------------------------------
# DEMO PROFILE ONLY. Installed by build-image.sh when
# ICN_APPLIANCE_DEMO_PROFILE=1. Run as root inside the appliance VM:
#
#     sudo icn-demo-seed [--json]                       # legacy member action-item loop
#     sudo icn-demo-seed --session organizer [--json]   # Rehearsal Node organizer session (#2386)
#     sudo icn-demo-seed --session member   [--json]    # Rehearsal Node member session (#2386)
#
# The --session path drives the Rehearsal Node organizer review->confirm loop
# (requires the daemon in ICN_GOVERNANCE_BUILD_MODE=rehearsal). It initializes
# the rehearsal workspace + binds a fictional assignee label to the operator DID
# ONCE, then mints a least-privilege per-role browser session (organizer =
# read+review+confirm; member = read+complete). It does NOT pre-seed an action
# item — the organizer creates it by confirming, and the member completes it.
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
# already applied). The default (legacy) path creates an additional open action
# item each run. The --session (Rehearsal Node) path is idempotent: it
# initializes the rehearsal workspace + binds the fictional label ONCE (only if
# absent, so an organizer-created item is never wiped) and mints a fresh
# least-privilege per-role session. For a clean slate use icn-demo-reset.
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
# Rehearsal-loop scope sets (#2386) — narrow, non-write, allowlisted, --local-mint-issuable:
#   REHEARSAL_SETUP: initializes the workspace + binds fictional labels. INTERNAL — never printed.
#     governance:read is the minimum genuinely required for the idempotent GET
#     probe (the read routes need it); governance:rehearsal:setup authorizes the
#     reset (init) + bindings. It carries NO write/admin/complete class.
#   ORGANIZER (browser): read + review + confirm. Cannot bind, initialize, complete, or write.
#   MEMBER (browser): read + completion-only (#2400). Cannot review, confirm, bind, or initialize.
REHEARSAL_SETUP_SCOPES="governance:read,governance:rehearsal:setup"
ORGANIZER_SCOPES="governance:read,governance:pending-publish:review,governance:pending-publish:confirm"
MEMBER_SCOPES="$BROWSER_SCOPES"
REHEARSAL_LABEL="Example member (fictional)"

JSON_OUT=0
SESSION_ROLE=""   # "" = legacy pre-seeded member loop; "organizer"|"member" = Rehearsal Node role session
FRESH=0           # --fresh (organizer only): reset the workspace to a NEW rehearsal generation before minting

log(){ [ "$JSON_OUT" = 1 ] || echo "[demo-seed] $*"; }
fatal(){ echo "[demo-seed] FATAL: $*" >&2; exit 1; }

# Argument parse. A --session role is validated against a CLOSED allowlist and
# only ever selects a scope constant above — it is never interpolated into any
# command (preserving the fixed-command safety property).
while [ $# -gt 0 ]; do
  case "$1" in
    --json) JSON_OUT=1; shift ;;
    --fresh) FRESH=1; shift ;;
    --session) shift; SESSION_ROLE="${1:-__missing__}"; [ $# -gt 0 ] && shift ;;
    --session=*) SESSION_ROLE="${1#--session=}"; shift ;;
    *) fatal "unknown argument: $1 (usage: icn-demo-seed [--json] [--session organizer|member] [--fresh])" ;;
  esac
done
case "$SESSION_ROLE" in
  ""|organizer|member) : ;;
  *) fatal "unknown --session role '$SESSION_ROLE' (allowed: organizer, member)" ;;
esac
# Reset (a new rehearsal generation) is an organizer act — refuse it for the
# member role and the legacy loop, so --fresh can never silently retire an
# organizer's in-progress items from a non-organizer path.
if [ "$FRESH" = 1 ] && [ "$SESSION_ROLE" != "organizer" ]; then
  fatal "--fresh requires --session organizer (reset is an organizer act)"
fi

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

# ---- Rehearsal Node helpers (#2386) ----
# HTTP status of a rehearsal-route call (curl runs as root against loopback,
# exactly like the standing-bootstrap / action-item calls below). A JSON body,
# when present, is built by python json.dumps so labels/DIDs are safely encoded.
rehearsal_status(){  # $1=METHOD $2=PATH $3=JWT [$4=BODY]
  local method="$1" path="$2" jwt="$3" body="${4:-}"
  if [ -n "$body" ]; then
    curl -s -o /dev/null -m 10 -w '%{http_code}' -X "$method" \
      -H "Authorization: Bearer $jwt" -H 'Content-Type: application/json' \
      -d "$body" "$GW$path"
  else
    curl -s -o /dev/null -m 10 -w '%{http_code}' -X "$method" \
      -H "Authorization: Bearer $jwt" "$GW$path"
  fi
}

# Idempotent one-time rehearsal setup: initialize the workspace and bind the
# fictional assignee label to the operator DID (a StaticList member of $DOMAIN
# via the applied package). Runs init+bind ONLY when the workspace does not
# already exist, so a re-mint never wipes an organizer-created item (reset =
# "new generation").
ensure_rehearsal_workspace(){  # $1=SETUP_JWT
  local setup="$1" status body
  status="$(rehearsal_status GET "/v1/gov/domains/$DOMAIN/rehearsal/pending-publish" "$setup")"
  if [ "$status" = "200" ] && [ "$FRESH" != 1 ]; then
    log "rehearsal workspace already initialized (generation preserved)."
    return 0
  fi
  if [ "$status" != "200" ] && [ "$status" != "404" ]; then
    fatal "unexpected HTTP $status probing the rehearsal workspace — is icnd in ICN_GOVERNANCE_BUILD_MODE=rehearsal? (rehearsal routes 404 otherwise)"
  fi
  if [ "$FRESH" = 1 ] && [ "$status" = "200" ]; then
    # --fresh on an existing workspace: reset starts a NEW generation. The
    # daemon's reset semantics are retire-not-erase — prior un-completed
    # fictional items are cancelled, recorded receipts remain permanent
    # process facts. RE-resetting an already-designated workspace is an
    # ORGANIZER act (governance:pending-publish:review) — the setup scope
    # only authorizes first designation (apps/governance rehearsal.rs). Mint
    # an INTERNAL organizer-scoped credential just for this call; like the
    # setup JWT it is never printed.
    log "starting a fresh rehearsal for $DOMAIN (--fresh: new generation, prior items retired) ..."
    local reset_jwt
    reset_jwt="$(mint_local_jwt "$ORGANIZER_SCOPES")"
    [ -n "$reset_jwt" ] || fatal "trusted local reset-JWT mint failed (icnctl --local-mint)."
    status="$(rehearsal_status POST "/v1/gov/domains/$DOMAIN/rehearsal/reset" "$reset_jwt" '{}')"
    [ "$status" = "200" ] || fatal "fresh rehearsal reset failed: HTTP $status (re-reset is an organizer act: governance:pending-publish:review)"
  else
    log "initializing the rehearsal workspace for $DOMAIN ..."
    status="$(rehearsal_status POST "/v1/gov/domains/$DOMAIN/rehearsal/reset" "$setup" '{}')"
    [ "$status" = "200" ] || fatal "rehearsal workspace init (reset) failed: HTTP $status (setup scope governance:rehearsal:setup)"
  fi
  body="$(python3 -c 'import json,sys; print(json.dumps({"label": sys.argv[1], "did": sys.argv[2]}))' "$REHEARSAL_LABEL" "$DID")"
  status="$(rehearsal_status POST "/v1/gov/domains/$DOMAIN/rehearsal/bindings" "$setup" "$body")"
  [ "$status" = "200" ] || fatal "rehearsal label binding failed: HTTP $status (label '$REHEARSAL_LABEL' -> operator DID; the operator must be a domain member — it is, via the package StaticList)"
  log "rehearsal workspace ready (label '$REHEARSAL_LABEL' bound to the operator DID)."
}

# ---- internal SETUP credential (scope depends on the path) ----
# Legacy path: governance:meeting:write (creates the pre-seeded action item).
# Rehearsal path (#2386): governance:rehearsal:setup (initializes the workspace
# + binds the fictional label). Either way the SETUP JWT is INTERNAL — never
# printed or returned.
if [ -n "$SESSION_ROLE" ]; then
  SETUP_JWT="$(mint_local_jwt "$REHEARSAL_SETUP_SCOPES")"
  [ -n "$SETUP_JWT" ] || fatal "trusted local rehearsal-setup JWT mint failed (icnctl --local-mint). Check ICN_GATEWAY_JWT_SECRET is present in $ENV_FILE and the keystore unlocks. This path signs with THIS VM's own gateway secret and never uses the public self-asserted /auth/verify flow (issue #2075)."
else
  SETUP_JWT="$(mint_local_jwt "$SETUP_SCOPES")"
  [ -n "$SETUP_JWT" ] || fatal "trusted local SETUP-JWT mint failed (icnctl --local-mint). Check ICN_GATEWAY_JWT_SECRET is present in $ENV_FILE and the keystore unlocks. This path signs with THIS VM's own gateway secret and never uses the public self-asserted /auth/verify flow (issue #2075)."
fi
AUTH_SETUP="Authorization: Bearer $SETUP_JWT"

log "applying NYCN institution package (fictional fixture institution)..."
as_icn /usr/local/bin/icnctl --data-dir "$DATA_DIR" institution bootstrap apply \
  -g "$GW" -c "$COOP_ID" --package "$PKG" --local-mint >/tmp/icn-demo-seed-bootstrap.log 2>&1 \
  || fatal "institution bootstrap apply failed (see /tmp/icn-demo-seed-bootstrap.log)"
log "institution package applied."

# Dev-gated standing bootstrap (best-effort; the loops do not strictly require
# it, but the shell's standing pane is richer with it). The endpoint bootstraps
# the AUTHENTICATED caller's DID (claims.sub) in the given jurisdiction — the
# body carries jurisdiction_id only, never a DID.
standing_note="bootstrap-standing: ok"
if ! curl -sf -m 5 -X POST -H "$AUTH_SETUP" -H 'Content-Type: application/json' \
      -d "{\"jurisdiction_id\":\"$DOMAIN\"}" "$GW/v1/commons/dev/bootstrap-standing" >/dev/null 2>&1; then
  standing_note="bootstrap-standing: unavailable (dev gate off or endpoint shape changed) — standing pane may be sparse; loop unaffected"
  log "WARN: $standing_note"
fi

# ============================================================================
# Rehearsal Node organizer->member loop (#2386): idempotent one-time workspace
# init + a least-privilege per-role session. No pre-seeded action item — the
# organizer creates it by confirming; the member completes it.
# ============================================================================
if [ -n "$SESSION_ROLE" ]; then
  ensure_rehearsal_workspace "$SETUP_JWT"
  case "$SESSION_ROLE" in
    organizer) ROLE_SCOPES="$ORGANIZER_SCOPES" ;;
    member)    ROLE_SCOPES="$MEMBER_SCOPES" ;;
  esac
  ROLE_JWT="$(mint_local_jwt "$ROLE_SCOPES")"
  [ -n "$ROLE_JWT" ] || fatal "trusted local $SESSION_ROLE-JWT mint failed (icnctl --local-mint)."
  log "minted a least-privilege $SESSION_ROLE session (the setup JWT stays internal, never printed)."
  # The organizer opens ?surface=organizer; the member opens the plain member surface.
  if [ "$SESSION_ROLE" = "organizer" ]; then SHELL_SUFFIX="?mode=live&surface=organizer"; else SHELL_SUFFIX="?mode=live"; fi

  if [ "$JSON_OUT" = 1 ]; then
    python3 -c 'import json,sys
print(json.dumps({"did": sys.argv[1], "jwt": sys.argv[2], "role": sys.argv[3],
                  "surface": sys.argv[3], "domain": sys.argv[4], "gateway": sys.argv[5],
                  "standing_note": sys.argv[6]}))' \
      "$DID" "$ROLE_JWT" "$SESSION_ROLE" "$DOMAIN" "$GW" "$standing_note"
    exit 0
  fi

  cat <<EOF

===================== ICN REHEARSAL NODE — $SESSION_ROLE session =====================
 Gateway (in-VM):    $GW
 Member shell:       http://localhost:8090/member-shell/$SHELL_SUFFIX
 Operator DID:       $DID
 Domain:             $DOMAIN   (fictional StaticList rehearsal domain)
 Bound label:        $REHEARSAL_LABEL  ->  the operator DID
 $standing_note

 $SESSION_ROLE session JWT (LOCAL VM ONLY — trusted local issuance, this VM's own
 per-instance gateway secret; least-privilege $ROLE_SCOPES):
 $ROLE_JWT

 Honesty labels:
   rehearsal : fictional data on an isolated node. Not a pilot, not live
               federation. Confirming creates REAL receipts + one action item
               on THIS VM only; receipts record process facts, grant no authority.
   NOT real  : no production, no pilot, no federation, no real members.

 Loop: organizer reviews -> confirms one fictional item -> a fresh least-privilege
 member session completes it -> steward verifies.
 Verify: sudo icn-demo-verify --rehearsal $DOMAIN
 Reset:  sudo icn-demo-reset   (then re-run icn-demo-seed --session organizer)
====================================================================================
EOF
  exit 0
fi

# ============================================================================
# Legacy pre-seeded member action-item loop (unchanged behavior).
# ============================================================================
# BROWSER JWT — the narrow, least-privilege credential handed to the member
# shell (governance:read + the single action-item completion class). The ONLY
# JWT this legacy path prints.
BROWSER_JWT="$(mint_local_jwt "$BROWSER_SCOPES")"
[ -n "$BROWSER_JWT" ] || fatal "trusted local BROWSER-JWT mint failed (icnctl --local-mint). Check ICN_GATEWAY_JWT_SECRET is present in $ENV_FILE and the keystore unlocks."
AUTH_BROWSER="Authorization: Bearer $BROWSER_JWT"
log "minted the narrow browser JWT (governance:read + action-item completion only)."

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
