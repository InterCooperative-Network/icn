---
Status: descriptive
Canonical: yes
Last Reviewed: 2026-04-15
---

# ICN State (living doc)

<!-- [sync edit] 2026-04-15: Consolidated stacked changelog into single current snapshot.
     Aligned crate list, merged PRs, and metrics to verified repo state.
     Phase model unchanged — phase classification is governance territory (PR C). -->

## Current status (2026-04-15 snapshot)

**Current phase:** Phase 2 — Pilot Launch (blocked on cooperative partners).
Active execution: NYCN institutional integration work (meetings, structures, activities, notification digest, Program/Milestone in review). Phase model classification is unchanged; see PHASE_PROGRESS.md for phase definitions and PR C for any future reclassification.

### Recently merged (since last STATE.md update 2026-04-11)

| PR | Title | Merged |
|----|-------|--------|
| #1547 | feat(governance): notification digest + action-item/meeting events | 2026-04-15 |
| #1546 | docs(dev): session handoff 2026-04-15 | 2026-04-15 |
| #1545 | docs(strategy): correct NYCN-Institutional-Design entity tree | 2026-04-15 |
| #1544 | docs(strategy): NYCN repo-shaped architecture spec + matrix + tranches | 2026-04-15 |
| #1543 | feat(governance): Meeting management primitive | 2026-04-15 |
| #1542 | chore(security): fix Security Audit CI failure | 2026-04-14 |
| #1540 | feat(governance): institutional structure + event model (Tranche 2, part 1) | 2026-04-14 |
| #1534 | docs(strategy): NYCN federation charter draft (CCL YAML) | 2026-04-14 |
| #1533 | feat(governance): consent-based decision mode | 2026-04-14 |
| #1532 | feat(governance): decision-to-action bridge | 2026-04-14 |
| #1529 | chore(repo): add GitHub Sponsors funding button | 2026-04-14 |
| #1527 | fix(ci): add timeout-minutes to docker-build-deploy jobs | 2026-04-11 |
| #1526 | docs: full refresh — archive 21 stale files | 2026-04-11 |
| #1525 | docs(architecture): Constitutional Genesis | 2026-04-11 |
| #1524 | fix(ci): add has_rust dual-signal guard | 2026-04-11 |

### Open PRs

| PR | Title | Branch | Status |
|----|-------|--------|--------|
| #1549 | docs(ai): constitutional core + workflow architecture migration | docs/workflow-migration | Open |
| #1548 | feat(governance): Program + Milestone primitives (Tranche 1a) | feat/program-milestone | Open |

### What landed since Phase 1 (Charter Engine)

Governance institutional primitives:
- Governance domains, structures, activities, parent (scope container) — #1540
- Decision-to-action bridge: accepted proposals create linked action items — #1532
- Consent-based decision mode — #1533
- Meeting management (schedule, agenda, attendance, minutes) — #1543
- Notification digest (pending votes, overdue items, upcoming meetings) — #1547
- NYCN architecture docs (repo-shaped spec, implementation matrix, execution tranches) — #1544
- NYCN institutional design correction (layered ontology) — #1545

Infrastructure:
- Security Audit CI fix (wasmtime bump) — #1522, #1542
- CI dual-signal guard — #1524
- Docker-build-deploy timeout fix — #1527
- 21-file doc refresh and archive — #1526

### Architectural decisions in force

- **Layered ontology (locked 2026-04-14):** Entities (sovereign) / Structures (non-sovereign, entity-owned) / Activities (time-bounded, entity-owned). Committees are Structures. Summit is Activity.
- **Program is a separate primitive** (not Activity extension): Milestones with machine-readable checks, parent_program_id for cycle-handoff. Spec in NYCN-Repo-Architecture-Spec.md §5.
- **Authority is capability-string based:** `RoleAssignment.authority_scope: Vec<String>`.
- **Sled key convention:** primary `<thing>:{id}`; secondary `<thing>_by_<scope>:{scope_id}:{id}`.
- **Gateway event naming:** `Governance<Thing><Verb>`.
- **Meaning Firewall preserved:** kernel crates have zero domain imports from governance/trust/ccl/coop.

## Architecture notes

