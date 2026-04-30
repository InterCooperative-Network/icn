---
title: "CCL Architecture — Source of Truth"
status: "current"
date: "2026-04-07"
audience: "contributors, reviewers, grant readers"
---

# CCL Architecture — Source of Truth

> This document is the canonical reference for what CCL is in ICN, what it
> owns, what it must not own, and where the current gaps are. It is grounded
> in actual repo state, not aspiration.

---

## What CCL Is (and Is Not)

CCL is ICN's **institutional rule-expression layer**. It gives cooperatives a
structured way to encode their governance logic, economic agreements, and
inter-coop relationships in a form the system can interpret and enforce.

**CCL IS:**
- An expression language for cooperative constitutions (charter YAML documents)
- A contract executor for explicit bilateral agreements between parties
- The Meaning Firewall boundary for charter semantics → kernel constraints
- The place where "supermajority" becomes a number the kernel enforces blindly

**CCL is NOT:**
- A general-purpose smart contract virtual machine
- A replacement for governance decisions (votes are governance; CCL interprets
  the rules that govern votes)
- A blockchain transaction language with gas economics
- A mechanism for arbitrary state mutation
- A dumping ground for "not yet implemented" institutional logic
- Speculative token or staking logic
- A system that operates outside the kernel/app separation

---

## The Two Runtime Roles of CCL

The `icn-ccl` crate serves two distinct purposes. They share a crate but
operate at different abstraction levels.

### Role 1: Contract Executor (AST / Interpreter / Runtime)

**What it does:** Evaluates bilateral CCL contracts against `icn-ledger`
state. A cooperative agreement can be encoded as a CCL contract and executed
deterministically, producing an `ExecutionReceipt`.

**Key components:**
- `ast.rs` — Contract, Rule, Stmt, Expr, Value AST nodes
- `interpreter.rs` — Tree-walking evaluator with capability checks
- `runtime.rs` — `ContractRuntime`: wires the interpreter to the ledger
- `fuel_estimator.rs` — Fuel metering (not Turing-complete by design)
- `registry.rs`, `registry_actor/` — Contract registry (store + retrieve)

**Execution model:**
```
CCL contract source
       ↓
AST parse (icn-ccl::ast)
       ↓
Interpreter + capability check (ReadLedger, WriteLedger, ReadTrust)
       ↓
Fuel metered execution against ContractRuntime
       ↓
ExecutionReceipt { contract_hash, inputs, outputs, fuel_used }
```

**Current production status:** Contract executor is implemented. Whether
production workloads actively use it for bilateral agreement enforcement is
not documented in the repo.

### Role 2: Charter→ConstraintSet Bridge (schema / bridge)

**What it does:** Translates a `CclDocument` (a YAML-encoded cooperative
constitution) into a `ConstraintSet` that the kernel can enforce blindly.
This is the **Meaning Firewall boundary for charters**.

**Key components:**
- `schema/mod.rs` — `CclDocument` YAML schema (governance, economics, agreement)
- `schema/governance.rs` — `GovernanceSchema`: decision types, bodies, thresholds, delegation
- `schema/economics.rs` — `EconomicsSchema`: capital, credit, surplus allocation
- `schema/agreement.rs` — `AgreementSchema`: federation boundaries, dispute ladder, exit
- `schema/bridge.rs` — `charter_to_constraints()`: the Meaning Firewall boundary
- `schema/expr.rs` — Expression evaluator for charter field expressions

**Translation model:**
```
CclDocument (charter YAML)
       ↓
charter_to_constraints(doc, ctx)
       ↓
ConstraintSet with custom keys:
  - min_votes_ordinary = 0.5
  - min_quorum_ordinary = 25.0
  - credit_limit = 400.0
  - surplus_reserves_pct = 0.20
  - body_board_seats = 7
  - settlement_cycle = "weekly"
  - dispute_stages = ["negotiation", "mediation", "arbitration"]
  ...
```

`CharterContext` provides runtime data (member count, patronage, trust score)
so expression fields like `"0.25 * members"` or `"min(1000, patronage * 0.5)"`
can be evaluated.

**Current production status:** `charter_to_constraints()` is implemented,
correct, and well-tested (24 tests covering governance, economics, agreement,
and expression evaluation). The production gap is at the **consumption edge**
— see below.

