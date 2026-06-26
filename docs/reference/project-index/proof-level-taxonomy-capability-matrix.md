---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-13
---

# Proof-Level Taxonomy and Capability Matrix

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This map adds a shared **proof-level vocabulary** and a capability matrix for the current organizer-rehearsal path. It is a claim-boundary aid, not a new source of truth.

This map exists for issue [#1796](https://github.com/InterCooperative-Network/icn/issues/1796) and directly supports the organizer-rehearsal milestone [#1746](https://github.com/InterCooperative-Network/icn/issues/1746). ICN now has many overlapping readiness states — implemented, partial, fixture-backed, local proof loop, live daemon proof, K3s/devnet proof, design-only, stale, aspirational. Without shared proof language, a contributor, facilitator, reviewer, or agent can accidentally over- or under-claim. This doc gives everyone the same words so the current demo/rehearsal state is honest, legible, and reusable.

It answers, for each capability: **what is real, what is fixture-backed, what runs locally, what has live-daemon proof, what is partner-rehearsal-ready, what is not production, what privacy boundary applies, and which issue/PR owns the next step.**

## Proof-level taxonomy (L0–L8)

Proof levels are **claim-boundaries**, not moral rankings. A higher level is not "better" and a lower level is not "bad" — the level states *how much evidence backs a claim* so the claim does not outrun the proof. A schema-only contract at L1 may be exactly the right maturity for its purpose; calling it L5 would be the error.

| Level | Name | What the level asserts |
|---|---|---|
| **L0** | Named / design-only | The concept exists in docs or issue text only. No stable schema, no implementation. |
| **L1** | Schema / contract exists | A stable-ish schema, ADR, contract URN, or type exists. No meaningful runtime proof required yet. |
| **L2** | Unit-tested | An implementation or schema has focused unit / round-trip validation (e.g. a schema with a validated example, a unit-tested function). |
| **L3** | Integration-tested | Multiple components interact under test. |
| **L4** | Local proof loop | A local operator can run a documented loop and produce the expected artifacts. |
| **L5** | Live daemon / gateway proof | The path runs against real `icnd` / gateway components, not just fixtures. |
| **L6** | Multi-node / devnet proof | More than one node or a federated runtime participates (e.g. the homelab K3s smoke path). |
| **L7** | Partner / organizer rehearsal | A non-core participant can review or rehearse with bounded, honest claims, behind the accessibility/privacy checklist below. |
| **L8** | Production hardening | Operational security, privacy, accessibility, recovery, observability, upgrade, and governance controls are hardened enough to support production claims. |

Levels are not strictly cumulative across axes: a capability can have a live path (L5) on a single node while its multi-node story (L6) is unproven, and a contract can be schema-validated (L2) without ever touching a daemon (L5). State the level that matches the strongest *honest* evidence, and note the gaps.

### Relationship to the existing status vocabulary

This taxonomy does **not** replace the implementation-state vocabulary in [`project-coverage-matrix.md`](project-coverage-matrix.md) (`implemented`, `implemented but partial`, `feature-gated`, `fixture-backed`, `package-local`, `design-direction`, `historical`, `unknown`). The two are **orthogonal axes**:

- **Status** answers *"is the code written, and how completely?"*
- **Proof level** answers *"how much verification evidence stands behind the claim?"*

A surface can be `implemented` (status) yet only L2 (proof) because no integration or live evidence has been recorded. Use both: status for what exists, proof level for what may be claimed. Rough alignment, not a lookup table:

| Proof level | Typical status band |
|---|---|
| L0–L1 | `design-direction` |
| L2 | `implemented but partial` / `fixture-backed` |
| L3 | `implemented` (tested) |
| L4–L5 | `implemented` (local / live proof) |
| L6 | `implemented` (devnet / K3s) |
| L7 | `package-local` / rehearsal |
| L8 | no current ICN capability claims L8 |

## Capability matrix (organizer-rehearsal path)

Evidence is grounded in merged commits on `main` and committed artifacts. **`STATE.md`'s last sync edit (2026-05-22) predates this cycle**, so it does not yet record [#1985](https://github.com/InterCooperative-Network/icn/pull/1985)→[#1999](https://github.com/InterCooperative-Network/icn/pull/1999); `STATE.md` remains the canonical per-PR record and should be read as lagging this matrix until its next sync. Where this matrix and a future `STATE.md` sync disagree, `STATE.md` wins.

| # | Capability | Proof level | Status | Evidence (PR / issue) | Privacy boundary | Next |
|---|---|---|---|---|---|---|
| 1 | Live receipt-chain audit path / 13-of-13 proof | **L5** (single-node) | implemented (live) | [#1985](https://github.com/InterCooperative-Network/icn/pull/1985); rerun via [#1997](https://github.com/InterCooperative-Network/icn/pull/1997) | repo-safe evidence packet; raw transcript gitignored | multi-node ([#1746](https://github.com/InterCooperative-Network/icn/issues/1746)); CI [#1955](https://github.com/InterCooperative-Network/icn/issues/1955) |
| 2 | Durable effect dispatch evidence | **L3** | implemented (tested) | [#1990](https://github.com/InterCooperative-Network/icn/pull/1990); `docs/spec/effect-dispatch-contract.md` | server-side institutional record; not a public surface | [#1993](https://github.com/InterCooperative-Network/icn/pull/1993) |
| 3 | Dispatch-evidence recovery / backfill | **L3** | implemented (tested) | [#1993](https://github.com/InterCooperative-Network/icn/pull/1993); related [#1986](https://github.com/InterCooperative-Network/icn/pull/1986) | as above | — |
| 4 | Decision-hash ledger lookup / index | **L3** | implemented (tested) | [#1996](https://github.com/InterCooperative-Network/icn/pull/1996); related [#1988](https://github.com/InterCooperative-Network/icn/pull/1988) | as above | — |
| 5 | One-command local receipt-chain rehearsal | **L4** | implemented (local proof) | [#1997](https://github.com/InterCooperative-Network/icn/pull/1997); `scripts/local_receipt_chain_13of13_rehearsal.sh` | repo-safe-by-construction evidence packet | CI gating on [#1955](https://github.com/InterCooperative-Network/icn/issues/1955) |
| 6 | Pending-publish summary row contract | **L2** | implemented but partial (contract) | [#1998](https://github.com/InterCooperative-Network/icn/pull/1998); [`pending-publish-summary.md`](../../contracts/pending-publish-summary.md) | read-model only; no executable payload | bind to a real publish surface (future) |
| 7 | Fixture-backed organizer rehearsal shell | **L2** | fixture-backed | [#1999](https://github.com/InterCooperative-Network/icn/pull/1999); `web/pilot-ui/fixtures/icn-organizer-demo/` | fixtures must be fictional; no private paths / Drive URLs | wire shell → live path ([#1746](https://github.com/InterCooperative-Network/icn/issues/1746)) |
| 8 | `/me/action-cards` / standing-derived queue | **L5** live; **L6** partial (K3s) | implemented but partial (3 of 5 paths); member surface now in served OpenAPI | [#1608](https://github.com/InterCooperative-Network/icn/issues/1608), [#1646](https://github.com/InterCooperative-Network/icn/issues/1646), [#2027](https://github.com/InterCooperative-Network/icn/pull/2027); see [`runtime-surface-map.md`](runtime-surface-map.md) | member standing / identity data must be protected | [#1646](https://github.com/InterCooperative-Network/icn/issues/1646) → [#1631](https://github.com/InterCooperative-Network/icn/issues/1631), [#1634](https://github.com/InterCooperative-Network/icn/issues/1634) |
| 9 | Private-data disclosure boundary / scoped vault | **L1** (design) | design-direction | [#1792](https://github.com/InterCooperative-Network/icn/issues/1792); [`artifact-registry-and-scoped-vault.md`](../../spec/artifact-registry-and-scoped-vault.md) | **no runtime enforcement**; privacy today is by exclusion | [#1792](https://github.com/InterCooperative-Network/icn/issues/1792) / [#1798](https://github.com/InterCooperative-Network/icn/issues/1798) / [#1767](https://github.com/InterCooperative-Network/icn/issues/1767) |
| 10 | NYCN organizer rehearsal milestone (overall) | components at **L2/L4/L5**; **L7 OPEN** | implemented but partial | [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) | no live private data; disclosure boundary design-only | [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) |
| 11 | DEV/DEMO appliance image — single-actor loop in one VM | **L5** (stranger-runnable local proof) | implemented (sealed July Demo Candidate 0.1) | [#2028](https://github.com/InterCooperative-Network/icn/pull/2028); `deploy/appliance/` (`ICN_APPLIANCE_DEMO_PROFILE=1`), [`DEMO_QUICKSTART.md`](../../../deploy/appliance/DEMO_QUICKSTART.md) | dev gates labeled; fictional fixture institution; image unsigned, not production | **single-actor only** — multi-person flow is a future lane, not built |

### Per-capability detail

The table above is the scannable view. The prose below carries the honest claim-boundary fields (real now / fixture-only / public wording / known gaps) that do not fit a cell.

**1. Live receipt-chain audit path / 13-of-13 proof — L5.**
- *Real now:* a governed `proposal → vote → close → allocation` flow runs against a real `icnd` + gateway and `icnctl audit verify --token --json` reports a complete 13/13 receipt chain.
- *Fixture-only / mocked:* none for the path itself — data is fictional, the gateway is real, ephemeral, loopback, and dev-gated.
- *Public/demo wording:* "live daemon/gateway proof of the 13/13 governed receipt chain on a local ephemeral node — proof of path, not proof of deployment readiness."
- *Known gaps:* single-node loopback only (no multi-node / L6); CI run is gated by the disk flake in [#1955](https://github.com/InterCooperative-Network/icn/issues/1955).

**2. Durable effect dispatch evidence — L3.** *Real now:* `ExecutionRecord.results` is persisted and an idempotent startup backfill reconstructs missing dispatch evidence. *Fixture-only:* none. *Public/demo wording:* "dispatch evidence is durably persisted and recovered on startup." *Known gaps:* durability is proven by tests, not by a recorded live multi-restart drill.

**3. Dispatch-evidence recovery / backfill — L3.** *Real now:* the backfill scan uses cursor/seek pagination (not offset), removing the crash-window and O(n²) concerns. *Fixture-only:* none. *Public/demo wording:* "recovery scan is bounded and crash-safe." *Known gaps:* recovery is unit/integration-proven, not exercised under a recorded fault-injection run.

**4. Decision-hash ledger lookup / index — L3.** *Real now:* ledger entries are indexed by `decision_hash`, fixing the crash-window and O(n²) lookup concerns. *Fixture-only:* none. *Public/demo wording:* "decision-hash lookups are indexed." *Known gaps:* none material at this level.

**5. One-command local receipt-chain rehearsal — L4.** *Real now:* one operator command spins a fresh ephemeral node/gateway, asserts 13/13, emits a repo-safe evidence packet conforming to `urn:icn:contract:rehearsal-evidence-export:v1`, validates it, and prints what is real / dev-gated / fixture-only / NOT production. *Fixture-only:* fictional inputs; all outputs are gitignored. *Public/demo wording:* "one-command local proof loop — not production, not a pilot, not live federation." *Known gaps:* requires a local build of `icnd`/`icnctl`; single node.

**6. Pending-publish summary row contract — L2.** *Real now:* a stable schema (`urn:icn:contract:pending-publish-summary:v1`, JSON Schema draft 2020-12) with a fictional example that validates via `docs/scripts/validate-preview-review.py --schema`; documented composition with `urn:icn:contract:preview-review:v1` ([#1745](https://github.com/InterCooperative-Network/icn/pull/1745)). *Fixture-only / mocked:* it is a **read-model contract** — it describes the rows an organizer was shown; it performs no mutation, implements no HTTP endpoint, and authorizes no formal pilot. *Public/demo wording:* "schema/contract for the pending-publish review row — read-model only, no endpoint, no mutation, not a formal pilot authorization." *Known gaps:* no producing endpoint binds to it yet.

**7. Fixture-backed organizer rehearsal shell — L2.** *Real now:* a demo-mode shell renders committed fictional fixtures (`rehearsal-shell.manifest.json`), validated by `docs/scripts/validate-rehearsal-shell-fixtures.py`. *Fixture-only / mocked:* **entirely** — there is no live daemon behind the shell in demo mode. *Public/demo wording:* "fixture-backed organizer rehearsal shell (demo mode) — a bounded rehearsal artifact; not live, not a formal pilot." *Known gaps:* not wired to the live gateway; not a partner rehearsal (L7) yet; the participation surface joining it to the live proof loop is the [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) milestone.

**8. `/me/action-cards` / standing-derived participation queue — L5 (live), L6 partial.** *Real now:* `GET /v1/gov/me/action-cards` emits proof-bearing receipt loops for three of five source paths (`proposal`/`vote`, `action_item`/`complete`, `meeting`/`attend`), with both a local HTTP proof loop and a homelab K3s smoke proof loop recorded (see [`current-truth-map.md`](current-truth-map.md)). *Fixture-only / mocked:* the two remaining source paths (`signal_rule`, `obligation_lifecycle`) are RFC-gated under [#1646](https://github.com/InterCooperative-Network/icn/issues/1646) and not emitted. *Public/demo wording:* "three of five currently emitted source paths, live — show three of five, not five of five." *Known gaps:* [#1631](https://github.com/InterCooperative-Network/icn/issues/1631), [#1634](https://github.com/InterCooperative-Network/icn/issues/1634), then [#1646](https://github.com/InterCooperative-Network/icn/issues/1646).

**9. Private-data disclosure boundary / scoped vault model — L1 (design).** *Real now:* a design-level spec names the `ScopedVault` boundary and the forward-direction `DisclosurePolicy` / `PrivacyClass` / `PrivateObjectRef` / `AccessReceipt` / `ExportReceipt` / `RedactionMap` vocabulary; an `icn-privacy` crate exists. *Fixture-only / mocked:* disclosure enforcement is **not implemented at runtime**. Rehearsal privacy today is achieved by **exclusion** (repo-safe-by-construction packets, fictional fixtures), **not** by an enforced disclosure boundary. *Public/demo wording:* "design-direction scoped-vault / disclosure model — NOT private-data handling; rehearsal privacy is by exclusion, not enforcement." *Known gaps:* wire-stable schema, encryption/key model, and runtime enforcement are all deferred ([#1792](https://github.com/InterCooperative-Network/icn/issues/1792), [#1798](https://github.com/InterCooperative-Network/icn/issues/1798), [#1767](https://github.com/InterCooperative-Network/icn/issues/1767)).

**10. NYCN organizer rehearsal milestone (overall) — components L2/L4/L5; L7 OPEN.** *Real now:* the kernel can prove (L5 receipt chain, L4 one-command rehearsal) and the rehearsal shell + contracts can rehearse (L2). *Fixture-only / mocked:* the integrated organizer-facing **operable** rehearsal — the participation surface joining the live proof loop to the rehearsal shell — does not exist yet. *Public/demo wording:* "fixture-backed organizer rehearsal plus a local proof loop; the operable participation surface is the open milestone — not a formal pilot, not live federation, not production." *Known gaps:* the participation surface ([#1746](https://github.com/InterCooperative-Network/icn/issues/1746)); disclosure enforcement ([#1792](https://github.com/InterCooperative-Network/icn/issues/1792)); the accessibility/privacy checklist below; the NYCN dogfood gossip-port preflight bug ([#1956](https://github.com/InterCooperative-Network/icn/issues/1956)); CI disk flake ([#1955](https://github.com/InterCooperative-Network/icn/issues/1955)).

**11. DEV/DEMO appliance image — single-actor loop in one VM — L5 (stranger-runnable local proof).** *Real now:* `ICN_APPLIANCE_DEMO_PROFILE=1` produces a reproducible bootable **VM image** (a template, not a physical device) that a stranger can build from public main and boot in QEMU/KVM. A node instance booted from it serves the member-shell and runs the full single-actor spine — standing → action card → discharge → receipt → evidence/audit — plus an in-VM 13/13 governed receipt-chain audit (`sudo icn-demo-verify --chain`; the in-VM helpers run as root). *Fixture-only / mocked:* the institution is the fictional NYCN fixture package; the shell's `?mode=demo` panes are fixture-backed. *Terminology (use precisely):* the **appliance image** is a reproducible VM image/template; a **running node instance** is a VM booted from it; the **hypervisor host** (Proxmox/KVM/cloud/local machine) runs one or many node instances; org/domain assignment and any future QR claim ceremony apply to a **running node instance**, never to the generic image and never to a physical box. *Public/demo wording:* "one local DEV/DEMO node instance proving the single-actor loop — not production, not a pilot, not federation, not multi-person, unsigned image, fictional data." *Known gaps / next lanes (not built):* multi-person interaction (two-member action flow), QR node-claim ceremony, commons resource allocation.

## Public / demo wording discipline

This section names the language that keeps claims inside the proof boundary. It is the proof-level lens on the project's existing red lines — for the full list (no crypto/token framing, regulatory vocabulary, no NYCN commitment claim, no live-federation claim), defer to [`show-readiness-map.md`](show-readiness-map.md).

**Prefer** (these stay inside the proof boundary):

- "fixture-backed organizer rehearsal"
- "local proof loop" / "live daemon/gateway proof"
- "not production" / "not a formal pilot authorization" / "not private-data handling"
- "bounded rehearsal artifact"
- "proof of path, not proof of deployment readiness"

**Avoid** (these claim past the evidence):

- "production-ready", "fully federated", "live pilot", "secure by default"
- "complete platform", "partner deployed"
- "ready for organizers" — unless the accessibility/privacy checklist below **and** the [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) acceptance conditions are both satisfied.

## Accessibility and privacy readiness (gate for "organizer-facing")

A capability does not reach **L7 (partner / organizer rehearsal)** on engineering proof alone. Anything presented as organizer-facing must also satisfy:

- **Plain-language labels** — no internal jargon, URNs, or hashes as the primary label.
- **Screen-reader / non-visual compatibility** — every state reachable and meaningful without sight.
- **Color-independent meaning** — status never carried by color alone.
- **No raw private paths or Drive URLs** in public fixtures — fictional data only.
- **Clear fixture-vs-live labeling** — the viewer always knows whether they are looking at a fixture or a live result.
- **Visible non-claims** — "not production / not a formal pilot / not live federation" stays on the surface, not buried in a footnote.
- **Visible receipt / evidence boundaries** — what is proven vs described vs expected is legible.

This checklist is why capability #7 (the fixture-backed shell) is L2, not L7: the shell exists, but the organizer-facing accessibility/privacy bar and the live participation surface are open under [#1746](https://github.com/InterCooperative-Network/icn/issues/1746). Disclosure enforcement that would make private organizer material safe to handle is design-only under [#1792](https://github.com/InterCooperative-Network/icn/issues/1792); until it lands, organizer rehearsal stays on fictional fixtures. See [`docs/design/ACCESSIBILITY_BASELINE.md`](../../design/ACCESSIBILITY_BASELINE.md) for the baseline this checklist draws on.

## Non-claims

This map does not claim:

- production readiness for any capability;
- a formal NYCN pilot, partnership, or signed agreement;
- live federation between cooperatives;
- private-data handling readiness (the disclosure boundary is design-only);
- that the fixture-backed rehearsal shell runs against a live daemon;
- that `STATE.md` already records the [#1985](https://github.com/InterCooperative-Network/icn/pull/1985)→[#1999](https://github.com/InterCooperative-Network/icn/pull/1999) cycle.

## When to update this map

- A capability's strongest honest evidence changes (a new test, a live run, a recorded devnet proof) → bump its proof level.
- A new capability enters the organizer-rehearsal path → add a row plus a detail block.
- A `STATE.md` sync absorbs the [#1985](https://github.com/InterCooperative-Network/icn/pull/1985)→[#1999](https://github.com/InterCooperative-Network/icn/pull/1999) cycle → update the lag note above and reconcile.
- The accessibility/privacy checklist or [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) acceptance conditions change → revisit which capabilities may be called organizer-facing (L7).

## Where to read deeper

| You want... | Read |
|---|---|
| What is real now (per-PR pointer) | [`current-truth-map.md`](current-truth-map.md), [`docs/STATE.md`](../../STATE.md) |
| What is or isn't show-ready, red lines | [`show-readiness-map.md`](show-readiness-map.md) |
| Subsystem coverage + status vocabulary | [`project-coverage-matrix.md`](project-coverage-matrix.md) |
| Real runtime surfaces | [`runtime-surface-map.md`](runtime-surface-map.md) |
| Which routes/`icnctl` commands back the rehearsal, and what is safe to show / steward-only / not-ready | [`organizer-rehearsal-operability-map.md`](organizer-rehearsal-operability-map.md) |
| The phase model | [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md) |
| Run / narrate / hand off the Candidate 0.1 demo (Row 11 operator companion) | [`docs/demo/JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md`](../../demo/JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md) |
