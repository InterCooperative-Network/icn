---
Status: descriptive
Canonical: no
Last Reviewed: 2026-07-27
---

# Current Truth Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This map is a fast pointer at those, not a parallel record.

This is a one-screen routing doc for the question *"what is real right now?"*. The per-PR record is in `STATE.md` (with stacked `[sync edit]` annotations) and the phase model is in `PHASE_PROGRESS.md`. This map exists to keep a fresh reader from confusing those with strategy docs, archive docs, or older planning material.

## Phase position

- **Phase 0** (Close the Demo) — ✅ complete (2026-03-18).
- **Phase 1** (Charter Engine) — ✅ complete (2026-03-18). YAML charter documents produce kernel-enforced constraints.
- **Phase 2** (Pilot Launch) — ⏳ **in progress**, not complete.

> **Phase 2 is in progress. NYCN is the intended first cooperative partner — active partnership track, not yet a formally committed pilot.**

The software side of the current wedge — the **Rehearsal Node organizer→member loop** — is merged and witnessed on an assembled image. What remains is human procedure: the real organizer presentation, pilot formalization, and the first operator rehearsal.

## What is real now

> Current `origin/main` is `425f513f24d7f45130273770f346e8b5bdddbf9f` (2026-07-26).

**Architecture boundary (merged 2026-07-22 → 2026-07-25).** The meaning firewall has one authoritative crate taxonomy and a gate that can actually fail (A1, `4bdae326`, #2452) — before this, the required check `exit 0`'d unconditionally and 17 hand-copied crate lists disagreed about which crates were kernel. A1 **measures** kernel/app separation honestly; it completes nothing and removes no dependency edge, and 16 `icn-core` boundary-debt edges are pinned as admitted debt. B0 (`c1ea355e`, #2454) inverted the community edge: `icn-core` has **zero direct `icn-community` dependency and zero direct `icn_community::` source references**, with construction and gossip-merge ownership moved to the `icnd` composition root. **This is not graph isolation** — `icn-core → icn-gateway → icn-community` still reaches. **B1 (the ledger edge) failed its design gate on 2026-07-25 and was never implemented. B2 has not begun.**

**Deployment boundary (merged 2026-07-26).** Public `main` no longer invokes a private registry, SSH, K3s rollout, self-hosted runner, or homelab cleanup (`75d15750`, #2455); the public lane is a generic OCI build with `push: false`. **Only the automatic private deployment from public CI was retired** — Kubernetes, K3s, and Helm remain optional operator material.

**Appliance on current-main content (witnessed 2026-07-26).** The demo-payload mode defect is fixed and merged (`425f513f`, #2456). The assembled single-node appliance was witnessed at build head `67a6566e`, whose tree is byte-identical to `main`: clean boot and firstboot, systemd `icnd` health, organizer and member rehearsal flows, least-privilege negatives, wrong-digest rejection, completion receipt created and re-fetched, outbound isolation, service restart, full VM reboot (`check.sh` 40/0). **Durable across reboot:** node identity, machine ID, config/genesis hashes, and the completion receipt (the boot ID changed, proving a real reboot). **Intentionally ephemeral:** the rehearsal workspace view, reconstructed by reseeding — do not read this as general workspace durability.

These surfaces exist, are merged to `main`, and were exercised end-to-end in the 2026-07-13 assembled-image KVM witness (image built from clean `main` `8c0fe926`, restrict=on boot) and re-exercised in the 2026-07-26 current-main-content witness described above:

- **Rehearsal organizer review→confirm runtime** (#2406) — build-mode-gated (`ICN_GOVERNANCE_BUILD_MODE=rehearsal`; routes 404 in every other mode), three narrow scopes, BLAKE3 `preview_digest` binding confirm to the exact previewed plan (wrong/stale digest → 409, fail-closed), confirm executes the real ADR-0026 ladder and creates one real action item.
- **Member-shell organizer surface** (#2407) — `web/member-shell` `?surface=organizer`, live-only guided review→confirm in the browser; axe-clean automated a11y (the human/AT pass is still owed under #2041).
- **Appliance wiring + no-paste launcher** (#2408) — `icn-demo-seed --session organizer|member` (least-privilege role JWTs, fresh member session — never a token upgrade), `icn-demo-verify --rehearsal` steward verifier.
- **Committed reproducible walkthrough driver** (#2409) — `deploy/appliance/smoke/smoke-local.sh --demo` drives the full loop + role negatives; this is the harness a recurring assembled-image lane (#2398) will run.
- **Member completion loop** — standing → action card → completion (narrow `governance:action-item:complete` scope, #2402) → durable completion receipt (survives restart).
- **Evidence export + steward verification** (#2394) — `urn:icn:contract:rehearsal-workflow-evidence:v1`, no DIDs/credentials exported; tampered packet rejected fail-closed.
- **Trusted-local appliance issuance** (#2396/#2397) — `icnctl … --local-mint` signs demo-session JWTs in-process with the node's own first-boot secret; `/auth/verify` stays fail-closed (#2075). This is appliance-local operator bootstrap, **not** production trusted issuance (#2080 open).

## What is not yet real

- **The human gates.** No organizer presentation has occurred (#1703/#1746; partner-side nycn #41/#52). No human assistive-technology pass (#2041). These are the project's primary open gates — software polish does not substitute.
- **Production trusted issuance** (#2080) — how institutions issue real positive authority remains open; the appliance's local mint does not generalize.
- **Recurring assembled-image CI** (#2398) — the walkthrough is protected manually (witnessed at `8c0fe926`); no scheduled runner builds and boots a fresh image per main advance yet.
- **Live federation / two-node** — Rehearsal Node v0.2 territory; nothing federates in production. **No two-node appliance proof has been executed.** When one is, it must separate transport connectivity, peer identity, enrollment/authority, state synchronization, receipts, and federation — two development nodes exchanging data is not live federation.
- **Cross-node community gossip** — dormant. Production wiring subscribes to `community:updates` without creating it, the gossip layer rejects publish to an undeclared topic under the reject policy, and publish failure is logged *after* local mutation, so peers can diverge (#2457). Pre-existing; B0 neither introduced nor fixed it, and #2457 explicitly withholds authorization for an opportunistic patch.
- **Kernel/app separation** — measured, not achieved. A1 pinned 16 `icn-core` boundary-debt edges; B0 removed one direct edge; **B1 was refused at design review** and composition-root consolidation must land before it can be retried.
- **An adopted deployment decision** — ADR-0086 (#2458) is `proposed`, not adopted, and `implementation_status: partially implemented`. **Merging a proposed ADR does not adopt it**; the `status:` field in `docs/adr/` owns that fact. Only the appliance profile has a retained witness. **Independent appliance restoration is blocked**: `icnctl backup` omits `/etc/icn/icnd.env`, so a node restored from a backup alone cannot reopen its keystore.
- **A settled evaluator package identity** — the lane shipped under "Common Sense (bootable) vertical slice", which was never an ICN-ratified identity; the correction to `icn-portable-evaluator` is PR #2435. **Read the live value from `deploy/appliance/evaluator/package-spec.env` (`PKG_STEM`)**, not from this map. Regardless of that: release payloads are genuine, and tags/assets at or below 0.0.3 are retained unchanged for checksum continuity — never renamed, never deleted.
- **Disclosure enforcement** — rehearsal privacy is by exclusion; `ScopedVault`/`DisclosurePolicy` remain design-only.
- **Provider-boundary slice 3** (#2393) — operational config categories (deploy/, scripts/, workflow literals) still carry concrete values.
- **K3s/devnet operational liveness** — an ops claim needing re-confirmation (`docs/status.toml`); do not present as currently proven.

## Open gates

| Gate | Owner / track | What unblocks it |
|---|---|---|
| NYCN organizer presentation | Matt + NYCN organizers | Schedule it; the facilitator gate package (nycn#100/#101) is steward-operable |
| Pilot formalization | NYCN organizers | Outcome of presentation; explicit cooperative consent |
| First operator rehearsal | NYCN + ICN ops | Recorded run per REHEARSAL-0004 |
| Human assistive-technology pass | Matt (human/AT) | Real screen-reader/keyboard/zoom run against member-shell (#2041) |
| Recurring assembled smoke | Infra | Runner with KVM + image-build capacity (#2398) |
| Production trusted issuance | Design + human review | #2080 architecture decision |
| Deployment-profile adoption | Matt (human decision) | Review and accept or amend ADR-0086 (PR #2458); it is `proposed`, and merging it does not adopt it |
| Community topic ownership | Design + human review | #2457 — decide topic owner, access rules, startup ordering, and failure semantics before any code lands |
| B1 ledger edge | Architecture review | One authoritative composition root, one authoritative ledger implementation, typed recovery commands, explicit authority, durable workflow evidence |

## Active risks

- **Mistaking strategy docs for current state.** `docs/strategy/*` carries long-arc planning; `STATE.md` and `PHASE_PROGRESS.md` carry truth.
- **Mistaking archive material for current state.** Anything under `docs/archive/` or with a snapshot marker is historical.
- **Overclaiming NYCN integration.** NYCN is the *intended* first partner; there is no formal commitment and no live integration.
- **Mistaking the witness for validation.** The assembled-image witnesses (2026-07-13 at `8c0fe926`, 2026-07-26 at current-main content) are automated evidence at one commit; they close no human gate.
- **Mistaking A1/B0 for finished kernel/app separation.** A1 made the firewall honest and B0 removed one direct edge. Sixteen `icn-core` boundary-debt edges remain pinned, the community crate is still transitively reachable through `icn-gateway`, and B1 was refused. "The firewall is clean" is not a claim this repository supports.
- **Mistaking the CI boundary change for dropping Kubernetes.** Only the *automatic private deployment from public CI* was retired. Kubernetes, K3s, and Helm remain optional operator deployment forms.
- **Mistaking durable identity for durable state.** The appliance keeps its identity and completion receipts across reboot; the rehearsal workspace view is intentionally ephemeral and is reseeded.

## Where to go next

| You want... | Read |
|---|---|
| The per-PR record | [`docs/STATE.md`](../../STATE.md) |
| The phase model | [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md) |
| What surfaces exist today | [`runtime-surface-map.md`](runtime-surface-map.md) |
| What is or isn't show-ready | [`show-readiness-map.md`](show-readiness-map.md) |
| The rehearsal runbook | [`docs/demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md`](../../demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md) |
