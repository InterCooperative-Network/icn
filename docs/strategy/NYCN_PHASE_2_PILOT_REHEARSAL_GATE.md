---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-03
---

# NYCN Phase 2 Pilot Rehearsal Gate

This document defines the human and operational gate required to move from
"Phase 2 machinery exists" to "NYCN pilot formally begins."

It is organizer-safe control-plane guidance. It does not advance Phase 2,
declare production readiness, or authorize live infrastructure mutation.

## Current State

- ICN action-card and proof-loop machinery exists for currently emitted paths.
- The NYCN drive-ingest ladder exists as a local, file-based, reviewable workflow.
- NYCN is the intended first cooperative partner.
- No formal NYCN pilot has started yet.

## Gate 1: Organizer Presentation

Required inputs:

- Optional facilitator prep (NYCN repo, checklist only — does **not** record
  that the presentation occurred):
  [REHEARSAL-0002 — Organizer gate prep](https://github.com/InterCooperative-Network/nycn/blob/main/docs/rehearsals/REHEARSAL-0002-organizer-gate-prep.md).
- Drive-ingest `START_HERE`, organizer briefing, and simple summit demo.
- Plain-language explanation of the ICN proof loop: organizer material becomes
  reviewable action items, operator decisions produce receipts, and provenance
  artifacts can be inspected without treating the rehearsal as live practice.
- Privacy and mutation boundaries, including no automatic Google Drive or
  Google Groups synchronization, no K3s mutation, and no NYCN private data in
  the public ICN repository.
- A clear statement of what organizers are being asked to decide.

Required organizer decision:

- Whether NYCN wants to proceed to a first operator rehearsal.
- Who acts as steward/operator.
- What real or fixture-equivalent material is safe to use.
- What success and failure mean for the rehearsal.

## Gate 2: Pilot Formalization

A formalized pilot means these are recorded before rehearsal work begins:

- The named body or organizers approving the rehearsal.
- The named steward/operator.
- The chosen rehearsal material.
- The written privacy boundary.
- The written success criteria.
- An explicit statement that this remains a rehearsal unless separately approved
  as live operating practice.

## Gate 3: First Operator Rehearsal

Expected outputs:

- Action-item review artifact.
- Organizer-authored decisions file.
- Publish dry-run.
- Assignee binding.
- Local publish plan.
- Local proof artifact.
- Federation-surface summary.
- Short rehearsal report.

## Non-Claims

This gate is not:

- Production readiness.
- Live federation integration.
- Replacement of existing NYCN organizing practice.
- Automatic Google Drive or Google Groups synchronization.
- A general-purpose pilot for 3-5 cooperatives.
- Phase 2 completion by itself unless `docs/STATE.md` and
  `docs/PHASE_PROGRESS.md` record the gate as met with evidence.

## Definition of Done

The gate is complete only when:

- Organizer presentation happened and is recorded.
- Formal pilot/rehearsal approval is recorded.
- Operator rehearsal ran against approved material.
- Outputs are committed or summarized in repo-safe form.
- `docs/STATE.md` and `docs/PHASE_PROGRESS.md` are updated with the evidence path.
