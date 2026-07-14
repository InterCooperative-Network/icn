#!/usr/bin/env bash
# ============================================================================
# icn-demo-status — DEV/DEMO-only read-only rehearsal status summary (JSON).
# ----------------------------------------------------------------------------
# DEMO PROFILE ONLY. Installed by build-image.sh when ICN_APPLIANCE_DEMO_PROFILE=1
# (as /usr/local/sbin/icn-demo-status). Run as root (reads /etc/icn/icnd.env).
#
# What it does: answers "is the rehearsal working, and where is it?" without
# SSH archaeology. It probes gateway health, mints a trusted-local JWT with
# ONLY `governance:read` (the read-surface scope — no setup, no review, no
# confirm, no complete), GETs the rehearsal pending-publish workspace and the
# receipts list, and prints a SANITIZED one-object JSON summary:
#   * counts and workspace generation only — no row titles, no DIDs, no
#     receipt hashes, and NEVER any credential.
# The read JWT is internal to this script; it is never printed or returned.
#
# Exit code is 0 whenever a summary could be produced (including "gateway
# unavailable" — that IS the status); non-zero only for operator errors
# (not root, env file missing).
#
# Consumed by the loopback demo-session endpoint (GET /v1/dev/demo/status)
# for the landing page's health/state strip, and usable directly:
#   sudo icn-demo-status
# ============================================================================
set -euo pipefail

ENV_FILE="/etc/icn/icnd.env"
DATA_DIR="/var/lib/icn"
GW="${ICN_DEMO_GATEWAY:-http://127.0.0.1:8080}"
COOP_ID="${ICN_DEMO_COOP_ID:-nycn}"
DOMAIN="${ICN_DEMO_DOMAIN:-nycn-federation-gov}"
# Read-only scope. The rehearsal read surfaces (pending-publish, receipts) are
# gated on `governance:read` + domain membership (apps/governance rehearsal.rs
# read_claims). This credential can not review, confirm, bind, reset, write,
# or complete anything.
STATUS_SCOPES="governance:read"

fatal(){ echo "[demo-status] FATAL: $*" >&2; exit 1; }

# --json accepted for symmetry with icn-demo-seed; output is always JSON.
while [ $# -gt 0 ]; do
  case "$1" in
    --json) shift ;;
    *) fatal "unknown argument: $1 (usage: icn-demo-status [--json])" ;;
  esac
done

[ "$(id -u)" -eq 0 ] || fatal "run as root: sudo icn-demo-status"
[ -f "$ENV_FILE" ]   || fatal "$ENV_FILE missing — has icn-appliance-firstboot run?"
command -v curl    >/dev/null || fatal "curl missing"
command -v python3 >/dev/null || fatal "python3 missing"

# shellcheck disable=SC1090
. "$ENV_FILE"
[ -n "${ICN_KEYSTORE_PASSPHRASE:-}" ] || fatal "ICN_KEYSTORE_PASSPHRASE not present in $ENV_FILE"
[ -n "${ICN_GATEWAY_JWT_SECRET:-}" ] || fatal "ICN_GATEWAY_JWT_SECRET not present in $ENV_FILE"
ICN_PASSPHRASE="$ICN_KEYSTORE_PASSPHRASE"

# Same environment-allowlist privilege drop as icn-demo-seed: secrets travel
# via environment (never argv), everything else is stripped.
ICN_CHILD_ENV_KEEP="ICN_KEYSTORE_PASSPHRASE ICN_PASSPHRASE ICN_GATEWAY_JWT_SECRET LANG LC_ALL LC_CTYPE TERM"
as_icn(){
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

CHECKED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

emit(){  # $1=health $2=routes_json $3=workspace_json_file_or_empty $4=receipts_json_file_or_empty
  # Sanitizing serializer: only counts / generation / status buckets leave this
  # process. Row titles, DIDs, hashes and anything else in the raw responses
  # are dropped here, by construction.
  python3 - "$1" "$2" "${3:-}" "${4:-}" "$CHECKED_AT" <<'PYEOF'
import json, sys
health, routes, ws_file, rc_file, checked_at = sys.argv[1:6]
out = {
    "gateway_health": health,
    "rehearsal_routes_mounted": routes == "true" if routes in ("true", "false") else None,
    "workspace": None,
    "receipts": None,
    "checked_at": checked_at,
}
def load(path):
    if not path:
        return None
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return None
ws = load(ws_file)
if isinstance(ws, dict):
    rows = ws.get("rows") or ws.get("pending_publish") or []
    by_status = {}
    if isinstance(rows, list):
        for r in rows:
            s = str((r or {}).get("status", "unknown"))
            by_status[s] = by_status.get(s, 0) + 1
    out["workspace"] = {
        "initialized": True,
        "generation": ws.get("generation"),
        "rows_total": len(rows) if isinstance(rows, list) else None,
        "rows_by_status": by_status,
    }
elif ws_file:
    out["workspace"] = {"initialized": False}
rc = load(rc_file)
if isinstance(rc, dict):
    receipts = rc.get("receipts") or rc.get("entries") or []
    out["receipts"] = {"total": len(receipts) if isinstance(receipts, list) else None}
print(json.dumps(out))
PYEOF
}

# 1) Gateway health (public route, no credential).
if ! curl -sf -m 4 "$GW/v1/health" >/dev/null 2>&1; then
  emit "unavailable" "null"
  exit 0
fi

# 2) Trusted-local read-only JWT (internal; never printed).
tok_out="$(as_icn /usr/local/bin/icnctl --data-dir "$DATA_DIR" auth token --gateway "$GW" --coop-id "$COOP_ID" -s "$STATUS_SCOPES" --local-mint 2>/dev/null || true)" # vocab-ok: icnctl CLI subcommand name
JWT="$(grep -oE 'eyJ[A-Za-z0-9_.-]+' <<<"$tok_out" | head -1)"
if [ -z "$JWT" ]; then
  emit "ok" "null"
  exit 0
fi

TMPD="$(mktemp -d)"
trap 'rm -rf "$TMPD"' EXIT

ws_code="$(curl -s -m 8 -o "$TMPD/ws.json" -w '%{http_code}' \
  -H "Authorization: Bearer $JWT" \
  "$GW/v1/gov/domains/$DOMAIN/rehearsal/pending-publish" || echo 000)"
rc_code="$(curl -s -m 8 -o "$TMPD/rc.json" -w '%{http_code}' \
  -H "Authorization: Bearer $JWT" \
  "$GW/v1/gov/domains/$DOMAIN/rehearsal/receipts" || echo 000)"

case "$ws_code" in
  200) emit "ok" "true" "$TMPD/ws.json" "$([ "$rc_code" = 200 ] && echo "$TMPD/rc.json" || echo "")" ;;
  404) # Either the workspace is not initialized yet, or icnd is not in
       # rehearsal build mode (routes unmounted). Both are honest "no
       # workspace visible" states; routes_mounted stays unknown.
       emit "ok" "null" "/nonexistent" ;;
  *)   emit "ok" "null" ;;
esac
exit 0
