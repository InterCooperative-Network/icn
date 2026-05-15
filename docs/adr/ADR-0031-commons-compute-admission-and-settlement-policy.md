---
id: "0031"
title: "Commons Compute Admission and Settlement Policy"
status: "accepted"
date: "2026-04-26"
deciders: ["Matt Faherty"]
tags: ["compute", "commons", "admission", "settlement", "mutual-credit", "back-fill"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "implemented"
references:
  - "ADR-0004 (ICN Models Obligations and Attestations, Not Payment Rails)"
  - "ADR-0005 (Commons Credit Settlement Architecture)"
  - "ADR-0007 (Commons Resource Policy — CCL Formula Extraction)"
  - "ADR-0030 (Compute Workload Manifest and Authority Boundary)"
  - "icn/crates/icn-compute/src/commons_pool.rs"
  - "icn/crates/icn-compute/src/receipt.rs"
  - "icn/crates/icn-ledger/src/settlement.rs"
  - "icn/crates/icn-compute/tests/commons_settlement_integration.rs"
---

# ADR 0031: Commons Compute Admission and Settlement Policy

## Status

**Accepted (2026-04-26).** Back-fill of the operational policy that has shipped with the commons pool and settlement engine. ADR-0005 set the commons-credit settlement direction; this ADR records what is enforced today.

## Context

ICN's commons compute is the shared-infrastructure layer: members and federated peers contribute compute capacity to a pool, and consumers draw from it under a trust-gated admission policy. Settlement runs on mutual credit, balanced earn/spend journal entries (ADR-0005), and is regulatory-safe in vocabulary (ADR-0004).

The pool, admission policy, and settlement engine have been implemented and tested for some time. The pieces:

- `CommonsPool` tracks affiliated and unaffiliated participant budgets per resource (cpu / memory / storage), with sybil rejection.
- The compute actor admits executors via PolicyOracle (trust score is one input; sybil heuristics are another).
- The `SettlementEngine` produces balanced journal entries when a compute receipt is settled.
- The cross-coop case (federation-scoped tasks) settles via the federation clearing path (ADR-0013 territory).

What was not written down: the policy that governs admission and settlement together. Future ADRs (governance-controlled admission policy; settlement challenge path) will plug into it; recording it now stabilizes the contract.

## Decision

### Admission policy

1. **Affiliated vs unaffiliated.** Participants are bucketed by trust class. Affiliated participants (trust class above a configured threshold) draw from the affiliated pool; unaffiliated draw from a separate unaffiliated pool that protects newcomers from being squeezed out by deep-trust participants.
2. **Sybil rejection.** Multiple low-trust DIDs originating from the same network identity, signing graph cluster, or temporal admission burst are rejected by configurable heuristics. The heuristics live in PolicyOracle; the pool consumes their decision.
3. **Capacity is honest.** Pool capacity is the sum of declared executor capacities, not aggregate optimism. Rejection at the pool boundary happens before workload start.
4. **Scope-aware admission.** A Coop-scoped task admits only executors that the coop has bound to its mandate; a Federation-scoped task admits federated executors only.

   *Entity-scope naming note (added 2026-05-14):* This ADR uses `Coop-scoped` (and "the coop") as the local-domain admission term because that was the vocabulary at the time of adoption. Current architecture (per `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 "Entity-scope vocabulary" and `docs/spec/institutional-domain.md`) reads this slot as `LocalDomain-scoped`: a scope owned by an `InstitutionalDomain` whose owning entity class may be `Cooperative`, `Community`, `Federation`, `Individual`, or another governed class permitted by policy. The settlement-side wording at step 4 of §"Settlement policy" below has the same disposition: read `Coop-scoped` as `LocalDomain-scoped`. Future schema and runtime work should prefer `LocalDomain` names; existing serialized `Coop`-prefixed strings remain valid for backward compatibility. The ADR's decision is preserved as-is.
5. **No governance gate today.** Pool-level admission is policy-decided (the PolicyOracle), but pool **policy itself** (thresholds, ratios, sybil heuristics) is not yet governed by a charter rule. This is a known gap (registry candidate ADR-0081 picks up the WASM-side governance; commons-pool governance is future work).

### Settlement policy

1. **Two-phase settlement.** Phase 1: compute produces a result and a `ComputeReceipt`. Phase 2: receipt is settled into the ledger as a balanced pair of journal entries (contributor earn, consumer spend).
2. **Balanced entries are required.** A receipt that does not balance is rejected at the settlement boundary. There are no half-settled receipts.
3. **Idempotent on receipt hash.** Re-settling the same receipt is a no-op. This is the only sanctioned "double-spend prevention" for commons compute.
4. **Scope-aware settlement.** Commons-scoped tasks settle within the commons-credit ledger; Coop-scoped tasks settle within the coop's internal ledger; Federation-scoped tasks settle via the federation clearing path.
5. **Disputable.** Settlement is a one-step operation but receipts remain queryable and challengeable per ADR-0029 (conflict resolution) once that resolution path is implemented.

### Vocabulary

ICN's commons settlement uses regulatory-safe vocabulary throughout:
- **Position**, not balance.
- **Settlement**, not payment.
- **Earn / spend**, not transfer.
- **Obligation / allocation**, not transaction.

This ADR locks the vocabulary in code paths and in public-facing surfaces.

## Consequences

- The commons pool is auditable: capacity is honest, admission is policy-driven, sybil rejection is structured.
- Settlement is replayable: re-running the settlement step on the same receipt produces the same journal entries.
- Trade-off: the policy is configurable but not yet governed. Until governance-controlled admission lands, "what counts as affiliated?" is a code-side configuration, not a charter rule.
- Trade-off: the affiliated/unaffiliated split is opinionated. Some institutions might want a single pool; the protection of newcomers is a design choice, not a hard requirement.

## Implementation status

Implemented.

Evidence:
- Pool: [icn/crates/icn-compute/src/commons_pool.rs](../../icn/crates/icn-compute/src/commons_pool.rs).
- Receipt: [icn/crates/icn-compute/src/receipt.rs](../../icn/crates/icn-compute/src/receipt.rs).
- Settlement engine: [icn/crates/icn-ledger/src/settlement.rs](../../icn/crates/icn-ledger/src/settlement.rs).
- Settlement test coverage: [icn/crates/icn-compute/tests/commons_settlement_integration.rs](../../icn/crates/icn-compute/tests/commons_settlement_integration.rs) (~272 lines: balanced entries, idempotency, scope validation, balance checks).
- Pool integration test: [icn/crates/icn-compute/tests/commons_integration.rs](../../icn/crates/icn-compute/tests/commons_integration.rs) (~1200 lines).

Open hardening item: [#992](https://github.com/InterCooperative-Network/icn/issues/992) — include `receipt_hash` in `SettlementEngine::settle_receipt` errors. Tracked under this ADR.

Future:
- Governance-controlled admission policy (charter-level admission thresholds): registry candidate, future ADR.
- Settlement challenge path that does not just refuse but produces a remediation: dependent on ADR-0029.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Single pool (no affiliated/unaffiliated split) | Lets deep-trust participants squeeze out newcomers. The split is the protection. |
| Settlement on result acceptance, no receipt step | Loses replay-grade audit. The two-phase shape exists so settlement is reproducible from the receipt alone. |
| Allow half-settled receipts with later reconciliation | Permanent state-mismatch hazard. Balanced-or-rejected is the audit-grade choice. |
| Use payment-rail vocabulary internally and translate at the boundary | Vocabulary discipline starts in the code paths. Internal "balance" vocabulary leaks into logs, error messages, and API surfaces. |
