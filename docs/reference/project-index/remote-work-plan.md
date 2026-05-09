---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-09
---

# Remote Work Plan

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md), [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), and [`source-of-truth-map.md`](source-of-truth-map.md). This plan coordinates useful work during a period when local agent-driven edit/test cycles are constrained. It does not change runtime truth.

## Purpose

This document tracks useful ICN work that can be prepared or executed without relying on a full local coding loop. It converts the recent full-project review into a bounded sequence of docs, public-surface, demo-planning, and issue-coordination tasks.

The goal is to keep progress moving while preserving ICN's truth discipline:

- do not claim Phase 2 completion;
- do not claim a formal NYCN pilot;
- do not claim production readiness or live federation;
- do not claim service hosting or Tool Commons implementation before runtime evidence exists;
- preserve regulatory-safe vocabulary;
- keep NYCN/private/partner-specific material out of public ICN surfaces unless explicitly intended and reviewed.

## Current anchors

Use these files before making claims:

| Topic | Anchor |
|---|---|
| Source ranking and conflict rules | [`source-of-truth-map.md`](source-of-truth-map.md) |
| Current implementation/state ledger | [`docs/STATE.md`](../../STATE.md) |
| Phase gates | [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md) |
| Current one-screen truth | [`current-truth-map.md`](current-truth-map.md) |
| Runtime/member surfaces | [`runtime-surface-map.md`](runtime-surface-map.md) |
| What may be shown externally | [`show-readiness-map.md`](show-readiness-map.md) |
| ICN ↔ NYCN boundary | [`pilot-and-nycn-map.md`](pilot-and-nycn-map.md) |
| Source tree orientation | [`source-tree-map.md`](source-tree-map.md) |
| Rust workspace orientation | [`rust-workspace-map.md`](rust-workspace-map.md) |
| CI / ops / deploy orientation | [`ci-ops-deploy-map.md`](ci-ops-deploy-map.md) |
| Demo readiness | [`docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md`](../../demo/ICN_SYSTEM_DEMO_READINESS_MAP.md) |
| Website truth boundary | [`docs/adr/ADR-0032-website-truth-boundary.md`](../../adr/ADR-0032-website-truth-boundary.md) |

## Priority order

### 1. Public-surface truth sync

