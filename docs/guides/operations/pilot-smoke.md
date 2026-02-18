# Pilot Smoke Runbook

## Goal

Provide a deterministic operator check for the pilot-critical linkage:

`DecisionReceipt -> TreasuryEffect::Spend -> LedgerEntry`

The smoke command fails fast with actionable output and exits non-zero on any broken link.

## Preconditions

- Run from repo root.
- `cargo`, `rg`, and `tee` installed.
- Rust dependencies already fetched.

## Fresh Environment Setup

1. Build the targeted test binary once:

```bash
cd icn
RUSTC_WRAPPER= cargo test -p icn-core --test treasury_integration --no-run
cd ..
```

2. Optional daemon startup check (not required for smoke test itself):

```bash
cd icn
RUSTC_WRAPPER= cargo run --bin icnd > /tmp/icnd-pilot.log 2>&1 &
ICN_DAEMON_PID=$!
curl -sf http://localhost:8000/v1/healthz
```

## One-Command Smoke

```bash
bash scripts/pilot_smoke.sh
```

Expected successful tail:

```text
SMOKE PASS
decision_receipt_id=...
decision_hash=...
ledger_entry_hash=...
```

## Runtime Contract

- Default timeout per test command: `120s`
- Override timeout: `ICN_SMOKE_TIMEOUT_SECS=180 bash scripts/pilot_smoke.sh`
- Override logfile path: `ICN_SMOKE_LOGFILE=/tmp/pilot-smoke.log bash scripts/pilot_smoke.sh`
- Exit code `0`: linkage verified
- Exit code `1`: linkage/assertion failure
- Exit code `2`: misconfiguration or missing local dependency

## What The Script Asserts

1. E2E provenance marker emitted (`ICN PILOT PROVENANCE CHAIN VERIFIED`)
2. Decision markers present and non-empty:
   - `PILOT_DECISION_RECEIPT_ID`
   - `PILOT_DECISION_HASH`
   - `PILOT_LEDGER_ENTRY_HASH`
3. Spend execution log includes `expected_nonce=Some(...)`
4. Field linkage assertions appear:
   - `decision_receipt_id: MATCHES`
   - `decision_hash: MATCHES`
5. Both targeted tests report pass summaries

## Failure Triage

If the script fails:

1. Open the emitted log file path from script output.
2. Re-run failing test directly:

```bash
cd icn
RUSTC_WRAPPER= cargo test -p icn-core --test treasury_integration test_decision_to_ledger_provenance_end_to_end -- --nocapture
RUSTC_WRAPPER= cargo test -p icn-core --test treasury_integration test_ledger_entry_carries_decision_provenance -- --nocapture
```

3. Check for missing linkage fields in test output:
   - `decision_receipt_id`
   - `decision_hash`
   - `expected_nonce`

## Reset / Cleanup

- Smoke script auto-removes its temporary logfile on success.
- If daemon was started for optional health check:

```bash
kill "$ICN_DAEMON_PID"
```
