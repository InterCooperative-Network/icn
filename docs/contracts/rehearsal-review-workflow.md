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
| `governance:rehearsal:setup` | DESIGNATING a rehearsal domain (its first workspace initialization) and binding labels to fictional identities — internal setup credential only |
| `governance:pending-publish:review` | review decisions, bounded edits, label assignment (labels only), previews, RE-resetting an already-designated workspace |
| `governance:pending-publish:confirm` | executing an approved, digest-bound mutation |
| `governance:read` | all read surfaces (list, detail, bindings, receipts, evidence) |
| `governance:write` (broad, technical operators) | accepted-also fallback for all three, per sub-capability doctrine (tested) |

All three rehearsal capabilities are non-implying siblings and
`governance:read` grants none of them. The organizer browser credential
carries review+confirm+read only: it can neither turn an arbitrary domain
into a rehearsal workspace (designation needs setup) nor bind identities —
and it never sees, stores, or submits a DID. A binding grants no authority,
must reference an identity that already holds domain membership, and any
rebinding invalidates outstanding previews (except while an interrupted
execution references the label — see recovery below — when rebinding is
refused with 409). Every route additionally requires the caller's own
domain membership; the path domain is the authority context.

**The binding-target membership check never falls open.** Membership must
be affirmatively established: a `StaticList` domain resolves it from the
source; a `TrustThreshold` domain requires a wired membership resolver that
confirms standing. A missing resolver, a resolver failure, or any other
indeterminate outcome DENIES the binding — this invariant is not deferred
to appliance wiring. Every deny is the same 422 with the same message, so
the response never distinguishes "provable non-member" from "membership
unknowable" and reveals nothing about hidden identities or standing in
other domains. (The CALLER-side gate keeps the permissive Bootstrap
dependency posture in Rehearsal mode, like every non-production surface;
only the binding target is held to the stricter affirmative rule, because
a binding feeds the completion loop.)

## Routes

All under `/v1/gov` (gateway mount). `{d}` = domain id, `{r}` = row id.

| Method + path | Scope | Behavior |
|---|---|---|
| `POST /domains/{d}/rehearsal/reset` | setup (first) / review (re-reset) | start a new rehearsal generation with the deterministic seed and a fresh restart-safe `run_id`; invalidates all previews; the prior run's fictional action items are cancelled through the normal status machinery (completed items and all receipts remain). Retirement scans the DURABLE item store for this surface's `meeting_context` marker in addition to the in-memory workspace, so items orphaned by a daemon restart are retired too (`prior_item_scan: complete\|failed` reports the scan outcome) |
| `GET /domains/{d}/rehearsal/pending-publish` | read | rows + review state (`404` until first reset) |
| `GET /domains/{d}/rehearsal/pending-publish/{r}` | read | row detail (`assignee_bound`, `executed`, version) |
| `POST .../{r}/review` `{decision, note?}` | review | decision ∈ {approve, reject, needs_edit, needs_more_info}; records a real `DecisionRecordedReceipt`; note ≤ 2000 bytes |
| `PUT .../{r}` `{plain_summary}` | review | bounded edit (≤ 256 bytes; the ONLY editable field); resets status to pending_review and clears approval |
| `POST .../{r}/assign` `{assignee_label?}` | review | assign by registered human label (≤ 120 bytes); unknown label → 422; resets status/approval; `null` clears |
| `POST /domains/{d}/rehearsal/bindings` `{label, did}` | setup | bind label → fictional DID (target must already hold domain membership, else 422); DID accepted on write, never echoed |
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

**Interrupted-confirm recovery (same process only):** the created action
item is marked in the workspace the moment it exists, before the
mutation-applied receipt. If any ladder step fails, an identical retried
confirm — same preview digest, same facilitator identity, same running
process — resumes exactly where the interruption happened:

