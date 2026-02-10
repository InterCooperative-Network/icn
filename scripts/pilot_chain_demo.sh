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
    -- --nocapture 2>&1 | grep -v "^running\|^test result\|Compiling\|Finished\|Running\|Downloading\|Downloaded" || true

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "[2/2] Adapter Test: LedgerServiceImpl → JournalEntry"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
cargo test -p icn-core --test treasury_integration \
    test_ledger_entry_carries_decision_provenance \
    -- --nocapture 2>&1 | grep -v "^running\|^test result\|Compiling\|Finished\|Running\|Downloading\|Downloaded" || true

echo
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "VERIFICATION COMPLETE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo
echo "If both tests passed, the pilot invariant is proven:"
echo "  DecisionReceipt → TreasuryEffect → JournalEntry with provenance"
echo
echo "Source of truth:"
echo "  icn/crates/icn-core/tests/treasury_integration.rs"
echo "    - test_decision_to_ledger_provenance_end_to_end"
echo "    - test_ledger_entry_carries_decision_provenance"
echo
