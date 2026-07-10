#!/usr/bin/env bash
# check.sh — lightweight pre-commit validation for the appliance scaffold.
#
# Runs:
#   1. bash -n on every shell script under deploy/appliance/.
#   2. --dry-run of build-image.sh and smoke-local.sh.
#   3. YAML parse on every *.yaml file under deploy/appliance/.
#   4. grep for forbidden vocabulary in appliance files
#      (payment / wallet / currency / balance / token / blockchain / crypto / timebank).
#   5. grep for obvious secret strings under deploy/appliance/.
#
# This is intentionally minimal. It is NOT a CI replacement.
#
# Exit codes:
#   0 — every check passed.
#   1 — one or more checks failed; details on stderr.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPLIANCE_DIR="$SCRIPT_DIR"

passed=0
failed=0
ok()   { passed=$((passed + 1)); printf '  OK  %s\n' "$*"; }
bad()  { failed=$((failed + 1)); printf '  FAIL %s\n' "$*" >&2; }
section() { printf '\n[check] %s\n' "$*"; }

# 1. bash -n on shell scripts
section "bash -n on shell scripts"
shopt -s nullglob
mapfile -t SHELL_FILES < <(find "$APPLIANCE_DIR" -type f -name '*.sh' | sort)
for f in "${SHELL_FILES[@]}"; do
    if bash -n "$f" 2>/dev/null; then
        ok "$f"
    else
        bad "$f (bash -n)"
    fi
done

# 2. dry-runs
section "dry-runs"
if bash "$APPLIANCE_DIR/build-image.sh" --dry-run >/dev/null 2>&1; then
    ok "build-image.sh --dry-run"
else
    bad "build-image.sh --dry-run"
fi
if bash "$APPLIANCE_DIR/smoke/smoke-local.sh" --dry-run >/dev/null 2>&1; then
    ok "smoke-local.sh --dry-run"
else
    bad "smoke-local.sh --dry-run"
fi

# 3. YAML parse on every yaml file
section "YAML parse"
if ! command -v python3 >/dev/null 2>&1; then
    bad "python3 missing; cannot YAML-parse"
else
    mapfile -t YAML_FILES < <(find "$APPLIANCE_DIR" -type f -name '*.yaml' | sort)
    for f in "${YAML_FILES[@]}"; do
        if python3 -c "import sys, yaml; yaml.safe_load(open(sys.argv[1]))" "$f" 2>/dev/null; then
            ok "$f"
        else
            # cloud-init user-data starts with the special #cloud-config
            # marker and contains the leading hash as a directive, but is
            # still valid YAML; we still want a parse pass.
            bad "$f (yaml.safe_load)"
        fi
    done
fi

# 4. forbidden vocabulary
# The scanner itself lists the forbidden vocabulary in this very file, so
# we exclude check.sh from the scan (its own enumeration is a known false
# positive).
#
# Word-boundary is expressed via -w (whole-word match) rather than the
# regex escape `\b`: in BRE/ERE, `\b` is interpreted as a literal backspace
# (0x08), not a word boundary, so older revisions of this scan were
# silently inert. -w is portable across GNU and BSD grep.
section "forbidden vocabulary scan"
FORBIDDEN_PATTERN='(payment|wallet|currency|balance|token|blockchain|crypto|timebank)'
# Lines tagged `vocab-ok: <justification>` are explicit, reviewable
# exceptions (same spirit as the sanitize gate's sanitize-ok marker). The
# only sanctioned use today is the literal `icnctl auth token` CLI
# subcommand name in the demo-profile scripts — a command name, not
# economic vocabulary. Tag sparingly; every tag is visible in review.
HITS="$(grep -rwniE "$FORBIDDEN_PATTERN" "$APPLIANCE_DIR" \
    --exclude="check.sh" 2>/dev/null | grep -v 'vocab-ok:' || true)"
if [ -z "$HITS" ]; then
    ok "no forbidden ICN-native vocabulary present"
else
    bad "forbidden vocabulary present:"
    printf '%s\n' "$HITS" >&2
fi

