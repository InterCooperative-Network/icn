---
title: "ICN Project State — 2026-03-26"
status: Current
truth_class: snapshot
canonical: false
last_reviewed: 2026-03-26
---

# ICN Project State — 2026-03-26

**Generated:** 2026-03-26 | **Scope:** Open PRs, open issues, recent commit history, sprint board, strategic docs

---

## Executive Summary

ICN (Intercooperative Network) is a P2P substrate daemon for cooperative coordination — a constraint-enforcement engine, not a blockchain. The project is **~75% complete** across 38 Rust crates, 4 apps, 3 binaries, a TypeScript SDK, a React Native SDK, and two web UIs. All core infrastructure phases (1–20) are complete; the active work is hardening governance execution honesty, provenance auditing, demo polish, and documentation governance.

**Active developer:** Matt Faherty (DownWithMatt) — solo, ~4-5 productive hours/day  
**Current sprint:** Sprint 26 (opened 2026-03-23) — CI coverage migration, governance execution chain, demo path fixes  
**Live deployment:** K3s cluster (VLAN 30), deployed since 2025-12-03; 5 demo flows scripted  
**Test baseline:** 2,287 tests (as of March 2026 status report); CI on rustc 1.88.0 (pinned)

---

## Recent Commit Activity (Last 40 Commits)

All 40 most recent commits fall within **2026-03-23 to 2026-03-25**. The work is highly focused on three threads:

### 1. Governance Execution Honesty (most significant)
- `feat(governance): wire FreezeMember acceptance to ledger + operability audit docs` (ef0d9bb) — First real governance→ledger execution bridge: when a FreezeMember proposal is accepted, the member's ledger account is operability-audited and flagged. Includes a six-plane operability audit document.
- `feat(provenance): governance→economics chain binding + JWT auth proof in direct-member entries` (249e399) — Binds governance decision receipts to downstream economics entries for full provenance chain.
- Related open PR: **#1440** (`fix(governance): domain_id threading + honest execution outcomes`) — adds `NotExecuted` terminal status for structurally non-executed effects; fixes `DistributeSurplus` provenance threading.

### 2. Demo Path Polish
- `fix(demo): demo-path fixes, icn-dev agent configs, verified walkthrough` (eac6972)
- `fix(demo): 5 demo-path polish bugs (#1432)` (aa94934)
- `fix(gateway): dark mode redesign and balance→position compliance fix (#1430)` (006d5c3)
- Sprint 27: `feat(sprint-27): demo polish, Flow 5 compute, website five-flow update` (b1e70a6) — added commons compute (trust-gated task admission) as 5th demo flow.

### 3. CI Gate Graduation
- Multiple CI gates graduated from warning → blocking: A11Y, SDK_TESTS, COMPLIANCE (terminology: payment→settlement, currency→unit, balance→position).
- Dependabot bumps: `actions/checkout` 4→6, `actions/setup-python` 5→6, `actions/upload-artifact` 4→7, `dorny/paths-filter` 3→4.
- CI fix: benchmark workflow concurrency group added; duplicate cargo test job removed from deploy workflow.

### 4. Compute Commons (Sprints 25–26)
- `feat(compute): pre-execution commons credit reservation (#1404)` — two-phase commit via `CommonsReserveCallback` + `CommonsReleaseCallback`.
- `refactor(compute): extract reservation helpers + idempotence tests (#1406)` — centralized reservation helpers, full lifecycle coverage (6 termination paths).
- `feat(sprint-28): gossip fan-out fix — compute task delivery restored (#1419)` — critical multi-subscriber callback fix.

---

## Open Pull Requests (3 open)

### PR #1442 — [WIP] Review and summarize current overall repo state (this PR)
- **Author:** Copilot | **State:** Draft | **Created:** 2026-03-26
- **Purpose:** This document.

### PR #1441 — chore(docs): documentation control plane and tranche-3 doc structure
- **Author:** fahertym | **State:** Open (not draft) | **Created:** 2026-03-26
- **Scope:** Introduces `docs/registry.toml` as machine-readable doc source of truth, `docs/scripts/doc_control_check.py` as validator, CI enforcement via `docs-freshness.yml`. Structural moves: scratch plans → `docs/archive/2026/`; runbooks → `docs/guides/operations/runbooks/`.
- **CI enforcement:** Hard fails on undeclared `docs/` subdirectories, invalid truth_class/role/canonical headers on 4 control-plane docs. Non-blocking warnings for 26 registry classification debt items.
- **Status:** Ready for review. No known blockers.

