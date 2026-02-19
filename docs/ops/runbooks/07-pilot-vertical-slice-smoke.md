# Pilot Vertical Slice Smoke

## Summary

Use this runbook to prove the pilot-critical provenance chain:

`Decision -> Effect -> Ledger`

The smoke command runs targeted integration tests and fails fast if linkage breaks.

## Prerequisites

1. Repository checked out locally.
2. Rust toolchain installed for the workspace.
3. Run from the repository root (the directory that contains `icn/`, `scripts/`, and `docs/`).

## Procedure

1. Run the one-command smoke verification:

```bash
cd "$(git rev-parse --show-toplevel)"
bash scripts/pilot_chain_demo.sh
```

Deterministic mode (run-twice invariant comparison + JSON artifact):

```bash
cd "$(git rev-parse --show-toplevel)"
PILOT_DETERMINISTIC=1 bash scripts/pilot_chain_demo.sh
cat tmp/pilot_chain_summary.json
```

Flow C treasury-governance operator demo:

```bash
cd "$(git rev-parse --show-toplevel)"
ICN_BASE_URL=http://127.0.0.1:7845 \
ICN_TOKEN=<jwt-token> \
ICN_COOP_ID=<coop-id> \
bash demo/flow-c-treasury-governance.sh
```

2. The script executes two tests:
   - `test_decision_to_ledger_provenance_end_to_end`
   - `test_ledger_entry_carries_decision_provenance`

3. The script enforces tripwires and exits non-zero on failure.

## Verification

Confirm all of the following in output:

1. `ICN PILOT PROVENANCE CHAIN VERIFIED`
2. `PILOT_LEDGER_ENTRY_HASH=...`
3. `TRIPWIRE: Tests passed`
4. Final banner: `PILOT INVARIANT PROVEN`
5. In deterministic mode:
   - `DETERMINISM VERIFIED` when all compared fields match under active policy
   - `DETERMINISM PARTIAL` when `decision_receipt_id` and `decision_hash` match but `ledger_entry_hash` differs (non-strict mode)

The script exit code must be `0`.

Deterministic mode writes `tmp/pilot_chain_summary.json` with:

- `deterministic_mode`
- `timestamp_utc`
- `gateway_url`
- `decision_receipt_id`
- `decision_hash`
- `ledger_entry_hash`

Strict toggle:

- `PILOT_DETERMINISTIC_STRICT=1` makes `ledger_entry_hash` mismatch a hard failure.

## Failure Handling

If the smoke check fails:

1. Capture full output and failing test name.
2. Re-run the specific test with logs:

```bash
cd icn
cargo test -p icn-core --test treasury_integration test_decision_to_ledger_provenance_end_to_end -- --nocapture
```

3. Check for provenance field mismatch:
   - `decision_receipt_id`
   - `decision_hash`
   - `ledger_entry_hash`

4. Block deployment until the smoke command passes again.

## Rollback

This runbook is verification-only and does not mutate deployed state.
No operational rollback is required.

## Related

- `scripts/pilot_chain_demo.sh`
- `demo/flow-c-treasury-governance.sh`
- [Troubleshooting](./05-troubleshooting.md)
- [Version Upgrade](./03-version-upgrade.md)
