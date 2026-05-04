---
Status: descriptive
Canonical: yes
Last Reviewed: 2026-05-04
---

# ICN State (living doc)

<!-- [sync edit] 2026-05-04 (post-#1725 / NYCN-#53 / #1732):
     Documentation and public-surface truth-sync. ICN #1725
     landed the generic no-CLI organizer/member rehearsal
     workflow spec at docs/pilots/no-cli-organizer-member-
     rehearsal-workflow.md — review-first, mutation-last,
     three roles distinguished (organizer / steward-operator /
     future member), accessibility baseline release-blocking
     for any user-facing shell, evidence export repo-safe by
     default. NYCN #53 landed the NYCN companion at
     docs/NO-CLI-ORGANIZER-REHEARSAL-WORKFLOW.md (in
     fahertym/nycn). ICN #1732 landed the website README
     civic design truth-sync, replacing stale Lexend / hex
     palette / "modern design system" / Tailwind framing
     with pointers to docs/design-language/ and
     website/src/styles/global.css as the single token
     surface. These are documentation and presentation-
     readiness changes only. No Phase 2 status change. NYCN
     remains the intended first cooperative partner, not a
     formally committed pilot. Implementation follow-ups
     remain open: ICN #1726-#1731 plus #1713 (organizer
     rehearsal shell, fixture-backed demo mode, generic
     preview/review read-model contract, repo-safe evidence
     export schema, private-overlay/DID-binding activation
     flow, accessibility review gate, ActionCard schema
     stabilization); NYCN #54-#58 (presentation wireframe
     deck, fixture demo packet, evidence packet example,
     holder-label/DID activation policy, accessibility/
     privacy checklist). Next substantive implementation
     path remains ICN #1729 (repo-safe evidence export
     schema). Do not read this sync as production readiness,
     live federation integration, implemented service
     hosting, K3s/DNS/Forgejo mutation, NYCN private-data
     handling, or resolved licensing. -->