Tracking issue: [#1779](https://github.com/InterCooperative-Network/icn/issues/1779)

Purpose: make website and pilot-ui public/demo-facing copy match current Phase 2 truth.

Initial targets:

- `website/src/pages/whats-real-now.astro`
- `website/src/content/docs/README.md`
- `web/pilot-ui/package.json`
- `web/pilot-ui/README.md`

Acceptance shape:

- `whats-real-now` reflects current member-standing/action-card/demo truth.
- Website copy does not claim Action Cards are merely unimplemented/design-only.
- Website docs mirror is corrected, regenerated, or marked stale/historical.
- Pilot UI metadata no longer presents the app as a timebank/payment/balance product.
- Pilot UI README describes the organizer/member demo, member standing, action cards, receipts, provenance, and demo-mode boundaries.
- No public ICN page newly leaks NYCN/Summit/private/reference-federation material.
- Safe vocabulary is preserved.

Non-goals:

- no runtime changes;
- no schema changes;
- no new contract URNs;
- no Phase 2 completion claim;
- no formal NYCN pilot claim;
- no live federation or production-readiness claim.

### 2. Project coverage matrix

Tracking issue: [#1689](https://github.com/InterCooperative-Network/icn/issues/1689)

Purpose: create a coverage-proof matrix so future project reviews are driven by repo inventories rather than ad hoc subsystem discovery.

Proposed artifact:

```text
docs/reference/project-index/project-coverage-matrix.md
```

Suggested columns:

```text
Area
Source-of-truth anchor
Source paths
Implementation status
Docs/public-surface status
Demo/show visibility
Package boundary
Privacy boundary
Drift risk
Reviewed date
Mapped in
Next action
```

Required status vocabulary:

- implemented;
- implemented but partial;
- feature-gated;
- fixture-backed;
- package-local;
- design-direction;
- historical;
- unknown / needs local verification.

The matrix should reconcile at least:

- every top-level family in `source-tree-map.md`;
- every crate/app/binary group in `rust-workspace-map.md`;
- every existing project-index map;
- website/pilot-ui/dashboard/API docs/icn-learn surfaces;
- the in-repo NYCN package and the external NYCN operator repo;
- ops/MCP/skills/drift checks/CI/deploy surfaces;
- stale/archive surfaces and unsafe vocabulary seams.

### 3. Missing subsystem maps

These may be separate PRs or grouped after the coverage matrix:

```text
docs/reference/project-index/identity-crypto-map.md
docs/reference/project-index/network-gossip-map.md
docs/reference/project-index/service-hosting-map.md
docs/reference/project-index/ccl-map.md
docs/reference/project-index/tool-commons-map.md
docs/reference/project-index/development-tooling-map.md
docs/reference/project-index/website-truth-map.md
docs/reference/project-index/pilot-ui-current-state-map.md
docs/reference/project-index/stale-and-archived-map.md
```

Minimum map rules:

- classify implementation status explicitly;
- cite the current anchor docs/source paths;
- distinguish runtime evidence from design direction;
- preserve ICN safe vocabulary;
- label stale/historical material rather than silently copying it forward.

### 4. Demo fixture planning

Tracking issue: [#1777](https://github.com/InterCooperative-Network/icn/issues/1777)

Purpose: prepare the next narrow demo slice.

Current demo path:

```text
Standing -> Action Cards
```

Desired next path:

```text
Standing -> Action Cards -> Governance proposal/vote fixture
```

Later path:

```text
Standing -> Action Cards -> Governance proposal/vote -> Receipt/provenance fixture
```

Implementation constraints for the future code PR:

- keep this frontend/demo-fixture scoped unless explicitly choosing the larger backend `--demo-mode` work;
- do not close #1727 unless a true fixture-backed demo mode exists for the actual pilot-ui organizer/member path;
- re-read the Rust structs and JSON schemas before inventing fixture fields;
- maintain demo/live/fixture boundary text;
- do not claim full demo readiness, live federation, production readiness, or formal NYCN pilot status.

### 5. Organizer and operator narrative prep

Prepare non-code artifacts that make the next human gate easier:

- one-page organizer explanation;
- five-minute demo script;
- what this proves / what this does not prove;
- member-facing walkthrough;
- operator-facing walkthrough;
- receipt/provenance explanation;
- NYCN presentation framing that preserves the active-track-not-formal-pilot boundary.

Likely location options:

```text
docs/strategy/
docs/demo/
docs/dev/
```

Pick the location based on whether the artifact is public-facing strategy, demo procedure, or developer handoff.

## Work blocks

### Block A — Truth-sync draft

Deliverables:

- draft revised `whats-real-now` copy;
- draft revised pilot-ui README;
- draft revised pilot-ui package metadata;
- classify website docs mirror as update/regenerate/historical.

Checks to run when local tooling is available:

```bash
python3 docs/scripts/doc_control_check.py --strict
cd website && npm ci && npm run build
cd web/pilot-ui && npm ci && npm run test && npm run test:e2e && npm run test:a11y
```

### Block B — Coverage matrix draft

Deliverables:

- `project-coverage-matrix.md` initial table;
- rows for all repo families and Rust workspace groups;
- explicit drift-risk column;
- next-action column that links to #1779, #1689, #1777, and other existing issues where available.

Checks to run when local tooling is available:

```bash
python3 docs/scripts/doc_control_check.py --strict
git diff --check
```

### Block C — Subsystem maps

Deliverables:

- identity/crypto map;
- network/gossip map;
- service hosting map;
- CCL map;
- Tool Commons map;
- development tooling map;
- stale/archive map.

Each map should include:

- source paths;
- canonical docs;
- runtime status;
- demo/public visibility;
- drift risks;
- non-claims.

### Block D — Future implementation handoff

Deliverable: a handoff for the next code-capable session, likely for #1777.

The handoff should state:

- current truth;
- exact objective;
- files likely to inspect;
- schema/source paths to re-read;
- test expectations;
- non-goals;
- forbidden claims and vocabulary.

## Recommended PR sequence

1. `docs(web,pilot-ui): sync public surfaces with current Phase 2 truth` — closes or advances #1779.
2. `docs(project-index): add project coverage matrix` — advances #1689.
3. `docs(project-index): add subsystem maps for identity/network/services/tooling` — advances #1689.
4. `demo(pilot-ui): add governance proposal/vote fixture slice` — advances #1777.

## Safe vocabulary

Preferred:

- settlement;
- unit;
- position;
- obligation;
- allocation;
- receipt;
- provenance;
- evidence;
- identity;
- member standing;
- action card.

Avoid in ICN-native framing:

- payment;
- currency;
- wallet;
- balance;
- token;
- timebank as the primary demo/product identity.

Historical or avoid-list contexts may mention forbidden terms only when clearly labeled as deprecated, historical, or unsafe framing.

## Non-claims checklist

Before any PR, issue, website copy, or presentation draft:

- [ ] Does it avoid claiming Phase 2 is complete?
- [ ] Does it avoid claiming NYCN is a formal pilot?
- [ ] Does it avoid claiming live federation?
- [ ] Does it avoid claiming production readiness?
- [ ] Does it avoid claiming implemented service hosting or Tool Commons runtime?
- [ ] Does it avoid payment/wallet/balance/currency/token framing?
- [ ] Does it distinguish fixture-backed, gateway-backed, docs-only, and design-direction work?
- [ ] Does it route source-of-truth conflicts through `source-of-truth-map.md`?

## Current recommended next action

Start with #1779.

Reason: public/demo-facing contradictions are more damaging than missing internal maps. Once the website and pilot-ui surfaces stop contradicting current runtime truth, add the coverage matrix and subsystem maps so future drift is harder to reintroduce.