- failure BEFORE item creation (gate/activation/plan recording): no item
  exists; the retry re-runs the remaining steps, reusing the gate receipt
  already recorded for this row version (gate record hashes include their
  timestamp, so recording a fresh observation in a later second would
  change the activation's gate basis and hard-conflict with an
  already-recorded activation);
- failure AFTER item creation but before the applied receipt: the row
  carries the pending item marker; review mutations on the row AND
  rebinding of the row's assignee label are blocked (409) so the recomputed
  digest still matches, and the retry resumes the SAME item — a retry can
  never create a second one;
- a retry by a DIFFERENT confirm-capable identity fails closed (409
  activation conflict): recovery belongs to the facilitator who started it,
  or to a reset.

**This recovery state is in-memory and does NOT survive a daemon restart.**
Cross-restart confirm idempotency is explicitly not claimed. After a
restart the workspace is gone (routes answer 404 until a new designation
reset); a partially confirmed row cannot be resumed, and its receipts stop
at the last recorded rung (e.g. a plan without an applied record — an
honest partial trail, readable by an operator from the receipt store). The
created-but-unreceipted action item is durable; the recovery path is the
next reset, which retires it (below). Every persistent identifier (session,
decision, activation, plan, application) carries the workspace `run_id`, so
identifiers are never reused across resets or daemon restarts even though
the seed content is deterministic.

**Concurrency:** the entire review surface is serialized under one lock
with no awaits inside critical sections; confirm's verify-then-execute is a
single critical section. Two simultaneous confirms of one row cannot both
execute — the loser observes the winner's execution and replays
idempotently (same digest) or conflicts (different digest).

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
`rehearsal_runtime` — served only in Rehearsal mode, and only for workspaces
in domains where the CALLER holds membership standing. A caller with no
member-visible workspace receives the static `committed_fixture` response,
exactly as if no workspace existed anywhere: one domain's rehearsal rows,
review state, and workspace existence are never observable from another
domain. `committed_fixture` (Bootstrap/Test, and Rehearsal before any
member-visible reset) and `live_runtime` (Production: no rows) are
unchanged.

The isolation filter fails closed on any provably-non-member domain and
never exports a DID. One posture caveat, symmetric with the caller gate on
every read route: the CALLER-side membership check keeps the permissive
Bootstrap posture, so on a `TrustThreshold` domain with **no** wired
membership resolver it resolves every caller as a member — there, a domain's
fictional (label-only, DID-free) review state is visible to any
authenticated `governance:read` caller. This is the deliberate asymmetry
noted under Capabilities: only the binding *target* is held to the
affirmative rule. Run rehearsals on `StaticList` fictional domains (as the
appliance profile does) or wire a resolver for strict cross-caller read
isolation.

## Evidence packet (`urn:icn:contract:rehearsal-workflow-evidence:v1`)

Derived from actual workspace outcomes: per-row outcome
(`executed | interrupted-execution | approved-not-executed | rejected |
edit-and-resubmit | deferred`), versions, preview digests and
plan/application record hashes for executed rows, the decision log (ids +
record hashes + `note_present` flags), label binding states, the `run_id`
and generation, non-claims, a privacy-review result, and TWO hashes over the
canonical packet content (the packet's JSON serialization with only the two
hash fields removed — `generated_at` and every other exported field are
bound): `packet_hash` (domain-separated BLAKE3, tag
`icn:gov:rehearsal_workflow_evidence:v1`, the ICN receipt convention) and
`packet_hash_sha256` (a mirror so a browser steward view can verify via
WebCrypto). A reusable validator
(`icn_governance_actor::rehearsal_workspace::validate_evidence_packet`)
recomputes both; tampering with any field fails verification. The packet
contains no DIDs, credentials, private-overlay values, paths, or topology.

## Non-claims

Not production governance, not a pilot, not live federation, not durable
workflow storage (the review workspace is node-lifetime and resettable; the
created items and receipts are the durable records). Identity binding here
is a fictional-rehearsal convenience, not the production private-overlay
architecture.
