#!/usr/bin/env bash
# Deterministic pilot smoke verification for decision -> effect -> ledger linkage.
# Exits non-zero on any failed assertion.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ICN_DIR="$ROOT_DIR/icn"
LOGFILE="${ICN_SMOKE_LOGFILE:-/tmp/icn_pilot_smoke_$$.log}"
TIMEOUT_SECS="${ICN_SMOKE_TIMEOUT_SECS:-120}"

if ! [[ "$TIMEOUT_SECS" =~ ^[0-9]+$ ]] || [ "$TIMEOUT_SECS" -le 0 ]; then
    echo "ERROR: ICN_SMOKE_TIMEOUT_SECS must be a positive integer (got '$TIMEOUT_SECS')."
    exit 2
fi

if command -v timeout >/dev/null 2>&1; then
    TIMEOUT_BIN="timeout"
elif command -v gtimeout >/dev/null 2>&1; then
    TIMEOUT_BIN="gtimeout"
else
    TIMEOUT_BIN=""
fi

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "ERROR: required command '$1' is not available."
        exit 2
    fi
}

fail_with_hint() {
    local reason="$1"
    echo "SMOKE FAILED: $reason"
    echo "Log file: $LOGFILE"
    echo "Hint: rerun the failing test with '-- --nocapture' and inspect markers."
    exit 1
}

assert_log_contains() {
    local pattern="$1"
    local reason="$2"
    if ! rg -q "$pattern" "$LOGFILE"; then
        fail_with_hint "$reason"
    fi
}

run_test() {
    local description="$1"
    shift
    local cmd=( "$@" )
    echo "==> $description"
    if [ -n "$TIMEOUT_BIN" ]; then
        if ! "$TIMEOUT_BIN" "${TIMEOUT_SECS}s" "${cmd[@]}" 2>&1 | tee -a "$LOGFILE"; then
            fail_with_hint "$description failed or exceeded ${TIMEOUT_SECS}s timeout"
        fi
    else
        if ! "${cmd[@]}" 2>&1 | tee -a "$LOGFILE"; then
            fail_with_hint "$description failed"
        fi
    fi
}

require_cmd cargo
require_cmd rg
require_cmd tee

: > "$LOGFILE"

cd "$ICN_DIR"

run_test \
    "Decision->Effect->Ledger E2E provenance test" \
    env RUSTC_WRAPPER= cargo test -p icn-core --test treasury_integration \
    test_decision_to_ledger_provenance_end_to_end -- --nocapture

run_test \
    "Ledger adapter provenance field test" \
    env RUSTC_WRAPPER= cargo test -p icn-core --test treasury_integration \
    test_ledger_entry_carries_decision_provenance -- --nocapture

assert_log_contains "ICN PILOT PROVENANCE CHAIN VERIFIED" \
    "provenance chain verification banner missing"
assert_log_contains "PILOT_DECISION_RECEIPT_ID=" \
    "decision_receipt_id marker missing"
assert_log_contains "PILOT_DECISION_HASH=" \
    "decision_hash marker missing"
assert_log_contains "PILOT_LEDGER_ENTRY_HASH=" \
    "ledger entry hash marker missing"
assert_log_contains "expected_nonce=Some\\(" \
    "expected_nonce was not observed in spend execution log"
assert_log_contains "decision_receipt_id: ✅ MATCHES" \
    "decision_receipt_id linkage assertion missing"
assert_log_contains "decision_hash:[[:space:]]+✅ MATCHES" \
    "decision_hash linkage assertion missing"
assert_log_contains "test result: ok\\. 1 passed" \
    "expected test pass summary missing"

decision_receipt_id="$(rg -o "PILOT_DECISION_RECEIPT_ID=.*" "$LOGFILE" | tail -n1 | cut -d= -f2-)"
decision_hash="$(rg -o "PILOT_DECISION_HASH=.*" "$LOGFILE" | tail -n1 | cut -d= -f2-)"
ledger_entry_hash="$(rg -o "PILOT_LEDGER_ENTRY_HASH=.*" "$LOGFILE" | tail -n1 | cut -d= -f2-)"

if [ -z "$decision_receipt_id" ] || [ -z "$decision_hash" ] || [ -z "$ledger_entry_hash" ]; then
    fail_with_hint "one or more pilot markers were empty"
fi

echo ""
echo "SMOKE PASS"
echo "decision_receipt_id=$decision_receipt_id"
echo "decision_hash=$decision_hash"
echo "ledger_entry_hash=$ledger_entry_hash"

rm -f "$LOGFILE"
