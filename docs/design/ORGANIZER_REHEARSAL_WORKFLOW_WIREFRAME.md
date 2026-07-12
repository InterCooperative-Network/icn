---
Status: descriptive
Canonical: no
Last Reviewed: 2026-07-12
---

# Organizer rehearsal review → confirm workflow wireframe (interactive)

## Purpose

This wireframe defines the **interactive** organizer surface for the
[no-CLI organizer/member rehearsal workflow](../pilots/no-cli-organizer-member-rehearsal-workflow.md):
review proposed work, request a bounded edit, ask for more information, reject,
assign by registered label, preview the exact mutation a node would make, and
**confirm** it — creating one real action item and its ADR-0026 receipt ladder on
an **isolated Rehearsal Node**, which the assigned member then completes on the
existing member surface.

It is rendered by the canonical browser client `web/member-shell/` behind an
explicit **`?surface=organizer`** selector, **live-mode only**, against a
gateway started in Rehearsal mode (`ICN_GOVERNANCE_BUILD_MODE=rehearsal`). It
drives the merged runtime contract in
[`rehearsal-review-workflow.md`](../contracts/rehearsal-review-workflow.md)
without inventing endpoint shapes.

## Relationship to the read-only preview wireframe

This document **extends** — it does not replace —
[`ORGANIZER_REVIEW_PREVIEW_WIREFRAME.md`](ORGANIZER_REVIEW_PREVIEW_WIREFRAME.md).
That wireframe remains the honest record of the **first** slice: a read-only,
fixture-backed `?mode=demo` preview whose review affordances are real **disabled**
buttons that record nothing (PR #2237). Nothing in this document retcons that one
into a mutation surface.

The change since then is that the **runtime now exists** (PR #2406): a
Rehearsal-mode-only gateway surface that records real decisions, binds a preview
digest to a confirmation, walks the real receipt ladder, and creates one real
action item. This wireframe describes the browser rendering of that runtime —
with **enabled** controls whose consequences are shown before any confirm — and
is the design of record for the browser slice (PR 1 of the browser/appliance
tranche). The read-only doc's disabled-affordance frames still describe the
demo surface, which is unchanged.

## Why `?surface=organizer`, live-only

The existing member-shell grain is `?mode=(demo|live)` plus a demo-only `?set=`
fixture selector plus resilient section toggles for live data. The organizer
workflow is a whole **role** view with its own multi-step state machine, its own
credential, and its own capabilities. The smallest honest seam is one new,
explicit role selector — `?surface=organizer` — that is **absent by default**
(the member surface is the default) and is honoured **only in live mode**,
because the workflow requires a running Rehearsal-mode node and a review/confirm
credential. There is no fixture "organizer demo": a fixture surface that *looked*
like it confirmed work would be fake success. Demo mode therefore keeps its
fixture-only, non-mutating guarantee, and `?mode=demo&surface=organizer` exposes
no mutation controls. Accessibility and behaviour are exercised against the live
surface with Playwright route interception (real DOM, real wire shapes, no real
node).

## Authority boundary (make it visible and semantic)

The surface must express three distinct authority levels; the organizer browser
credential is the narrow middle band and **never** holds the outer two:

```text
setup / steward authority (internal credential — NOT in the browser):
  initialize the first workspace for a domain (designation)
  bind a registered label to a fictional identity (holds a DID)

organizer authority (the browser credential in this PR):
  read · review · bounded edit · assign an existing label · preview · confirm

member authority (a separate credential / the member surface):
  read · complete their assigned action item
```

The organizer surface **never** initializes a workspace, **never** binds a label
to an identity, and **never** handles, accepts, infers, displays, or transmits a
DID. Unbound labels are shown as *"needs steward setup"*; the browser cannot fix
that. This mirrors the runtime scopes: the browser credential carries
`governance:read`, `governance:pending-publish:review`, and
`governance:pending-publish:confirm` — never `governance:rehearsal:setup`,
`governance:write`, `governance:action-item:complete`, `governance:meeting:write`,
`entity:write`, or `coop:admin`.

## Runtime contract binding (screen → route)

All paths are under the gateway's `/v1/gov` mount. `{d}` is the selected domain.

| Screen / action | Method + route | Scope used |
|---|---|---|
| Standing / eligible domains | `GET /me/standing` | `governance:read` |
| Proposed-work list | `GET /domains/{d}/rehearsal/pending-publish` | `governance:read` |
| Row detail | `GET /domains/{d}/rehearsal/pending-publish/{row}` | `governance:read` |
| Registered labels | `GET /domains/{d}/rehearsal/bindings` | `governance:read` |
| Review decision | `POST …/{row}/review` | `pending-publish:review` |
| Bounded edit (`plain_summary` ≤256B) | `PUT …/{row}` | `pending-publish:review` |
| Assign / clear label | `POST …/{row}/assign` | `pending-publish:review` |
| Exact mutation preview | `GET …/{row}/preview` | `pending-publish:review` |
| Confirm (digest only) | `POST …/{row}/confirm` | `pending-publish:confirm` |
| Receipts read-back | `GET /domains/{d}/rehearsal/receipts` | `governance:read` |
| Evidence packet | `GET /domains/{d}/rehearsal/evidence-export` | `governance:read` |

The organizer surface never calls `POST …/rehearsal/bindings` (setup) or
`POST …/rehearsal/reset` (steward/designation). Those are the internal credential's.

## Human sequence

```text
Open ?surface=organizer (live) → connect with the organizer credential
  → see standing and the permanent Rehearsal-mode boundary
  → select a domain (explicit choice when standing shows more than one)
  → inspect proposed work (list → one row)
  → review: approve / reject / request an edit / ask for more information
  → where appropriate, edit the one allowlisted summary field
  → assign by registered label (or leave/clear)
  → preview exactly what the node will create
  → confirm the bound preview (a separate, explicit screen)
  → see the created action item and the process receipts
  → continue as the assigned member (member surface) to complete the card
  → inspect the completion receipt and the privacy-safe evidence summary
```

## Frames

### Frame 0 — entry, connect, permanent Rehearsal boundary

```text
┌──────────────────────────────────────────────────────────────────┐
│ ICN Member Shell — Organizer rehearsal                            │
│ REHEARSAL MODE · fictional data · not a pilot, not live federation │  ← text banner, always visible
│                                                                    │
│ Connect to a local Rehearsal-mode node                             │
│ Gateway address [http://localhost:8080          ]                  │
│ Organizer credential [ •••••••••••••••••••••• ]                    │
│   Needs governance:read, governance:pending-publish:review, and    │
│   governance:pending-publish:confirm. It must NOT carry setup,     │
│   write, or completion authority, and never contains a DID.        │
│ [ Connect and load the rehearsal workspace ]                       │
└──────────────────────────────────────────────────────────────────┘
```

Required behavior: the Rehearsal boundary is **text**, not a color-only badge,
and is present before any control. The credential is captured into page memory
only and the input is blanked immediately after capture (existing discipline);
it is transmitted only as an `Authorization` header to the entered gateway.

### Frame 1 — domain selection

```text
┌──────────────────────────────────────────────────────────────────┐
│ Which domain are you rehearsing in?                                │
│ ( ) Example Working Group   (member)                               │
│ ( ) Example Stewards Circle  (member)                              │
│ [ Open workspace ]                                                 │
└──────────────────────────────────────────────────────────────────┘
```

Derive eligible domains from `standing.domains[]`. **Auto-select** when exactly
one is eligible; **require an explicit choice** when more than one. Zero eligible
→ plain explanation ("you are not a member of a domain with a rehearsal
workspace"), standing still shown.

### Frame 2 — workspace not initialized (404)

```text
┌──────────────────────────────────────────────────────────────────┐
│ This domain's rehearsal workspace has not been set up yet.         │
│ A steward initializes the fictional workspace using an internal    │
│ setup credential. This organizer view cannot create it.            │
│ [ Check again ]                                                    │
└──────────────────────────────────────────────────────────────────┘
```

On `GET …/pending-publish` → 404: explain; **do not** silently call reset; **do
not** ask for or acquire setup authority; keep standing usable.

### Frame 3 — proposed-work list → one row

```text
┌──────────────────────────────┬───────────────────────────────────┐
│ Proposed work (generation 1) │ Draft a sample agenda for the next │
│ 1 Action item · Awaiting rev.│ organizing cycle.                  │
│ 2 Decision · Needs more info │ Kind: Action item   Status: Await… │
│ 3 Attendance · Approved      │ Scope: Example Working Group        │
│                              │ Governing body: Example Stewards   │
│                              │ Assignee label: Example member      │
│                              │   (label only — not an identity)    │
│                              │ Authority: assigned action item     │
│                              │ Risk: ○ Low   Expected: completion │
│                              │ Source: committed fixture           │
└──────────────────────────────┴───────────────────────────────────┘
```

Single-column on narrow screens (list precedes the selected row). The selected
row is an `<article>` with a heading; the list is a semantic list. Status, risk,
and origin always carry words + glyph, never color alone. Assignee is always
**Assignee label**, never DID.

### Frame 4 — review, edit, assign

```text
┌──────────────────────────────────────────────────────────────────┐
│ Review this proposed work                                          │
│ [ Approve for the next step ] [ Reject ]                           │
│ [ Request an edit ] [ Ask for more information ]                   │
│ Note (optional, ≤2000) [__________________________________]        │
│ Approving does not by itself publish or create anything.           │
│                                                                    │
│ Edit the summary (only field you can change, ≤256 characters)      │
│ [ Draft a sample agenda for the next organizing cycle.        ]    │
│ Editing returns this to “awaiting review” and clears any prior     │
│ approval and preview.                                              │
│                                                                    │
│ Assign to a registered label                                       │
│ [ Example member ▼ ]  (bound)   [ Clear assignment ]               │
│ An unbound label needs steward setup before you can confirm.       │
└──────────────────────────────────────────────────────────────────┘
```

Only the four runtime decisions are offered (`approve`, `reject`, `needs_edit`,
`needs_more_info`), rendered as the plain labels above; the raw value lives in a
`data-*` attribute for tests, never in copy. Editing targets only `plain_summary`
(≤256 bytes) — no raw-JSON or hidden-field editing. Assignment lists registered
labels from the bindings route with a bound/unbound flag; the organizer may
select or clear an existing label, never create or rebind one, and never sees a
DID. Any review/edit/assignment invalidates a prior preview.

### Frame 5 — exact mutation preview

```text
┌──────────────────────────────────────────────────────────────────┐
│ Preview: what confirming will create                              │
│ Action: Create one action item on this Rehearsal Node             │
│ Title:        Draft a sample agenda for the next organizing cycle │
│ Description:  <generated plan description>                         │
│ Domain:       Example Working Group                               │
│ Assignee:     Example member (label, bound)                       │
│ Authority basis: assigned action item                             │
│ Risk:         ○ Low                                               │
│ Expected receipts: gate result · activation · plan · applied      │
│ Reversible:   No — creates a real item and permanent receipts     │
│ Creates a real action item: Yes                                   │
│ Privacy: value-withheld — identities appear only as labels        │
│ ▸ Technical details (digest, ids)                                 │
│ [ Continue to confirm ]  [ Cancel ]                               │
└──────────────────────────────────────────────────────────────────┘
```

The preview is a pure `GET`; it is requested only when the row is currently
approved and its assignment is valid. Render **every** human-relevant mutation
field returned by the runtime (`title`, `description`, `domain_id`,
`assignee_label` + bound flag, `authority_basis`, `risk_level`,
`receipt_expected`/`expected_receipts`, `reversible` = No, `permanence_note`,
`privacy_note`, `confirmable`, and that a real action item is created). The
64-hex `preview_digest` lives under **Technical details** only; the client
**never** recomputes or reinterprets it. `confirmable: false` (unbound assignee)
disables Continue with a plain reason.

### Frame 6 — confirm (separate, explicit)

```text
┌──────────────────────────────────────────────────────────────────┐
│ Confirm this rehearsal mutation                                    │
│ You are about to create one real action item and permanent process │
│ receipts on this isolated Rehearsal Node. Fictional data; not a    │
│ pilot; receipts record process facts and grant no authority.       │
│ [ Confirm and create the action item ]   [ Go back ]               │
└──────────────────────────────────────────────────────────────────┘
```

Confirmation is a **separate screen** from preview. It sends **only** the
`preview_digest`. The Confirm button disables itself on submit so one interaction
cannot emit two confirm requests. Success (201 first / 200 idempotent replay both
read as success):

```text
┌──────────────────────────────────────────────────────────────────┐
│ ✓ Created one action item on this Rehearsal Node                   │
│ It is now assigned to “Example member” and appears as an action    │
│ card. Receipts below record what happened; they grant no authority.│
│ ▸ Technical details (action_item_id, ladder ids + hashes)          │
│ [ View receipts ]  [ View evidence summary ]                       │
│ [ Continue as the assigned member ]                                │
└──────────────────────────────────────────────────────────────────┘
```

An idempotent replay is rendered honestly ("this preview was already confirmed;
nothing new was created").

### Frame 7 — interrupted / stale preview

A stale preview (`409`) clears the local preview and returns the organizer to the
refreshed row with: *"The proposed work changed since this preview was made.
Review the current version and preview again."* No Confirm button remains enabled
across a stale state. An interrupted execution (a pending item recorded but the
applied receipt not yet written) is surfaced as an honest **partial** state that
an identical retry resumes — never as a second item and never as false success.

### Frame 8 — receipts and evidence

Receipts (`GET …/receipts`) list the ladder classes with ids + hashes only (no
DIDs), each in plain language ("Process session opened", "Gate result",
"Activation crossed", "Mutation plan recorded", "Mutation applied", plus the
per-row decisions). Evidence (`GET …/evidence-export`) renders the value-withheld
packet summary: per-row outcome, decision log with real receipt hashes,
run/generation, binding **flags** (no DIDs), the privacy-review result, and the
non-claims. Both packet hashes (`packet_hash` BLAKE3 + `packet_hash_sha256`) are
shown under technical detail and described as server-computed and independently
verifiable by a steward; **the browser does not claim to have verified them**
(steward/appliance cryptographic verification is the next slice).

### Frame 9 — continue as the assigned member

"Continue as the assigned member" navigates to the member surface (drop
`?surface=organizer`) where the member connects with a member credential
(`governance:read` + `governance:action-item:complete`) and completes the card on
the existing, unchanged completion flow. In this PR the role transition is a
navigation with a fresh credential entry; the appliance PR replaces manual entry
with a fresh least-privilege local session (never a token upgrade).

## State machine

A single organizer workflow state (not scattered booleans), guarded by a
monotonic request generation so an abandoned response can never render into a
newer connection/domain/row:

```text
disconnected → loading-standing → domain-selection → loading-workspace
  → workspace-uninitialized (404, terminal-until-retry)
  → row-list → row-selected → editing → review-submitting → preview-ready
  → confirming → confirmed
                     ↘ stale (409 → back to row-selected, preview cleared)
                     ↘ error (surfaced, recoverable)
```

Invariants:

- changing the domain clears prior rows, preview, and selection;
- changing the selected row clears the prior preview;
- any edit / review / assignment clears the prior preview;
- a control is disabled during its own in-flight mutation;
- a stale or failed request can never leave an **enabled** Confirm button;
- one interaction never emits two confirm requests;
- a new or failed connection attempt never retains a previous organizer's rows.

Implemented with a monotonic `organizerSeq` (mirroring the existing `liveLoadSeq`
pattern) and/or `AbortController`; every async continuation bails when its
captured generation is stale.

## Error states (organizer-facing copy, not raw codes)

| Status | Meaning shown to the organizer |
|---|---|
| 401 | Your session is unavailable or expired — connect again. |
| 403 | This credential lacks the authority for that step. |
| 404 | The workspace is not set up, or that item is no longer available. |
| 409 | The item changed (stale preview / conflict / pending recovery / reassignment) — review the refreshed state. |
| 422 | That label is not bound, or this kind of work cannot be confirmed here. |
| 500 | The node could not complete the request — nothing was assumed done. |

Raw backend detail goes to the console, never as the primary surface (existing
`liveFetch` discipline). A stale `409` clears the local preview.

## Accessibility & i18n (in scope for the 12-category gate)

Every new string uses the i18n catalog (`t()` / `data-i18n`), preserving the
pseudo-locale (`?lang=qps-ploc`) and RTL (`?lang=ar`). DOM is built only with
`createElement`/`textContent` (no dynamic `innerHTML`), native controls, visible
focus, logical keyboard order, ≥44×44 targets, 200% zoom reflow, a 375px
single-column layout with no horizontal scroll, and reduced-motion behavior; no
meaning by color alone. After mutations, focus moves to the resulting heading /
status summary and restrained `aria-live` announces the outcome; errors and help
are associated with the responsible control; the consequence text is available
before Confirm.

**An automated axe/Playwright pass does not close #2041.** The 12-category
[accessibility gate](ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) categories that
require a human assistive-technology pass (3.2 screen reader, 3.9 AT + non-mouse
input, 3.3 human 200% zoom + contrast tool, 3.5 switch control) are marked
"Pass with documented follow-ups → #2041" in the PR, never "Pass".

## Credential handling (this PR)

Manual organizer credential entry, memory-only: held only in the in-page state
object, never localStorage/sessionStorage/cookies/IndexedDB/URL/service-worker,
blanked from the input after capture, never displayed after submission, never
logged, sent only to the entered gateway as a Bearer header, and dropped on a new
or failed connection attempt. A later appliance PR removes manual entry from the
normal assembled walkthrough in favor of a fresh least-privilege local session.

## Implementation boundary (PR 1)

In scope: the `?surface=organizer` live surface in `web/member-shell/` driving
the merged runtime routes above; the organizer workflow state machine; manual
memory-only credential entry; full i18n + accessibility; Playwright
route-interception behavioral + axe coverage; this wireframe.

Out of scope (later PRs, explicitly): appliance/session wiring and the
one-command assembled loop; a real assembled-image witness; removing manual
credential entry; NYCN packaging or facilitator materials; the human
assistive-technology pass; post-merge truth-sync. The browser reuses the runtime
contract unchanged; if a runtime defect is found, the smallest correction is
isolated in its own commit/PR, not folded into the browser change.

## Non-claims

Carried verbatim from the runtime contract
([`rehearsal-review-workflow.md`](../contracts/rehearsal-review-workflow.md)):

> Not production governance, not a pilot, not live federation, not durable
> workflow storage (the review workspace is node-lifetime and resettable; the
> created items and receipts are the durable records). Identity binding here is a
> fictional-rehearsal convenience, not the production private-overlay
> architecture.

And for this browser slice specifically:

- Not an organizer-ready, organizer-validated, or accessibility-validated
  surface; automated accessibility is not a human assistive-technology pass.
- Not a production or pilot deployment, not live federation, not private-data
  handling.
- The browser never handles a DID and never holds setup, write, completion, or
  admin authority.
- Receipts record process facts and grant no authority; a successful software
  rehearsal is not organizer approval.
- Does not complete #1726, #1728, #2386, #1746, or #2041 — the human gates
  (#2041 assistive technology, #1703 organizer presentation / first operator
  rehearsal, #1746 operable-rehearsal milestone) stay open.
