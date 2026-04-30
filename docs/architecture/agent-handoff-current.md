---
title: "Agent Handoff — Current State"
status: "current"
date: "2026-04-08"
audience: "next Claude Code agent (cold start)"
supersedes: []
---

# Agent Handoff — Current State

> **Read this document first if you are starting a new session on ICN.**
> It is written for a staff-engineer-level agent entering with no prior
> conversational context. Everything here is anchored to files in this repo.
> Prose claims you cannot locate in code are false — update this doc instead.

---

## 0. Critical First Facts (read before touching anything)

1. **The working tree has ~45 uncommitted files and ~8 untracked new docs.**
   Large accumulated tranche spanning steward/SDIS semantic correction,
   governance execution truthfulness, CCL architecture clarification, and
   CCL runtime consumption. Run `git status --short` before doing anything.
   Do NOT `git reset` or `git stash` without reading this doc first.

2. **This handoff doc and most of the recent architecture docs are themselves
   uncommitted.** They live in the working tree, not in `main`. The next
   commit should include them.

3. **ICN is a governable institutional operating system. Not a blockchain.
   Not a DeFi protocol. Not a smart-contract VM.** If you find yourself
   reaching for staking, slashing, liquidity, yield, minting, or gas mental
   models, stop and read ADR-0014 and ADR-0015.

4. **Stewardship is a trust/mandate office, not a bonded role.** The previous
   tranche removed all bond/slash semantics from stewardship. See ADR-0014.
   If you see any new `bond_amount` field on a steward type, that is a
   regression — reject it.

5. **Governance must be executable or explicitly record-only, never silently
   dropped.** Every accepted proposal produces a `KernelEffect` — which may
   be `NoOp { reason }` with an explicit justification. Silent pass-through
   is an ADR-0015 Invariant C violation.

6. **The meaning firewall is enforced by CI.** Kernel crates
   (`icn-core`, `icn-gateway`, `icn-gossip`, `icn-net`) must not import
   domain crates (`icn-trust`, `icn-governance`, `icn-ccl`). The
   "Meaning Firewall Check" gate in `.github/workflows/ci.yml` is required
   and blocking.

7. **Repo topology has two roots.** Monorepo root is `/home/ubuntu/projects/icn`.
   Rust workspace is `/home/ubuntu/projects/icn/icn`. Cargo commands run from
   the workspace root. Docs live at the monorepo root under `docs/`.

---

## 1. Read-First List (in order, ~90 minutes)

| # | File | Why |
|---|------|-----|
| 1 | `docs/architecture/agent-handoff-current.md` | This doc — orientation |
| 2 | `docs/architecture/icn-operating-model.md` | How the layers actually compose (309 lines) |
| 3 | `docs/adr/ADR-0015-semantic-architecture-invariants.md` | The 6 invariants you must preserve |
| 4 | `docs/adr/ADR-0014-stewardship-semantics-boundary.md` | Why stewardship has no bond/slash |
| 5 | `docs/adr/ADR-0016-ccl-architecture-boundary.md` | What CCL is and isn't |
| 6 | `docs/architecture/governance-execution-inventory.md` | Every proposal variant's classification |
| 7 | `docs/architecture/ccl-architecture.md` | Custom key consumption state (476 lines) |
| 8 | `docs/sdis/steward-lifecycle.md` | Full steward state machine with kernel effects |
| 9 | `icn/crates/icn-kernel-api/src/effects.rs` | Canonical `KernelEffect` enum |
| 10 | `icn/crates/icn-governance/src/sdis.rs` | `StewardPenalty` enum — canonical steward sanctions |

After this list, you have the full strategic context for current work.

---

## 2. Executive Summary

ICN has just completed a multi-tranche semantic and execution-truthfulness
hardening pass. Three arcs landed simultaneously:

1. **Steward/SDIS semantic correction** — stewardship is now unambiguously a
   trust/mandate office. All bond/slash semantics are removed. The
   `SanctionSteward` variants are status/authority changes only.

2. **Governance execution truthfulness** — ~23 proposal types now wire to
   real kernel effects, 9 are explicitly record-only, 4 are recognized but
   deferred pending service-layer work, and 5 fail-fast with explicit gap
   messages. The "silently drop" case is eliminated.

3. **CCL architecture clarification + consumption closures** — CCL's two
   runtime roles (contract executor vs. charter→constraints bridge) are
   explicitly bounded. Charter `credit_limit` expressions are now consumed
   by the ledger via the typed `EconomicPolicyView` adapter. Charter
   validator moved from dishonest pass-through to explicit
   `Pass`/`Fail`/`Deferred`. Ordinary charter thresholds now drive governance
   vote evaluation.

The repo posture is **architecturally coherent, with named remaining gaps**.
The remaining work is finite and enumerable — see §8 for the next tranches.

---

## 3. What Changed Recently (last ~2 weeks)

### Arc A — Steward/SDIS Semantic Correction
**Commits landed on main:**
`d2eb2257` (Tranche 14 ReconfirmSteward), `66c5355b` (Tranche 15 ReinstateSteward),
`71bce39f` (Tranche 16 SuspendSteward), `eff41c5f` (Tranche 13 real SdisService executor)

**ADR:** `docs/adr/ADR-0014-stewardship-semantics-boundary.md`
**Canonical doc:** `docs/sdis/steward-lifecycle.md`

**What was removed** (see ADR-0014 for the full list):
- `StewardPenalty::BondSlash { amount: i64 }` variant
- `StewardRecord.bond_amount` field
- `CommonsHandle::add_steward_bond()` and `slash_steward_bond()`
- Gateway endpoints `POST /v1/steward/{id}/bond/add` and `/bond/slash`
- All bond fields from `AppointStewardRequest`, `RegisterStewardRequest`

