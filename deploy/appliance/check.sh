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
HITS="$(grep -rwniE "$FORBIDDEN_PATTERN" "$APPLIANCE_DIR" \
    --exclude="check.sh" 2>/dev/null || true)"
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

# Summary
printf '\n[check] passed=%d failed=%d\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
