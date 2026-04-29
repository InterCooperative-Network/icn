---
Status: descriptive
Canonical: no
Last Reviewed: 2026-04-29
---

# Show-Readiness Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This map describes what is *show-ready* — the line between what is honest to demonstrate and what should not be presented as finished.

This document is the load-bearing one for outside-facing conversations. Anyone presenting ICN to organizers, cooperative developers, technically curious contributors, or grant reviewers should walk this list before the meeting.

## What can be shown now

These are real and may be presented honestly:

- **The project thesis.** ICN is digital public infrastructure for cooperatives, communities, and federations. It is a constraint engine: apps translate meaning into constraints; the kernel enforces constraints without understanding meaning. (See [`docs/architecture/THE_COMMONS.md`](../../architecture/THE_COMMONS.md) and [`docs/genesis.md`](../../genesis.md).)
- **The public site.** [`intercooperative.network`](https://intercooperative.network) — including [What is ICN](https://intercooperative.network/what-is-icn), [What's Real Now](https://intercooperative.network/whats-real-now), [For Cooperatives](https://intercooperative.network/for-cooperatives), and [Get Involved](https://intercooperative.network/get-involved).
- **The current proof-loop story.** Decision → action → receipt is real, end-to-end, for the three currently emitted action-card source paths (proposal/vote, action_item/complete, meeting/attend). Both locally and on K3s.
- **Roadmap truth.** Phase 0 done, Phase 1 done, Phase 2 in progress. Phase 2 machinery in place; remaining work is human procedure. ([`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), [`docs/STATE.md`](../../STATE.md), [`docs/strategy/ICN-Roadmap-Live.md`](../../strategy/ICN-Roadmap-Live.md).)
- **NYCN as the intended first cooperative partner.** Active partnership track. Drive-ingest operator ladder is merged end-to-end on the NYCN side; ICN-side proof paths are documented. The next step is presenting the merged ladder + proof-loop machinery to NYCN organizers.
- **Member-facing standing / action-card / receipt concept.** The shapes are real and exercised. Show the surfaces ([`runtime-surface-map.md`](runtime-surface-map.md)) and the proof-path docs.
- **Documentation control plane.** Honest, auditable, versioned: hand-maintained `INDEX.md`, machine-readable `registry.toml`, generated `DOCUMENT_REGISTRY.md`, validator in CI, regulatory-compliance linter on the avoid list. ([`docs-control-map.md`](docs-control-map.md).)
- **Substrate maturity claims.** ~75% implementation, 6,400+ tests, 35 library crates + 4 runtime apps + 3 binaries, K3s deployment running since 2025-12-03. State these as written; do not extrapolate.

## What should not be shown as finished

These are real things that exist in the repo but are not in a finished state. **Do not present them as production capabilities.**

- **A live production cooperative network.** ICN runs on a homelab K3s cluster. There is no live multi-cooperative production deployment. Members do not currently log in to a hosted cooperative on ICN.
- **A formal NYCN pilot.** NYCN is the *intended* first cooperative partner. There is no signed partnership, no committed launch date, no formal pilot agreement. The relationship is in good faith and active, not formal.
- **Live federation integration.** No two cooperatives currently federate over ICN in production. The federation primitives exist as code; the cross-org coordination story is Phase 3 in [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), not Phase 2.
- **One-click / non-technical deployment.** Phase 2 deliverables for one-command per-coop deployment, charter customization workflow, and pilot onboarding guide are not yet shipped.
- **A complete mobile app.** The React Native SDK and mobile member-app work exist. The mobile UX spec is at [`docs/mobile/icn-mobile-ux-spec-v1.md`](../../mobile/icn-mobile-ux-spec-v1.md). Treat it as a build-facing spec, not a shipped product.
- **Action-card runtime as fully expanded.** Two RFC-gated source paths (`signal_rule`, `obligation_lifecycle`) are pending under [#1646](https://github.com/InterCooperative-Network/icn/issues/1646). Show three of five, not five of five.
- **K3s teardown semantics as designed.** Smoke artifacts persist on the cluster; namespaced teardown is not yet specified ([#1679](https://github.com/InterCooperative-Network/icn/issues/1679)).

## Suggested first-demo narrative

A short, honest, non-pitch-shaped narrative for first conversations:

1. **ICN is institutional infrastructure** for democratic organizations — cooperatives, communities, federations. It is not a platform, not a marketplace, not a token economy.
2. **It helps democratic organizations turn decisions into auditable action.** A decision generates an action card; completing the action generates an append-only receipt; that receipt can be retrieved over HTTP and is portable.
3. **A member sees their standing, their scope, their action cards, and their receipts** through real endpoints. The proof loop is end-to-end on three currently emitted source paths, both locally and on K3s.
4. **NYCN is the intended first real-world rehearsal track.** Active partnership track. The drive-ingest operator ladder walks an organizer's drive content into ICN action-item proofs without requiring a live federation step.
5. **The next step is organizer presentation and pilot formalization.** Phase 2 machinery is in place; the remaining gate is human procedure, not engineering.

Lean toward "show, then explain" rather than "explain, then show": real surfaces beat slide decks for outside readers.

## Red lines

These are language or framing choices that should never appear in any external-facing material — slides, write-ups, posts, demos.

| Red line | Why it matters |
|---|---|
| **No crypto / web3 / token framing.** ICN has no token, no native currency, no speculative instruments. Mutual credit is bilateral. Receipts are evidentiary, not tradable assets. | Regulatory framing; doctrine ([`THE_COMMONS.md`](../../architecture/THE_COMMONS.md), [`genesis.md`](../../genesis.md)). |
| **No financial-product framing.** ICN is not a payment system, not a fintech app, not a banking layer. The vocabulary that matters: settlement, unit, identity, position, obligation, allocation. **Avoid:** payment, currency, wallet, balance. | Regulatory framing; CI's Regulatory Compliance Linter enforces this. |
| **No NYCN commitment claim.** Do not represent NYCN as a launched, formal, or signed pilot. | Truthfulness; partnership is active but not formalized. |
| **No live-federation claim.** No two cooperatives are federating in production. | Truthfulness; this is Phase 3, not Phase 2. |
| **No "platform landlord" aesthetics.** ICN is not a SaaS, not a hosted product, not "let us run it for you." It is infrastructure cooperatives stand up themselves or with a cooperative developer. | Doctrine ([`THE_COMMONS.md`](../../architecture/THE_COMMONS.md) §Non-Goals). |
| **No production-readiness claim across the substrate.** Some surfaces are mature; many are uneven; the pilot machinery is in place but unrehearsed at scale. | Truthfulness; do not generalize from the strongest parts. |
| **No "this replaces governance / legal / financial professionals" claim.** ICN provides infrastructure for what those people and institutions choose to do; it does not replace them. | Truthfulness, dignity. |

## Internal review checklist before an outside-facing event

- [ ] Does the deck / page / talk represent Phase 2 as in progress (not complete)?
- [ ] Does it represent NYCN as the intended first cooperative partner (not a committed pilot)?
- [ ] Does it avoid the vocabulary in the avoid list?
- [ ] Does it avoid token / web3 / financial-product framing?
- [ ] Does it represent action-card runtime as three of five currently emitted source paths (not five of five)?
- [ ] Does it represent the K3s deployment as homelab-class (not production multi-tenant)?
- [ ] If it claims a number (LOC, tests, crates, phases), does that number come from `STATE.md` / `PHASE_PROGRESS.md`?

## Where to read deeper

| Topic | Doc |
|---|---|
| Doctrine — what ICN is for | [`docs/architecture/THE_COMMONS.md`](../../architecture/THE_COMMONS.md) |
| Doctrine — what is immutable in the substrate | [`docs/genesis.md`](../../genesis.md) |
| Public site copy as canonical "what we say" | [`intercooperative.network`](https://intercooperative.network) |
| Current truth (per-PR record) | [`docs/STATE.md`](../../STATE.md) |
| Phase model | [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md) |
