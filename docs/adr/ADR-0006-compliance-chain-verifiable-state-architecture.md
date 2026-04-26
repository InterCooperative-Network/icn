# ADR-0006: Compliance Chain — Verifiable State Architecture

**Date**: 2026-03-02
**Status**: accepted
**Tags**: compliance, governance, economics, journal, meaning-firewall
**Sprint**: Sprint 14 (PRs #1315, #1318, #1320, #1325, #1326, #1327)
**Epic**: #1302 — Regulatory-safe verifiable state architecture

## Context

ICN was using terminology ("ledger", "payment", "balance", "currency") that mapped directly to regulated financial instruments. NY cooperative law (§93) requires internal capital accounts and patronage tracking. Legal review flagged the need for a clear audit trail linking governance decisions to execution receipts.

## Decision

Four-part compliance chain shipped in Sprint 14:

1. **Terminology rename** (PR #1315): `payment` → `settlement`, `currency` → `unit`, `balance` → `position` across all gateway API routes. The internal crate remains `icn-ledger` but user-facing text uses "state change journal" (not "ledger").

2. **Provenance chain** (PR #1318): `JournalEntry.provenance` is now required (`ProvenanceRef` enum). Every state change carries a reference to the governance decision or receipt that authorized it.

3. **Obligation lifecycle** (PR #1320): `Obligation` type with `AssetType::Claim` models commitments with a full lifecycle (proposed → accepted → fulfilled/expired). Not a financial instrument — a coordination primitive.

4. **PatronageTracker** (PR #1325): Tracks member contributions per NY Corp Law §93 internal capital accounts. Emits patronage allocations as `JournalEntry` records.

5. **DelegationManager persistence** (PR #1326): Delegation state gossip-synced across nodes.

6. **ExecutionReceiptGate** (PR #1327): Enforces that every execution must have a linked governance decision. Closes the governance→execution linkage.

## Consequences

- Every state change is traceable to a governance authorization — full audit chain
- Terminology is defensible in regulatory contexts (not payment rails)
- Apps using old route names (`/v1/payment/...`) must migrate to `/v1/settle/...`
- `icn-ledger` crate name is preserved for internal use; only user-visible text is changed
- CommonsResourcePolicy (S15) extracts the credit formula to CCL — not in this ADR

## Alternatives Considered

| Alternative | Why rejected |
|-------------|-------------|
| Keep "ledger" in user-facing text | Regulatory exposure — maps to financial instrument definitions |
| Optional provenance | Audit trail has gaps — can't prove compliance for any individual entry |
| Payment-style obligation model | Triggers money transmission regulations |
