---
Status: descriptive
Canonical: no
Last Reviewed: 2026-04-29
---

# Pilot and NYCN Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This map describes the ICN ↔ NYCN boundary and is intentionally short — substantive NYCN material lives in the NYCN-side docs and is linked, not duplicated.

## What NYCN is in the Phase 2 framing

**NYCN (NY Cooperative Network) is the intended first cooperative partner for the Phase 2 pilot.**

It is **not yet a formally committed pilot**. The next concrete step is presenting the merged drive-ingest ladder + ICN proof-loop machinery to NYCN organizers to formalize the pilot. Subsequent gates: partnership formalization, then first operator pilot rehearsal against real (or fixture-equivalent) organizer material.

> Anyone reading these maps should not represent NYCN as a committed pilot or as a live federation member. It is an active partnership track, in good faith.

## Two distinct repos — do not conflate

| Repo | What it is | Where |
|---|---|---|
| **This repo** (`InterCooperative-Network/icn`) | The ICN substrate. Contains the in-monorepo NYCN institution package at `institutions/nycn/` (boundary-clean, suitable for later extraction). | This file lives here. |
| **`fahertym/nycn`** | The separate NYCN operator repo where the drive-ingest operator ladder lives. Pure file-in / file-out tooling and localhost-only operator-gated runners. | External; references in `STATE.md` and below. |

`institutions/nycn/` (here) is the institution package — the structures, charter material, and configuration that describes NYCN as an ICN entity. `fahertym/nycn` (external) is the day-to-day operator workflow — the procedural spine that walks organizer material into ICN action-item proofs.

## The mutation boundary

This is the load-bearing safety property of the NYCN partnership track:

- **NYCN-side tools are pure (no network) or localhost-only operator-gated.** No tool in `fahertym/nycn` ever mutates a remote ICN cluster.
- **K3s mutation is ICN-side and operator-authorized.** It happens in the proof-path runbooks documented in this repo, not in NYCN-side tooling.
- **Two-flag operator gate.** The local publisher (NYCN ladder layer 5) requires two operator flags plus a localhost-only `--gateway` to actually publish. Default is preflight (no execution).

This boundary is what allows NYCN organizers to inspect the pipeline confidently: nothing they accidentally run can change a remote cluster.

## ICN-side proof-path artifacts

These live in this repo:

| Artifact | Path | What it records |
|---|---|---|
| Local HTTP proof loop | [`docs/dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md`](../../dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md) | A holder-shell session walking `/me/action-cards` → `PUT .../status` → `GET .../completion-receipt` against a localhost gateway. Closes the proof loop end-to-end without any cluster involvement. |
| K3s smoke proof closure | [`docs/dev/NYCN_K3S_PROOF_PATH.md`](../../dev/NYCN_K3S_PROOF_PATH.md) | Operator-authorized exercise of the same proof loop against the deployed K3s cluster, image `91a63eec`. |

## NYCN-side ladder (in `fahertym/nycn`, summary only)

The NYCN drive-ingest operator ladder is merged end-to-end as NYCN PRs #21–#34 (see `STATE.md` for the per-PR enumeration). Conceptually, it is a procedural spine:

1. Parser → review artifact
2. Review decisions YAML (organizer-authored)
3. Publish dry-run
4. Assignee binding
5. Local publisher (preflight default; execute fenced behind two operator flags + localhost-only `--gateway`)
6. Local proof runner (walks `/me/action-cards` → `PUT .../status` → `GET .../completion-receipt`)
7. Federation surface bridge (pure file-in / file-out, keyed on the cross-node deterministic blake3 `record_hash` from `ActionItemCompletionReceipt`)
8. Operator pilot runbook + no-network ladder checker

Plus: organizer briefing, simple summit demo, start-here onboarding pass, one-command local preflight runner, operating-surfaces inventory + Google-Groups boundary policy + repo-safe communication-groups fixture, communication-groups directory tool, and operating-surfaces directory tool.

## What is real now

- The NYCN ladder runs end-to-end against a localhost ICN gateway.
- The ICN-side K3s smoke proof has been exercised once, operator-authorized, against the deployed image.
- The institutional-operability runtime needed to support a pilot deployment (charter activation, role binding, member standing, action-item completion receipts) exists and is exercised.

## What is not yet real

- No formal NYCN pilot commitment.
- No live federation integration. (NYCN-side tools deliberately do not perform any live federation.)
- No first operator pilot rehearsal recorded yet.
- No public case study.

## Open follow-ups

| Item | Tracking |
|---|---|
| Present the merged ladder + ICN proof-loop machinery to NYCN organizers | (human procedural; next concrete step) |
| Formalize the pilot partnership | (downstream of presentation) |
| First operator pilot rehearsal against real (or fixture-equivalent) organizer material | (downstream of formalization) |
| K3s/devnet smoke artifact cleanup / teardown semantics | [#1679](https://github.com/InterCooperative-Network/icn/issues/1679) |
| Action-card runtime expansion (RFC-gated source paths) | [#1646](https://github.com/InterCooperative-Network/icn/issues/1646), gated on [#1631](https://github.com/InterCooperative-Network/icn/issues/1631) and [#1634](https://github.com/InterCooperative-Network/icn/issues/1634) |
| `NYCN-Bootstrap-Runbook.md` operational artifact | (deferred follow-up) |
| `NYCN-Schema-Mapping.md` ny-coop-net crosswalk | (deferred follow-up) |

## Where to read deeper

| Topic | Doc |
|---|---|
| Per-PR record of ICN-side NYCN-supporting work | [`docs/STATE.md`](../../STATE.md) |
| Phase model | [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md) |
| NYCN architecture spec (in this repo) | [`docs/strategy/NYCN-Repo-Architecture-Spec.md`](../../strategy/NYCN-Repo-Architecture-Spec.md) |
| NYCN execution tranches (in this repo) | [`docs/strategy/NYCN-Execution-Tranches.md`](../../strategy/NYCN-Execution-Tranches.md) |
| Cooperative developer prep brief | [`docs/strategy/COOPERATIVE_DEVELOPER_DISCOVERY_BRIEF.md`](../../strategy/COOPERATIVE_DEVELOPER_DISCOVERY_BRIEF.md) |
| In-monorepo NYCN institution package | [`institutions/nycn/`](../../../institutions/nycn/) |
