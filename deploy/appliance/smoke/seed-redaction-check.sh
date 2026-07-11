#!/usr/bin/env bash
# seed-redaction-check.sh
#
# VM-free regression test for issue #2396 hardening: a failed/incomplete demo
# seed must NEVER reproduce the captured seed JSON (which carries the browser
# session JWT) into any diagnostic output. Exercises the REAL diagnostic helper
# (smoke/lib-seed-diag.sh) with a sentinel JWT and asserts the sentinel never
# appears in stdout or stderr; also statically guards smoke-local.sh against
# reintroducing a raw `$SEED_JSON` echo on the incomplete-output path.
#
# Usage: bash deploy/appliance/smoke/seed-redaction-check.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/lib-seed-diag.sh"
SMOKE="$SCRIPT_DIR/smoke-local.sh"

fail=0
ok()  { printf '[seed-redaction-check] ok: %s\n' "$*"; }
bad() { printf '[seed-redaction-check] FAIL: %s\n' "$*" >&2; fail=1; }

[ -f "$LIB" ]   || { bad "lib-seed-diag.sh not found at $LIB"; exit 1; }
[ -f "$SMOKE" ] || { bad "smoke-local.sh not found at $SMOKE"; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "[seed-redaction-check] SKIP: jq not installed"; exit 0; }

# shellcheck source=deploy/appliance/smoke/lib-seed-diag.sh
. "$LIB"

# A recognizable sentinel standing in for a real JWT. If it ever surfaces in a
# diagnostic, the redaction has regressed.
SENTINEL="eyJSENTINELdoNOTleakTHIStokenABCDEF0123456789"

# --- Case 1: well-formed JSON but missing fields (jwt present, item_id empty) ---
payload_json="{\"did\":\"did:icn:demo\",\"jwt\":\"$SENTINEL\",\"item_id\":\"\",\"domain\":\"nycn-federation-gov\",\"gateway\":\"http://127.0.0.1:8080\",\"standing_note\":\"bootstrap-standing: ok\"}"
out1="$(seed_incomplete_diag "$payload_json" " item_id" 2>&1)"
if printf '%s' "$out1" | grep -q "$SENTINEL"; then
    bad "sentinel JWT leaked in the JSON-parsed diagnostic: $out1"
else
    ok "JSON-parsed incomplete diagnostic withholds the JWT"
fi
printf '%s' "$out1" | grep -q "item_id" || bad "diagnostic should name the missing field (item_id)"
printf '%s' "$out1" | grep -qi "withheld" || bad "diagnostic should state the JWT was withheld"

# --- Case 2: malformed (non-JSON) payload that still contains a JWT-like string ---
payload_bad="garbage-not-json $SENTINEL trailing"
out2="$(seed_incomplete_diag "$payload_bad" " jwt item_id domain" 2>&1)"
if printf '%s' "$out2" | grep -q "$SENTINEL"; then
    bad "sentinel JWT leaked in the non-JSON diagnostic: $out2"
else
    ok "non-JSON incomplete diagnostic withholds the JWT"
fi
printf '%s' "$out2" | grep -qi "not valid json" || bad "non-JSON diagnostic should say the payload was not valid JSON"

# --- Static guard: smoke-local.sh must not echo the raw seed payload/JWT ---
# Any diagnostic line (err/log/echo/printf) that references $SEED_JSON or
# $DEMO_JWT MUST route through seed_incomplete_diag (which redacts). A bare
# reference — e.g. the old `err "...: $SEED_JSON"` — is a leak. Comments are
# stripped before matching so the explanatory comment does not false-positive.
leaks="$(sed 's/#.*$//' "$SMOKE" \
    | grep -nE '(err|log|echo|printf)\b.*\$(SEED_JSON|DEMO_JWT)\b' \
    | grep -v 'seed_incomplete_diag' || true)"
if [ -n "$leaks" ]; then
    bad "smoke-local.sh has a diagnostic that echoes the raw seed payload/JWT:"
    printf '%s\n' "$leaks" >&2
else
    ok "smoke-local.sh routes seed-payload diagnostics through seed_incomplete_diag (no raw echo)"
fi

if [ "$fail" -ne 0 ]; then
    printf '[seed-redaction-check] RESULT: FAIL\n' >&2
    exit 1
fi
printf '[seed-redaction-check] RESULT: PASS\n'
