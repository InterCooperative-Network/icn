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

cd "$(git rev-parse --show-toplevel)"

DETERMINISTIC_MODE="${PILOT_DETERMINISTIC:-0}"
DETERMINISTIC_STRICT="${PILOT_DETERMINISTIC_STRICT:-0}"
GATEWAY_URL="${ICN_GATEWAY_URL:-http://127.0.0.1:7845}"
SUMMARY_DIR="tmp"
SUMMARY_FILE="${SUMMARY_DIR}/pilot_chain_summary.json"

DECISION_RECEIPT_ID=""
DECISION_HASH=""
LEDGER_ENTRY_HASH=""

json_escape() {
    local value="$1"
    value="${value//\\/\\\\}"
    value="${value//\"/\\\"}"
    value="${value//$'\n'/\\n}"
    printf '%s' "$value"
}

write_summary_json() {
    mkdir -p "$SUMMARY_DIR"
    local timestamp_utc
    timestamp_utc="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    cat >"$SUMMARY_FILE" <<EOF
{
  "deterministic_mode": $([ "$DETERMINISTIC_MODE" = "1" ] && echo "true" || echo "false"),
  "timestamp_utc": "$(json_escape "$timestamp_utc")",
  "gateway_url": "$(json_escape "$GATEWAY_URL")",
  "decision_receipt_id": "$(json_escape "$DECISION_RECEIPT_ID")",
  "decision_hash": "$(json_escape "$DECISION_HASH")",
  "ledger_entry_hash": "$(json_escape "$LEDGER_ENTRY_HASH")"
}
EOF
}

run_demo_once() {
    local run_label="$1"
    local logfile="/tmp/icn_pilot_demo_${run_label}_$$.log"
    local fail=0

    echo "╔══════════════════════════════════════════════════════════════════╗"
    echo "║         ICN PILOT: Decision → Effect → Ledger Demo               ║"
    echo "╚══════════════════════════════════════════════════════════════════╝"
    echo
    echo "Run label: ${run_label}"
    echo "Gateway URL: ${GATEWAY_URL}"
    echo

    # Move into the Rust workspace
    cd icn

    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "[1/2] E2E Chain Test: Decision → Executor → Ledger"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo
    cargo test -p icn-core --test treasury_integration \
        test_decision_to_ledger_provenance_end_to_end \
        -- --nocapture 2>&1 | tee "$logfile"

    echo
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "[2/2] Adapter Test: LedgerServiceImpl → JournalEntry"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo
    cargo test -p icn-core --test treasury_integration \
        test_ledger_entry_carries_decision_provenance \
        -- --nocapture 2>&1 | tee -a "$logfile"

    echo
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "TRIPWIRE VERIFICATION"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if grep -q "ICN PILOT PROVENANCE CHAIN VERIFIED" "$logfile"; then
        echo "✅ TRIPWIRE: 'ICN PILOT PROVENANCE CHAIN VERIFIED' found"
    else
        echo "❌ TRIPWIRE FAILED: 'ICN PILOT PROVENANCE CHAIN VERIFIED' missing"
        fail=1
    fi

    if grep -q "PILOT_LEDGER_ENTRY_HASH=" "$logfile"; then
        LEDGER_ENTRY_HASH=$(grep -m1 "^PILOT_LEDGER_ENTRY_HASH=" "$logfile" | cut -d= -f2-)
        echo "✅ TRIPWIRE: Ledger entry hash = $LEDGER_ENTRY_HASH"
    else
        echo "❌ TRIPWIRE FAILED: 'PILOT_LEDGER_ENTRY_HASH' missing"
        fail=1
    fi

    if grep -q "^PILOT_DECISION_RECEIPT_ID=" "$logfile"; then
        DECISION_RECEIPT_ID=$(grep -m1 "^PILOT_DECISION_RECEIPT_ID=" "$logfile" | cut -d= -f2-)
        echo "✅ TRIPWIRE: Decision receipt id = $DECISION_RECEIPT_ID"
    else
        echo "❌ TRIPWIRE FAILED: 'PILOT_DECISION_RECEIPT_ID' missing"
        fail=1
    fi

    if grep -q "^PILOT_DECISION_HASH=" "$logfile"; then
        DECISION_HASH=$(grep -m1 "^PILOT_DECISION_HASH=" "$logfile" | cut -d= -f2-)
        echo "✅ TRIPWIRE: Decision hash = $DECISION_HASH"
    else
        echo "❌ TRIPWIRE FAILED: 'PILOT_DECISION_HASH' missing"
        fail=1
    fi

    if grep -q "✅ MATCHES" "$logfile"; then
        echo "✅ TRIPWIRE: Provenance field assertions passed"
    else
        echo "❌ TRIPWIRE FAILED: '✅ MATCHES' not found"
        fail=1
    fi

    if grep -q "test.*ok" "$logfile"; then
        echo "✅ TRIPWIRE: Tests passed"
    else
        echo "❌ TRIPWIRE FAILED: No 'test.*ok' found"
        fail=1
    fi

    cd ..
    if [ "$fail" -eq 0 ]; then
        echo "════════════════════════════════════════════════════════════════════"
        echo "  PILOT INVARIANT PROVEN: All tripwires passed"
        echo "════════════════════════════════════════════════════════════════════"
        rm -f "$logfile"
        return 0
    fi

    echo "════════════════════════════════════════════════════════════════════"
    echo "  PILOT INVARIANT FAILED: See $logfile for details"
    echo "════════════════════════════════════════════════════════════════════"
    return 1
}

