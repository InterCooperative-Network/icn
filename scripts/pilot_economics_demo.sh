#!/usr/bin/env bash
# ICN Pilot Economics Demo - Proves Economic Determinism
#
# This script runs the Sprint 8 economic chain tests and displays:
# DecisionReceipt → AllocationReceipt → SettlementIntent → LedgerEntry
#
# The economic invariant: same inputs → same canonical hashes across nodes.
#
# Usage: ./scripts/pilot_economics_demo.sh

set -euo pipefail

cd "$(dirname "$0")/.."

LOGFILE="/tmp/icn_economics_demo_$$.log"

echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║       ICN PILOT: Economics Substrate Determinism Demo            ║"
echo "╚══════════════════════════════════════════════════════════════════╝"
echo

# Move into the Rust workspace
cd icn

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[1/3] Economic Chain Two-Node Determinism Test"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
cargo test -p icn-core --test economic_chain_two_node \
    -- --nocapture 2>&1 | tee "$LOGFILE"

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[2/3] CanonicalReceipt Trait Tests"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
cargo test -p icn-kernel-api receipts:: \
    -- --nocapture 2>&1 | tee -a "$LOGFILE"

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[3/3] SettlementIntent + AssetType Tests"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
cargo test -p icn-kernel-api economics:: \
    -- --nocapture 2>&1 | tee -a "$LOGFILE"

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "ECONOMIC DETERMINISM TRIPWIRES"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Machine-assertable tripwires
FAIL=0

# Check for cross-node hash equality proof
if grep -q "ALL CANONICAL HASHES MATCH" "$LOGFILE" || grep -q "test_economic_chain_canonical_hash_determinism.*ok" "$LOGFILE"; then
    echo "✅ TRIPWIRE: Cross-node determinism verified"
else
    echo "❌ TRIPWIRE FAILED: Cross-node determinism not verified"
    FAIL=1
fi

# Check for settlement intent hash stability
if grep -q "test.*intent.*ok" "$LOGFILE"; then
    echo "✅ TRIPWIRE: SettlementIntent hash stability verified"
else
    echo "❌ TRIPWIRE FAILED: SettlementIntent hash stability not verified"
    FAIL=1
fi

# Check for allocation receipt tests
if grep -q "test.*allocation.*ok" "$LOGFILE"; then
    echo "✅ TRIPWIRE: AllocationReceipt tests passed"
else
    echo "❌ TRIPWIRE FAILED: AllocationReceipt tests not found"
    FAIL=1
fi

# Check overall test pass
if grep -q "test result: ok" "$LOGFILE"; then
    echo "✅ TRIPWIRE: All test suites passed"
else
    echo "❌ TRIPWIRE FAILED: Some tests failed"
    FAIL=1
fi

echo
if [ $FAIL -eq 0 ]; then
    echo "════════════════════════════════════════════════════════════════════"
    echo "  ECONOMIC DETERMINISM INVARIANT PROVEN"
    echo ""
    echo "  CanonicalReceipt trait enforces cross-node hash equality."
    echo "  SettlementIntent + AllocationReceipt produce deterministic hashes."
    echo "  Same economic inputs → same canonical outputs."
    echo "════════════════════════════════════════════════════════════════════"
    rm -f "$LOGFILE"
    exit 0
else
    echo "════════════════════════════════════════════════════════════════════"
    echo "  ECONOMIC DETERMINISM INVARIANT FAILED: See $LOGFILE for details"
    echo "════════════════════════════════════════════════════════════════════"
    exit 1
fi