---

## The Meaning Firewall Boundary

```
┌──────────────────────────────────────────────────────────────────┐
│  COOPERATIVE SEMANTICS (CCL layer)                               │
│                                                                  │
│  "supermajority threshold"  →  0.667                             │
│  "credit limit = min(1000, patronage * 0.5)"  →  400.0          │
│  "20% to reserves until 6mo operating"  →  0.20 + condition     │
│  "weekly net settlement"  →  "weekly" + "net_settlement"         │
│                                                                  │
│  charter_to_constraints(doc, ctx)                                │
├──────────────────────────────────────────────────────────────────┤
│  MEANING FIREWALL                                                │
│  (kernel never imports icn-ccl; never reads charter semantics)  │
├──────────────────────────────────────────────────────────────────┤
│  KERNEL LAYER                                                    │
│                                                                  │
│  ConstraintSet { rate_limit, credit_multiplier, custom: {...} }  │
│  enforced blindly — no cooperative semantics visible here        │
└──────────────────────────────────────────────────────────────────┘
```

`charter_to_constraints()` is the only function that should cross this
boundary for charter semantics. Nothing downstream of it should re-implement
the translation.

---

## Production Wiring (CharterPolicyOracle)

In the daemon (`bins/icnd/src/main.rs`), `CharterPolicyOracle` is constructed
and registered as a PolicyOracle:

```rust
// Actual wiring (simplified from icnd/src/main.rs:260-286)
let charter_oracle = CharterPolicyOracle::new(
    CharterContext::new().with_members(100),  // ← hardcoded
    &charter_doc,
);
supervisor.register_oracle("charter", Arc::new(charter_oracle));
```

The oracle calls `charter_to_constraints()` at evaluation time and returns the
`ConstraintSet` to the kernel. The ConstraintSet's standard fields
(`rate_limit`, `trust_score`) are consumed by the kernel. Most custom keys are
now consumed at specific decision boundaries; see Gap 1 below.

---

## Known Production Gaps

These are explicitly named, not silently missing.

### Gap 1: Custom ConstraintSet keys — PARTIALLY CLOSED

**Status:** Three consumption seams now closed: `min_votes_ordinary` /
`min_quorum_ordinary` (governance vote evaluation), `credit_limit`
(ledger credit extension), and `surplus_reserves_pct` (treasury distribution
gate — 2026-04-08). `equity_range_*` and `settlement_*` remain orphaned.

**What is now enforced (governance):** When `GovernanceActor::CloseProposal`
evaluates a vote, it consults the charter oracle as a **second fallback**
(after protocol params, before domain config defaults) via
`GovernanceActor::get_thresholds_from_charter()`. If a charter is deployed for
the domain being voted in, its `min_votes_ordinary` and `min_quorum_ordinary`
keys are read and applied as the threshold.

**Wiring path (governance):**
```
CloseProposal handler
  → get_thresholds_from_params()  [protocol params — first priority]
  → get_thresholds_from_charter() [charter oracle — second priority]
  → domain.config.thresholds_for_proposal() [defaults — last resort]
```

The charter oracle is injected via `BootstrapHandles.charter_oracle` →
`GovernanceDeps.charter_oracle` → `init_governance_services()` →
`GovernanceHandle::with_charter_oracle()`.

**What is now enforced (economics):** `Ledger::process_entry()` consults a
charter-derived credit limit via the typed `EconomicPolicyView` adapter
**before** falling back to `dynamic_limit_manager` or `credit_policy_manager`.
When the charter defines a `credit_limit` expression, the ledger evaluates it
per-member (using `cleared_volume` as the patronage proxy and the member's
trust score) and uses that as the credit cap. A `Some(limit)` return is
**ENFORCED**; a `None` return is **FALLBACK_APPLIED** to the static chain.

**Wiring path (economics):**
```
process_entry credit-limit gate
  → charter_economic_view.credit_limit_for(...) [charter — ENFORCED]
  → dynamic_limit_manager.get_effective_limit() [dynamic — FALLBACK_APPLIED]
  → credit_policy_manager.calculate_static_credit_limit() [static — FALLBACK_APPLIED]
```

