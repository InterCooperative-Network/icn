#!/usr/bin/env bash
# demo-seed-auth-check.sh
#
# Deterministic, VM-free static check that icn-demo-seed.sh authenticates via
# TRUSTED LOCAL issuance, hands the browser a LEAST-PRIVILEGE credential, and
# handles the signing secret safely:
#
#   1. Session JWTs are minted via icnctl's `--local-mint` path (in-process
#      signing with this node's own gateway secret), NOT a bare mint that would
#      hit the self-asserted /auth/verify flow — which #2075 fail-closes on the
#      demo's routable 0.0.0.0 bind.
#   2. `institution bootstrap apply` likewise uses `--local-mint`.
#   3. The seed fails closed if the signing secret (ICN_GATEWAY_JWT_SECRET) is
#      absent.
#   4. The signing secret travels ONLY through the environment (exported, never a
#      CLI flag / never a literal), so it cannot leak to a process list or journal.
#   5. The seed bakes NO credential: no hardcoded JWT/bearer literal.
#   6. The seed never calls the self-asserted /v1/auth/verify endpoint directly.
#   7. The BROWSER JWT is least-privilege: governance:read + governance:action-item:complete
#      (the completion-only capability, #2400) — no governance:meeting:write (which
#      would also authorize creating items/meetings), no coop:*, no broad governance:write.
#   8. The SETUP JWT is internal: only the browser JWT is ever emitted/printed.
#   9. The icn child process gets a MINIMAL environment (no --preserve-environment;
#      an explicit allowlist), so unrelated root env never reaches icnctl.
#
# This asserts SCRIPT STRUCTURE only. The runtime proof — the full seed loop
# completing (and the browser JWT being denied admin routes) against a
# demo-profile image — runs via `icn-rehearsal-node.sh smoke-image`. PASS here is
# not a live proof.
#
# Usage: bash deploy/appliance/smoke/demo-seed-auth-check.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SEED="$SCRIPT_DIR/../scripts/icn-demo-seed.sh"

fail=0
say() { printf '[demo-seed-auth-check] %s\n' "$*"; }
ok()  { printf '[demo-seed-auth-check] ok: %s\n' "$*"; }
bad() { printf '[demo-seed-auth-check] FAIL: %s\n' "$*" >&2; fail=1; }

[ -f "$SEED" ] || { bad "icn-demo-seed.sh not found at $SEED"; exit 1; }

# 1. Session JWTs minted via trusted local issuance (mint line carries --local-mint).
if grep -qE 'auth[[:space:]].*--local-mint' "$SEED"; then
    ok "session JWTs use trusted local issuance (--local-mint)"
else
    bad "session-JWT mint is not --local-mint (a bare mint would hit the #2075-blocked self-asserted /auth/verify on the demo 0.0.0.0 bind)"
fi

# 2. Institution bootstrap apply uses trusted local issuance (command spans lines).
if grep -A2 'institution bootstrap apply' "$SEED" | grep -q -- '--local-mint'; then
    ok "institution bootstrap apply uses --local-mint"
else
    bad "institution bootstrap apply does not use --local-mint (would hit the same self-asserted 403)"
fi

# 3. Fail-closed precondition: the seed requires the signing secret.
if grep -qE '\[ -n "\$\{ICN_GATEWAY_JWT_SECRET' "$SEED"; then
    ok "requires ICN_GATEWAY_JWT_SECRET (fail-closed precondition)"
else
    bad "no fail-closed check that ICN_GATEWAY_JWT_SECRET is present"
fi

# 4. Signing secret passed only through the environment, never a CLI flag.
if grep -qE 'export ICN_KEYSTORE_PASSPHRASE ICN_PASSPHRASE ICN_GATEWAY_JWT_SECRET' "$SEED" \
   && ! grep -qE -- '--jwt-secret|--signing-secret|--secret[ =]' "$SEED"; then
    ok "signing secret confined to the environment (exported; no secret on any command line)"
else
    bad "signing secret is not confined to the environment (or appears on a command line)"
fi

# 5. No baked credential: no hardcoded JWT/bearer literal.
if grep -qE 'eyJ[A-Za-z0-9_.-]{10,}' "$SEED"; then
    bad "hardcoded JWT/bearer literal found in the seed"
else
    ok "no hardcoded JWT/bearer literal"
fi

# 6. The seed does not call the self-asserted endpoint directly (match the real
#    /v1/ endpoint path — prose that names the flow is fine).
if grep -qE '/v1/auth/verify' "$SEED"; then
    bad "seed calls the self-asserted /v1/auth/verify endpoint directly"
else
    ok "seed does not call /v1/auth/verify directly"
fi

# 7. Browser JWT is least-privilege: governance:read + the completion-only
#    action-item capability (#2400), and NOTHING broader — in particular NOT
#    governance:meeting:write, which would also authorize creating items/meetings.
browser_scopes="$(awk -F'"' '/^BROWSER_SCOPES=/{print $2; exit}' "$SEED")"
if [ "$browser_scopes" = "governance:read,governance:action-item:complete" ]; then
    ok "browser JWT is completion-only least-privilege: governance:read,governance:action-item:complete"
else
    bad "BROWSER_SCOPES is not the expected completion-only set (got: '${browser_scopes:-<unset>}'); it must be governance:read,governance:action-item:complete — no governance:meeting:write, no coop:*, no broad governance:write"
fi

# 8. SETUP JWT is internal — only the BROWSER JWT is emitted/printed.
if grep -qE '"\$BROWSER_JWT"' "$SEED"; then
    ok "the emitted JSON carries the browser JWT (\$BROWSER_JWT)"
else
    bad "the emitted JSON does not carry \$BROWSER_JWT"
fi
# The output section (JSON emit + the printed heredoc) begins at the JSON_OUT
# branch; the SETUP JWT must not appear anywhere from there on.
emit_start="$(grep -nE 'if \[ "\$JSON_OUT" = 1 \]' "$SEED" | head -1 | cut -d: -f1)"
if [ -n "$emit_start" ] && awk -v n="$emit_start" 'NR>=n' "$SEED" | grep -q 'SETUP_JWT'; then
    bad "the internal SETUP JWT appears in the output section — it must never be printed"
else
    ok "the internal SETUP JWT is never emitted/printed"
fi

# 9. Minimal child environment: no active --preserve-environment; explicit allowlist.
if grep -E -- '--preserve-environment' "$SEED" | grep -qvE '#|no --preserve|NOT '; then
    bad "seed still uses runuser --preserve-environment (forwards the whole root env)"
else
    ok "no active --preserve-environment (minimal child environment)"
fi
if grep -q 'ICN_CHILD_ENV_KEEP' "$SEED"; then
    ok "child environment is stripped to an explicit allowlist (ICN_CHILD_ENV_KEEP)"
else
    bad "no explicit environment allowlist (ICN_CHILD_ENV_KEEP) — child env may leak unrelated root vars"
fi

if [ "$fail" -eq 0 ]; then
    say "PASS: demo seed uses trusted local issuance + least-privilege browser JWT; #2075 self-asserted path untouched"
    exit 0
else
    say "FAIL"
    exit 1
fi
