#!/bin/bash
# Meaning Firewall Check - CI Script
#
# This script verifies that kernel crates don't directly import domain-specific types.
# Run this in CI to catch accidental firewall breaches.
#
# Usage: ./scripts/check-meaning-firewall.sh
#
# Exit codes:
#   0: No violations detected (firewall is clean)
#   1: Violations detected (expected before Phase 2 completes)
#
# Tracking Issues:
#   - #865: Gateway trust manager → oracle
#   - #866: Gossip topic access control → oracle
#   - #867: Ledger entry validation → oracle

set -e

cd "$(dirname "$0")/.."

KERNEL_CRATES="icn-net icn-gateway icn-gossip icn-ledger"
VIOLATIONS=0

echo "=== Meaning Firewall Check ==="
echo ""

# Check for icn_trust imports in kernel crate source files
echo "Checking for forbidden imports in kernel crates..."
for crate in $KERNEL_CRATES; do
    CRATE_DIR="icn/crates/$crate/src"
    if [ -d "$CRATE_DIR" ]; then
        COUNT=$(grep -rE "use icn_trust::" "$CRATE_DIR" 2>/dev/null | wc -l || true)
        if [ "$COUNT" -gt 0 ]; then
            echo "  ⚠️  $crate: $COUNT icn_trust imports"
            VIOLATIONS=$((VIOLATIONS + COUNT))
        else
            echo "  ✅ $crate: clean"
        fi
    fi
done

echo ""
if [ "$VIOLATIONS" -gt 0 ]; then
    echo "RESULT: $VIOLATIONS violation(s) detected"
    echo ""
    echo "This is expected until Phase 2 completes."
    echo "After Phase 2, this script should report 0 violations."
    echo ""
    echo "To see detailed violations, run:"
    echo "  cargo test -p icn-core --lib meaning_firewall -- --nocapture"
    exit 1
else
    echo "RESULT: Firewall is clean! 🎉"
    exit 0
fi
