#!/usr/bin/env bash
# demo-seed-auth-check.sh
#
# Deterministic, VM-free static check that icn-demo-seed.sh authenticates via
# TRUSTED LOCAL issuance and never via the public self-asserted path:
#
#   1. The session JWT is minted via icnctl's `--local-mint` path (in-process
#      signing with this node's own gateway secret), NOT a bare mint that would
#      hit the self-asserted /auth/verify flow — which #2075 fail-closes on the
#      demo's routable 0.0.0.0 bind.
#   2. `institution bootstrap apply` likewise uses `--local-mint`.
#   3. The seed fails closed if the signing secret (ICN_GATEWAY_JWT_SECRET) is
#      absent.
#   4. The signing secret is passed ONLY through the environment (never a CLI
#      flag / never a literal), so it cannot leak to a process list or journal.
#   5. The seed bakes NO credential: no hardcoded JWT/bearer literal.
#   6. The seed never calls the self-asserted /v1/auth/verify endpoint directly.
#
# This asserts SCRIPT STRUCTURE only. The runtime proof — the full seed loop
# completing against a demo-profile image without weakening auth — runs via
# `deploy/appliance/scripts/icn-rehearsal-node.sh smoke-image`. PASS here is not
# a live proof.
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

# 1. Session JWT minted via trusted local issuance (the mint line carries --local-mint).
if grep -qE 'auth[[:space:]].*--local-mint' "$SEED"; then
    ok "session JWT uses trusted local issuance (--local-mint)"
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
if grep -qE 'ICN_GATEWAY_JWT_SECRET="\$ICN_GATEWAY_JWT_SECRET"' "$SEED" \
   && ! grep -qE -- '--jwt-secret|--signing-secret|--secret[ =]' "$SEED"; then
    ok "signing secret confined to the environment (no secret on any command line)"
else
    bad "signing secret is not confined to the environment"
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

if [ "$fail" -eq 0 ]; then
    say "PASS (6/6): demo seed uses trusted local issuance; #2075 self-asserted path untouched"
    exit 0
else
    say "FAIL"
    exit 1
fi