run_demo_once "run1"

RUN1_DECISION_RECEIPT_ID="$DECISION_RECEIPT_ID"
RUN1_DECISION_HASH="$DECISION_HASH"
RUN1_LEDGER_ENTRY_HASH="$LEDGER_ENTRY_HASH"

if [ "$DETERMINISTIC_MODE" = "1" ]; then
    echo
    echo "Deterministic mode enabled: running second pass for invariant comparison..."
    run_demo_once "run2"

    RUN2_DECISION_RECEIPT_ID="$DECISION_RECEIPT_ID"
    RUN2_DECISION_HASH="$DECISION_HASH"
    RUN2_LEDGER_ENTRY_HASH="$LEDGER_ENTRY_HASH"

    DET_FAIL=0
    LEDGER_HASH_MISMATCH=0
    if [ "$RUN1_DECISION_RECEIPT_ID" != "$RUN2_DECISION_RECEIPT_ID" ]; then
        echo "❌ DETERMINISM FAILED: decision_receipt_id mismatch"
        echo "  run1: $RUN1_DECISION_RECEIPT_ID"
        echo "  run2: $RUN2_DECISION_RECEIPT_ID"
        DET_FAIL=1
    fi
    if [ "$RUN1_DECISION_HASH" != "$RUN2_DECISION_HASH" ]; then
        echo "❌ DETERMINISM FAILED: decision_hash mismatch"
        echo "  run1: $RUN1_DECISION_HASH"
        echo "  run2: $RUN2_DECISION_HASH"
        DET_FAIL=1
    fi
    if [ "$RUN1_LEDGER_ENTRY_HASH" != "$RUN2_LEDGER_ENTRY_HASH" ]; then
        LEDGER_HASH_MISMATCH=1
        if [ "$DETERMINISTIC_STRICT" = "1" ]; then
            echo "❌ DETERMINISM FAILED: ledger_entry_hash mismatch"
            echo "  run1: $RUN1_LEDGER_ENTRY_HASH"
            echo "  run2: $RUN2_LEDGER_ENTRY_HASH"
            DET_FAIL=1
        else
            echo "⚠️ DETERMINISM WARN: ledger_entry_hash mismatch"
            echo "  run1: $RUN1_LEDGER_ENTRY_HASH"
            echo "  run2: $RUN2_LEDGER_ENTRY_HASH"
            echo "  continuing because PILOT_DETERMINISTIC_STRICT is not set"
        fi
    fi

    if [ "$DET_FAIL" -ne 0 ]; then
        echo "Strict mode enabled; failing due to ledger_entry_hash drift."
        echo "Deterministic check failed."
        exit 1
    fi
    if [ "$LEDGER_HASH_MISMATCH" -eq 1 ] && [ "$DETERMINISTIC_STRICT" != "1" ]; then
        echo "⚠️ DETERMINISM PARTIAL: decision ids/hashes match"
        echo "ledger_entry_hash differs."
        echo "set PILOT_DETERMINISTIC_STRICT=1 to enforce ledger hash stability."
    else
        echo "✅ DETERMINISM VERIFIED: run1 and run2 invariant identifiers match under active policy"
    fi
    DECISION_RECEIPT_ID="$RUN1_DECISION_RECEIPT_ID"
    DECISION_HASH="$RUN1_DECISION_HASH"
    LEDGER_ENTRY_HASH="$RUN1_LEDGER_ENTRY_HASH"
    write_summary_json
    echo "Summary artifact written to $SUMMARY_FILE"
fi