The `EconomicPolicyView` trait lives in `icn-ledger::credit_policy` and is
implemented by `apps/charter::CharterPolicyOracle`. The view is injected via
the daemon's `charter_accepted_hook`: when a charter is ratified through
governance, the hook deploys the doc into the oracle and schedules a tokio
task to bind the oracle as the ledger's `charter_economic_view`.

**Truth state taxonomy** (logged at `tracing::debug` from `process_entry`):

| State | Meaning | Trigger |
|---|---|---|
| `ENFORCED` | Charter `credit_limit` governs the account | View returns `Some(limit)` |
| `FALLBACK_APPLIED` | Charter has no economic policy | View returns `None`, static chain runs |
| `UNSUPPORTED` | No charter view configured | `set_charter_economic_view()` never called |

**What is now enforced (surplus reserves — 2026-04-08):** `LedgerServiceImpl::submit_treasury_entry()`
gates `DistributeSurplus` operations against the charter-derived `surplus_reserves_pct`.
Before appending the journal entry, it reads the treasury balance, computes
`distributable_pool = (-treasury_balance).max(0)` (ICN mutual-credit: treasury
surplus is represented as a NEGATIVE balance), and enforces
`req.amount <= pool * (1 - reserves_pct)`. The typed `SurplusPolicyView` trait
lives in `icn-ledger::credit_policy`; `CharterPolicyOracle::reserves_pct_for()`
implements it in `apps/charter`.

**Wiring path (surplus reserves):**
```
submit_treasury_entry (DistributeSurplus only)
  → ledger.charter_surplus_view_and_id()
  → view.reserves_pct_for(charter_id)     [ENFORCED if Some]
  → distributable_pool * (1 - pct) check  [FALLBACK_APPLIED if None]
  → proceed or return Err                  [UNSUPPORTED if no view]
```

Binding: `charter_accepted_hook` (in `bins/icnd/src/main.rs`) calls
`ledger.set_charter_surplus_view()` on Charter ratification.

**Startup recovery (2026-04-08):** The in-memory restart gap is closed.
`icn-core/src/supervisor/lifecycle.rs` calls
`governance_handle.list_accepted_charter_proposals()` at startup (after
governance actor init, before gateway). This scans the Sled-backed proposal
store and re-invokes `charter_accepted_hook` for each accepted Charter
proposal, rebinding both `EconomicPolicyView` and `SurplusPolicyView` on the
ledger before the first transaction is processed.

`GovernanceHandle::list_accepted_charter_proposals()` (in
`apps/governance/src/actor.rs`) returns plain `Vec<(String, String)>` —
no `icn_governance::` references cross into `icn-core`, keeping the
governance ratchet at zero.

Truth states after restart: `ENFORCED` if ≥1 accepted charter recovered;
`UNSUPPORTED` if governance store is empty or unreadable.
Recovery is idempotent: each restart replays the same hook pipeline.

**What remains orphaned:** `equity_range_*` and `settlement_*`. These keys are
produced by `charter_to_constraints()` but have no consumer wiring. See ADR-0016 Phase 3.

### Gap 2: CharterContext member count — PARTIALLY CLOSED

**Location:** `bins/icnd/src/main.rs` (charter ratification hook) and
`apps/charter/src/oracle.rs` (evaluate method)

**What is now improved:** `CharterPolicyOracle::evaluate()` reads an optional
`member_count` from request metadata. Callers that supply
`PolicyContext::with_metadata("member_count", &n.to_string())` get live
threshold evaluation. `GovernanceActor::get_thresholds_from_charter()` passes
the actual eligible voter count as this metadata.

`CharterPolicyOracle::thresholds_for(charter_id, decision_type, member_count)`
was added as an explicit query interface returning `(approval_ratio, quorum_count)`.

**What remains hardcoded:** The charter ratification hook in `bins/icnd/src/main.rs`
(line ~274) still uses `CharterContext::new().with_members(100)` when
deploying a charter. This is the startup-time frozen context, separate from the
request-time live context. The frozen context affects the stored ConstraintSet
snapshot; the live member count path overrides it at query time.

**Resolution:** The frozen context should use the actual member count from the
membership service when deploying. Lower priority than the query-time path.

### Gap 3: CharterValidator — HONEST STUB (not a silent pass)

