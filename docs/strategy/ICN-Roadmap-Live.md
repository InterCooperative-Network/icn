# ICN Roadmap — Live

*Last updated: 2026-03-22 (Sprint 23 — Baseline Lock + Narrative Surface)*

> This document tracks sprint-level progress against the ICN development roadmap.
> For architecture and phase history, see `docs/ARCHITECTURE.md` and `docs/PHASE_HISTORY.md`.

---

## Completed Sprints

### Sprint 22 — Meaning Firewall Completion
**Theme**: Complete extraction of all hardcoded policy constants to typed config structs
**Status**: Closed 2026-03-22
**Completed**:
- icn-security: `max_violations_per_hour` + `violation_retention_secs` extracted to `MisbehaviorThresholdsConfig` (PR #1389)
- icn-obs: 5 attestation threshold constants extracted to `AttestationConfig` (PR #1390)
- icn-compute: `CharterPriority` preemption routing + credit ceiling validation extracted to `ComputePolicyConfig` (PR #1391)
- icn-ledger: `CreditPolicy` + `NewMemberPolicy` factory values extracted to `CreditPolicyConfig` (PR #1392, highest regulatory priority)
- `docs/meaning-firewall-audit.md` updated to mark all Sprint 22 remediations complete (PR #1392)

---

## Sprint 23 — Baseline Lock + Narrative Surface
**Theme**: Legitimacy through convergence. Repo state, board state, and narrative state must agree.
**Status**: Active (2026-03-22)
**Governing rule**: A task is only "done" when repo state, board state, and narrative state agree.

### Track A — Operations Closure
- s23-t1: CI Test Coverage classified as non-blocking observational gate (commit 57f5cc00)
- s23-t2: Dirty file committed — session log from #1394 skills rewrite (commit 627857a2)
- s23-t3: Stale `1310-execution-receipt-gate` worktree removed
- s23-t4: Sprint 22 formally closed; Sprint 23 board populated

### Track B — P0 Residue
- s23-t5: #1095 CRDT OrSet + LwwRegister — deferred to Sprint 24 with rationale (pending)
- s23-t6: #1096 ContainerRuntime trait — implementation in icn-kernel-api (pending)
- s23-t7: #1131 Storage governance spec written; issue closed (commit e16625bc)

### Track C — Baseline Narrative
- s23-t8: Platform baseline document published (`docs/state/ICN-Platform-Baseline-2026-03.md`) (pending)
- s23-t9: Roadmap refresh — this document (in progress)
- s23-t10: Demo path validated and documented (pending)

---

## Sprint 24 Candidates — Commons Compute Hardening

*These are shaped candidates only. Full Sprint 24 planning is out of scope for Sprint 23.*

**#925 — feat(compute): Commons resource pool and contribution accounting**
- Scope: `CommonsPool` type for aggregate capacity tracking across all commons participants; unaffiliated node participation; commons credit earning/spending proportional to contributed resources
- Rationale: First full Commons Compute primitive — enables nodes without org membership to participate in resource pools and earn commons credits

**#947 — feat(compute): Unaffiliated node participation protocol**
- Scope: Protocol for independent (cell-less) nodes to announce capacity with `commons_share = 1.0`, claim `Commons`-scoped tasks, and submit execution receipts for settlement; gossip integration for `NodeCapacityAnnounce` with `cell_id: None`
- Rationale: Depends on #925; makes commons participation legible at the gossip layer so task placement can route to commons nodes

**#964 — feat(compute): Stale commons pool participant expiry**
- Scope: Configurable expiry logic (default 5 minutes) to evict commons participants that stop announcing; periodic expiry check on new announcements or background timer; expiry metrics
- Rationale: Closes the `last_announce` TODO in `icn-compute/src/commons_pool.rs`; prevents stale nodes accumulating indefinitely in pool capacity accounting

---

## Sprint 25 Candidates (Provisional)

| # | Title | Theme |
|---|-------|-------|
| #862 | Naming Primitive | Federation Semantics |
| #863 | Federation Agreements | Federation Semantics |

---

## External Surfaces (Parallel Track)

| # | Area | Status |
|---|------|--------|
| #1366 | Website | Deferred — depends on narrative stability |
| #1368 | React Native SDK | Deferred |
| #1369 | SDK externalization | Deferred |