### PR #1440 — fix(governance): domain_id threading + honest execution outcomes (NotExecuted)
- **Author:** fahertym | **State:** Open (not draft) | **Created:** 2026-03-26
- **Scope:** 4 stacked commits fixing governance effect execution honesty: (1) defer treasury without decision_hash, (2) document mixed `not_executed + success → Confirmed` aggregation behavior, (3) lock `EffectResult` invariants with regression tests, (4) `DistributeSurplus` provenance + honest default execution.
- **Known limitation:** Mixed `success + not_executed → Confirmed` aggregation is accepted model for this tranche — documented and tested, not redesigned.
- **Status:** Active, awaiting final review. CI verification steps documented in PR body.

---

## Open Issues (27 open)

### P0 / Epics
| # | Title | Epic | Type |
|---|-------|------|------|
| #1147 | [EPIC] Vertical Slice: Identity → Governance → Compute → Receipts → Audit | — | P0 |
| #1099 | [EPIC] ICN Pilot Completion - End-to-End Demos | — | P0 |

### Provenance / Arch Invariants (Active Sprint Area)
| # | Title | Priority |
|---|-------|----------|
| #1438 | feat(provenance): federation/auditor-accessible provenance query endpoint | tier:2-observability |
| #1436 | refactor(provenance): disambiguate ProvenanceRef::DirectMember.signature field semantics | tier:2-observability |
| #1435 | fix(provenance): replace JWT-secret-bound HMAC fingerprint with publicly verifiable per-entry signing | tier:1-correctness |
| #1012 | [Wave 6] Legibility Dashboards: UX Spec for Constraint Visibility | spec |
| #1011 | [Wave 5] Constitutional Genesis: Canonical Bootstrap Documentation | spec |
| #1010 | [Wave 4] Adversarial Model: Threat Documentation and Chaos Harness | spec |
| #1009 | [Wave 3] Attestation Model: Canonical Schema and Dispute Pathway | spec |
| #863 | feat(ccl): Federation Agreement Support | impl |
| #862 | feat(kernel): Phase 7 - Naming Primitive | impl |

### Trust Hardening
| # | Title | Tier |
|---|-------|------|
| #1054 | test(trust): validate bottleneck percentages with flamegraph profiling | tier:3-perf |
| #1053 | perf(trust): implement reverse edge index for O(1) input lookup | tier:3-perf |
| #1050 | perf(kernel-api): benchmark PolicyOracle async path overhead | tier:3-perf |
| #1049 | test(trust): audit test coverage for trust_score_detailed error paths | good-first-issue |
| #1048 | feat(obs): add trust query latency histograms | good-first-issue |
| #996 | test(trust): add fault injection and stress tests for cache invalidation | tier:3-perf |

### Commons Compute
| # | Title |
|---|-------|
| #1401 | infra(ci): hung docker-build-deploy Test job starves self-hosted runner |
| #992 | fix(compute): include receipt_hash in SettlementEngine::settle_receipt errors |
| #959 | refactor(compute): evaluate further SignatureBytes wire format optimization |

### Infrastructure / Service Discovery
| # | Title |
|---|-------|
| #937 | test(core): Service discovery integration tests |
| #875 | security(core): Add API-level rate limiting for manifest parsing |
| #873 | perf(core): StateSnapshot copy-on-write optimization |
| #1095 | [PR10] CRDT OrSet + LwwRegister implementation |

### External / Community
| # | Title |
|---|-------|
| #1369 | chore(website): live roadmap page — render roadmap-current.yaml with milestones |
| #1368 | chore(website): community infrastructure — Matrix room, Good First Issues, mailing list |
| #1366 | feat(sdk): React Native SDK stabilization — key management, push notifications, receipt verification |

**Good first issues:** #1049, #1048, #992, #959, #937, #875

---

## Sprint Board (Sprint 26, opened 2026-03-23)