**Location:** `crates/icn-ccl/src/charter_validator.rs`

**What changed:** `evaluate_rule_basic()` now returns `ValidationResult::deferred()`
instead of `ValidationResult::pass()`. A new `RuleStatus` enum explicitly
distinguishes `Pass`, `Fail`, and `Deferred`. The validator's `has_failures()`
method excludes deferred results — deferred rules do not cause failures.

**What is now true:**
- `ValidationResult::is_deferred()` — rules that haven't been evaluated are
  labelled, not silently treated as passing
- `has_deferred()` — callers can detect that validation was incomplete
- Transaction-level charter rules (min/max amounts, restricted currencies) are
  still not enforced, but the non-enforcement is explicit and detectable

**What remains incomplete:** Full CCL interpreter wiring for transaction-level
rules. Not a product requirement until ledger-boundary charter enforcement is
scoped.

---

## What CCL Should Own (Going Forward)

| Concern | Owned by CCL | Notes |
|---------|-------------|-------|
| Vote threshold expressions | Yes | via charter YAML + bridge |
| Quorum expressions | Yes | via charter YAML + bridge |
| Credit limit formulas | Yes | via charter YAML + bridge |
| Surplus allocation rules | Yes | via charter YAML + bridge |
| Inter-coop agreement terms | Yes | via agreement schema |
| Dispute resolution ladder | Yes | via agreement schema |
| Exit notice periods | Yes | via agreement schema |
| Bilateral contract execution | Yes | via AST/interpreter/runtime |
| Governance vote decisions | No | those are proposals + kernel effects |
| Membership record mutations | No | those are governance effects |
| Identity / steward state | No | those are SDIS governance effects |
| Treasury balance management | No | that is governed accounting |
| Generic state machine logic | No | that belongs in domain crates |

---

## CCL Composition with Other Layers

### CCL + Governance

Governance votes determine WHAT happens (a Spend, an AppointSteward, a
ConfigChange). CCL charter rules determine HOW votes are structured (what
threshold, what quorum, what delegation rules apply). The sequence is:

```
Charter ratification (governance vote)
       ↓
CclDocument stored in CommonsStore
       ↓
CharterPolicyOracle loads CclDocument
       ↓
charter_to_constraints() → ConstraintSet
       ↓
Governance vote evaluated against ConstraintSet.custom["min_votes_*"]
       ↓
Vote accepted/rejected based on charter-derived threshold
```

Step 5 is now **partially wired**: `GovernanceActor::get_thresholds_from_charter()`
implements this path for `min_votes_ordinary` and `min_quorum_ordinary`.
Protocol parameter overrides still take priority; charter thresholds are the
second fallback. Domain config defaults are last resort.

The remaining gap: non-ordinary decision types (`supermajority`, `constitutional`)
use the same fallback chain but require the charter to export
`min_votes_supermajority` / `min_quorum_supermajority` keys — which
`charter_to_constraints()` does produce if the charter document declares them.

### CCL + Economics

Charter economic rules (credit limits, surplus allocation, equity ranges)
should constrain ledger operations. The sequence:

```
Credit extension request
       ↓
CharterPolicyOracle → ConstraintSet.custom["credit_limit"]
       ↓
Ledger checks: requested_amount <= credit_limit
       ↓
Accept or reject extension
```

The current gap: the ledger's credit extension logic does not read
`credit_limit` from the ConstraintSet.

### CCL + SDIS / Trust

Steward appointment and mandate conditions can be expressed in CCL at the
policy layer (not the kernel layer). The charter's governance rules define
who can appoint stewards and what quorum is required — that feeds into
governance vote thresholds, not into steward state directly.

Trust score thresholds in CCL expressions (e.g., `"trust_score >= 0.4"`)
are charter-level eligibility conditions, not kernel enforcement rules.
The `CharterContext.with_trust_score()` binding exists for this purpose.

### CCL + Federation

The `agreement` schema section encodes inter-cooperative agreements:
settlement cycles, dispute ladders, exit conditions. These feed into
`FederationService` configuration via the same ConstraintSet bridge.

The `BilateralClearingAgreement` terms (settlement_interval, max_imbalance)
are currently set by governance proposal. In the future, these could be
derived from a CCL agreement document and consumed by the federation clearing
engine.

