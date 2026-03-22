# Sprint 24 Planning — Commons Compute Hardening

**Date:** 2026-03-22
**Status:** Draft — pending sprint kickoff
**Author:** Matt Faherty
**Input:** Sprint 23 close memo + codebase audit

---

## Governing Constraint

> Sprint 23 is administratively complete. It is not equivalent to full demo readiness.

This distinction must not be flattened at Sprint 24 kickoff. The board is clean. The state is more legible. Several real things landed. None of that is the same as "the demo works end-to-end."

---

## 1. Current State Entering Sprint 24

### What is true

- Sprint 22 archived. Sprint 23 board shows 10/10 done.
- `main` passes all required CI gates. Test Coverage is an acknowledged exception (runner ceiling, documented in `ops/state/ci-exceptions.md` — non-blocking, not a code regression).
- Kernel API surface is legible: `StorageClass`, `ContainerRuntime`, `CrdtType` all defined. CRDT implementations and `ContainerAttestation` explicitly deferred.
- Platform baseline doc, storage governance spec, roadmap, and demo inventory all committed.

### What is not true

- The project is not demo-ready across all three flows.
- "Sprint 23 complete" does not mean a presenter can run the demo clean.

---

## 2. Demo Debt: Two Distinct Classes

The demo debt is **not one thing**. Collapsing it into "demo polish" will cause it to get deprioritized until it blocks a presentation.

### Class 1 — Missing Script Debt

| Flow | Script | Status |
|------|--------|--------|
| WASM Compute (Flow A) | none | No script exists |
| Discovery (Flow B) | none | No script exists |

These flows have no demo exercise at all. The gateway routes for compute and discovery exist; there is no scripted walkthrough. This is **authoring work** — writing the scripts, finding the right sequence of API calls, making them presenter-grade.

### Class 2 — ~~Deployment/Seeding Debt~~ RESOLVED (2026-03-22)

| Flow | Script | Status | Validated |
|------|--------|--------|-----------|
| Patronage (Flow 2) | `flow-2-patronage.sh` | **Working** | 2026-03-22 live test |

**Live validation result (2026-03-22):** All three ledger routes return expected results with a valid auth token:
- `GET /v1/ledger/brightworks-cooperative/history` → **200** (3 prior transactions present)
- `GET /v1/ledger/brightworks-cooperative/position/{did}` → **200**
- `POST /v1/ledger/brightworks-cooperative/settle` → **201** (transaction hash returned)

The "fragile — ledger routes 404" characterization in the Sprint 23 demo-path doc was incorrect. The s23-t10 subagent read historical warning comments in the script (scope gaps resolved as of 2026-03-18) as evidence of current failures without running the script. No 404s were ever observed in live testing.

The hardcoded `BRIGHTWORKS_NODE_DID` in `flow-2-patronage.sh:50` matches the running pod's DID exactly. Seeded data is present. No repair needed.

**Flow 2 is not pre-sprint prerequisite work.** It is a proven flow.

### Class 2 — Proven Flows

| Flow | Script | Status |
|------|--------|--------|
| Governance (Flow 1) | `flow-1-governance.sh` | Proven |
| Patronage (Flow 2) | `flow-2-patronage.sh` | Proven (validated 2026-03-22) |
| Federation (Flow 3) | `flow-3-federation.sh` | Proven |

---

## 3. Recommended Sprint Boundary

### Pre-Sprint-24 Prerequisite Work

One remaining prerequisite (Flow 2 was validated and removed):

| Task | Class | Description | Estimated size |
|------|-------|-------------|---------------|
| ~~**p24-pre-1**~~ | ~~Deployment~~ | ~~Flow 2 repair~~ | **Resolved** — live test 2026-03-22 confirms working |
| **p24-pre-2** | Ops | Confirm `cargo-tarpaulin` infrastructure fix path — either dedicated runner or switch to `llvm-cov`. Do not let this drift another sprint. | 2–4 hrs investigation |

p24-pre-2 is the only remaining ops item before Sprint 24 kickoff.

### Sprint 24 Proper — Commons Compute Hardening

The spine is #925 → #947 → #964, in dependency order:

| Issue | Title | Why it goes here |
|-------|-------|-----------------|
| **#925** | Commons resource pool and contribution accounting | First primitive — enables nodes to pool resources and track contribution |
| **#947** | Unaffiliated node participation protocol | Depends on #925 accounting model; enables nodes outside existing coops |
| **#964** | Stale commons pool participant expiry | Correctness requirement for #925/#947; without expiry, pools accumulate dead entries |

These three form a closed loop: you can't do #947 without #925 accounting, and both produce incorrect long-term state without #964 expiry.

### What Should Not Silently Bleed In

