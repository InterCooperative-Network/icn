---
Status: draft
Canonical: no
Last Reviewed: 2026-06-21
---

# Source-of-Truth Map

> This file explains **which ICN sources outrank which other sources** when repo material disagrees. It is an orientation and triage map, not a replacement for current state docs, code, ADRs, RFCs, or tests.

ICN has a large documentation and implementation surface. Some files are current implementation truth. Some are current architecture direction. Some are historical archaeology. Some are package-local. Some are generated. Some are deliberately aspirational.

The point of this map is simple:

> **Do not let old, detailed, or rhetorically strong documents outrank current code, current state docs, accepted decisions, and recent evidence.**

## Canonical hierarchy

When two sources appear to disagree, use this hierarchy by default.

| Rank | Source | Use as truth for | Notes |
|---:|---|---|---|
| 1 | Current source code and tests | Implemented behavior | Prefer current `main` over old docs. Test-only fixtures prove test behavior, not production readiness. |
| 2 | `docs/STATE.md` and `docs/PHASE_PROGRESS.md` | Current project state, phase posture, recently landed work | These are the primary state anchors and should be truth-synced after state-changing PRs. |
| 3 | Accepted ADRs and RFCs | Architectural decisions and accepted design constraints | Accepted status matters. Proposed/draft docs are design direction, not shipped behavior. |
| 4 | Recent merged PR bodies, issue closure notes, and review dispositions | Evidence for what changed and why | Use this layer to interpret state changes not yet reflected in maps. Do not treat stale PR bodies as truth after follow-up commits unless updated. |
| 5 | Runtime contracts, schemas, OpenAPI surfaces, and contract docs | External API / package-facing shapes | Verify against code when possible; schema `$id` and status fields may carry compatibility risks. |
| 6 | Project-index maps | Navigation, orientation, subsystem placement | These maps route readers to truth-bearing sources. They do not override truth-bearing sources. |
| 7 | Architecture / strategy / design-direction docs | Intended direction, doctrine, future shape | Useful for why and where next; not proof that something is implemented. |
| 8 | Generated repo records | Mechanical inventory | Generated records prove files existed at generation time, not that their contents are current truth. |
| 9 | Historical archives, dev journals, preserved WIP branches, old planning waves | Archaeology and invariant mining | Useful for ideas/tests/docs to port. Never merge or cite as current state without triage. |

## Decision status vocabulary

Use the document front matter and local wording before treating a source as normative.

| Status signal | Meaning |
|---|---|
| `Status: operational` | Describes an active process or control document, but still may not be canonical truth. |
| `Status: descriptive` | Describes an existing shape or routing layer; verify implementation elsewhere. |
| `Status: design-direction` | Future or intended architecture; not implementation evidence. |
| `Status: draft` | Work in progress; use carefully. |
| `Status: living` | Maintained over time, but still inspect its canonical flag and scope. |
| `Canonical: no` | Orientation/supporting material only. It may be useful; it does not outrank canonical state/code/accepted decisions. |

## Current truth anchors

These are the first files to inspect before making claims about ICN’s current state.

