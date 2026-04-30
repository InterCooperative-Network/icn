---
title: "ADR-0016: CCL Architecture Boundary"
status: "accepted"
date: "2026-04-07"
supersedes: []
context: "CCL architecture review — Phase 1 hardening; Phase 2/3/4 partially implemented 2026-04-07"
---

# ADR-0016: CCL Architecture Boundary

## Status

Accepted — Phases 2, 3, and 4 partially implemented (see Implementation Roadmap for current state)

## Context

ICN's Cooperative Contract Language (CCL) has grown into two distinct runtime
mechanisms that share a crate but serve different purposes. Without an
explicit boundary definition, both mechanisms risk being misunderstood,
misused, or silently expanded into roles they should not hold.

Additionally, the existing CharterPolicyOracle computes `ConstraintSet` custom
keys correctly but those keys are never consumed — a production gap at the
consumption edge, not a computation gap.

This ADR defines the architectural boundary for CCL, documents what it should
and must not own, and establishes the enforcement direction.

---

## Decision

### CCL has two distinct runtime roles. Both are legitimate. Both have scope limits.

**Role 1 — Contract Executor (icn-ccl AST/interpreter/runtime)**

CCL is a deterministic, fuel-metered contract language for encoding explicit
inter-party agreements between cooperatives and members. It executes against
`icn-ledger` state via `ContractRuntime`. It is:

- An agreement enforcement engine (multi-party rules, bilateral obligations)
- A ledger-aware rule evaluator (read access to journal entries, balances)
- A deterministic, auditable computation that produces `ExecutionReceipt`

It is NOT:
- A general smart contract VM
- A replacement for governance decisions
- An on-chain token engine with gas economics
- A mechanism for arbitrary state mutation outside the ledger
- A place to encode single-coop internal policy that could live in the kernel

**Role 2 — Charter→ConstraintSet Bridge (icn-ccl schema/bridge)**

`charter_to_constraints()` is the Meaning Firewall boundary for charters.
It translates a `CclDocument` (YAML-encoded cooperative constitution) into a
`ConstraintSet` that the kernel enforces blindly. This is a PolicyOracle
input-stage, not execution. It is:

- A semantic translation layer: cooperative rules → generic constraints
- The place where threshold expressions, credit limits, and surplus rules
  become kernel-enforceable numbers
- Read-only with respect to state (no ledger access, no side effects)

It is NOT:
- A rule enforcement engine (it computes; it does not enforce)
- An executor (it produces ConstraintSet inputs to the oracle; evaluation
  happens at request time in CharterPolicyOracle)
- A contract executor (see Role 1 for that)

---

### The Critical Gap (as of 2026-04-07)

`charter_to_constraints()` is implemented and well-tested. The production
wiring has a gap at the **consumption edge**:

1. **Custom keys are computed but orphaned.** `CharterPolicyOracle` calls
   `charter_to_constraints()` and returns the ConstraintSet, but no production
   code path reads custom keys like `min_votes_ordinary`, `credit_limit`, or
   `surplus_reserves_pct` from any `ConstraintSet`. They are computed and
   dropped.

2. **`CharterContext` is hardcoded to 100 members.** In
   `bins/icnd/src/main.rs`, the oracle is constructed with:
   ```rust
   CharterContext::new().with_members(100)
   ```
   This means all threshold expressions using `members` evaluate against a
   fixed 100-member context, regardless of actual membership.

3. **`CharterValidator::evaluate_rule_basic()` is a complete stub.** All
   rules pass unconditionally. Transaction-level charter validation is not
   enforced.

These are **production enforcement gaps**, not design flaws. The architecture
is correct; the consumption wiring is incomplete.

---

### Invariants (enforced going forward)

**Invariant A — CCL does not own generic state mutation.**
CCL contracts access ledger state (read). CCL contracts produce
`ExecutionReceipt`. CCL contracts do not directly mutate governance state,
membership records, or identity records. Those are governance effects.

**Invariant B — The bridge is the Meaning Firewall boundary.**
`charter_to_constraints()` is the only place where charter semantics
(threshold ratios, credit formulas, surplus rules) are converted to
`ConstraintSet`. No other code may re-implement this conversion.

**Invariant C — Custom ConstraintSet keys must be consumed.**
Any custom key produced by `charter_to_constraints()` must have a documented
consumer — either in `CharterPolicyOracle`, in the governance execution layer,
or in a named future component. Orphaned keys are production gaps, not
intentional no-ops.

**Invariant D — CharterContext must reflect live membership.**
The `CharterContext` passed to `charter_to_constraints()` at request time must
bind `members` from the actual cooperative's current member count, not a
hardcoded constant.

**Invariant E — CharterValidator stubs must be named.**
`evaluate_rule_basic()` stubs (rules that always pass) must be explicitly
annotated with `// STUB: <reason>` and tracked in the gap inventory. A
silently-passing stub is a production gap, not a feature.

---

## Implementation Roadmap

### Phase 1 — Document the gap (this ADR) ✅ COMPLETE
- Named the production enforcement gaps
- Established invariants A–E

### Phase 2 — Close CharterContext hardcoding — PARTIAL
- **Done:** `CharterPolicyOracle::evaluate()` now reads optional `member_count`
  from request metadata. Callers passing `PolicyContext::with_metadata("member_count", …)`
  get live threshold evaluation. `GovernanceActor::get_thresholds_from_charter()`
  passes the actual eligible voter count.
