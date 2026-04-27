---
Status: descriptive
Canonical: yes
Last Reviewed: 2026-04-27
---

# ICN State (living doc)

<!-- [sync edit] 2026-04-27 (post-#1663): Action-card runtime now has
     proof-bearing receipt loops for all three currently emitted source
     paths: proposal/vote (#1660), action_item/complete (#1661), and
     meeting/attend (#1663). Issue #1646 remains open with two RFC-
     gated paths still pending: signal_rule (gated on #1631) and
     obligation_lifecycle (gated on #1634). Phase model unchanged. -->

<!-- [sync edit] 2026-04-27: Append the action-card runtime sequence
     (/me/action-cards endpoint, proposal/vote receipt linkage,
     action_item completion receipt seam) landed via #1659/#1660/#1661.
     Issue #1646 remains open; meeting/attend, signal_rule, and
     obligation_lifecycle source paths remain pending. Phase model
     unchanged. -->

<!-- [sync edit] 2026-04-26: Append the institutional-operability sequence
     (live charter activation, person-directory overlay, /me/standing,
     authority_scope plumbing) and the doctrine/ADR canonicalization that
     landed since 2026-04-15. Open-PR table updated; 4-15 entries kept
     intact below for continuity. Phase model unchanged. -->

<!-- [sync edit] 2026-04-15: Consolidated stacked changelog into single current snapshot.
     Aligned crate list, merged PRs, and metrics to verified repo state.
     Phase model unchanged — phase classification is governance territory (PR C). -->

## Current status (2026-04-27 snapshot)

**Current phase:** Phase 2 — Pilot Launch (blocked on cooperative partners).
Active execution: institutional-operability runtime (live charter activation, person-directory overlay, `/me/standing`, `authority_scope` plumbing) plus the action-card runtime (`/me/action-cards` endpoint with proof-loop linkage to `GovernanceDecisionReceipt` for proposal/vote, `ActionItemCompletionReceipt` for action_item/complete, and `MeetingAttendanceReceipt` for meeting/attend). NYCN package side dogfoods these via the institution-package boundary. Phase model classification is unchanged; see PHASE_PROGRESS.md for phase definitions.

### Recently merged (since 2026-04-15)

| PR | Title | Merged |
|----|-------|--------|
| #1663 | feat(governance): add meeting attendance receipts | 2026-04-27 |
| #1662 | docs(state): record action-card runtime landing (#1659/#1660/#1661) | 2026-04-27 |
| #1661 | feat(governance): add action item completion receipts | 2026-04-27 |
| #1660 | feat(governance): connect action cards to receipts | 2026-04-27 |
| #1659 | feat(gateway): add member action cards endpoint | 2026-04-27 |
| #1658 | docs(sync): record ICN Academy repo creation | 2026-04-27 |
| #1656 | docs(site): add curated docs pathways | 2026-04-27 |
| #1637 | docs: reframe feedback doctrine and canonicalize ADR location | 2026-04-26 |
| #1630 | feat(governance): plumb authority_scope through assign_role end-to-end | 2026-04-25 |
| #1627 | feat(governance): add GET /me/standing read model | 2026-04-25 |
| #1626 | feat(governance): person-directory overlay for bootstrap role assignment | 2026-04-25 |
| #1625 | fix(coop): release sled db lock before reopen test | 2026-04-25 |
| #1624 | feat(governance): live charter activation endpoint | 2026-04-25 |
| #1622 | docs(strategy): institutional ecosystem arc — NYCN as first ecosystem seed | 2026-04-24 |
| #1621 | fix(governance): persist domains across gateway restart in standalone mode | 2026-04-24 |
| #1620 | fix(web): derive steward dashboard gateway URL from request context | 2026-04-24 |
| #1619 | feat(infra): add soft pod anti-affinity for ICN daemons | 2026-04-23 |
| #1618 | feat(ci): add Atlas-backed sccache setup for ci-runner | 2026-04-23 |
| #1617 | fix(bootstrap): treat remaining create conflicts as idempotent | 2026-04-22 |
| #1616 | docs(monitoring): document Helm access path for kube-prometheus-stack upgrade | 2026-04-22 |
| #1614 | fix(monitoring): move Prometheus to Atlas-backed persistent storage | 2026-04-22 |
| #1593 | docs(nycn): live-validate bootstrap apply and rewrite runbook | 2026-04-19 |
| #1592 | test(icnctl): NYCN bootstrap apply integration tests | 2026-04-19 |
| #1591 | fix(gateway): colon-safe proposal index keys with one-shot migration | 2026-04-19 |
| #1590 | fix(governance): close residual acceptance-closure atomicity hazards | 2026-04-18 |
| #1586 | feat(governance): add generic institution bootstrap package path | 2026-04-18 |

### Recently merged (2026-04-15 snapshot, retained)

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
| #1636 | chore(toolchain): upgrade Rust 1.88.0 → 1.95.0 | copilot/upgrade-rust-1-88-to-1-95 | Open — fmt fix pushed; tests running |

### What landed since Phase 1 (Charter Engine)

Action-card runtime (added 2026-04-27, all currently emitted source paths now proof-bearing — issue #1646 remains open for the two RFC-gated paths):
- `GET /v1/gov/me/action-cards` member endpoint with closed source/action enums — #1659
- Proposal/vote action card → `GovernanceDecisionReceipt` proof linkage, end-to-end test — #1660
- `action_item`/`complete` source path emits append-only `ActionItemCompletionReceipt` (ADR-0026 Layer 2); persist-before-commit semantics; full-update handler routes status changes through receipt-bearing path — #1661
- `meeting`/`attend` source path emits append-only `MeetingAttendanceReceipt` (ADR-0026 Layer 2) keyed by `(meeting_id, attendee_did)`; `Present` and `Remote` are receipt-bearing transitions, `Absent` is not; `recorded_by` is the authenticated caller (distinct from `attendee_did` for steward-recorded attendance); persist-before-commit semantics — #1663
- Source paths currently emitted by `/me/action-cards`: `proposal`/`vote`, `meeting`/`attend`, `action_item`/`complete`
- **Proof loop verified end-to-end for all three currently emitted source paths.**
- Pending under #1646 (RFC-gated): `signal_rule` source path (gated on #1631); `obligation_lifecycle` source path (gated on #1634)

Institutional-operability runtime (added 2026-04-22 → 2026-04-26):
- Generic institution bootstrap package path — #1586
- Bootstrap-apply 409 idempotency for repeated bootstrap runs — #1617
- Persistent governance domains across gateway restart in standalone mode — #1621
- Live charter activation endpoint — #1624
- Person-directory overlay for bootstrap role assignment (DID binding) — #1626
- `GET /me/standing` read model — #1627
- `authority_scope` plumbed end-to-end through `assign_role` — #1630
- Feedback/support doctrine rename + ADR canonicalization under `docs/adr/` — #1637
- NYCN bootstrap apply integration tests + live-validate runbook — #1592, #1593

Governance institutional primitives:
- Governance domains, structures, activities, parent (scope container) — #1540
- Decision-to-action bridge: accepted proposals create linked action items — #1532
- Consent-based decision mode — #1533
- Meeting management (schedule, agenda, attendance, minutes) — #1543
- Notification digest (pending votes, overdue items, upcoming meetings) — #1547
- NYCN architecture docs (repo-shaped spec, implementation matrix, execution tranches) — #1544
- NYCN institutional design correction (layered ontology) — #1545
- Residual acceptance-closure atomicity hazards closed — #1590
- Colon-safe proposal index keys with one-shot migration — #1591

Infrastructure:
- Atlas-backed Prometheus persistent storage — #1614
- Atlas-backed sccache for ci-runner — #1618
- Soft pod anti-affinity for ICN daemons — #1619
- Helm path documented for kube-prometheus-stack — #1616
- Steward dashboard derives gateway URL from request context — #1620
- Security Audit CI fix (wasmtime bump) — #1522, #1542
- CI dual-signal guard — #1524
- Docker-build-deploy timeout fix — #1527
- 21-file doc refresh and archive — #1526

### Architectural decisions in force

- **Layered ontology (locked 2026-04-14):** Entities (sovereign) / Structures (non-sovereign, entity-owned) / Activities (time-bounded, entity-owned). Committees are Structures. Summit is Activity.
- **Program is a separate primitive** (not Activity extension): Milestones with machine-readable checks, parent_program_id for cycle-handoff. Spec in NYCN-Repo-Architecture-Spec.md §5.
- **Authority is capability-string based today, typed model frozen for migration:** `RoleAssignment.authority_scope: Vec<String>` remains the shipped surface; the constitutional object model (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`) is frozen in [ADR-0014](adr/ADR-0014-constitutional-object-model.md) and is the target of a subsequent additive migration. No behavior change has shipped yet.
- **Sled key convention:** primary `<thing>:{id}`; secondary `<thing>_by_<scope>:{scope_id}:{id}`.
- **Gateway event naming:** `Governance<Thing><Verb>`.
- **Meaning Firewall:** CI ratchet enforces no new kernel/domain import regressions. Pre-existing domain imports in icn-core and icn-gateway remain; full extraction is ongoing work.

## Architecture notes

- Repo root is not a Cargo workspace; Rust workspace lives in `icn/`.
- Workspace: 35 crates in `icn/crates/` + 4 app crates in `icn/apps/` + 3 binaries = 42 packages.
  - **Crates (in `icn/crates/`):** icn-api, icn-authz, icn-ccl, icn-commons, icn-community, icn-compute, icn-coop, icn-core, icn-crypto, icn-crypto-pq, icn-encoding, icn-entity, icn-federation, icn-gateway, icn-gossip, icn-governance, icn-http-kit, icn-identity, icn-kernel-api, icn-ledger, icn-naming, icn-net, icn-obs, icn-privacy, icn-protocol, icn-rpc, icn-security, icn-services, icn-snapshot, icn-steward, icn-store, icn-testkit, icn-time, icn-trust, icn-zkp.
  - **App crates (in `icn/apps/`):** icn-governance-actor, icn-ledger-actor, icn-membership-app, icn-charter-app.
  - **Binaries:** icnd, icnctl, icn-console.
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