**What was added:**
- `StewardPenalty::Censure` (unit variant, institutional record only)
- `Sdis::SuspendSteward`, `Sdis::ReinstateSteward`, `Sdis::ReconfirmSteward` effects
- `Sdis::UpdateJurisdictionTier` effect (deferred executor)
- Real `SdisServiceImpl` wired to `CommonsHandle` via governance executor
- Test `icn/crates/icn-governance/tests/steward_semantic_invariants.rs`
- `test_steward_penalty_variants` guard — any re-introduction of bond fields breaks

**Code anchors:**
- `icn/crates/icn-governance/src/sdis.rs` — `StewardPenalty` enum (canonical)
- `icn/crates/icn-governance/src/steward.rs` — `StewardRecord` (no bond fields)
- `icn/crates/icn-commons/src/handle.rs` — `CommonsHandle` API (no bond methods)
- `icn/crates/icn-kernel-api/src/effects.rs` — `SdisEffect` variants
- `icn/crates/icn-core/src/services/sdis_service.rs` — `SdisServiceImpl`
- `icn/apps/governance/src/handlers/execution.rs` — `translate_payload_to_effects` SDIS arm
- `icn/crates/icn-core/src/supervisor/governance_executor.rs` — `DefaultEffectExecutor::execute_sdis`

### Arc B — Governance Execution Truthfulness
**Commits landed on main (Tranches 9-16, plus the suspension gates):**
`c086dfe7`, `92915954`, `bc34393f`, `8a4ceb24`, `a7e9077a`, `ce63932e`,
`eff41c5f`, `d2eb2257`, `66c5355b`, `71bce39f`, `54c421e4`, `1b96da95`,
`aa5b7589`, `cdbb4e44`, `6b8d2c5b`, `8bb55b44`

**Canonical inventory:** `docs/architecture/governance-execution-inventory.md`

**What changed:**
- `translate_payload_to_effects()` now handles ALL `ProposalPayload` variants
  explicitly. No more catch-all `Err` that hides unhandled types.
- Four classifications are used consistently:
  `FULLY WIRED` (29) / `PARTIALLY WIRED` (2) / `RECORD-ONLY NoOp` (9) /
  `DEFERRED` (4) / `FAIL-FAST` (5).
- `Charter` proposals now produce `Protocol::SetGovernanceConfig`, closing
  the loop from ratification to oracle deployment.
- `TransferBetweenBudgets` now produces `Treasury::Transfer`.
- `BondIssuance` and `ShareRedemption` wired to real treasury mutation.
- `JoinFederation`, `LeaveFederation`, `EstablishClearing`, `VouchForCoop`
  fully wired.
- `FreezeMember` enforcement is synchronous in the ledger path.
- Suspended members are excluded from proposal creation, voting, delegation,
  and delegation expansion at close time.

**Code anchors:**
- `icn/apps/governance/src/handlers/execution.rs` — `translate_payload_to_effects()`
- `icn/crates/icn-core/src/supervisor/governance_executor.rs` — `DefaultEffectExecutor`
- `icn/crates/icn-kernel-api/src/effects.rs` — `KernelEffect` / `TreasuryEffect` / `SdisEffect` / `FederationEffect` / `ControlEffect`
- `icn/crates/icn-kernel-api/src/governance.rs` — `ProposalPayload` variants
- `icn/crates/icn-core/tests/emitted_surface_contract.rs` — contract enforcement test

### Arc C — CCL Architecture Clarification
**ADR:** `docs/adr/ADR-0016-ccl-architecture-boundary.md` (accepted, phases 2-4 partial)
**Canonical doc:** `docs/architecture/ccl-architecture.md`

**What it defines:**
- CCL Role 1: **Contract Executor** (AST/interpreter) — multi-party
  agreement execution against ledger state. Deterministic. Fuel-metered.
  Produces `ExecutionReceipt`.
- CCL Role 2: **Charter→ConstraintSet Bridge** — translates YAML charter
  documents into `ConstraintSet` for the kernel. Read-only. No side effects.
  Via `charter_to_constraints()` in `icn-ccl/src/schema/bridge.rs`.
- Explicit non-scope: CCL is NOT a smart-contract VM, NOT a token engine,
  NOT a place to encode single-coop internal policy that belongs in the kernel.

**Four numbered production gaps** are named in the doc (Gaps 1-4). Gap 1
(custom key consumption) is now largely closed: governance thresholds,
`credit_limit`, and `surplus_reserves_pct` are all consumed. `equity_range_*`
and `settlement_*` remain orphaned. Gap 2 (`CharterContext.members=100`
hardcoding) remains the main open caveat.

### Arc D — CCL Runtime Consumption Closures
**Part of ADR-0016 Phases 2, 3, 4 — partially implemented.**

**Phase 2 (governance thresholds) — CLOSED:**
- `GovernanceActor::get_thresholds_from_charter()` added
- Reads `min_votes_ordinary` and `min_quorum_ordinary` from charter oracle
- Used as second-priority fallback in `CloseProposal` after protocol params
- Wired via `BootstrapHandles.charter_oracle` → `GovernanceDeps.charter_oracle`
  → `GovernanceHandle::with_charter_oracle()`
- Live `member_count` passed via `PolicyContext::with_metadata("member_count", …)`
- `CharterPolicyOracle::thresholds_for(charter_id, decision_type, member_count)`
  added as explicit query interface

