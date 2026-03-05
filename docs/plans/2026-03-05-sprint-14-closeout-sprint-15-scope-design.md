# Sprint 14 Closeout + Sprint 15 Scope

**Date:** 2026-03-05
**Status:** Approved

---

## Context

Sprint 14 opened with four epics: regulatory-safe verifiable state architecture (compliance),
IPv6 dual-stack transport, pilot Flow C completion, and service-discovery hardening.

By 2026-03-05 the compliance foundation is further along than the sprint file reflects:

| Task | PR | State |
|------|----|-------|
| t1 — UX language guide | #1316 | merged |
| t2 — API rename (payment→settlement etc.) | #1315 | merged |
| t6 — JournalEntry provenance required (ProvenanceRef) | #1318 | merged |
| t7 — Obligation type + state machine | #1320 | merged |

PR #1322 (`fix/icn-core-backup-restore-provenance`) is open and fixing call-site fallout from the
ProvenanceRef migration — merge this as hygiene before starting new compliance work.

---

## Design Decision: "No New Conceptual Surfaces" Rule

Sprint 14 is locked to a **pilot truth + regulatory spine** bundle.
Nothing that opens a new conceptual surface (new type model, new semantic layer) ships in S14
unless it is on the critical path to Flow C or ExecutionReceiptGate.

This rule eliminates:
- t4 (auth semantics) — hardening, not critical path
- t12 (CCL credit formula) — semantics-heavy; high debate risk
- t14 (timestamp cleanup) — pure cleanup
- t8/t9/t18 (IPv6 type refactor) — structural; would expand to eat the sprint

---

## Sprint 14 — Final Scope

**Anchor:** Flow C demo runs end-to-end. ExecutionReceiptGate enforces governance→execution
linkage. Gateway listens on IPv6. Sprint file reflects reality.

### Execution order (priority-first, never cleanup-before-demo)

1. **t10 — CoopActor fallback atomicity** (issue #1090)
   Flow C is the forcing function. Everything else is secondary until this runs.

2. **t5 — Commons integration test, multi-node** (issue #949, S13 carryover)
   Proof harness. Once it passes, the pilot has a spine.

3. **t11 — PatronageTracker** (issue #1307)
   NY Corp Law §93 internal capital accounts. Unblocked now that t7 (Obligation) merged.

4. **t15 — DelegationManager persist + gossip sync** (issue #1309)
   After Flow C is stable, wire the compliance spine without chasing moving targets.

5. **t16 — ExecutionReceiptGate** (issue #1310)
   Enforces governance→execution linkage. Blocked by t15.

6. **t3 — Dual-stack bind defaults `[::]`/`[::1]`** (issue #1296)
   Low blast radius. Do last so it cannot become "mysterious networking day."

7. **Hygiene**
   - Merge PR #1322 (provenance call-site fixes)
   - Prune `1303-api-rename` worktree (PR merged, 14 behind main)
   - Update sprint file to mark t1/t2/t6/t7 done
   - Never do bureaucracy before the demo runs.

### Definition of done

- `cargo test` passes with CoopActor fallback covering the atomicity scenario
- Commons multi-node integration test green
- PatronageTracker tracks capital accounts per member, persists correctly
- DelegationManager survives restart via gossip sync
- ExecutionReceiptGate rejects execution without a governance receipt
- Gateway and daemon bind `[::]` / `[::1]` by default
- Sprint file updated, stale worktree pruned

---

## Sprint 15 — Proposed Scope

**Theme:** Networking types refactor + cleanup batch

No new compliance surfaces — consolidate and clean.

### Epics

#### IPv6 Phase 0+A — type model (sequential, gates later work)

1. **t8 — EndpointCandidate type + ConnectionCandidate refactor** (issue #1297)
   Core structural move: endpoints become sets of candidates with metadata, not single strings.
   *This is the gravity item — do it first in S15 and timebox it.*

2. **t9 — mDNS collects all addresses per peer** (issue #1298)
   Retain all A/AAAA results from discovery. Blocked by t8.

3. **t18 — KnownPeer endpoint field + capability gating flag** (issue #1300)
   Closes discovery → trust/capability → dialing loop. Blocked by t8.

#### IPv6 Phase B — dialing (stretch)

4. **t13 — Happy Eyeballs multi-path dialer** (issue #1299) — blocked by t8
5. **t20 — TURN relay proxy IPv6 support** (issue #1301) — blocked by t13 + t18

#### Standalone hardening batch

6. **t4 — auth semantics: GET /v1/services/:id returns 404 not 401** (issue #1120)
7. **t14 — timestamp validation + ServiceEndpointResponse update** (issue #1051)

#### Semantics batch (with doc + test scaffolding)

8. **t12 — extract commons credit formula to CCL PolicyOracle** (issue #1308)
   Meaning firewall fix. Needs design doc before implementation.

#### Compliance continuation

9. **t17 — AllocationProposal for participatory budgeting** (issue #1311) — blocked by t16 (S14)
10. **t19 — regulatory compliance linter CI gate** (issue #1312) — stretch, goes last

### Exception rule

Pull **t4** into S14 only if Flow C demo surfaces unauthenticated-read behaviour that breaks
the demo narrative. Otherwise it stays in S15.

---

## Deferred to backlog (not in S15 scope)

Items deferred from S14 that are not yet scheduled:

- #925 Commons resource pool + contribution accounting
- #947 Unaffiliated node participation protocol
- #953 Persistent service registry storage
- #862 Phase 7 Naming Primitive
- #863 Federation Agreement Support
- #1095–#1098 CRDT / ContainerRuntime / Genesis / devnet compose
- #1135–#1138 Kernel cleanup phases A1–A4
- #1142–#1146 Backup validation, vertical slice integration test, demo script
- #1009–#1012 Wave 3–6 spec documents