| Task | Title | Status |
|------|-------|--------|
| s26-t1 | Migrate CI coverage to llvm-cov on Zentith self-hosted runner | in-progress |
| s26-t2 | Review dependabot PR #1422: bump dtolnay/rust-toolchain 1.88→1.100 | blocked |
| s26-t3 | Resolve fix/demo-path-bugs branch dirty files | done |
| s26-t4 | Complete Claude agent definition rewrite | in-progress |
| s26-t5 | Unblock CI: main branch CI run stuck in queued | done |
| s26-t6 | Governance→economics provenance hardening (INV-2/5/6): chain endpoint, allocation receipts, /health/full, 4 regression tests | in-review (PR #1434) |

**Previously shipped (Sprint 25):** Pre-execution commons credit reservation (PR #1404/#1405) — two-phase commit for commons tasks with full lifecycle coverage.

---

## Subsystem Reality Check (from Operability Audit 2026-03-25)

### ✅ Fully Proven (live, tested)

| Subsystem | Evidence |
|-----------|----------|
| **Identity / Auth** | Ed25519 DIDs, challenge-response JWT auth (1hr tokens), constant-time verification, 14+ unit tests |
| **Trust graphs** | Three weighted graphs (social/economic/technical), `TrustPolicyOracle` wired for rate limiting |
| **Ledger / Mutual credit** | Double-entry, multi-currency, credit limits, patronage distribution, settlement, budget system |
| **Governance state machine** | 22 proposal payload types, delegation voting, charter hook, 547 governance tests |
| **Compute commons** | Pre-execution credit reservation (two-phase), 6 task termination paths covered |
| **Gateway API** | 20+ governance endpoints, settlement, ledger, receipts, provenance chain queries |
| **Receipt chain** | `AllocationReceipt` + `SettlementIntent`, Blake3 canonical hashes, 6 REST endpoints, `icnctl receipts` commands |
| **Demo flows** | 5 scripted flows operational on K3s (governance, patronage, federation, reporting, commons compute) |

### ⚠️ Gaps Flagged in Audit

| Area | Gap | Severity |
|------|-----|----------|
| Governance execution | **Accepted proposals don't execute most payloads** — only `FreezeMember` and `Charter` have real execution bridges | Critical |
| Provenance fingerprinting | HMAC fingerprint is JWT-secret-bound; federation replicas and auditors cannot verify (#1435) | High |
| Trust→ledger wiring | Trust scores don't gate credit limits in `credit_policy.rs` | High |
| Trust→governance | Tally computation is unweighted regardless of trust score | High |
| Federation clearing | Hard rejection at `settlement.rs:120-129`; cross-coop clearing unimplemented | High |
| Ledger dedup set | In-memory only; HashSet loses state on restart | High |
| API credit ceilings | `ledger.rs:43-44` TODO — not exposed in API | High |

---

## Strategic Context

### Phase Completion Status
| Phase | Status |
|-------|--------|
| Phase 0 — Foundation + 4-flow demo | ✅ Complete (merged Mar 13, 2026) |
| Phase 1 — Federation demo system | ✅ Complete (merged Mar 13, 2026) |
| Phase 2 — Demo-Ready + Pilot pitch | 🚧 In progress (Sprint 17 plan active) |
| Phase 3+ | ⏳ Not started |

### Active Strategic Priorities (Sprint 17 plan, end-of-March deadline)
1. **`icnctl audit verify`** — CLI command walking Decision→Allocation→Execution chain with signature verification (vertical slice punchline)
2. **AllocationProposal** (#1311) — governance proposal type that produces `AllocationReceipt`; closes the 6-link receipt chain
3. **Compliance terminology** (#1303) — payment→settlement, currency→unit, balance→position (required for STF grant application)
4. **Grant applications** — Outta Excuses + Verizon Digital Ready (March 31 deadline)
5. **NY Co-op Summit workshop proposal** (May call-for-presenters)

### Key Architectural Invariants
- **Meaning Firewall**: kernel crates hold only `Arc<dyn Fn(...)>` callbacks — no semantic business logic (enforced; Sprint 22 completed config extraction)
- **Determinism**: same inputs → same outputs → same ledger state (upheld)
- **No panics in protocol paths** (upheld; Result<T,E> throughout)
- **Canonical encodings** (upheld; Blake3 + deterministic hashing)
- **Adversarial-by-default** (upheld; trust gates all resource access)

---

## Infrastructure Notes

- **CI:** GitHub Actions on `ubuntu-latest`; self-hosted runner (`ci-runner`, Zentith) used for coverage only. Benchmark workflow concurrency group added (ADR-0010). Runner starvation issue documented and fixed.
- **Rust toolchain:** Pinned to 1.88.0 in `icn/rust-toolchain.toml`. Dependabot PR #1422 (1.88→1.100) is blocked pending review.
- **K3s cluster:** Live on VLAN 30 (icn-dev, 10.8.30.45). 4 coop pods + monitoring stack (Grafana). Self-hosted runner confirmed running.
- **Build artifacts:** 143GB cleared from icn-dev in March; recurring disk pressure risk noted.

---

## Key Files for Context

| Purpose | Path |
|---------|------|
| Sprint board | `ops/state/sprint/current.json` |
| Sprint history | `ops/state/sprint/history/` |
| Gap analysis | `docs/strategy/ICN-Gap-Analysis-March-2026.md` |
| March sprint plan | `docs/strategy/ICN-Sprint-March17.md` |
| Live roadmap | `docs/strategy/ICN-Roadmap-Live.md` |
| Operability audit | `docs/strategy/operability-audit-2026-03-25.md` |
| Phase history | `docs/PHASE_HISTORY.md` |
| Kernel/app boundary | `docs/architecture/KERNEL_APP_SEPARATION.md` |
| CI status | `docs/ci/CI_CURRENT_STATUS.md` |
| March status report | `docs/status/icn-status-march-2026.md` |