**Phase 3 (economics — `credit_limit`) — CLOSED (this session):**
- `icn-ledger::credit_policy::EconomicPolicyView` trait added — typed adapter
- `Ledger::process_entry()` consults charter view before dynamic/static cascade
- Three-level precedence: charter → dynamic limit manager → static credit policy
- Truth states logged: `ENFORCED` / `FALLBACK_APPLIED` / `UNSUPPORTED`
- `apps/charter::CharterPolicyOracle::credit_limit_for()` implements the trait
- Daemon `charter_accepted_hook` spawns tokio task to bind oracle to ledger
- 6 new tests (3 in ledger, 3 in charter-app) prove the consumption path

**Phase 3 (economics — `surplus_reserves_pct`) — CLOSED (2026-04-08):**
- `icn-ledger::credit_policy::SurplusPolicyView` trait added — typed adapter
- `LedgerServiceImpl::submit_treasury_entry()` gates `DistributeSurplus` before
  appending journal entry; checks `distributable_pool = (-treasury_balance).max(0)`
- ICN mutual-credit accounting: treasury surplus is a NEGATIVE balance; `debit`
  during distribution brings it toward zero
- Three truth states logged: `ENFORCED` / `FALLBACK_APPLIED` / `UNSUPPORTED`
- `apps/charter::CharterPolicyOracle::reserves_pct_for()` implements the trait
- Daemon `charter_accepted_hook` binds oracle to ledger via `set_charter_surplus_view()`
- 3 new tests in icn-core + 3 in icn-charter-app prove the enforcement path

**Phase 3 (startup persistence) — CLOSED (2026-04-08):**
- `GovernanceHandle::list_accepted_charter_proposals()` added to
  `apps/governance/src/actor.rs` — returns `Vec<(charter_id, charter_yaml)>` by
  scanning the Sled-backed governance store; no `icn_governance::` types cross
  into `icn-core` (governance ratchet stays at 0)
- `icn-core/src/supervisor/lifecycle.rs` calls this at startup (after governance
  actor init, before gateway) and re-invokes `charter_accepted_hook` for each
  recovered charter — rebinding both `EconomicPolicyView` and `SurplusPolicyView`
- truth_state=`ENFORCED` from first transaction after restart; `UNSUPPORTED` only
  if governance store is empty/unreadable
- 3 new tests in `apps/governance/tests/charter_recovery_proof.rs` prove:
  (1) accepted charters are returned, (2) non-charter/non-accepted excluded,
  (3) data survives sled close+reopen (the actual restart scenario)
- The `with_members(100)` frozen context gap (ADR-0016 Phase 2 Remaining) is
  unchanged; the recovery path uses the same context as the original ratification

**Phase 4 (CharterValidator honesty) — CLOSED:**
- `evaluate_rule_basic()` returns `ValidationResult::deferred()`, not `pass()`
- New `RuleStatus` enum: `Pass`, `Fail`, `Deferred`
- `has_deferred()` method lets callers detect incomplete evaluation
- Charter validation at transaction level still reports truth state, not
  dishonest pass-through

**Code anchors for Arc D:**
- `icn/crates/icn-ledger/src/credit_policy.rs` — `EconomicPolicyView` and `SurplusPolicyView` traits
- `icn/crates/icn-ledger/src/ledger.rs` — `process_entry()` cascade ~line 2895; `set_charter_surplus_view()`
- `icn/crates/icn-core/src/services/ledger_service.rs` — `submit_treasury_entry()` enforcement ~line 804
- `icn/apps/charter/src/oracle.rs` — `CharterPolicyOracle::credit_limit_for`, `reserves_pct_for`
- `icn/apps/governance/src/actor.rs` — `GovernanceHandle::list_accepted_charter_proposals()`
- `icn/crates/icn-core/src/supervisor/lifecycle.rs` — startup charter recovery block (after governance init)
- `icn/apps/governance/tests/charter_recovery_proof.rs` — 3 restart recovery proofs
- `icn/crates/icn-ccl/src/charter_validator.rs` — `RuleStatus`, `ValidationResult`
- `icn/crates/icn-ccl/src/charter_rules.rs`
- `icn/apps/governance/src/actor.rs` — `get_thresholds_from_charter()`
- `icn/bins/icnd/src/main.rs` — `charter_accepted_hook` wiring (both economic and surplus views)

---

## 4. Current Architecture Posture

### The 4 layers (see `icn-operating-model.md` for detail)

```
Governance  →  decision authority, proposal lifecycle, vote evaluation
Economics   →  governed double-entry accounting, treasury, clearing
Identity    →  SDIS, stewards, POP, recovery (trust-governed, not stake-governed)
Federation  →  scoped inter-coop coordination, bilateral clearing agreements
```

### The flow

```
Member action
  → Governance proposal submitted
  → Vote (suspension-gated: suspended members excluded)
  → Tally reaches quorum + approval (charter-derived thresholds consulted)
  → translate_payload_to_effects()
  → Vec<KernelEffect>
  → DefaultEffectExecutor executes each effect
  → EffectResult persisted
  → Audit chain: decision_hash → journal_entry → state_hash
```

### The kernel/app split

- **Kernel crates** (must not import domain crates):
  `icn-core`, `icn-kernel-api`, `icn-gateway`, `icn-gossip`, `icn-net`,
  `icn-ledger`, `icn-store`, `icn-time`, `icn-identity`, `icn-obs`
- **App crates** (may import domain, define policy, translate to constraints):
  `apps/governance`, `apps/ledger`, `apps/charter`, `apps/membership`,
  `apps/trust` (if still present)