- Repo root is not a Cargo workspace; Rust workspace lives in `icn/`.
- Workspace: 39 library crates + 3 binaries = 42 packages.
  - **Crates:** icn-api, icn-authz, icn-ccl, icn-charter-app, icn-commons, icn-community, icn-compute, icn-coop, icn-core, icn-crypto, icn-crypto-pq, icn-encoding, icn-entity, icn-federation, icn-gateway, icn-gossip, icn-governance, icn-governance-actor, icn-http-kit, icn-identity, icn-kernel-api, icn-ledger, icn-ledger-actor, icn-membership-app, icn-naming, icn-net, icn-obs, icn-privacy, icn-protocol, icn-rpc, icn-security, icn-services, icn-snapshot, icn-steward, icn-store, icn-testkit, icn-time, icn-trust, icn-zkp.
  - **Binaries:** icnd, icnctl, icn-console.
  - **App crates (in `icn/apps/`):** icn-governance-actor, icn-ledger-actor, icn-membership-app, icn-charter-app.
- Web UI: web/pilot-ui (PWA), web/dashboard (static).
- SDKs: sdk/typescript, sdk/react-native.
- Deployment: native/systemd, Docker Compose, Kubernetes, Helm (deploy/README.md).

## Decisions (durable)

- Mutual TLS with client certificates enabled (2025-12-18).
- DID-TLS binding verification enabled.
- Some QUIC/chaos tests ignored in CI due to timing; run manually as needed.

## Constraints (durable)

- Run Rust build/test commands from `icn/`.
- Tokio async only; avoid blocking operations in async paths.
- No panics in protocol/network/actor runtime paths.
- Demo status docs note STUN discovery disabled for local-only testing.

## References

- docs/PHASE_PROGRESS.md — phase tracking
- docs/architecture/KERNEL_APP_SEPARATION.md — kernel/app boundary
- docs/strategy/NYCN-Repo-Architecture-Spec.md — NYCN institutional architecture
- docs/strategy/NYCN-Execution-Tranches.md — NYCN 7-tranche execution plan
- docs/dev/handoff-2026-04-15.md — latest session handoff
- deploy/README.md — deployment options

---

## Historical snapshots

<details>
<summary>2026-04-11 snapshot (PR #1520–#1522)</summary>

- **PR #1520** (website cleanup) merged 2026-04-10
- **PR #1522** (`fix/coop-store-sled-lock`) merged 2026-04-11 — wasmtime bump + sled lock fix
- **PR #1521** closed as superseded by #1522
- Pilot Vertical Slice Hardening sprint complete: #1214, #1221, #1220, #1222
- Issue #862 (naming) closed as superseded — implemented as `icn-naming`
- Issue #1401 (hung docker CI) closed — root cause already removed in #1403

</details>

<details>
<summary>2026-03-18 snapshot (Phase 0 + Phase 1 complete)</summary>

- Phase 1 (Charter Engine) complete — PRs #1336 + #1337
- Charter bridge, CharterPolicyOracle, 5 CCL templates, icnctl charter CLI, ratification flow all landed
- Phase 0 (Close the Demo) complete — all 4 flows passing on K3s cluster
- 4,287 tests, ~420K Rust LOC

</details>

<details>
<summary>2026-03-14 snapshot (Governance Demo Sprint)</summary>

- Fixed: Gateway governance routes 404 (actix-web scope ordering)
- Fixed: Vote tally (CastVote missing voter DID)
- Built: demo pipeline (start-demo.sh, demo-governance.py, demo.html)
- 547 tests passing, cold-start demo 18/18

</details>

<details>
<summary>2026-02-18 snapshot (Economics Consolidation)</summary>

- Sprint 8-10 complete: deterministic economic receipt chain
- CanonicalReceipt, AllocationReceipt, SettlementIntent, ReceiptStore
- 6 REST endpoints for receipt/ledger provenance
- Pilot UI Receipts tab, icnctl receipts commands

</details>

<details>
<summary>2026-01-20 snapshot (Code review findings)</summary>

- Repo-wide TODO scan captured
- Large module candidates: icnctl/main.rs (9445 lines), icn-ledger (5447), icn-gateway governance (4650), icn-core governance_handlers (4243)

</details>