<!-- [sync edit] 2026-05-02 (post-#1695/#1696/#1697/#1698/#1699/#1700/#1701):
     Follow-up May-cycle queue is now merged on main: Dependabot
     Actions major-version bumps (#1695-#1698), wasmtime security bump
     (#1699), unified dev-environment bootstrap (#1700), and the prior
     state sync (#1701). Open PR queue is empty at this sync. Phase 2
     remains in progress. NYCN remains the intended first cooperative
     partner, not a formally committed pilot. The exact next gate is
     defined in docs/strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md:
     organizer presentation -> pilot formalization -> first operator
     rehearsal. Do not read this sync as production readiness, live
     federation integration, implemented service hosting, K3s/DNS/
     GitHub/Forgejo mutation, NYCN private-data handling, or resolved
     licensing. -->

<!-- [sync edit] 2026-04-29 (post-NYCN-#32, ICN PR queue clean):
     NYCN drive-ingest work has continued past the operator
     ladder + runbook landed in #28. Now also merged on the
     NYCN side: organizer briefing + simple summit demo (#29);
     start-here onboarding pass with quickstarts and glossary
     (#30); one-command local preflight runner (#31); whole-
     NYCN operating-surfaces inventory + Google-Groups boundary
     policy + repo-safe communication-groups fixture (#32). A
     small steward-facing communication-groups directory tool
     was open as NYCN #33 at last sync and may have merged
     since — verify before reading. ICN PR #1665 (Dependabot
     TS SDK dev-deps) merged 2026-04-29; ICN open-PR queue is
     empty as of this sync. The cooperator-developer prep
     brief landed alongside this sync at
     docs/strategy/COOPERATIVE_DEVELOPER_DISCOVERY_BRIEF.md.
     Phase 2 framing unchanged from the prior sync: NYCN is
     the intended first cooperative partner (not yet formally
     committed); the next concrete step is presenting the
     merged ladder + ICN proof-loop machinery to NYCN
     organizers; partnership formalization and first operator
     pilot rehearsal remain. Issue #1646 still open;
     signal_rule and obligation_lifecycle source paths remain
     RFC-gated. -->

<!-- [sync edit] 2026-04-29 (post-#1675/#1677, post-NYCN-#28):
     Action-item completion-receipt retrieval endpoint is now live
     (`GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt`,
     #1675). Local HTTP proof loop closure for the action-item path
     is recorded in #1676; the operator-authorized K3s NYCN smoke
     proof closure against deployed image 91a63eec is recorded in
     #1677. NYCN's drive-ingest operator ladder (#21–#28 in
     fahertym/nycn) is now merged: parser → review → decisions →
     publish dry-run → assignee binding → local publisher → local
     proof runner → federation surface bridge → operator pilot
     runbook + ladder checker. The procedural spine that walks
     organizer material into ICN action-item proofs is real.
     Phase 2 framing change: NYCN is the intended first
     cooperative partner; not yet a formally committed pilot.
     The next concrete step is **presenting the merged ladder
     + ICN proof-loop machinery to NYCN organizers** to
     formalize the pilot partnership. Subsequent gates are
     partnership formalization and the first operator pilot
     rehearsal against real (or fixture-equivalent) organizer
     material. Phase 2 remains ⏳ until those happen and are
     recorded. Issue #1646 still open; signal_rule and
     obligation_lifecycle source paths remain RFC-gated. -->

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

## Current status (2026-05-02 snapshot)

**Current phase:** Phase 2 — Pilot Launch. NYCN is the intended first cooperative partner (active partnership track, not yet a formally committed pilot). The next concrete step is presenting the merged drive-ingest ladder + ICN proof-loop machinery to NYCN organizers. Subsequent gates are pilot formalization, then first operator rehearsal against real (or fixture-equivalent) organizer material. The exact gate is defined in [NYCN Phase 2 Pilot Rehearsal Gate](strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md). The Phase 2 *machinery* is in place end-to-end; what remains is the human procedure — present, formalize, rehearse — and recording each step.
Active execution: institutional-operability runtime (live charter activation, person-directory overlay, `/me/standing`, `authority_scope` plumbing) plus the action-card runtime (`/me/action-cards` endpoint with proof-loop linkage to `GovernanceDecisionReceipt` for proposal/vote, `ActionItemCompletionReceipt` for action_item/complete, and `MeetingAttendanceReceipt` for meeting/attend). The action-item completion-receipt retrieval endpoint shipped as #1675; the local HTTP proof loop closure is documented in #1676 and the K3s smoke proof closure is recorded in #1677. NYCN's drive-ingest operator ladder (NYCN #21–#28 in `fahertym/nycn`) is merged end-to-end, with subsequent NYCN #29–#32 also merged: organizer briefing + summit demo, start-here onboarding pass, one-command local preflight runner, and whole-NYCN operating-surfaces inventory plus Google-Groups boundary policy. NYCN #33 (steward-facing communication-groups directory tool) was open at the prior sync and may have merged since. Since then, ICN also merged repo-governance and strategy documentation for licensing metadata, RFC-0017 activation, repo-record generation, a generated repo-record snapshot, licensing/autonomy planning, and sovereign service hosting design, followed by CI/dependency maintenance, unified bootstrap docs, and the prior state sync. Those are documentation/control-plane or maintenance landings only: RFC-0017 is not implemented, service hosting is not deployed, DNS/K3s/GitHub/Forgejo state was not mutated, NYCN private data was not handled, and licensing is not resolved. Phase model classification is unchanged; see PHASE_PROGRESS.md for phase definitions.

### Recently merged (since 2026-04-15)

| PR | Title | Merged |
|----|-------|--------|
| #1701 | docs(state): sync May-cycle project truth | 2026-05-02 |
| #1700 | chore: unify dev environment setup into scripts/bootstrap.sh | 2026-05-02 |
| #1699 | fix(compute): bump wasmtime for RUSTSEC-2026-0114 | 2026-05-02 |
| #1698 | ci: bump actions/setup-node from 4 to 6 | 2026-05-02 |
| #1697 | ci: bump actions/checkout from 4 to 6 | 2026-05-02 |
| #1696 | ci: bump actions/github-script from 8 to 9 | 2026-05-02 |
| #1695 | ci: bump softprops/action-gh-release from 2 to 3 | 2026-05-02 |
| #1694 | docs(architecture): add sovereign service hosting stack | 2026-05-02 |
| #1693 | docs(licensing): add autonomy-focused strategy matrix | 2026-05-02 |
| #1691 | docs(project-index): add generated repo record snapshot | 2026-05-01 |
| #1690 | docs(project-index): add full repo record protocol | 2026-05-01 |
| #1688 | docs(rfcs): RFC-0017 draft → active (Tool Install Infrastructure) | 2026-05-01 |
| #1686 | docs(licensing): document current license metadata and open questions | 2026-05-01 |
| #1678 | docs(state): sync to post-#1675/#1677 and post-NYCN-#28 reality | 2026-04-29 |
| #1665 | deps(ts-sdk): bump the dev-dependencies group in /sdk/typescript with 2 updates | 2026-04-29 |
| #1677 | docs(dev): record K3s NYCN action-item receipt proof path | 2026-04-29 |
| #1676 | docs(dev): record action-item completion receipt endpoint | 2026-04-29 |
| #1675 | feat(governance): add completion-receipt endpoint for action items | 2026-04-29 |
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

No open PRs at this sync.

### What landed since Phase 1 (Charter Engine)

May-cycle repo governance and strategy documentation (added 2026-05-01 → 2026-05-02; documentation/control-plane only, not runtime deployment):
- Licensing metadata and open questions documented — #1686.
- RFC-0017 moved from draft to active for Tool Install Infrastructure — #1688. Active means accepted for implementation; it does not mean the tool install infrastructure is implemented.
- Full repo-record protocol/generator added — #1690.
- Generated ICN repo-record snapshot added — #1691. This is a mechanical inventory snapshot, not an interpretive atlas.
- Licensing/autonomy strategy matrix added — #1693. Planning only; no relicensing happened.
- Sovereign service hosting stack added — #1694. Design direction only; no Forgejo deployment, DNS mutation, K3s mutation, hosted-service rollout, or GitHub cutover happened.
- Follow-up maintenance/state queue merged — #1695-#1701. This includes CI action bumps, a wasmtime security bump, unified bootstrap setup, and a prior state sync; none of these changes starts a NYCN pilot or completes Phase 2.
- NYCN organizer/operator rehearsal gate defined — [docs/strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md](strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md). The gate remains organizer presentation -> pilot formalization -> first operator rehearsal.

Action-card runtime (added 2026-04-27 → 2026-04-29, all currently emitted source paths now proof-bearing — issue #1646 remains open for the two RFC-gated paths):
- `GET /v1/gov/me/action-cards` member endpoint with closed source/action enums — #1659
- Proposal/vote action card → `GovernanceDecisionReceipt` proof linkage, end-to-end test — #1660
- `action_item`/`complete` source path emits append-only `ActionItemCompletionReceipt` (ADR-0026 Layer 2); persist-before-commit semantics; full-update handler routes status changes through receipt-bearing path — #1661
- `meeting`/`attend` source path emits append-only `MeetingAttendanceReceipt` (ADR-0026 Layer 2) keyed by `(meeting_id, attendee_did)`; `Present` and `Remote` are receipt-bearing transitions, `Absent` is not; `recorded_by` is the authenticated caller (distinct from `attendee_did` for steward-recorded attendance); persist-before-commit semantics — #1663
- `GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt` retrieval endpoint — #1675; closes the proof loop on the read side so a holder shell that completed an `action_item`/`complete` action card can fetch the persisted `ActionItemCompletionReceipt` over HTTP instead of relying on in-process tests or on-disk Sled inspection. Authorization mirrors the rest of the action-item read surface (`governance:read` scope plus domain membership; the receipt's bound `domain_id` is asserted to match the path parameter so cross-domain probes are rejected).
- Local HTTP proof loop closure recorded in `docs/dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md` — #1676.
- K3s smoke proof closure (operator-authorized, against deployed image `91a63eec`) recorded in `docs/dev/NYCN_K3S_PROOF_PATH.md` — #1677. K3s smoke records remain durable devnet proof artifacts; full namespaced teardown semantics are not yet specified (tracking issue planned).
- Source paths currently emitted by `/me/action-cards`: `proposal`/`vote`, `meeting`/`attend`, `action_item`/`complete`
- **Proof loop verified end-to-end for all three currently emitted source paths, both locally and on K3s.**
- Pending under #1646 (RFC-gated): `signal_rule` source path (gated on #1631); `obligation_lifecycle` source path (gated on #1634)

NYCN drive-ingest operator ladder (added 2026-04-29; lives in `fahertym/nycn`):
- Parser → review artifact (`drive-ingest-review/v1`) — NYCN #21, #22
- Review decisions YAML (organizer-authored)
- Publish dry-run (`drive-ingest-action-item-publish-dry-run/v1`) — NYCN #23
- Assignee binding (`drive-ingest-action-item-publish-dry-run-bound/v1`) — NYCN #24
- Local publisher (`drive-ingest-local-publish-plan/v1`; preflight default, execute fenced behind two operator flags + localhost-only `--gateway`) — NYCN #25
- Local proof runner (`drive-ingest-local-proof/v1`; walks `/me/action-cards` → `PUT .../status` → `GET .../completion-receipt`) — NYCN #26
- Federation surface bridge (`drive-ingest-federation-surface/v1`; pure file-in/file-out summary records keyed on the cross-node deterministic blake3 `record_hash` from `ActionItemCompletionReceipt`) — NYCN #27
- Operator pilot runbook + no-network ladder checker — NYCN #28
- Organizer briefing + simple summit demo (partner-facing, civic tone, anti-pitch) — NYCN #29
- Start-here onboarding pass (`START_HERE.md`, `ORGANIZER_QUICKSTART.md`, `STEWARD_QUICKSTART.md`, `GLOSSARY.md`) — NYCN #30
- One-command local preflight runner (`local_preflight_runner` orchestrating the full chain in a single deterministic, no-network run; preserves both human-review boundaries) — NYCN #31
- Whole-NYCN operating-surfaces inventory + Google-Groups boundary policy + repo-safe communication-groups fixture (no live sync, no private data committed) — NYCN #32
- Steward-facing communication-groups directory tool (`tools/nycn-ops`; pure file-in / file-out validator + renderer) — NYCN #33 (open at last sync; verify status before reading)
- The ladder defends a hard mutation boundary: every layer is either pure (no network) or localhost-only operator-gated. K3s mutation is never allowed by NYCN-side tools. ICN-side K3s exercise lives in `docs/dev/NYCN_K3S_PROOF_PATH.md` (#1677), not in the NYCN repo.

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
- docs/architecture/THE_COMMONS.md — Capital-C Commons doctrine (what ICN exists to enable)
- docs/architecture/MEMBER_STANDING.md — `/me/standing` design contract (member-facing standing + accessibility)
- docs/architecture/KERNEL_APP_SEPARATION.md — kernel/app boundary
- docs/strategy/NYCN-Repo-Architecture-Spec.md — NYCN institutional architecture
- docs/strategy/NYCN-Execution-Tranches.md — NYCN 7-tranche execution plan
- docs/strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md — exact Phase 2 organizer/operator gate before a formal NYCN pilot begins
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