- **Domain crates** (used by apps only):
  `icn-trust`, `icn-governance`, `icn-ccl`, `icn-coop`, `icn-community`,
  `icn-commons`, `icn-federation`, `icn-steward`, `icn-compute`, `icn-zkp`,
  `icn-privacy`

### Policy consumption pattern

Apps translate domain meaning into either:
1. `ConstraintSet` (for per-request decisions — `PolicyOracle::evaluate()`)
2. `Vec<KernelEffect>` (for governance dispatch — `translate_payload_to_effects()`)
3. **Typed adapter view** (for structured read at a specific decision boundary —
   new pattern, first instance is `EconomicPolicyView`)

The typed-adapter pattern is the preferred way forward when a decision
boundary needs structured, per-call policy lookup that's larger than a
`ConstraintSet` key but smaller than a full `KernelEffect` dispatch.

---

## 5. Subsystem Status Map

| Subsystem | Status | Why |
|---|---|---|
| **Governance dispatch** | STRONG | 29 wired, 9 record-only, 4 deferred, 5 fail-fast — all named. Suspension gating enforced at every entry point. |
| **Governance state machine** | STRONG | Charter thresholds consulted, protocol params override, domain defaults fallback. Tally → effect → audit is deterministic. |
| **CCL / policy** | PARTIAL | Two roles explicitly bounded. Governance thresholds + credit_limit + surplus_reserves_pct consumed. `equity_range_*` and `settlement_*` still orphaned. Charter validator truth-stated. |
| **Identity / SDIS** | MOSTLY REAL | Bond/slash removed. Appoint, Suspend, Reinstate, Reconfirm, Sanction all wired. `ModifyThreshold`, `ApproveAuthority`, `RevokeAuthority`, `RevocationAppeal`, `ForceKeyRotation` fail-fast (need design). |
| **Stewardship model** | STRONG | ADR-0014 semantic lock-in. Tests assert no bond fields. Authority flows from governance ratification + trust, not capital. |
| **Economics / ledger** | STRONG | Double-entry, provenance-hashed, Merkle-DAG. Charter-derived credit limits now consumed. Bond issuance, share redemption, surplus distribution all wired. |
| **Treasury** | STRONG | Spend, CreateBudget, Allocate, Transfer, IssueBond, RedeemShares, DistributeSurplus all wired. CancelBudget/ReclaimBudget are intentionally record-only. |
| **Commons credit / settlement** | STRONG | Commons settlement dedup survives restart (tests prove it). |
| **Compute / commons** | PARTIAL | `dispute.rs` and `result_quorum.rs` modified but not fully reviewed in this tranche. Dispute outcome completeness is a known gap (`Partial` outcome only). |
| **Networking / gateway** | STRONG | QUIC/TLS, DID-TLS binding, trust-gated rate limits. No recent structural changes. |
| **Federation** | STRONG | ADRs 0011-0013 accepted. Join, Leave, EstablishClearing, VouchForCoop wired. TerminateClearing and RevokeVouch deferred. |
| **Observability / provenance** | STRONG | Every effect produces an `EffectResult`. Audit chain: decision_hash → journal_entry → state_hash is legible. Prometheus metrics on kernel paths. |
| **Member / operator surfaces** | PARTIAL | Gateway REST exists. icnctl CLI exists. Pilot UI (web/pilot-ui) exists but not re-verified this tranche. `gateway/src/api/steward/mod.rs` was gutted of 156 lines — verify it still exposes what pilot UI needs. |
| **Public surface / website** | NEEDS REVIEW | `website/` is absorbed into monorepo. Content may be behind ADR-0014/0015 framing. See §12. |

**Legend:**
- **STRONG / MOSTLY REAL** — code matches the architectural claims
- **PARTIAL** — core works, named gaps remain
- **FRAGILE** — works but has incomplete error handling or missing coverage
- **NEEDS REVIEW** — not verified in current tranche, may drift from docs

---

## 6. Truth-State Taxonomy

Use this language consistently when documenting the state of any path:

| State | Meaning | When to use |
|---|---|---|
| **STORED** | The data exists in a durable store | "The charter YAML is stored in the Sled store under key `charter:<id>`" |
| **TRANSLATED** | A domain concept has been converted to kernel primitives | "The payload was translated to `Vec<KernelEffect>`" |
| **CONSUMED** | The translated form is read by a decision boundary | "The `credit_limit` custom key is consumed by `Ledger::process_entry`" |
| **ENFORCED** | Consumption actually gates behavior at runtime | "Entries exceeding the charter limit are rejected. Logged `truth_state="ENFORCED"`" |
| **FALLBACK_APPLIED** | A prior layer returned nothing; a named fallback ran | "Charter has no `credit_limit`; static policy applied. Logged `truth_state="FALLBACK_APPLIED"`" |
| **DEFERRED** | The dispatch path is wired but the executor returns `not_executed=true` | "`UpdateJurisdictionTier` is recognized, translated, but CommonsHandle lacks the update method" |
| **RECORD-ONLY** | Accepted → `KernelEffect::NoOp { reason }` intentionally. The vote IS the act. | "`Warning` sanction produces no state change; the governance receipt is the institutional record" |
| **UNSUPPORTED** | Intentionally unhandled with explicit error | "`ModifyThreshold` returns `TranslationError::unsupported` — no kernel threshold registry exists yet" |
| **FAIL-FAST** | Subcategory of UNSUPPORTED: the error is raised immediately, not silently swallowed | "`ApproveAuthority` fails at translation time with an explicit message" |

Counter-examples (do not use):
- "Should work" — not a truth state
- "Looks good" — not a truth state
- "Silently passes" — if you find this, it is a bug, not a state