- **Flow A / Flow B demo scripts** — these are script-authoring work, not commons-compute engineering. They do not belong inside Sprint 24 unless explicitly scoped as a parallel track with a named owner.
- **CRDT implementations** (#1095) — deferred to Sprint 24 with explicit rationale, but this is a P2 item. If Sprint 24 is already carrying #925/#947/#964, CRDT risks competing for the same Rust engineering attention. Defer to Sprint 25 unless a specific commons-compute use case requires it.
- **ContainerAttestation** — same risk profile as CRDT. Sprint 25 candidate.
- **Coverage CI infrastructure** — this is ops/infra, not engineering. Assign it outside the sprint if possible.

---

## 4. Proposed Task Breakdown

### Pre-requisite / Unblockers (before Sprint 24 kickoff)

~~**p24-pre-1: Stabilize Flow 2 (Patronage)**~~ — **Closed 2026-03-22.** Live validation confirmed all three ledger routes return 200/201 with proper auth. No repair needed.

**p24-pre-2: Decide Coverage CI path**
- Why: Red CI is acknowledged but not resolved; can't drift another sprint
- Owner: DevOps / infra
- Dependency: None
- Success: Either (a) dedicated larger runner is provisioned and tarpaulin job completes, or (b) `llvm-cov` migration is committed to with a named target sprint, or (c) the job is marked as permanently advisory with a PR that removes it from `failure` classification

### Sprint 24 Core

**s24-t1: Commons resource pool (#925)**
- Why: First commons-compute primitive; without it, #947 has no accounting model to build on
- Owner: Rust / icn-compute domain
- Dependency: None (independent of demo work)
- Success: `CommonsResourcePool` type exists, contribution accounting is implemented, tests pass, kernel/app boundary respected (no domain leakage into kernel)

**s24-t2: Unaffiliated node participation (#947)**
- Why: Enables nodes outside cooperative membership to participate in commons pools
- Owner: Rust / icn-compute + icn-federation
- Dependency: s24-t1 (accounting model must exist)
- Success: An unaffiliated node can join a commons pool, contribute resources, and have contributions recorded via #925 accounting

**s24-t3: Stale participant expiry (#964)**
- Why: Without expiry, pools accumulate entries for nodes that have gone offline; correctness degrades over time
- Owner: Rust / icn-compute
- Dependency: s24-t1, s24-t2
- Success: Expiry policy is configurable, stale entries are purged on schedule, no data loss for active participants

### Optional Follow-on / Demo Polish

**s24-opt-1: Flow A demo script (WASM Compute)**
- Why: Missing script; no presenter-grade WASM demo exists
- Owner: Demo ops / script authoring
- Dependency: None (gateway routes exist)
- Success: `flow-1-wasm.sh` runs end-to-end and narrates a WASM task execution

**s24-opt-2: Flow B demo script (Discovery)**
- Why: Discovery is currently automatic/implicit; no scripted walkthrough exists
- Owner: Demo ops
- Dependency: None
- Success: Discovery behavior is observable and narrated in a script

**s24-opt-3: CRDT implementations (#1095)**
- Why: OrSet + LwwRegister are enum stubs; zero concrete implementations
- Owner: Rust / icn-kernel-api domain
- Dependency: None (kernel-level, independent of commons-compute)
- Priority: P2 — include only if bandwidth exists without displacing core tasks

---

## 5. Risk Notes

### Bandwidth collision risk

If `p24-pre-1` (flow-2 seeding fix) is not resolved before Sprint 24 opens, it will be treated as a sprint task and will compete with #925/#947/#964 for the same engineering attention. Flow debugging and API implementation are not the same skill domain, but they compete for the same person-hours and sprint focus. **Pull it forward.**

### False bucket risk

The most common planning error here is treating all demo debt as one work item labeled "demo polish." It is not. The two classes require different skills, different time commitments, and different urgency levels. Deployment/seeding fixes should be handled by whoever owns the K3s cluster. Script authoring is a separate track. Commons-compute engineering is the third track. These should not share a bucket.

### Hidden coupling: DID staleness

`flow-2-patronage.sh` has a hardcoded `BRIGHTWORKS_NODE_DID`. The script comments this explicitly: "If pods are rebuilt, this DID changes." This means every K3s cluster rebuild silently invalidates the demo. This is a fragility that will recur. The longer-term fix is for `reseed-federation-demo.sh` to export the seeded DIDs into a file that the flow scripts source dynamically, rather than having them hardcoded. This is a one-time cleanup task, not a sprint item, but it should be noted.

### CRDT and ContainerAttestation scope creep risk

Both were deferred to Sprint 24 per the Sprint 23 deferral rationale. If Sprint 24 is already carrying #925/#947/#964, adding both deferred items creates a sprint that is too wide. Choose one or defer both to Sprint 25. Explicit scoping at kickoff prevents this from becoming an implicit assumption.

---

## Recommended Next Action

Before Sprint 24 planning is considered structurally sound, three things need to be resolved:

1. **Run `reseed-federation-demo.sh` on the current K3s cluster.** Confirm whether flow-2 works afterward. If yes, the fix is done. If no, diagnose further (NodePort routing, DID mismatch, scope gap).

2. **Decide on Coverage CI.** Not a hard decision, but it needs to be a decision — not another sprint of acknowledged drift.

3. **Confirm CRDT and ContainerAttestation are explicitly out of Sprint 24 scope,** or name a specific owner and bounded task for each. No ambiguity about whether they're in or out.

Once these three are resolved, Sprint 24 kickoff can proceed with a clean spine around #925/#947/#964.
