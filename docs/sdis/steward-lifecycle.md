---
title: "Steward Lifecycle — Source of Truth"
status: "current"
date: "2026-04-07"
context: "ADR-0014 / ADR-0015 semantic hardening"
---

# Steward Lifecycle — Source of Truth

> This document is the canonical reference for what a steward is,
> how they are appointed, disciplined, and removed, and what kernel effects
> each transition produces.

---

## What a Steward Is

A **steward** is an institutional office holder in the SDIS (Sovereign Digital
Identity System). Stewards are trusted agents appointed via democratic governance
to perform identity verification ceremonies (POP), participate in recovery flows,
and maintain the integrity of the identity layer.

**Stewardship is NOT:**
- A staking position (no bond required)
- A paid service role (no payment for individual attestations)
- A permanent appointment (all terms are time-bounded)
- A financial fiduciary (no custody of funds)

**Stewardship IS:**
- A governance-ratified institutional mandate
- A trust office backed by community confidence, not capital
- An accountable role with formal disciplinary mechanisms
- A revocable appointment governed by democratic process

> See ADR-0014: Stewardship Semantics Boundary.

---

## Steward Status Values

```
Active          → performing duties normally
Suspended       → temporarily removed from active duty (reason logged)
Probation       → active but under enhanced monitoring (record-only, policy-layer)
Removed/Revoked → permanently removed, cannot be reinstated
```

`Probation` is tracked at the governance record and CCL/policy layer. The kernel
stores `Active` or `Suspended { reason }` only — probation is not a kernel-level
status change (see ADR-0015 Invariant A).

---

## Lifecycle Transitions

### 1. Appointment (`AppointSteward`)

**Trigger:** `SdisProposal::AppointSteward` accepted by governance vote  
**Kernel effect:** `SdisEffect::ApproveSteward`  
**State change:** New `StewardRecord` created, status = `Active`  
**Requirements:**
- Candidate DID
- At least 1 sponsor vouching for the candidate
- Region specification
- Term length in seconds

```
∅ ─[AppointSteward]→ Active
```

### 2. Reconfirmation (`ReconfirmSteward`)

**Trigger:** `SdisProposal::ReconfirmSteward` accepted  
**Kernel effect:** `SdisEffect::ReconfirmSteward`  
**State change:** `term_end` extended to `new_term_end`  
**Note:** Stewards must be periodically reconfirmed. Expired stewards cannot perform ceremonies.

```
Active(term_end=T) ─[ReconfirmSteward]→ Active(term_end=T')
```

### 3. Suspension (`SuspendSteward` or `SanctionSteward(Suspension)`)

**Trigger:**
- `SdisProposal::SuspendSteward` accepted (direct suspension governance)
- `SdisProposal::SanctionSteward { penalty: Suspension { duration } }` accepted

**Kernel effect:** `SdisEffect::SuspendSteward`  
**State change:** Status = `Suspended { reason }`  
**Note:** Duration is advisory and preserved in the audit trail. Timed
auto-reinstatement is NOT enforced by CommonsHandle — a `ReinstateSteward`
governance vote is required to restore active status.

```
Active ─[SuspendSteward]→ Suspended { reason }
```

### 4. Reinstatement (`ReinstateSteward`)

**Trigger:** `SdisProposal::ReinstateSteward` accepted  
**Kernel effect:** `SdisEffect::ReinstateSteward`  
**State change:** Status restored to `Active` (idempotent if already active)

```
Suspended ─[ReinstateSteward]→ Active
Active    ─[ReinstateSteward]→ Active  (idempotent)
```

### 5. Removal (`RemoveSteward` or `SanctionSteward(Removal)`)

**Trigger:**
- `SdisProposal::RemoveSteward` accepted (requires super-majority: 67%, 7-day vote, 24h delay)
- `SdisProposal::SanctionSteward { penalty: Removal }` accepted (same voting thresholds)

**Kernel effect:** `SdisEffect::RevokeSteward`  
**State change:** Status = `Removed { reason }`  
**Note:** Removal is permanent. A removed steward cannot be reinstated.
A new `AppointSteward` proposal is required if the same person is later
re-appointed (new DID required by convention).

```
Active    ─[RemoveSteward]→ Removed { reason }
Suspended ─[RemoveSteward]→ Removed { reason }
```

### 6. Disciplinary Sanctions (record-only)