| Area | First source | Then inspect |
|---|---|---|
| Current project posture | `docs/STATE.md` | `docs/PHASE_PROGRESS.md`, latest merged PRs |
| Phase / pilot readiness | `docs/PHASE_PROGRESS.md` | the NYCN rehearsal gate (in the partner `InterCooperative-Network/nycn` repo — there is no in-repo `docs/strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md`), open Phase 2 issues (#1703) |
| Runtime architecture | `docs/ARCHITECTURE.md` | `docs/architecture/KERNEL_APP_SEPARATION.md`, current Rust workspace |
| Package boundary | `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` | standalone `InterCooperative-Network/nycn`, `institutions/nycn/` reconciliation status |
| Member surfaces | runtime code + `runtime-surface-map.md` | `/me/standing`, `/me/action-cards`, pilot-ui demo fixtures |
| Demo readiness | open/merged pilot-ui PRs | `docs/pilots/no-cli-organizer-member-rehearsal-workflow.md`, issues #1724 / #1727 / #1777 |
| NYCN package | standalone `InterCooperative-Network/nycn` repo | NYCN README, `docs/ICN-INTEGRATION.md`, package manifest, holder-label policy |
| Private overlay / activation | `docs/spec/private-overlay-did-activation-flow.md` | NYCN `docs/HOLDER-LABEL-DID-POLICY.md` |
| Compute | ADR-0030 / ADR-0031 | current `icn-compute`, gateway compute routes, compute wiring |
| Tool commons | `docs/architecture/COOPERATIVE_TOOL_COMMONS.md` | RFC-0017 and future tool-install work; treat as design-direction |
| Service hosting | `docs/architecture/SERVICE_HOSTING_MODEL.md` | issue #1710; treat as design-direction until implemented |
| Repo atlas | issue #1689 | `repo-atlas.md`, `full-repo-record.md`, generated records when present |

## Conflict rules

### Code vs docs

If code/tests and docs disagree, prefer code/tests for implemented behavior and open a docs truth-sync issue or PR.

Do **not** assume the docs are wrong about intent. They may describe the intended direction. But do not describe intended direction as shipped behavior.

### State docs vs project-index maps

If `STATE.md` / `PHASE_PROGRESS.md` and a project-index map disagree, prefer the state docs. Then update the map.

Project-index files are routing maps. They should be short and current enough to orient readers, but they are not the ledger of truth.

### Accepted ADR/RFC vs proposed ADR/RFC

Accepted decisions define constraints. Proposed/draft ADRs and RFCs describe design direction.

When discussing proposed material, say so explicitly:

```text
proposed, not implemented
accepted, not fully implemented
implemented but partial
implemented and exercised
```

### Standalone NYCN repo vs ICN `institutions/nycn/`

Treat the standalone `InterCooperative-Network/nycn` repo as the active NYCN package/rehearsal home unless a current ICN state doc or follow-up reconciliation PR says otherwise.

Treat ICN `institutions/nycn/` as boundary-sensitive scaffold / package-local material that needs explicit reconciliation before being treated as current operational truth.

### Demo fixtures vs live state

Demo fixtures are fictional and deterministic. They prove UI/demo rendering and contract shape. They do not prove live participant state, live federation, production readiness, or formal NYCN pilot status.

### Historical archive vs current doctrine

Historical archive material can contain valuable invariants, tests, and doctrine. It does not become current truth until hand-ported, reviewed, and merged against current `main`.

This applies especially to preserved WIP branches and old planning waves.

## Required labels in summaries

When summarizing a subsystem, use one of these labels.

| Label | Use when |
|---|---|
| `implemented` | Current code and tests support the claim. |
| `implemented but partial` | The surface exists but has known gaps or limited coverage. |
| `feature-gated` | Exists only behind feature, runtime, or environment gates. |
| `fixture-backed` | Demonstrated through committed fictional fixture data only. |
| `gateway-backed` | Requires a running gateway / live local service path. |
| `docs-only / design-direction` | Written direction exists but no runtime implementation is claimed. |
| `package-local` | Belongs in an institution package, not ICN core. |
| `private-boundary` | Must not expose real private participant/partner/source data. |
| `stale / historical` | Useful archaeology, not current truth. |
| `unknown / needs local verification` | The repo review could not verify the claim. |

## Overclaim guardrails

Do not claim any of the following unless current state docs and runtime evidence explicitly support it:

- ICN is production-ready.
- NYCN is a formal pilot.
- Live federation is deployed.
- Backend `--demo-mode` exists for the full stack.
- Demo fixtures are real participant state.
- Private-overlay activation has happened.
- ICN stores private partner data in public repos.
- CCL institutional-process language is implemented.
- External settlement bridge primitives are implemented beyond their accepted/draft design state.
- Tool Commons services are installable / live.
- Service hosting is ICN-native.
- Compute outputs decide governance outcomes.

## Update procedure

Update this file when:

- a new canonical state source is introduced;
- a project-index map becomes stale enough to misroute readers;
- NYCN package/source-of-truth ownership changes;
- a design-direction doc is promoted to accepted ADR/RFC or implementation;
- a stale/historical document is archived, ported, or explicitly superseded;
- a new repo or major subsystem joins the ICN project family.

When in doubt, update the truth-bearing source first, then update this map.

## Relationship to adjacent maps

| File | Relationship |
|---|---|
| `current-truth-map.md` | Short orientation to what is real now. This file explains how to rank sources when that answer is contested. |
| `repo-atlas.md` | Directory/subsystem placement. This file tells readers which evidence outranks which. |
| `full-repo-record.md` | Mechanical record protocol. This file explains how to interpret records against truth-bearing sources. |
| `stale-and-archived-map.md` | Future companion map for known stale/historical docs and preserved WIP material. |
| `nycn-package-map.md` | Future companion map for standalone NYCN package ownership and ICN boundary reconciliation. |
