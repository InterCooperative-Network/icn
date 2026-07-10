#!/usr/bin/env bash
# net-restrict-check.sh
#
# Deterministic, VM-free check that smoke-local.sh constructs the QEMU
# user-mode netdev with the intended isolation posture (#1727 / #2386):
#
#   1. --demo (default)      -> restrict=on present + all three hostfwds
#   2. --demo + ALLOW=1      -> restrict=on ABSENT (explicit, loud override)
#   3. base (non-demo)       -> restrict=on ABSENT, SSH hostfwd only
#                               (ordinary smoke behavior unchanged)
#   4. --demo + custom ports -> hostfwds reflect the configured ports
#                               (the string is constructed, not hard-coded)
#
# This asserts CONFIGURATION CONSTRUCTION only. The runtime proof — a guest
# that cannot reach a verified host-loopback canary listener — runs inside
# `smoke-local.sh --real --demo` and needs a built demo-profile image.
# PASS here is NOT a live no-outbound proof.
#
# Usage: bash deploy/appliance/smoke/net-restrict-check.sh

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SMOKE="$SCRIPT_DIR/smoke-local.sh"

fail=0
say()  { printf '[net-restrict-check] %s\n' "$*"; }
bad()  { printf '[net-restrict-check] FAIL: %s\n' "$*" >&2; fail=1; }

[ -f "$SMOKE" ] || { bad "smoke-local.sh not found at $SMOKE"; exit 1; }

# Run --print-netdev with every netdev-affecting variable SCRUBBED first, so
# an operator's ambient demo environment (exported ICN_APPLIANCE_*_PORT /
# ALLOW_OUTBOUND from an earlier smoke run) cannot change what "default"
# means and fail a valid checkout. Per-case overrides are passed as leading
# KEY=VAL arguments; remaining arguments are smoke-local flags.
netdev() {
    local assigns=()
    while [ "$#" -gt 0 ] && [[ "$1" == *=* ]]; do
        assigns+=("$1")
        shift
    done
    env -u ICN_APPLIANCE_ALLOW_OUTBOUND \
        -u ICN_APPLIANCE_SSH_PORT \
        -u ICN_APPLIANCE_GW_FWD_PORT \
        -u ICN_APPLIANCE_SHELL_FWD_PORT \
        ${assigns[@]+"${assigns[@]}"} \
        bash "$SMOKE" "$@" --print-netdev
}

# Case 1: demo default -> restricted, all forwards present.
ND="$(netdev --demo)"
case "$ND" in
    *restrict=on*) : ;;
    *) bad "case 1: --demo default lacks restrict=on: $ND" ;;
esac
for want in "hostfwd=tcp:127.0.0.1:2222-:22" "hostfwd=tcp:127.0.0.1:18080-:8080" "hostfwd=tcp:127.0.0.1:18090-:8090"; do
    case "$ND" in
        *"$want"*) : ;;
        *) bad "case 1: --demo default missing forward '$want': $ND" ;;
    esac
done

# Case 2: explicit override -> unrestricted.
ND="$(netdev ICN_APPLIANCE_ALLOW_OUTBOUND=1 --demo)"
case "$ND" in
    *restrict=on*) bad "case 2: ALLOW_OUTBOUND=1 still restricted: $ND" ;;
    *) : ;;
esac

# Case 3: base smoke unchanged -> no restrict, SSH forward only.
ND="$(netdev)"
case "$ND" in
    *restrict=on*) bad "case 3: non-demo smoke gained restrict=on (behavior change): $ND" ;;
    *) : ;;
esac
case "$ND" in
    *"hostfwd=tcp:127.0.0.1:2222-:22"*) : ;;
    *) bad "case 3: SSH hostfwd missing: $ND" ;;
esac
case "$ND" in
    *18080*|*18090*) bad "case 3: non-demo smoke gained demo forwards: $ND" ;;
    *) : ;;
esac

# Case 4: constructed, not hard-coded — custom ports must appear.
ND="$(netdev ICN_APPLIANCE_SSH_PORT=2299 ICN_APPLIANCE_GW_FWD_PORT=28080 ICN_APPLIANCE_SHELL_FWD_PORT=28090 --demo)"
for want in "hostfwd=tcp:127.0.0.1:2299-:22" "hostfwd=tcp:127.0.0.1:28080-:8080" "hostfwd=tcp:127.0.0.1:28090-:8090" "restrict=on"; do
    case "$ND" in
        *"$want"*) : ;;
        *) bad "case 4: custom-port netdev missing '$want': $ND" ;;
    esac
done

if [ "$fail" -eq 0 ]; then
    say "PASS: 4/4 netdev-construction cases (static check; runtime canary lives in smoke-local.sh --real --demo)."
    exit 0
fi
exit 1
