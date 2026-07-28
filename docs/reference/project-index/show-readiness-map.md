---
Status: descriptive
Canonical: no
Last Reviewed: 2026-07-28
---

# Show-Readiness Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This map describes what is *show-ready* — the line between what is honest to demonstrate and what should not be presented as finished. For the broader claim-discipline rules these red lines sit inside, see [`claim-boundaries.md`](claim-boundaries.md).

This document is the load-bearing one for outside-facing conversations. Anyone presenting ICN to organizers, cooperative developers, technically curious contributors, or grant reviewers should walk this list before the meeting.

## What can be shown now

These are real and may be presented honestly:

- **The project thesis.** ICN is digital public infrastructure for cooperatives, communities, and federations. It is a constraint engine: apps translate meaning into constraints; the kernel enforces constraints without understanding meaning. (See [`docs/architecture/THE_COMMONS.md`](../../architecture/THE_COMMONS.md) and [`docs/genesis.md`](../../genesis.md).)
- **The public site.** [`intercooperative.network`](https://intercooperative.network) — including [What is ICN](https://intercooperative.network/what-is-icn), [What's Real Now](https://intercooperative.network/whats-real-now), [For Cooperatives](https://intercooperative.network/for-cooperatives), and [Get Involved](https://intercooperative.network/get-involved).
- **The Rehearsal Node organizer→member loop.** The current show story. On an appliance built from clean `main`: an organizer opens a no-paste browser session, reviews a pending item, edits, assigns, sees an exact preview, and confirms — the confirm is digest-bound (a stale or tampered preview fails closed) and executes the real governance ladder, creating one real action item; a member (fresh least-privilege session) completes it and a durable completion receipt binds the act. Evidence exports without identities or credentials and a steward verifier accepts it (and rejects a tampered packet). Witnessed end-to-end on a fresh assembled image 2026-07-13 (`docs/STATE.md`, the dated `[sync edit] 2026-07-13` block — the file is append-only and newest-first, so find it by date, not by position; re-exercised in the 2026-07-26 current-main-content witness recorded in the `2026-07-27` block; runbook: [`docs/demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md`](../../demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md)).
- **The proof-loop story.** Decision → action → receipt is real end-to-end for the three currently emitted action-card source paths (proposal/vote, action_item/complete, meeting/attend), locally and on the assembled appliance image.
- **The sovereign single node, on current-main content.** An assembled Debian appliance built from a tree byte-identical to `main` `425f513f` was witnessed on 2026-07-26 through clean boot and first boot, `icnd` under systemd returning health, the organizer and member rehearsal flows, least-privilege negatives, wrong-digest rejection, a completion receipt created and re-fetched, outbound isolation, service restart, and a full VM reboot. It is honest to say **the node kept its identity and the one completion receipt the witness created across a real reboot** (boot ID changed; identity, machine ID, config and genesis hashes did not; that specific completion receipt remained re-fetchable). The witness covered a single completion receipt — do not generalise it to receipt durability at large. It is **not** honest to imply general state durability — the rehearsal workspace view is intentionally ephemeral and is reseeded.
- **An honest architectural boundary, honestly measured.** The meaning firewall now has one authoritative crate taxonomy and a gate that can fail (A1, #2452) — worth showing precisely *because* it publishes 16 pinned boundary-debt edges instead of claiming a clean boundary. B0 (#2454) moved community construction out of `icn-core` into the daemon composition root. Say "we measured the boundary and are paying the debt down one edge at a time," not "the kernel is clean."
- **The corrected evaluator package identity.** From 0.0.4 the portable evaluator is `icn-portable-evaluator` (#2435); the owner of that fact is `deploy/appliance/evaluator/package-spec.env`. You may state plainly that the earlier name was never an ICN-ratified identity and was corrected. Do not reintroduce the old name into new material, and do not rename or delete published assets at or below 0.0.3 — they are retained so existing checksums keep verifying, and their payloads were always genuine ICN bytes.
- **Roadmap truth.** Phase 0 done, Phase 1 done, Phase 2 in progress. The software side of the rehearsal wedge is witnessed; the remaining gates are human (organizer presentation, assistive-technology pass). ([`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), [`docs/STATE.md`](../../STATE.md).)
- **NYCN as the intended first cooperative partner.** Active partnership track. The organizer-gate package on the NYCN side is independently steward-operable (facilitator guide, fail-closed validator, closed human-outcome vocabulary). The next step is the presentation itself.
- **Member-facing standing / action-card / receipt surfaces.** Real and exercised; show the member shell and the receipt loop.
- **Documentation control plane.** Honest, auditable, versioned: `registry.toml` + validators in CI, regulatory-compliance linter, readiness-overclaim linter, truth-spine and drift checks.
- **Substrate scale, stated from generated sources only.** Workspace composition and test counts change; quote them from [`docs/status.toml`](../../status.toml) / [`docs/STATE.md`](../../STATE.md) at the time of the conversation (as of 2026-07-13: 48 workspace members — 38 crates + 7 apps + 3 binaries). Do not quote deployment-age or uptime figures at all: current K3s liveness is an ops claim flagged `NEEDS OPS RE-CONFIRMATION` in `status.toml`.

## What should not be shown as finished

These are real things that exist in the repo but are not in a finished state. **Do not present them as production capabilities.**

- **A live production cooperative network.** There is no live multi-cooperative production deployment. Members do not currently log in to a hosted cooperative on ICN. The homelab K3s deployment's current liveness needs ops re-confirmation — do not claim "running since <date>".
- **A formal NYCN pilot.** NYCN is the *intended* first cooperative partner. No signed partnership, no committed launch date, no formal pilot agreement.
- **Human validation.** The assembled-image witness is automated evidence; **no real organizer has run the loop** (#1703/#1746) and **no human assistive-technology pass has occurred** (#2041). Never imply organizer acceptance or accessibility sign-off.
- **Live federation integration.** No two cooperatives federate over ICN in production. Federation primitives exist as code; two-node rehearsal is v0.2 territory.
- **Production trusted issuance.** The appliance's trusted-local mint is operator bootstrap on the local node. How institutions issue real positive authority is an open architecture decision (#2080).
- **One-click / non-technical deployment.** Per-coop one-command deployment, charter customization workflow, and pilot onboarding guides are not shipped.
- **A complete mobile app.** React Native SDK and mobile examples are parked until the browser interaction model is organizer-validated.
- **Action-card runtime as fully expanded.** Two source paths remain RFC-gated (`signal_rule` under #1631/#1711, `obligation_lifecycle` under #1634/#1712). Show three of five, not five of five.
- **Recurring assembled-image CI.** The appliance walkthrough is protected by a committed harness plus manual witnesses, not yet by a standing scheduled lane (#2398).
- **Kernel/app separation.** Measured, not achieved. A1 pinned 16 `icn-core` boundary-debt edges; B0 removed one direct edge but `icn-core → icn-gateway → icn-community` still reaches; **B1 (the ledger edge) was refused at design review** and B2 has not begun. Never say the firewall is complete or that the kernel is domain-free.
- **A two-node or multi-node appliance proof.** None has been executed. The bounded *plan* is now on `main` (`a0b970ac`, #2463), which makes it easier to mistake for progress — **a plan is not evidence.** Gate 4 is BLOCKED pending a reviewed offline receipt-bundle exporter/verifier; Gate 3 (institutional enrollment) is optional and omitting it restricts Node B to "technical witness"; federation is explicitly not exercised. Do not demonstrate two development nodes exchanging data and call it federation — a real proof must separate transport connectivity, peer identity, enrollment/authority, state synchronization, receipts, and federation.
- **Cross-node community gossip.** Dormant: production wiring subscribes to `community:updates` without creating it, so publishes are rejected under the reject policy and the failure is logged after local mutation (#2457). Do not show community sync between nodes.
- **A chosen deployment story.** ADR-0086 (#2458) is `proposed`, not adopted — and **merging the ADR would not change that**; adoption is a separate human decision recorded in its `status:` field. You may describe the *direction* — appliance as the canonical sovereign node, Compose as a disposable devnet, Kubernetes/K3s as optional operator infrastructure, native Linux as an advanced install — but say it is proposed. **Docker Compose is not sovereign-node proof**, and Kubernetes has **not** been retired from ICN (only the automatic private deployment from public CI was).
- **Appliance recovery.** `icnctl backup` omits `/etc/icn/icnd.env`, so a node restored from a backup alone cannot reopen its keystore. Independent restoration is blocked; do not present backup/restore as an operator-ready capability.

## Suggested first-demo narrative

A short, honest, non-pitch-shaped narrative for first conversations:

1. **ICN is institutional infrastructure** for democratic organizations — cooperatives, communities, federations. Not a platform, not a marketplace, not a token economy.
2. **Boot the Rehearsal Node.** A single appliance image, offline-capable, built from public `main`. No credentials are pasted; sessions are least-privilege by construction.
3. **An organizer turns a pending item into assigned work.** Review → edit → assign → exact preview → digest-bound confirm. The confirm executes the real governance ladder — what was previewed is exactly what happens, or it fails closed.
4. **A member completes the work and the institution gets a receipt.** Standing → action card → completion → durable receipt. Receipts record facts and grant no authority.
5. **The evidence leaves with the institution.** Export the packet (no identities, no credentials), verify it independently, tamper with it and watch verification fail.
6. **The next step is the organizer presentation and pilot formalization.** The remaining gate is human procedure, not engineering.

Lean toward "show, then explain": real surfaces beat slide decks.

## Red lines

These are language or framing choices that should never appear in any external-facing material — slides, write-ups, posts, demos.

| Red line | Why it matters |
|---|---|
| **No crypto / web3 / token framing.** ICN has no token, no native currency, no speculative instruments. Mutual credit is bilateral. Receipts are evidentiary, not tradable assets. | Regulatory framing; doctrine ([`THE_COMMONS.md`](../../architecture/THE_COMMONS.md), [`genesis.md`](../../genesis.md)). |
| **No financial-product framing.** ICN is not a payment system, not a fintech app, not a banking layer. The vocabulary that matters: settlement, unit, identity, position, obligation, allocation. **Avoid:** payment, currency, wallet, balance. | Regulatory framing; CI's Regulatory Compliance Linter enforces this. |
| **No NYCN commitment claim.** Do not represent NYCN as a launched, formal, or signed pilot. | Truthfulness; partnership is active but not formalized. |
| **No live-federation claim.** No two cooperatives are federating in production. | Truthfulness. |
| **No deployment-age / uptime claim.** Current K3s liveness is flagged `NEEDS OPS RE-CONFIRMATION`; a stale "running since <date>" is an overclaim even if it was once true. | Truthfulness; `docs/status.toml` is the owner of this claim. |
| **No "kernel/app separation is complete" claim.** A1 measured the boundary and pinned 16 debt edges; B0 removed one; B1 was refused. | Truthfulness; `scripts/firewall-taxonomy.toml` is the owner of this claim. |
| **No "ICN dropped Kubernetes" claim** — and equally, no claim that a K3s cluster is the product. Only the automatic private deployment from public CI was retired; Kubernetes/K3s/Helm remain optional operator material. | Accuracy in both directions; PR #2455 is the owner of this boundary. |
| **No two-node / multi-node proof claim.** None has been run. Connectivity between development nodes is not federation. | Truthfulness. |
| **No adopted-deployment-decision claim.** ADR-0086 is `proposed`; merging a proposed ADR does not adopt it. | Truthfulness; ADR status field is the owner. |
| **No human-validation claim.** No organizer has accepted the loop; no assistive-technology pass has been performed. Automated a11y checks are not the human gate. | Truthfulness; #2041/#1703/#1746 are open. |
| **No "platform landlord" aesthetics.** ICN is not a SaaS, not a hosted product, not "let us run it for you." | Doctrine ([`THE_COMMONS.md`](../../architecture/THE_COMMONS.md) §Non-Goals). |
| **No production-readiness claim across the substrate.** Some surfaces are mature; many are uneven; the appliance ships `non_production=true, signed=false`. | Truthfulness; do not generalize from the strongest parts. |
| **No "this replaces governance / legal / financial professionals" claim.** | Truthfulness, dignity. |

## Internal review checklist before an outside-facing event

- [ ] Does the deck / page / talk represent Phase 2 as in progress (not complete)?
- [ ] Does it represent NYCN as the intended first cooperative partner (not a committed pilot)?
- [ ] Does it avoid the vocabulary in the avoid list, and token / web3 / financial-product framing?
- [ ] Does it distinguish the automated assembled-image witness from human validation (organizer + assistive technology), which has not occurred?
- [ ] Does it represent action-card runtime as three of five currently emitted source paths?
- [ ] Does it avoid any deployment-age / uptime / "running since" figure?
- [ ] If it claims a number (LOC, tests, crates, members), does that number come from `status.toml` / `STATE.md` at conversation time?

## Where to read deeper

| Topic | Doc |
|---|---|
| Doctrine — what ICN is for | [`docs/architecture/THE_COMMONS.md`](../../architecture/THE_COMMONS.md) |
| Doctrine — what is immutable in the substrate | [`docs/genesis.md`](../../genesis.md) |
| The rehearsal runbook (the demo script) | [`docs/demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md`](../../demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md) |
| Public site copy as canonical "what we say" | [`intercooperative.network`](https://intercooperative.network) |
| Current truth (per-PR record) | [`docs/STATE.md`](../../STATE.md) |
| Phase model | [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md) |
