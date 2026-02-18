# Pilot Vertical Slice Smoke

## Summary

Use this runbook to prove the pilot-critical provenance chain:

`Decision -> Effect -> Ledger`

The smoke command runs targeted integration tests and fails fast if linkage breaks.

## Prerequisites

1. Repository checked out locally.
2. Rust toolchain installed for the workspace.
3. Run from repo root (`/home/ubuntu/projects/icn`).

## Procedure

1. Run the one-command smoke verification:

```bash
bash scripts/pilot_chain_demo.sh
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

The script exit code must be `0`.

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
- [Troubleshooting](./05-troubleshooting.md)
- [Version Upgrade](./03-version-upgrade.md)