The following sanctions are **institutional record-only** — they do not change
the steward's operational status in the kernel. They are recorded in the
governance audit trail and may inform future decisions. Policy/CCL layers may
apply downstream consequences (e.g., trust score adjustments).

| Sanction | Kernel Effect | Notes |
|----------|--------------|-------|
| `Warning` | `NoOp` | First-level rebuke; governance record only |
| `Censure` | `NoOp` | Formal institutional rebuke; stronger than Warning |
| `Probation` | `NoOp` | Enhanced monitoring conditions; tracked at policy layer |

```
Active ─[SanctionSteward(Warning)]→  Active  (+ governance record)
Active ─[SanctionSteward(Censure)]→  Active  (+ governance record)
Active ─[SanctionSteward(Probation)]→ Active  (+ governance record + policy conditions)
```

### 7. Tier Change (`UpdateJurisdictionTier` or `SanctionSteward(TierDemotion)`)

**Trigger:**
- `SdisProposal::UpdateJurisdictionTier` accepted
- `SdisProposal::SanctionSteward { penalty: TierDemotion { new_tier } }` accepted

**Kernel effect:** `SdisEffect::UpdateJurisdictionTier`  
**Current state (2026-04-07):** 🔜 DEFERRED — executor returns `not_executed=true`  
pending `CommonsHandle::update_jurisdiction_tier` implementation.

**Tiers:**
| Tier | Meaning |
|------|---------|
| Tier1 | Standard operations |
| Tier2 | Enhanced monitoring required |
| Tier3 | All operations require co-signing |

```
Active(Tier1) ─[TierDemotion(Tier2)]→ Active(Tier2)   (deferred)
Active(Tier2) ─[TierDemotion(Tier3)]→ Active(Tier3)   (deferred)
```

---

## Complete State Machine

```
                      ┌─────────────────────────────────────────────────────────┐
                      │                                                         │
                 AppointSteward                                                 │
                      │                                                         │
     ∅ ───────────────▼──────────────────────────────────┐                     │
                   Active                                 │                     │
                    │   ▲                                 │                     │
     SuspendSteward │   │ ReinstateSteward                │                     │
                    ▼   │                                 │ Warning / Censure / │
                 Suspended                                │ Probation (NoOp)    │
                    │                                     │                     │
     RemoveSteward  │                                     │                     │
     (or Sanction   │                                     │                     │
      Removal)      │                                     │                     │
                    ▼                                     ▼                     │
                  Removed ◄────────────────────── RemoveSteward ────────────────┘
                  (terminal)
```

---

## Voting Requirements by Transition

| Transition | Quorum | Approval | Min Period | Exec Delay |
|------------|--------|----------|-----------|-----------|
| AppointSteward | 25% | 51% | 5 days | 12h |
| ReconfirmSteward | 25% | 51% | 5 days | 12h |
| RemoveSteward | 40% | 67% | 7 days | 24h |
| SuspendSteward | 25% | 51% | 5 days | 12h |
| ReinstateSteward | 25% | 51% | 5 days | 12h |
| SanctionSteward(Warning/Censure/Probation) | 25% | 51% | 3 days | 6h |
| SanctionSteward(Suspension) | 25% | 51% | 3 days | 6h |
| SanctionSteward(Removal) | 40% | 67% | 7 days | 24h |
| SanctionSteward(TierDemotion) | 30% | 60% | 5 days | 12h |
| UpdateJurisdictionTier | 25% | 51% | 5 days | 12h |

---

## Canonical Implementation Files

| Concern | File |
|---------|------|
| Domain types | `crates/icn-governance/src/steward.rs` |
| Proposal types | `crates/icn-governance/src/sdis.rs` |
| Persistent storage | `crates/icn-governance/src/steward_store.rs` |
| Kernel effects | `crates/icn-kernel-api/src/effects.rs` (`SdisEffect`) |
| Translation layer | `apps/governance/src/handlers/execution.rs` |
| Executor | `crates/icn-core/src/supervisor/governance_executor.rs` |
| Service trait | `crates/icn-kernel-api/src/services.rs` (`SdisService`) |
| Service impl | `crates/icn-core/src/services/sdis_service.rs` |
| CommonsHandle | `crates/icn-commons/src/handle.rs` |
| Semantic guardrails | `crates/icn-governance/tests/steward_semantic_invariants.rs` |
| ADR | `docs/adr/ADR-0014-stewardship-semantics-boundary.md` |
| ADR | `docs/adr/ADR-0015-semantic-architecture-invariants.md` |