---

## What Good CCL Coverage Looks Like

When the production gaps are closed, the following should be true:

1. **Charter controls governance thresholds.** A cooperative that ratifies a
   charter with `threshold: supermajority` for constitutional decisions should
   see those decisions fail at 51% approval. The charter-derived threshold
   should be the authoritative source.

2. **Charter controls credit limits.** A member requesting credit beyond the
   charter formula should be denied by the ledger layer, with a rejection
   referencing the constraint source.

3. **CharterContext reflects live membership.** A cooperative with 200 members
   should evaluate quorum expressions differently than one with 20 members.

4. **Custom keys have documented consumers.** Every key produced by
   `charter_to_constraints()` maps to a component that reads it. The mapping
   is documented and tested.

5. **CharterValidator stubs are named.** Any rule path that doesn't evaluate
   is annotated `// STUB` and appears in the gap inventory.

---

## Implementation Roadmap

### High Priority (unblocked)

**Close CharterContext hardcoding** — 1 day
- Bind `CommonsHandle::member_count()` into oracle construction
- Pass actual member count at oracle request time, not at daemon startup

**Document custom key consumers** — 0.5 days
- Map each custom key family to its intended consumer
- Update bridge.rs with per-key consumer documentation

### Medium Priority (needs cross-layer work)

**Wire governance threshold consumption** — 2-3 days
- Governance vote approval logic reads `min_votes_*` from ConstraintSet
- Governance vote quorum check reads `min_quorum_*`
- Integration test: charter-derived threshold actually gates approval

**Wire credit limit consumption** — 2-3 days
- Ledger credit extension reads `credit_limit` from ConstraintSet
- Integration test: charter-derived credit limit blocks over-extension

### Lower Priority (charter enforcement completeness)

**CharterValidator stub removal** — 3-5 days
- Evaluate transaction rules against actual transaction data
- Only needed when transaction-level charter enforcement is a product requirement

**Agreement schema → FederationService** — 3-5 days
- CCL agreement settlement terms drive `BilateralClearingAgreement` config
- Requires FederationService to accept ConstraintSet settlement config

---

## Guardrails

### ADR-0016

See `docs/adr/ADR-0016-ccl-architecture-boundary.md`. Invariants:
- A: CCL does not own generic state mutation
- B: `charter_to_constraints()` is the only Meaning Firewall boundary for charters
- C: Custom ConstraintSet keys must be consumed (documented consumer required)
- D: CharterContext must reflect live membership
- E: CharterValidator stubs must be named

### Naming guidance

| Wrong | Right |
|-------|-------|
| "smart contract" | "cooperative agreement" or "CCL contract" |
| "on-chain execution" | "ledger-linked contract execution" |
| "gas metering" | "fuel metering" |
| "governance is CCL" | "governance uses CCL-derived constraints" |
| "CCL enforces votes" | "CCL expresses thresholds; governance enforces votes" |

### What does NOT belong in CCL

- Staking or bond logic (no financial collateral in CCL)
- Token minting or burning
- Validator selection logic
- Cross-chain asset transfers
- Any logic that would make CCL look like a blockchain VM

---

## Canonical Implementation Files

| Concern | File |
|---------|------|
| CCL charter schema | `crates/icn-ccl/src/schema/mod.rs` |
| Governance schema | `crates/icn-ccl/src/schema/governance.rs` |
| Economics schema | `crates/icn-ccl/src/schema/economics.rs` |
| Agreement schema | `crates/icn-ccl/src/schema/agreement.rs` |
| Meaning Firewall boundary | `crates/icn-ccl/src/schema/bridge.rs` |
| Expression evaluator | `crates/icn-ccl/src/schema/expr.rs` |
| Contract AST | `crates/icn-ccl/src/ast.rs` |
| Contract interpreter | `crates/icn-ccl/src/interpreter.rs` |
| Contract runtime | `crates/icn-ccl/src/runtime.rs` |
| Charter validator (stub) | `crates/icn-ccl/src/charter_validator.rs` |
| Daemon oracle wiring | `bins/icnd/src/main.rs:260-286` |
| ADR | `docs/adr/ADR-0016-ccl-architecture-boundary.md` |