---

## 7. Architectural Invariants (DO NOT BREAK)

These are codified in ADR-0015 as Invariants A-F. Quick reference:

| # | Invariant | Canonical file |
|---|---|---|
| A | Stewardship is an institutional office. No bond. No slash. Authority from governance ratification + trust. | `icn-governance/src/sdis.rs`, `icn-governance/src/steward.rs`, ADR-0014 |
| B | Identity recovery is trust-governed. Recovery participant authority is NEVER capital-weighted. | `icn-zkp/`, `icn-steward/`, `docs/sdis/steward-lifecycle.md` |
| C | Governance must be executable or explicitly record-only. Never silently dropped. | `apps/governance/src/handlers/execution.rs`, `icn-core/tests/emitted_surface_contract.rs` |
| D | Economics is governed accounting. Every movement authorized by governance, expressed as journal entries, traceable to audit receipts. | `icn-ledger/src/ledger.rs`, `apps/ledger/`, ADR-0015 |
| E | Kernel/policy boundary holds. Kernel crates never import domain crates. | `.github/workflows/ci.yml` (Meaning Firewall Check), `icn/.claude/rules/kernel-boundary.md` |
| F | Federation is scoped institutional coordination with explicit bilateral agreements. Not permissionless decentralization. | `icn-federation/`, ADRs 0011-0013 |

**Red flags that indicate an invariant is being violated:**

- A new `bond_amount` field anywhere near a steward type → A
- A new recovery quorum weighted by balance → B
- `Ok(vec![])` for a non-trivially-record-only proposal → C
- Any balance transfer that bypasses `translate_payload_to_effects()` → D
- `use icn_trust::` or `use icn_governance::` inside a kernel crate → E
- "Permissionless" or "trustless" language in federation docs → F

---

## 8. Next Tranches (prioritized)

### Immediate next tranche (high value, low architectural risk)

**Tranche I: `surplus_reserves_pct` consumption at patronage settlement — ✅ CLOSED (2026-04-08)**
- **What was done:** Added `SurplusPolicyView` trait to `icn-ledger::credit_policy`.
  Implemented by `CharterPolicyOracle::reserves_pct_for()` in `apps/charter/src/oracle.rs`.
  Enforcement runs inside `LedgerServiceImpl::submit_treasury_entry()` for
  `DistributeSurplus` — before journal entry is appended, the distributable pool
  is checked against the charter-mandated reserves floor.
  Daemon wired via `charter_accepted_hook` in `bins/icnd/src/main.rs`.
  Three truth states: `ENFORCED` / `FALLBACK_APPLIED` / `UNSUPPORTED`.
  Tests in `icn-core`, `icn-ledger`, `icn-charter-app` all green.

**Tranche II: Charter view persistence across daemon restart — ✅ CLOSED (2026-04-08)**
- **What was done:** `GovernanceHandle::list_accepted_charter_proposals()` added to
  `apps/governance/src/actor.rs` scans the Sled governance store for accepted Charter
  proposals and returns `Vec<(charter_id, charter_yaml)>` (no domain types in the return).
  `icn-core/src/supervisor/lifecycle.rs` calls this at startup and re-invokes
  `charter_accepted_hook` for each recovered charter, rebinding `EconomicPolicyView`
  and `SurplusPolicyView` before the gateway starts.
  Truth state: `ENFORCED` from first transaction; `UNSUPPORTED` only if store is empty.
  3 tests in `apps/governance/tests/charter_recovery_proof.rs` prove the restart path.
  Governance ratchet stays at 0 (no `icn_governance::` refs in icn-core).
- **Remaining caveat:** `with_members(100)` frozen context unchanged — this is
  ADR-0016 Phase 2 Remaining, separate from startup persistence.

### Tranche III closed (2026-04-08)

**Tranche III: `UpdateJurisdictionTier` executor — ✅ CLOSED (2026-04-08)**
- **What was done:**
  - `StewardRecord` gained `jurisdiction_tier: Option<JurisdictionTier>` field
    (`#[serde(default)]` — old records deserialize safely as `None`).
    Added `StewardRecord::set_jurisdiction_tier()` mutator.
    File: `icn-governance/src/steward.rs`.
  - `CommonsHandle::update_jurisdiction_tier(steward_id, new_tier: &str)` added.
    Tier string parsed inside `icn-commons` (where `icn-governance` is available),
    then stored via read-mutate-write on Sled. Audit logged.
    Files: `icn-commons/src/inner.rs`, `icn-commons/src/handle.rs`.
  - `SdisService` trait got `update_jurisdiction_tier(UpdateJurisdictionTierRequest)
    → UpdateJurisdictionTierResult`. Request/result types added to `icn-kernel-api`.
    File: `icn-kernel-api/src/services.rs`.
  - `SdisServiceImpl::update_jurisdiction_tier` implemented: looks up steward by DID,
    calls `CommonsHandle::update_jurisdiction_tier`. No `icn_governance::` refs in
    `icn-core` — string passes the firewall boundary unchanged.
    File: `icn-core/src/services/sdis_service.rs`.
  - `governance_executor.rs` `SdisEffect::UpdateJurisdictionTier` arm replaced:
    was `not_executed: true`; now calls `service.update_jurisdiction_tier()`.
    File: `icn-core/src/supervisor/governance_executor.rs`.
  - E2E test stub updated (trait impl now complete).
    File: `icn-gateway/tests/e2e_institutional_flow.rs`.
  - `SdisEffect::UpdateJurisdictionTier` doc comment updated (no longer deferred).
    File: `icn-kernel-api/src/effects.rs`.
