#!/bin/bash
# Meaning Firewall Check - CI Script (honest, fail-capable rewrite 2026-07-22)
#
# Verifies that EXCEPTION-FREE kernel-class crates contain no domain-crate
# imports in src/. Crate classes come from the single-source taxonomy
# (scripts/firewall-taxonomy.toml); nothing is hardcoded here.
#
# Kernel crates that carry [[exception]] entries (today: icn-core) are covered
# by the finer-grained ratchet tests (icn-core meaning_firewall.rs) and the
# Cargo-level checks (firewall_denylist.py) — this script enforces the crates
# whose debt is ZERO, so they can never regress silently.
#
# History: the previous version always exited 0 ("allow CI pass during active
# refactoring") — a required branch-protection check that structurally could
# not fail. That defect is what this rewrite removes.
#
# Usage: ./scripts/check-meaning-firewall.sh
# Exit codes: 0 = clean; 1 = violations; 2 = environment/taxonomy error

set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v python3 >/dev/null; then
    echo "ERROR: python3 required (taxonomy loader)" >&2
    exit 2
fi

# Load kernel/domain lists + exception edges from the taxonomy.
# FAIL-CLOSED: capture first — `eval "$(cmd)"` swallows the loader's exit code
# (empty eval is a no-op), which would skip the check silently.
if ! TAXONOMY_LISTS="$(python3 .github/scripts/firewall_taxonomy.py --bash)"; then
    echo "ERROR: failed to load scripts/firewall-taxonomy.toml" >&2
    exit 2
fi
eval "$TAXONOMY_LISTS"
if [ "${#KERNEL_CRATES[@]}" -eq 0 ]; then
    echo "ERROR: taxonomy produced an empty kernel list — refusing to pass with nothing scanned" >&2
    exit 2
fi

# Kernel crates with at least one exception entry (they carry pinned debt and
# are enforced by ratchets instead).
declare -A HAS_EXCEPTION=()
for edge in "${ALLOWED_EDGES[@]:-}"; do
    # Guard the empty expansion a zero-exception taxonomy produces under set -u
    # (an empty key is a bad associative-array subscript — found by the
    # failure-path battery's clean-fixture case).
    [ -n "$edge" ] || continue
    HAS_EXCEPTION["${edge%%->*}"]=1
done

VIOLATIONS=0

echo "=== Meaning Firewall Check (taxonomy-driven) ==="
echo ""
echo "Kernel class: ${KERNEL_CRATES[*]}"
echo ""

for crate in "${KERNEL_CRATES[@]}"; do
    CRATE_DIR="icn/crates/$crate/src"
    if [ ! -d "$CRATE_DIR" ]; then
        echo "  ⚠️  $crate: no src directory found" >&2
        continue
    fi
    if [ -n "${HAS_EXCEPTION[$crate]:-}" ]; then
        echo "  ⏭  $crate: has pinned exceptions — enforced by ratchet tests + firewall_denylist.py"
        continue
    fi
    CRATE_HITS=0
    for dep in "${FORBIDDEN[@]}"; do
        PATTERN="use ${dep//-/_}::"
        # meaning_firewall.rs holds pattern literals for the ratchet tests.
        COUNT=$(grep -r --include='*.rs' --exclude='meaning_firewall.rs' -E "$PATTERN" "$CRATE_DIR" 2>/dev/null | wc -l || true)
        if [ "$COUNT" -gt 0 ]; then
            echo "  ❌ $crate: $COUNT \`$PATTERN\` import(s)"
            # `|| true`: head closing the pipe early SIGPIPEs grep under pipefail.
            { grep -rn --include='*.rs' --exclude='meaning_firewall.rs' -E "$PATTERN" "$CRATE_DIR" | head -5 | sed 's/^/       /'; } || true
            CRATE_HITS=$((CRATE_HITS + COUNT))
        fi
    done
    if [ "$CRATE_HITS" -eq 0 ]; then
        echo "  ✅ $crate: clean"
    fi
    VIOLATIONS=$((VIOLATIONS + CRATE_HITS))
done

echo ""
if [ "$VIOLATIONS" -gt 0 ]; then
    echo "RESULT: $VIOLATIONS NEW violation(s) detected in exception-free kernel crates"
    echo ""
    echo "Exception-free kernel crates must stay free of domain imports."
    echo "Remediation:"
    echo "  1) Remove the import — use PolicyOracle/ConstraintSet or kernel-api traits"
    echo "  2) If unavoidable, add a reviewed [[exception]] to scripts/firewall-taxonomy.toml"
    echo "     (that moves the crate to ratchet-based enforcement — a tracked, shrinking state)"
    echo "Reference: docs/architecture/KERNEL_APP_SEPARATION.md, docs/ci/GATE_RATCHET_PLAN.md"
    exit 1
fi

# Honest claim boundary: this script proves only the slice it scans.
echo "RESULT: No new unpinned domain imports in exception-free kernel crates."
echo "Pinned firewall debt remains elsewhere: ${#ALLOWED_EDGES[@]} kernel exception edges"
echo "(tracked in scripts/firewall-taxonomy.toml; enforced by firewall_denylist.py"
echo "and the icn-core meaning_firewall.rs ratchets). The firewall is NOT 'done' —"
echo "this gate only proves no NEW debt entered its scan scope."
exit 0