# 5. obvious secret strings
section "obvious-secret-string scan"
# Allow placeholder / example / commentary lines; flag anything that looks
# like an actual assigned secret-bearing string.
SECRET_REGEX='(password|passphrase|secret|api[_-]?key|jwt|private[_-]?key)[[:space:]]*[:=][[:space:]]*[A-Za-z0-9/+_=]{16,}'
SECRET_HITS="$(grep -rniE "$SECRET_REGEX" "$APPLIANCE_DIR" 2>/dev/null \
    | grep -viE 'INVALIDREPLACEME|placeholder|example|smoke-test-placeholder' \
    || true)"
if [ -z "$SECRET_HITS" ]; then
    ok "no obvious secret strings"
else
    bad "obvious secret strings present:"
    printf '%s\n' "$SECRET_HITS" >&2
fi

# 6. demo netdev isolation construction (#1727 / #2386)
#
# Asserts smoke-local.sh constructs the QEMU user-net string with guest
# isolation on by default in --demo (restrict=on + all hostfwds), with the
# explicit ICN_APPLIANCE_ALLOW_OUTBOUND=1 override honored and the base
# (non-demo) smoke unchanged. Static construction check only — the runtime
# canary proof runs inside `smoke-local.sh --real --demo`.
section "demo netdev isolation construction"
if bash "$APPLIANCE_DIR/smoke/net-restrict-check.sh" >/dev/null; then
    ok "netdev isolation construction (4/4 cases)"
else
    bad "netdev isolation construction (run: bash deploy/appliance/smoke/net-restrict-check.sh)"
fi

# 7. typed manifest emit/verify round-trip (skip-aware / opt-in)
#
# Proves the typed appliance manifest path (`icnctl appliance emit-manifest`
# #2259 + `verify-manifest` #2260, wired into build-image.sh #2261) still honors
# its contract — without building a real image. To keep this script lightweight
# it does NOT build Rust by default: it reuses a prebuilt icnctl when one exists
# and otherwise SKIPs (not a failure). When ICN_APPLIANCE_CHECK_TYPED_MANIFEST=1
# the check is REQUIRED and icnctl is rebuilt from current source first, so the
# round-trip can never pass/fail against a stale prebuilt binary (old emit/verify
# code left in target/ by an earlier checkout or cache).
section "typed manifest emit/verify round-trip"
ROUNDTRIP="$APPLIANCE_DIR/manifest-roundtrip-check.sh"
REPO_ROOT="$(cd "$APPLIANCE_DIR/../.." && pwd)"
ICNCTL_BIN=""
if [ "${ICN_APPLIANCE_CHECK_TYPED_MANIFEST:-0}" = "1" ]; then
    # Required mode: rebuild icnctl from CURRENT source before selecting it. A
    # preexisting target/ artifact is intentionally NOT reused, so the round-trip
    # always exercises current emit/verify code. --release matches build-image.sh,
    # which ships the release binary.
    printf '  ..   building icnctl --release from current source (ICN_APPLIANCE_CHECK_TYPED_MANIFEST=1)\n'
    if ( cd "$REPO_ROOT/icn" && cargo build -p icnctl --release >/dev/null 2>&1 ); then
        ICNCTL_BIN="$REPO_ROOT/icn/target/release/icnctl"
    fi
else
    # Default mode: reuse a prebuilt icnctl if present; never force a Rust build.
    for cand in "$REPO_ROOT/icn/target/release/icnctl" "$REPO_ROOT/icn/target/debug/icnctl"; do
        [ -x "$cand" ] && ICNCTL_BIN="$cand" && break
    done
fi
if [ -n "$ICNCTL_BIN" ] && [ -x "$ICNCTL_BIN" ]; then
    if bash "$ROUNDTRIP" "$ICNCTL_BIN"; then
        ok "typed manifest round-trip ($ICNCTL_BIN)"
    else
        bad "typed manifest round-trip"
    fi
elif [ "${ICN_APPLIANCE_CHECK_TYPED_MANIFEST:-0}" = "1" ]; then
    bad "typed manifest round-trip required (ICN_APPLIANCE_CHECK_TYPED_MANIFEST=1) but the icnctl --release build failed"
else
    printf '  SKIP typed manifest round-trip — no prebuilt icnctl found\n'
    printf '       enable it: (cd icn && cargo build -p icnctl) then re-run, or\n'
    printf '       force it:  ICN_APPLIANCE_CHECK_TYPED_MANIFEST=1 bash %s\n' "$0"
fi

# Summary
printf '\n[check] passed=%d failed=%d\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