- **Tests:** 4 new in `sdis_service.rs`:
  `test_update_jurisdiction_tier_persists_durably` — happy path, tier appears in Sled.
  `test_update_jurisdiction_tier_idempotent` — re-applying same tier succeeds.
  `test_update_jurisdiction_tier_unknown_did_fails` — error on unknown DID.
  `test_update_jurisdiction_tier_invalid_tier_string_fails` — error on bad tier string.
- **Validation:** `cargo fmt --all -- --check` ✓, `cargo clippy -p icn-governance
  -p icn-commons -p icn-kernel-api -p icn-core -- -D warnings` ✓,
  `cargo test -p icn-core --lib` 381/381 pass ✓ (meaning firewall ratchet at 0),
  4/4 new UpdateJurisdictionTier tests pass ✓.
- **Architectural constraint preserved:** `icn_governance::` references in `icn-core/src/`
  remain 0. Tier parsing happens inside `icn-commons`. Test assertions use `Debug`
  formatting to avoid importing governance types into icn-core test code.

### Near-next tranches (Tranche IV is now the immediate priority)

**Tranche IV: `TerminateClearing` / `RevokeVouch` executors**
- **Why:** Currently `DEFERRED`. Federation lifecycle is asymmetric
  (`Join`/`Leave`/`Establish` work; `Terminate`/`Revoke` don't).
- **Seam closed:** Federation state machine completes.
- **Files:** `icn-federation/src/service.rs` (add `terminate_clearing`,
  `revoke_vouch`), `icn-core/src/supervisor/init_federation.rs` (wire),
  `icn-core/src/supervisor/governance_executor.rs` (remove `Deferred`).
- **Success:** Vote → clearing agreement removed → `ClearingPosition`
  archived → audit receipt.

### Important but lower priority

**Tranche V: `CharterContext.members=100` hardcoded**
- **Why:** Frozen deploy-time context. Query-time path handles live member
  count, but stored ConstraintSet snapshot uses 100. Cosmetic but misleading.
- **Files:** `bins/icnd/src/main.rs` (~line 274)
- **Success:** Charter deploy reads actual membership service for member count.

**Tranche VI: Non-ordinary charter decision types**
- **Why:** Governance actor only queries `min_votes_ordinary`/`min_quorum_ordinary`.
  `supermajority` and `constitutional` decision types exist but their charter
  thresholds are not consulted.
- **Files:** `apps/governance/src/actor.rs::get_thresholds_from_charter`,
  `apps/charter/src/oracle.rs::thresholds_for` (already parameterized by
  decision type), `icn-ccl/src/schema/bridge.rs` (verify key generation).
- **Success:** A charter with `governance.supermajority.min_votes = 0.67`
  actually raises the threshold for `DecisionType::Supermajority` proposals.

**Tranche VII: `DisputeResolution` outcome completeness**
- **Why:** Only `Outcome::Partial` currently computes compensation.
  `Uphold`, `Reject`, `VoidTransaction` produce empty compensation vectors.
- **Files:** `apps/governance/src/handlers/execution.rs` dispute arm,
  `icn-compute/src/dispute.rs`.

**Tranche VIII: `ModifyThreshold` / `ApproveAuthority` design**
- **Why:** Currently fail-fast. Requires a kernel-level authority/threshold
  registry that does not yet exist.
- **Needs ADR:** Define threshold registry schema, CCL bindings, enforcement
  model. This is design work, not implementation.

### Explicitly deferred (do not start without new instruction)

- **Test hardening sweep** (icn-commons has 0 tests, icn-naming has
  `unimplemented!()` panics, icnd/icn-console have 0 unit tests) — valuable
  but unrelated to the current architectural spine.
- **Multi-coop enforcement** (issue #769, 8 TODOs across RPC handlers) —
  still assumes single-coop in places. Needs its own tranche.
- **Grant application materials** — parallel workstream, not code.
- **Demo CLI flow (`five-minute-coop.sh`)** — high external-audience value
  but depends on surfaces (gateway + icnctl) being stable. Tranches I-IV
  should land first so the demo does not need retrofit.

---

## 9. Key Files and Directories

### Canonical type definitions
- `icn/crates/icn-kernel-api/src/effects.rs` — `KernelEffect`, `TreasuryEffect`,
  `SdisEffect`, `FederationEffect`, `ControlEffect`, `ProtocolEffect`,
  `DisputeEffect`, `MembershipEffect`, `ResourceEffect`
- `icn/crates/icn-kernel-api/src/governance.rs` — `ProposalPayload`,
  `DecisionType`, `ProposalOperation`
- `icn/crates/icn-kernel-api/src/authz.rs` — `PolicyOracle`, `PolicyRequest`,
  `PolicyDecision`, `ConstraintSet`, `ConstraintValue`
- `icn/crates/icn-governance/src/sdis.rs` — `StewardPenalty`, `SdisProposal`
- `icn/crates/icn-governance/src/steward.rs` — `StewardRecord`

### Translation and execution
- `icn/apps/governance/src/handlers/execution.rs` — `translate_payload_to_effects()`
  (this is the single biggest source of truth for "what does an accepted
  proposal do?")
- `icn/crates/icn-core/src/supervisor/governance_executor.rs` —
  `DefaultEffectExecutor` (dispatches effects to service layer)
- `icn/crates/icn-core/src/supervisor/init_governance.rs` — wiring
- `icn/crates/icn-core/src/services/sdis_service.rs` — SDIS service layer

### Economics and ledger
- `icn/crates/icn-ledger/src/ledger.rs` — main ledger, `process_entry()` at ~2800
- `icn/crates/icn-ledger/src/credit_policy.rs` — `EconomicPolicyView` trait,
  `CreditPolicyManager`
- `icn/apps/ledger/src/` — patronage, settlement, earnings, surplus

### CCL and charter
- `icn/crates/icn-ccl/src/schema/bridge.rs` — `charter_to_constraints()`
- `icn/crates/icn-ccl/src/charter_validator.rs` — `CharterValidator`,
  `RuleStatus`, `ValidationResult`
- `icn/crates/icn-ccl/src/charter_rules.rs` — rule evaluation
- `icn/apps/charter/src/oracle.rs` — `CharterPolicyOracle`,
  `credit_limit_for()`, `thresholds_for()`
- `icn/bins/icnd/src/main.rs` — `charter_accepted_hook` (ratification wiring)

### Gateway and surfaces
- `icn/crates/icn-gateway/src/server.rs`
- `icn/crates/icn-gateway/src/api/steward/mod.rs` — steward gateway
  (recently gutted of 156 lines; verify completeness)
- `icn/crates/icn-gateway/src/commons_mgr.rs`
- `icn/bins/icnctl/src/main.rs` — CLI

### Tests that enforce invariants
- `icn/crates/icn-governance/tests/steward_semantic_invariants.rs`
  (NEW — asserts bond-free stewardship)
- `icn/crates/icn-core/tests/emitted_surface_contract.rs`
  (dispatch completeness)
- `icn/crates/icn-core/tests/charter_enforcement_integration.rs`

---

## 10. Known Traps and False Impressions

### "The sprint file says..."
**Trap:** `icn/ops/state/sprint/current.json` lags real state by multiple sprints.
**Reality:** Use `git log --oneline -50` and this handoff doc for truth.

### "There is no governance dispatch gap anymore"
**Partial truth.** 29 proposals are wired. But `UpdateJurisdictionTier`,
`TerminateClearing`, `RevokeVouch` are still `DEFERRED`, and 5 SDIS proposals
are still `FAIL-FAST`. See `governance-execution-inventory.md` for the exact
count.

### "Charter policy is now fully consumed"
**Partial truth.** `min_votes_ordinary`, `min_quorum_ordinary`, `credit_limit`,
and `surplus_reserves_pct` are consumed. `equity_range_*` and `settlement_*`
are not. See `ccl-architecture.md` Gap 1.

### "CCL is a smart contract language"
**False.** CCL has two distinct runtime roles (ADR-0016). Role 1 is a
fuel-metered contract executor for bilateral agreements. Role 2 is a
read-only schema translation layer for charters. Neither is a smart-contract VM.

### "Stewards are bonded, you just need to find the `bond_amount` field"
**False.** ADR-0014 removed all bond/slash semantics. Tests assert their
absence. If an old doc or grant narrative mentions steward bonds, the doc
is stale — not the code.

### "The website/docs say X, so X is true"
**Trap.** The website (`website/`) and some strategy docs may still frame
ICN in pre-ADR-0014/0015 language. Code is authoritative. Doc updates in
the website are a known remaining tranche.

### "The LSP says there's a type error"
**Trap.** After large edits, rust-analyzer is slow to reindex. Run
`cargo check -p <crate>` before trusting LSP diagnostics.

### "Background command is hung"
**Trap.** The Bash tool's background tasks must be retrieved via `TaskOutput`
with a generous timeout. Don't poll. Don't `sleep`. Don't retry.

### "`cargo test` can use `--test-threads=1` freely"
**Trap.** Unit tests run parallel; integration tests (`--test '*'`) must be
serial. See the icn CLAUDE.md for the exact invocations.

---

## 11. Validation Ladder

Use the lowest rung that actually answers your question. Escalate only if
a rung fails.

### Rung 1: Targeted type-check (~5-30s)
```
cd icn && cargo check -p <crate>
```
Fast feedback on compile errors in a single crate.

### Rung 2: Full-crate clippy (~30s-2min)
```
cd icn && cargo clippy -p <crate> --all-targets -- -D warnings
```
Catches lint issues across the crate's bin/lib/tests/examples.

### Rung 3: Targeted test (~30s-2min per crate)
```
cd icn && cargo test -p <crate> --lib <test_pattern>
```
Runs specific tests. Pattern matches test fn names, not files.

### Rung 4: Touched-crate ladder (~3-10min)
```
cd icn && cargo fmt --all --check
cd icn && cargo clippy -p A -p B -p C --all-targets -- -D warnings
cd icn && cargo test -p A -p B -p C --lib
```
The standard bar for "ready to commit" on a focused tranche.

### Rung 5: Workspace ladder (~10-30min)
```
cd icn && cargo fmt --all --check
cd icn && cargo clippy --workspace --all-targets -- -D warnings
cd icn && cargo test --workspace --lib
cd icn && cargo test --workspace --test '*' -- --test-threads=1
```
Use before merging a multi-subsystem PR. CI does this anyway.

### Rung 6: Live K3s deployment
K3s cluster is 10.8.30.40-42. See `docs/HOMELAB_DEPLOYMENT.md` and
`deploy/k8s/`. Use only when a change actually affects runtime behavior —
e.g., charter view wiring across daemon restart.

### Specific test patterns worth remembering
```
cargo test -p icn-ledger --lib charter_           # Charter credit limit tests
cargo test -p icn-charter-app --lib credit_limit  # Charter oracle tests
cargo test -p icn-core --lib supervisor           # Bootstrap wiring
cargo test -p icn-core --test emitted_surface_contract  # Dispatch completeness
cargo test -p icn-governance --test steward_semantic_invariants  # ADR-0014 guard
```

---

## 12. Website / Public Surface Context

### What is the website for
The site at `website/` is the public-facing narrative of what ICN is. It is
built with Astro 5, reads docs from `docs/` at build time, deployed to
`intercooperative.network` via GitHub Pages.

### Messaging constraints (must preserve)
- **ICN is digital public infrastructure / a coordination OS.** Never
  "ledger", "token", or "payment system" — regulatory framing is critical.
- **Stewards are institutional offices.** Never "validators", "stakers",
  "bonded nodes".
- **CCL is cooperative contract language.** Never "smart contracts"
  (unqualified — disambiguate in the doc if the word is unavoidable).
- **Federation is scoped cooperative coordination.** Never "permissionless
  network" or "global decentralized".
- **Economics is governed accounting / mutual credit.** Never "yield",
  "liquidity", "swap", "mining".

### Public claims must be calibrated to repo truth
Do NOT write claims like "ICN executes every governance decision" unless
you have checked the governance-execution-inventory. The current truth is:
"ICN translates every proposal to a kernel effect; most produce real
state changes; a small number are intentionally record-only; a small
number are named deferrals; 5 are explicit fail-fast gaps." Lead with
precision.

### Likely next website tranche
After Tranches I-IV land, the website "Architecture" and "Roadmap" pages
need an update reflecting:
- Stewardship semantic boundary (ADR-0014)
- Governance execution truthfulness (ADR-0015 Invariant C)
- CCL architecture boundary (ADR-0016)
- Charter consumption closures

Do not preemptively update the website before the referenced work is live
in `main`.

---

## 13. Open Architectural Questions

These are questions the next agent may encounter and should NOT resolve
unilaterally — flag them and ask.

1. **Threshold registry for SDIS `ModifyThreshold`.** Where do thresholds
   live? Per-domain? Per-charter? Per-cooperative? How does the governance
   actor read them at vote time? (Currently fail-fast.)

2. **Authority registry for SDIS `ApproveAuthority` / `RevokeAuthority`.**
   What does an "authority" mean in the kernel? Is it a capability token?
   A DID attribute? A separate registry? (Currently fail-fast.)

3. **DID rotation coordination for `ForceKeyRotation`.** Touches identity,
   network, and steward layers. Needs its own ADR. (Currently fail-fast.)

4. **Non-ordinary charter decision types.** The infrastructure exists for
   `supermajority` and `constitutional` decision types, but the governance
   actor only queries `ordinary`. Is this a conscious scope cut or an
   oversight?

5. **Charter persistence and view re-binding.** Should `CharterPolicyOracle`
   own the Sled persistence, or should the daemon persist charters
   separately and re-hydrate on startup?

6. **Compute dispute outcome model.** `DisputeResolution` currently only
   handles `Partial` for compensation. Should `Uphold`/`Reject`/`VoidTransaction`
   have their own compensation formulas, or should they be
   `KernelEffect::NoOp` with a governance receipt?

7. **Commons credit vs. mutual credit terminology.** Both phrases appear
   in the codebase. Are they synonyms, or does "commons credit" refer
   specifically to the settlement model in `apps/ledger/settlement/`?

---

## 14. What the Next Agent Should Do First

In order:

1. **Read this doc, then `icn-operating-model.md`, then ADRs 0014/0015/0016.**
   Do not skip. They contain the only copy of the strategic reasoning.

2. **Run `git status --short`** in the monorepo root. Understand the
   uncommitted state. Do not `git stash` or `git reset`.

3. **Run `cd icn && cargo check --workspace`** to verify the working tree
   compiles. If not, there is an unfinished edit somewhere — read the
   error, understand, fix.

4. **Run `cd icn && cargo test --workspace --lib`** to verify tests pass.
   Do not continue with new work until they do.

5. **Ask the user** what tranche they want next. Do not assume. This doc
   lists candidates in §8 but the user's priority may not match the doc.

6. **If the user says "continue the architectural pass"**, start with
   **Tranche IV** (`TerminateClearing` / `RevokeVouch` executors). Tranches I,
   II, and III are all complete as of 2026-04-08.

---

## 15. Do Not Break These

Running list of things a careless edit could regress:

- Meaning firewall (kernel imports domain) — CI-enforced, but still possible
  to break locally
- `StewardRecord` bond-free — tests assert but a new field would compile
- `translate_payload_to_effects` dispatch completeness — `emitted_surface_contract`
  test catches this
- Charter economic view binding on ratification — only tested at unit level
- Governance actor suspension gates (open_proposal, create_delegation,
  vote expansion) — tests exist but coverage is not exhaustive
- `ADR-0016` Phase 4: charter validator honest `Deferred` return — easy to
  regress to `Pass`

---

## 16. Final Pointers

- **User's global behavior rules:** `/home/ubuntu/CLAUDE.md` and
  `/home/ubuntu/projects/CLAUDE.md` and `/home/ubuntu/projects/icn/CLAUDE.md`.
  The icn CLAUDE.md is the most strict. Read it before the first code edit.
- **Agent runtime memory index:**
  `/home/ubuntu/.claude/projects/-home-ubuntu-projects/memory/MEMORY.md`.
  This has K3s cluster state, recent PR merge lessons, working style notes.
- **Live cluster state:** K3s is at 10.8.30.40-42. Gateway on port 30080.
  Prometheus on 30090. Don't assume it's down — check before claiming.
- **This document's accuracy is your problem.** If you find a claim here
  that doesn't match the repo, update this doc before doing anything else.
  The handoff is only useful if it is truthful.

---

*End of handoff. Good hunting.*
