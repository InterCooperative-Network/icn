# ICN Roadmap — Live

*Last updated: 2026-04-29 (post-#1680, post-NYCN-#34)*

> This document tracks long-arc planning context for ICN development.
> For **canonical current state and recently-merged work**, read
> `docs/STATE.md` and `docs/PHASE_PROGRESS.md` — those are
> truth-synced and re-bumped with every state-changing PR.
> This file is a roadmap-and-rationale companion that points at
> them; it is not a per-PR changelog.
> For architecture and phase history, see `docs/ARCHITECTURE.md`
> and `docs/PHASE_HISTORY.md`.

---

## Current state (2026-04-29 snapshot)

**Active workstream:** Phase 2 — Pilot Launch. NYCN is the
intended first cooperative partner (active partnership track,
not yet a formally committed pilot). The Phase 2 *machinery* is
in place end-to-end; what remains is the human procedure —
present the merged ladder + ICN proof-loop machinery to NYCN
organizers, formalize, rehearse — and recording each step.

**Canonical source of truth:** `docs/STATE.md` and
`docs/PHASE_PROGRESS.md`. Those docs name every recently-merged
PR with its date and effect. This file does not duplicate
their tables.

**What changed between this doc's prior 2026-04-08 sync and the
current 2026-04-29 sync** (high-level only; see `STATE.md` for
the per-PR record):

- **Institutional-operability runtime** landed across 2026-04-22 → 2026-04-26: live charter activation endpoint (#1624), person-directory overlay for bootstrap role assignment (#1626), `GET /me/standing` read model (#1627), `authority_scope` plumbed end-to-end through `assign_role` (#1630), generic institution bootstrap package path (#1586), bootstrap-apply 409 idempotency (#1617), persistent governance domains across gateway restart (#1621), feedback/support doctrine + ADR canonicalization (#1637).
- **Action-card runtime** landed 2026-04-27 with proof-bearing receipt loops for all currently emitted source paths: `GET /v1/gov/me/action-cards` (#1659), proposal/vote → `GovernanceDecisionReceipt` (#1660), `action_item`/`complete` → `ActionItemCompletionReceipt` (#1661), `meeting`/`attend` → `MeetingAttendanceReceipt` (#1663). Issue #1646 remains open for the two RFC-gated paths (`signal_rule`, `obligation_lifecycle`).
- **Action-item completion-receipt retrieval** shipped 2026-04-29: `GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt` (#1675). Local HTTP proof loop closure documented in #1676. Operator-authorized K3s NYCN smoke proof closure against deployed image `91a63eec` recorded in #1677.
- **Truth-sync docs** updated in #1678 and #1680 to reflect every landing above plus the new cooperator-developer prep doc at `docs/strategy/COOPERATIVE_DEVELOPER_DISCOVERY_BRIEF.md`.
- **NYCN side**: the drive-ingest operator ladder shipped end-to-end in `fahertym/nycn` PRs #21–#34 (parser → review → decisions YAML → publish dry-run → assignee binding → local publisher → local proof runner → federation surface bridge → operator pilot runbook + ladder checker → organizer briefing + simple summit demo → start-here onboarding pass → one-command local preflight runner → whole-NYCN operating-surfaces inventory + Google-Groups boundary policy → communication-groups directory tool → operating-surfaces directory tool). The ladder defends a hard mutation boundary: every layer is either pure (no network) or localhost-only operator-gated. K3s mutation is never allowed by NYCN-side tools; ICN-side K3s exercise lives under `docs/dev/NYCN_K3S_PROOF_PATH.md` (#1677).
- **Issue #1679** filed 2026-04-29 for K3s/devnet smoke artifact cleanup / teardown semantics. Inventory-only as the suggested first concrete step.

**Phase 2 framing:** NYCN is the intended first cooperative
partner. The next concrete step is presenting the merged ladder
+ ICN proof-loop machinery to NYCN organizers to formalize the
pilot. Subsequent gates: partnership formalization, then first
operator pilot rehearsal against real (or fixture-equivalent)
organizer material. Phase 2 stays ⏳ until those happen and are
recorded.

The cadence formerly tracked here as "Governance Dispatch
Tranches" continues to advance through executor-wiring work;
its per-tranche status is reflected in `docs/STATE.md`,
`docs/PHASE_PROGRESS.md`, and the git log, and is no longer
re-published in this file. The 2026-04-08 tranche table that
was here previously is preserved below in the "Historical"
section for continuity.

> **Rule:** Do not treat the per-sprint sections below as
> current status. They are historical only. For current state,
> read `docs/STATE.md`, `docs/PHASE_PROGRESS.md`, and the git
> log.

---

## Historical: 2026-04-08 tranche snapshot

The Governance Dispatch Tranche table as it stood at the prior
2026-04-08 sync, retained for continuity. Current tranche
status is reflected in `docs/STATE.md`, `docs/PHASE_PROGRESS.md`,
and the git log.

| Tranche | Scope | Status (as of 2026-04-08) |
|---|---|---|
| 9 | TrustThreshold suspension enforcement | ✅ Merged |
| 10 | BondIssuance treasury mutation | ✅ Merged |
| 11 | ShareRedemption treasury mutation | ✅ Merged |
| 12 | LeaveFederation dispatch | ✅ Merged (#1506) |
| 13 | Real `SdisService` executor, dual-path closure | ✅ Merged |
| 14 | ReconfirmSteward + firewall fixes | ✅ Merged |
| 15 | ReinstateSteward governance dispatch | ✅ Merged (#1509) |
| 16 | SuspendSteward governance dispatch | ✅ Merged (#1510) |
| 17 | UpdateJurisdictionTier executor | 🚧 Local on `fix/steward-bond-semantics` (icn-dev) |
| 18 | TerminateClearing + RevokeVouch executors | ⏳ Next |

The overlaid "Agent Handoff Tranche" track (`I`, `II`, `III`,
...) covers ADR-0016 closures (surplus reserves consumption,
charter view persistence, etc.) and advances on its own
cadence. Tranches I, II, and III closed 2026-04-08.

---

## Pre-2026-04-08: completed sprints

The per-sprint summary that historically lived in this file
covers sprints 22 through 28 (Meaning Firewall completion
through Gossip fan-out fix). Retained below for continuity but
**not authoritative** — the canonical record of completed work
is `docs/PHASE_HISTORY.md` plus git log.

- **Sprint 24** — Commons-compute spine. Two-tier trust threshold for pool admission, stale participant expiry, V1 scheduling-time credit enforcement floor. Closed 2026-03-23. See `docs/strategy/sprint-24-close.md`.
- **Sprint 25** — ADR-0016 Phase 2 work. Closed via `chore(ops): close sprint 25, open sprint 26` (commit `92de849e`).
- **Sprint 26** — Interim tranche; see commits between `92de849e` and Sprint 27 opening.
- **Sprint 27** — Demo-Ready + Compute Story. Flow 5 commons compute trust-gated admission, demo polish, website five-flow update, summit proposal. Key commits: `1600a1fb`, `b1ba40c7`, `cb22e1e5`, `8ab59cff`.
- **Sprint 28** — Gossip fan-out fix (compute task delivery restored). Commit `4ae8401f` (PR #1419).

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