- **Done:** `CharterPolicyOracle::thresholds_for(charter_id, decision_type, member_count)`
  added as an explicit query interface.
- **Remaining:** The charter ratification hook at daemon startup still uses
  `CharterContext::new().with_members(100)` for the frozen deploy-time context.
  This is a lower-priority cosmetic gap since the query-time path is live.

### Phase 3 — Close custom key consumption gap — PARTIAL
- **Done (governance):** `GovernanceActor::get_thresholds_from_charter()` reads
  `min_votes_ordinary` and `min_quorum_ordinary` from the charter oracle and
  applies them as threshold fallback in `CloseProposal`. Wired via
  `BootstrapHandles.charter_oracle` → `GovernanceDeps.charter_oracle` →
  `GovernanceHandle::with_charter_oracle()`.
- **Done (charter ratification):** `ProposalPayload::Charter` now produces a
  `Protocol(SetGovernanceConfig)` effect via `translate_payload_to_effects()`.
  This closes the loop: ratification → effect → oracle deployment.
- **Done (economics):** `Ledger::process_entry()` now consults a charter-derived
  credit limit via the typed `EconomicPolicyView` adapter before falling back
  to the dynamic/static chain. The trait lives in
  `icn-ledger::credit_policy::EconomicPolicyView` and is implemented by
  `apps/charter::CharterPolicyOracle::credit_limit_for()`. The daemon's
  `charter_accepted_hook` schedules a tokio task to bind the oracle as the
  ledger's `charter_economic_view` after each ratification. Charter
  expressions (e.g. `min(1000, patronage * 0.5 * trust_score)`) are evaluated
  per-member with the ledger's `cleared_volume` as the patronage proxy. Three
  truth states are logged: `ENFORCED` (charter governs), `FALLBACK_APPLIED`
  (charter present, no `credit_limit` key), `UNSUPPORTED` (no view set).
- **Done (surplus reserves — 2026-04-08):** `surplus_reserves_pct` is now
  consumed at `LedgerServiceImpl::submit_treasury_entry()` in
  `icn-core/src/services/ledger_service.rs`. The typed `SurplusPolicyView`
  adapter trait lives in `icn-ledger::credit_policy`. It is implemented by
  `CharterPolicyOracle::reserves_pct_for()` in `apps/charter/src/oracle.rs`
  (reading from `charter_to_constraints()`). The daemon's `charter_accepted_hook`
  in `bins/icnd/src/main.rs` binds the oracle to the ledger via
  `ledger.set_charter_surplus_view()` on each Charter ratification.
  Enforcement is distribution-gated: distributions that would leave the treasury
  below `reserves_pct × pool` are rejected with `truth_state=ENFORCED`.
  Three truth states are logged: `ENFORCED` (charter governs), `FALLBACK_APPLIED`
  (charter present, no `surplus_reserves_pct` key), `UNSUPPORTED` (no view set).
- **Done (startup persistence — 2026-04-08):** The in-memory-only binding gap is
  closed. `GovernanceHandle::list_accepted_charter_proposals()` (in
  `apps/governance/src/actor.rs`) returns `Vec<(charter_id, charter_yaml)>` by
  scanning the Sled-backed governance store for accepted `Charter` proposals.
  `icn-core/src/supervisor/lifecycle.rs` calls this at startup (after governance
  actor init, before gateway starts) and re-invokes `charter_accepted_hook` for
  each recovered charter — rebinding both `EconomicPolicyView` and
  `SurplusPolicyView` on the ledger. Invariant D (live membership) and the
  `with_members(100)` stub are unchanged; the recovery path uses the same frozen
  context as the original ratification hook. `equity_range_*` and `settlement_*`
  keys remain orphaned — not part of this tranche.
- **Remaining:** `equity_range_*` and `settlement_*` keys are still orphaned.
  These are produced by `charter_to_constraints()` but have no consumer wiring.

### Phase 4 — Name CharterValidator stubs ✅ COMPLETE
- `evaluate_rule_basic()` now returns `ValidationResult::deferred()` not `pass()`
- New `RuleStatus` enum: `Pass`, `Fail`, `Deferred` — deferred ≠ pass
- `has_deferred()` method allows callers to detect incomplete evaluation
- All tests updated to use method calls (`r.passed()`) not field access

---

## Consequences

**Positive:**
- CCL's two roles are explicitly bounded, preventing scope creep
- The production enforcement gap is named and tracked, not silently accepted
- Custom key consumers can be implemented incrementally against a known spec

**Negative:**
- Phase 3 (key consumption) requires changes across governance execution and
  the ledger layer — non-trivial

**Neutral:**
- CCL contract executor (Role 1) is unaffected by this ADR — it operates
  correctly for bilateral agreement execution

---

## Related

- ADR-0014: Stewardship Semantics Boundary
- ADR-0015: Semantic Architecture Invariants
- `docs/architecture/ccl-architecture.md` — full architecture review
- `crates/icn-ccl/src/schema/bridge.rs` — `charter_to_constraints()`
- `crates/icn-ccl/src/charter_validator.rs` — evaluation stub
- `bins/icnd/src/main.rs:274` — hardcoded `with_members(100)`
