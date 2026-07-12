---
Status: descriptive
Canonical: yes
Last Reviewed: 2026-07-11
---

# Rehearsal review workflow — runtime contract (v1)

**Surface:** organizer pending-publish review/confirm on an isolated
Rehearsal Node. **Issues:** #1726, #1728, #2386, #1746.

## Where it exists

These routes are mounted **only** when the governance context is built in
`GovernanceContextBuildMode::Rehearsal` (`ICN_GOVERNANCE_BUILD_MODE=rehearsal`,
exact value; unknown or missing values fall back to Bootstrap, which does NOT
mount them). In Production, Bootstrap, and Test the routes do not exist
(404). They are deliberately **not** part of the public OpenAPI document:
they never exist on a production surface, so the production API contract
does not advertise them. This document is their contract.

Everything reviewable here is **fictional rehearsal material** seeded from the
same deterministic in-code generator that backs the committed-fixture
pending-publish summary. Confirmed mutations, however, are **real governance
records** on the rehearsal node: one action item per confirmed row, plus the
real ADR-0026 process receipts. Receipts record process facts; nothing on
this surface grants authority.

## Capabilities

| Scope | Grants |
|---|---|
| `governance:pending-publish:review` | review decisions, bounded edits, label assignment, label→fictional-DID binding, previews, workspace reset |
| `governance:pending-publish:confirm` | executing an approved, digest-bound mutation |
| `governance:read` | all read surfaces (list, detail, bindings, receipts, evidence) |
| `governance:write` (broad, technical operators) | accepted-also fallback for both, per sub-capability doctrine |

Review and confirm are non-implying siblings; neither implies the other and
`governance:read` grants neither. Reset and binding are deliberately part of
`review` (repeating the fictional rehearsal is an organizer act; a binding
grants no authority, and rebinding invalidates outstanding previews).
Every route additionally requires domain membership; the path domain is the
authority context.

## Routes

All under `/v1/gov` (gateway mount). `{d}` = domain id, `{r}` = row id.

| Method + path | Scope | Behavior |
|---|---|---|
| `POST /domains/{d}/rehearsal/reset` | review | initialize/re-seed the deterministic workspace; bumps `generation` (invalidates all previews); recorded receipts and created items are NOT erased |
| `GET /domains/{d}/rehearsal/pending-publish` | read | rows + review state (`404` until first reset) |
| `GET /domains/{d}/rehearsal/pending-publish/{r}` | read | row detail (`assignee_bound`, `executed`, version) |
| `POST .../{r}/review` `{decision, note?}` | review | decision ∈ {approve, reject, needs_edit, needs_more_info}; records a real `DecisionRecordedReceipt`; note ≤ 2000 bytes |
| `PUT .../{r}` `{plain_summary}` | review | bounded edit (≤ 256 bytes; the ONLY editable field); resets status to pending_review and clears approval |
| `POST .../{r}/assign` `{assignee_label?}` | review | assign by registered human label (≤ 120 bytes); unknown label → 422; resets status/approval; `null` clears |
| `POST /domains/{d}/rehearsal/bindings` `{label, did}` | review | bind label → fictional DID; DID accepted on write, never echoed |
| `GET /domains/{d}/rehearsal/bindings` | read | labels + `bound` flags only |
| `GET .../{r}/preview` | review | pure read; requires current approval; returns the exact mutation fields + `preview_digest`; non-action-item kinds → 422 (reviewable, not executable) |
| `POST .../{r}/confirm` `{preview_digest}` | confirm | digest-verified execution (below); duplicate identical confirm → 200 idempotent replay; different digest after execution → 409 |
| `GET /domains/{d}/rehearsal/receipts` | read | ladder receipts recorded this workspace lifetime (ids + hashes, no DIDs) |
| `GET /domains/{d}/rehearsal/evidence-export` | read | value-withheld evidence packet (below) |

All request bodies use `#[serde(deny_unknown_fields)]` — unknown fields are
rejected (400), not ignored.

## Preview→confirm binding

Preview computes a **domain-separated BLAKE3 digest** (tag
`icn:gov:rehearsal_plan:v1`) over the canonical plan document
`urn:icn:rehearsal-plan:v1`: domain, row id, workspace generation, row
version, action kind, exact title/description, assignee label **and its
currently bound DID**, due date, priority, authority basis, risk, expected
receipt category, provenance, origin, reversibility. The browser receives the
human-readable fields plus the digest — never the DID.

Confirm **recomputes the digest from current state** and compares
(constant-time; `blake3::Hash` equality). Any intervening edit, review
decision, re-assignment, label re-binding, or workspace reset changes the
digest, so the stale preview fails closed (409) and a fresh preview is
required. The digest bytes are persisted as the plan `body_hash`, so the
binding is auditable in the receipt chain.

## Confirm execution (real machinery)

On a verified confirm the node records, in order, through the existing
`GovernanceManager` ladder (#ADR-0026): `ProcessSessionOpened` (once per
workspace generation) → `ProcessGateResult` (`ScopeConfirmation`/`Pass`) →
`ActivationCrossed` (referencing the approving `DecisionRecordedReceipt` and
the gate hash) → `MutationPlanRecorded` (`body_hash` = preview digest) → **one
real action item** via `create_action_item` (priority medium, fictional
assignee DID if bound, else unassigned) → `MutationApplied` (referencing the
plan and a result hash binding the created item). The response returns every
id and record hash. Assigned items surface on the normal member action-card
view and are completable with the existing completion-only capability
(`governance:action-item:complete`, #2400) — unchanged by this surface.

Rows whose assignee label is **unbound** cannot be confirmed (409).
Non-action-item kinds are reviewable and previewable but **not executable**
in this slice (422).

## Summary origin

`GET /v1/gov/me/pending-publish-summary` gains one origin value:
`rehearsal_runtime` — served only in Rehearsal mode once a workspace exists
(rows then reflect live review state). `committed_fixture` (Bootstrap/Test,
and Rehearsal before any reset) and `live_runtime` (Production: no rows) are
unchanged.

## Evidence packet (`urn:icn:contract:rehearsal-workflow-evidence:v1`)

Derived from actual workspace outcomes: per-row outcome
(`executed | approved-not-executed | rejected | edit-and-resubmit |
deferred`), versions, preview digests and plan/application record hashes for
executed rows, the decision log (ids + record hashes + `note_present`
flags), label binding states, non-claims, a privacy-review block, and a
`packet_hash` — SHA-256 over the canonical packet content (excluding
`packet_hash`/`generated_at`) so any tampering is detectable by
recomputation, including in a browser via WebCrypto. The packet contains no
DIDs, credentials, private-overlay values, paths, or topology.

## Non-claims

Not production governance, not a pilot, not live federation, not durable
workflow storage (the review workspace is node-lifetime and resettable; the
created items and receipts are the durable records). Identity binding here
is a fictional-rehearsal convenience, not the production private-overlay
architecture.
