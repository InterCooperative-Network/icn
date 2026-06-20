---
Status: operational
Canonical: no
Last Reviewed: 2026-06-20
---

# ICN Project Index

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This index is navigation, not state.

This directory is a **show-ready orientation layer** for the ICN repository. Its job is to let an outside reader — organizer, cooperative developer, contributor, or agent — figure out what ICN is, where its real surfaces live, and what to read next, without having to crawl the entire repo first.

## What this is

- An **orientation map**: short routing docs from "I am role X, where do I start?" to the right canonical doc, source tree, or external URL.
- A **retrieval guide**: each map points at the canonical truth-bearing files (`STATE.md`, `PHASE_PROGRESS.md`, registry, source) rather than restating them.
- A **show-readiness layer**: helps callers tell the difference between what is real now, what is in progress, and what is intentionally off-limits to demo.

## What this is not

- **Not current truth.** Truth lives in `docs/STATE.md` and `docs/PHASE_PROGRESS.md`. Those are re-bumped with every state-changing PR. The maps here describe the *shape* of the project, not the day-to-day record.
- **Not a replacement for `docs/STATE.md`.**
- **Not a generated file inventory.** Listing every file is a job for tooling (`git ls-files`, registry summary), not for hand-maintained docs.
- **Not a sales pitch.** Public messaging lives on the website (`website/` and `intercooperative.network`). This directory speaks to readers who already opened the repo.

## Truth layer / claim discipline

ICN already has several truth- and freshness-tracking systems. **This index does not replace them, and there is deliberately no separate `docs/truth/` tree** — a parallel truth root would be a competing source of truth, the exact thing the meaning-firewall and source-precedence discipline exist to prevent. Instead, here is which *existing* system answers which question:

| Question | Authoritative system |
|---|---|
| Which source wins when two disagree? (precedence, status labels, overclaim guardrails) | [`source-of-truth-map.md`](source-of-truth-map.md) |
| How strong is the proof behind a claim? (L0–L8 ladder) | [`proof-level-taxonomy-capability-matrix.md`](proof-level-taxonomy-capability-matrix.md) |
| What truth-class / canonical status does a doc carry? | [`docs/registry.toml`](../../registry.toml) + [`docs-control-map.md`](docs-control-map.md) (checked by `docs/scripts/doc_control_check.py`) |
| How complete is a subsystem's implementation? | [`docs/status.toml`](../../status.toml) |
| Is an `ARCHITECTURE.md` section stale? | [`docs/freshness.toml`](../../freshness.toml) (checked by `docs/scripts/freshness-check.py`; broader SME re-review tracked in [#2047](https://github.com/InterCooperative-Network/icn/issues/2047)) |
| What is real now vs planned? | [`current-truth-map.md`](current-truth-map.md) → then [`docs/STATE.md`](../../STATE.md) + [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md) |
| What is safe to show / claim publicly? | [`show-readiness-map.md`](show-readiness-map.md) + [`website-truth-map.md`](website-truth-map.md) |
| What routes does the gateway actually expose, and which reach OpenAPI? | [`generated/route-inventory.md`](generated/route-inventory.md) — mechanical scan, regenerate with `docs/scripts/route_inventory.py` ([#2112](https://github.com/InterCooperative-Network/icn/issues/2112)) |

**Discipline (for humans and agents):**

- **Orientation maps are not canonical state.** Everything in this directory is `Canonical: no`. Defer to code/tests first, then `STATE.md`/`PHASE_PROGRESS.md`, then accepted ADRs/RFCs — the full ranking is in [`source-of-truth-map.md`](source-of-truth-map.md).
- **Bound every claim by its proof level.** Do not describe an L0–L1 design as implemented, or an L4 fixture/demo as live. Public claims must not exceed what `show-readiness-map.md` plus current canonical state support.
- **Use the existing implementation-status labels** (from `source-of-truth-map.md`); do not invent new ones:
  `implemented` · `implemented but partial` · `feature-gated` · `fixture-backed` · `gateway-backed` · `docs-only / design-direction` · `package-local` · `private-boundary` · `stale / historical` · `unknown / needs local verification`.
- **Proof ladder** (from `proof-level-taxonomy-capability-matrix.md`): `L0` named/design-only → `L1` schema/contract → `L2` unit-tested → `L3` integration-tested → `L4` local proof loop → `L5` live daemon/gateway → `L6` multi-node/devnet → `L7` partner/organizer rehearsal → `L8` production hardening.

## Start here by role

| If you are... | Start with... | Then... |
|---|---|---|
| **Organizer / cooperative evaluator** | [`show-readiness-map.md`](show-readiness-map.md) | [`current-truth-map.md`](current-truth-map.md) |
| **Cooperative developer** (e.g. evaluating ICN as substrate for a coop's tooling) | [`current-truth-map.md`](current-truth-map.md) | [`runtime-surface-map.md`](runtime-surface-map.md), [`docs/strategy/COOPERATIVE_DEVELOPER_DISCOVERY_BRIEF.md`](../../strategy/COOPERATIVE_DEVELOPER_DISCOVERY_BRIEF.md) |
| **Technical contributor** (writing code) | [`source-tree-map.md`](source-tree-map.md) | [`rust-workspace-map.md`](rust-workspace-map.md), [`AGENTS.md`](../../../AGENTS.md), [`docs/GETTING_STARTED.md`](../../GETTING_STARTED.md) |
| **Documentation contributor** | [`docs-control-map.md`](docs-control-map.md) | [`source-of-truth-map.md`](source-of-truth-map.md), [`docs/INDEX.md`](../../INDEX.md), [`docs/DOCUMENTATION_CONTROL_SYSTEM.md`](../../DOCUMENTATION_CONTROL_SYSTEM.md) |
| **Designer / accessibility reviewer** | [`runtime-surface-map.md`](runtime-surface-map.md) | [`docs/design-language/brief-v0.md`](../../design-language/brief-v0.md), [`docs/architecture/MEMBER_STANDING.md`](../../architecture/MEMBER_STANDING.md) |
| **Operator / infra person** | [`ci-ops-deploy-map.md`](ci-ops-deploy-map.md) | [`docs/operations/deployment/HOMELAB_DEPLOYMENT.md`](../../operations/deployment/HOMELAB_DEPLOYMENT.md), [`deploy/README.md`](../../../deploy/README.md) |
| **Agent / Claude Code session** | [`current-truth-map.md`](current-truth-map.md) | [`source-of-truth-map.md`](source-of-truth-map.md), [`docs/ai/ICN_CONSTITUTIONAL_CORE.md`](../../ai/ICN_CONSTITUTIONAL_CORE.md), [`AGENTS.md`](../../../AGENTS.md), [`CLAUDE.md`](../../../CLAUDE.md) |

## All maps

| Map | Purpose |
|---|---|
| [`current-truth-map.md`](current-truth-map.md) | What is real, what is not, what gates remain — pointing at `STATE.md` / `PHASE_PROGRESS.md` for the per-PR record. |
| [`source-of-truth-map.md`](source-of-truth-map.md) | Which sources outrank which when repo material disagrees; conflict rules, overclaim guardrails, and status labels. |
| [`source-tree-map.md`](source-tree-map.md) | Top-level repo surfaces and what each is for. |
| [`repository-map.md`](repository-map.md) | Cross-repo map: `icn`, `nycn`, `icn-learn`, `icn-community-bridge` — roles, visibility, merge order. |
| [`rust-workspace-map.md`](rust-workspace-map.md) | The Rust workspace under `icn/` grouped by rough layer (kernel/identity/networking/ledger/governance/etc.). |
| [`docs-control-map.md`](docs-control-map.md) | How `docs/INDEX.md`, `docs/registry.toml`, `DOCUMENT_REGISTRY.md`, and `doc_control_check.py` relate; truth classes; how to add a doc. |
| [`runtime-surface-map.md`](runtime-surface-map.md) | Real runtime surfaces a member or app actually touches today (`/me/standing`, `/me/action-cards`, completion-receipt retrieval, etc.). |
| [`ci-ops-deploy-map.md`](ci-ops-deploy-map.md) | CI workflows, deploy paths, K3s smoke runbooks — routing only. |
| [`show-readiness-map.md`](show-readiness-map.md) | What can be shown now, what should not be shown as finished, the suggested demo narrative, and red lines. |
| [`project-coverage-matrix.md`](project-coverage-matrix.md) | Coverage-style matrix: subsystem → anchors → drift and show risks. |
| [`proof-level-taxonomy-capability-matrix.md`](proof-level-taxonomy-capability-matrix.md) | Proof-level taxonomy (L0–L8) as shared claim-boundary vocabulary, plus a capability matrix for the current organizer-rehearsal path. Supports #1746, narrows #1796. |
| [`identity-crypto-map.md`](identity-crypto-map.md) | Identity, keys, DIDs, signing — where code and docs live. |
| [`network-gossip-map.md`](network-gossip-map.md) | QUIC, gossip, discovery — runtime surfaces and overclaim guardrails. |
| [`ccl-map.md`](ccl-map.md) | Cooperative Contract Language interpreter and governance wiring. |
| [`service-hosting-map.md`](service-hosting-map.md) | Service hosting design direction vs runtime (no finished “hosting product” claim). |
| [`tool-commons-map.md`](tool-commons-map.md) | Tool Commons framing vs what exists in-tree today. |
| [`pilot-ui-current-state-map.md`](pilot-ui-current-state-map.md) | What pilot-ui proves in demo mode tab-by-tab. |
| [`stale-and-archived-map.md`](stale-and-archived-map.md) | Stale public or archived paths and how to repair drift. |
| [`website-truth-map.md`](website-truth-map.md) | Website pages vs truth boundaries and review prompts. |
| [`remote-work-plan.md`](remote-work-plan.md) | **Superseded pointer** — remote coordination backlog folded into this index + handoffs. |

## Vocabulary the maps use

ICN is described as **digital public infrastructure** for cooperatives, communities, and federations. The maps use the project's preferred vocabulary:

- **Avoid:** payment, currency, wallet, balance.
- **Prefer:** settlement, unit, identity, position, obligation, allocation.

This is regulatory framing, not aesthetic preference. ICN is not a financial product, not a token system, not a platform.

## When to update these maps

- The shape changes: a new top-level directory, a new crate layer, a new app, a new control-plane doc.
- A surface listed here moves, is renamed, or is retired.
- Phase or pilot framing changes (NYCN status, action-card source paths, etc.) — the deltas belong in `STATE.md` and `PHASE_PROGRESS.md` first; if they reshape what is *show-ready*, also update [`show-readiness-map.md`](show-readiness-map.md) and [`current-truth-map.md`](current-truth-map.md).

The maps here are intentionally short. If a map starts growing into a runbook or a spec, that material belongs under `docs/guides/`, `docs/spec/`, or `docs/architecture/` instead.
