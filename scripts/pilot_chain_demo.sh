#!/usr/bin/env bash
# ICN Pilot Chain Demo - Proves Decision -> Effect -> Ledger provenance
#
# This script runs the pilot-critical provenance tests and displays the
# complete chain: decision_receipt_id -> decision_hash -> ledger_entry_hash
#
# If either test fails, the pilot claim is NOT defensible.
#
# Usage: ./scripts/pilot_chain_demo.sh

set -euo pipefail

cd "$(dirname "$0")/.."

LOGFILE="/tmp/icn_pilot_demo_$$.log"

echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║         ICN PILOT: Decision → Effect → Ledger Demo               ║"
echo "╚══════════════════════════════════════════════════════════════════╝"
echo

# Move into the Rust workspace
cd icn

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[1/2] E2E Chain Test: Decision → Executor → Ledger"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
cargo test -p icn-core --test treasury_integration \
    test_decision_to_ledger_provenance_end_to_end \
    -- --nocapture 2>&1 | tee "$LOGFILE"

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[2/2] Adapter Test: LedgerServiceImpl → JournalEntry"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
cargo test -p icn-core --test treasury_integration \
    test_ledger_entry_carries_decision_provenance \
    -- --nocapture 2>&1 | tee -a "$LOGFILE"

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "TRIPWIRE VERIFICATION"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Machine-assertable tripwires
FAIL=0

if grep -q "ICN PILOT PROVENANCE CHAIN VERIFIED" "$LOGFILE"; then
    echo "✅ TRIPWIRE: 'ICN PILOT PROVENANCE CHAIN VERIFIED' found"
else
    echo "❌ TRIPWIRE FAILED: 'ICN PILOT PROVENANCE CHAIN VERIFIED' missing"
    FAIL=1
fi

if grep -q "PILOT_LEDGER_ENTRY_HASH=" "$LOGFILE"; then
    HASH=$(grep "PILOT_LEDGER_ENTRY_HASH=" "$LOGFILE" | cut -d= -f2)
    echo "✅ TRIPWIRE: Ledger entry hash = $HASH"
else
    echo "❌ TRIPWIRE FAILED: 'PILOT_LEDGER_ENTRY_HASH' missing"
    FAIL=1
fi

if grep -q "✅ MATCHES" "$LOGFILE"; then
    echo "✅ TRIPWIRE: Provenance field assertions passed"
else
    echo "❌ TRIPWIRE FAILED: '✅ MATCHES' not found"
    FAIL=1
fi

if grep -q "test.*ok" "$LOGFILE"; then
    echo "✅ TRIPWIRE: Tests passed"
else
    echo "❌ TRIPWIRE FAILED: No 'test.*ok' found"
    FAIL=1
fi

echo
if [ $FAIL -eq 0 ]; then
    echo "════════════════════════════════════════════════════════════════════"
    echo "  PILOT INVARIANT PROVEN: All tripwires passed"
    echo "════════════════════════════════════════════════════════════════════"
    rm -f "$LOGFILE"
    exit 0
else
    echo "════════════════════════════════════════════════════════════════════"
    echo "  PILOT INVARIANT FAILED: See $LOGFILE for details"
    echo "════════════════════════════════════════════════════════════════════"
    exit 1
fi
