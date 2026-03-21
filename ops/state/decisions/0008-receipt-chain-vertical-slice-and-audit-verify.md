# ADR-0008: Receipt Chain Vertical Slice and Audit Verify

**Date**: 2026-03-19
**Status**: accepted
**Tags**: compliance, audit, cli, integration-test
**Sprint**: Sprint 15 (PRs #1351, #1354, issue #1147)

## Context

Sprint 15 needed a verifiable end-to-end proof that the full receipt chain works: Identity → Governance → Execution → Receipt → Audit. This was required as evidence for the grant application and to close epic #1302.

## Decision

Two complementary artifacts:

1. **Vertical slice integration test** (PR #1351): A single integration test (`receipt_chain_vertical_slice`) that exercises the full path from identity creation through governance proposal, execution, receipt issuance, and audit verification. Lives in `crates/icn-core/tests/`.

2. **`icnctl audit verify`** (PR #1354, issue #1147): CLI command that walks the receipt chain for a given node/scope, checks for orphan receipts (no linked governance decision), and reports chain integrity. Typed deserialization, orphan check, sprint board integrated.

Together these close the audit trail gap: the integration test proves the chain works in CI; `icnctl audit verify` proves it works on a live cluster.

## Consequences

- Epic #1302 closed with verified evidence (PR #1353)
- Grant artifacts can reference verifiable receipt chain as a technical claim
- `icnctl audit verify` is now the standard tool for live cluster compliance checks
- Integration test must stay as a required CI check (not optional/flaky)

## Alternatives Considered

| Alternative | Why rejected |
|-------------|-------------|
| Manual curl-based demo only | Not reproducible, not CI-enforced |
| Audit as separate service | Over-engineering — CLI subcommand is sufficient for this stage |
