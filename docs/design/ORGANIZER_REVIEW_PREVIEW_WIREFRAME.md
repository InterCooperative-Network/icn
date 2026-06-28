---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-28
---

# Organizer review preview wireframe

## Purpose

This wireframe defines the smallest browser-first surface for steps 3–6 of the [no-CLI organizer/member rehearsal workflow](../pilots/no-cli-organizer-member-rehearsal-workflow.md): preview proposed rows, inspect the available review choices, see the organizer-readable assignee label, and understand what a later mutation would create.

The first implementation is **read-only and fixture-backed**. It renders committed fictional packets in `?mode=demo`; it records no decision and performs no mutation. The organizer sees plain-language consequences before any future confirm step. Steward/operator commands remain outside the organizer path.

## Contract and fixture inputs

The surface composes two sibling packets by contract role, never by inventing a third combined schema:

| Input | Role in the surface | Current fixture |
|---|---|---|
| `urn:icn:contract:preview-review:v1` | Review-boundary wrapper: title, overall status, privacy/repo-safety posture, source-material summary | `web/pilot-ui/fixtures/icn-organizer-demo/preview-review.pending-publish-summary.json` |
| `urn:icn:contract:pending-publish-summary:v1` | Row body: plain summary, governing body, assignee label, review affordances, mutation preview, expected receipt category | `docs/contracts/pending-publish-summary.example.json` |

The browser-serving implementation may add a checked copy of the row fixture under `web/pilot-ui/fixtures/icn-organizer-demo/`, because the Pilot UI test server does not serve `docs/`. Any such copy must validate against the same schema and be mechanically checked against the canonical example so the packets cannot silently diverge.

## Sequence

```text
Review overview
  → inspect one proposed row
  → see available review choices (fixture-only, disabled)
  → confirm the human-readable assignee label
  → read “what would change” and expected receipt category
  → return to the list
```

The initial slice stops there. A later, separately reviewed mutation surface would add decision recording and confirmation. Rendering this wireframe must never make an `approve`, `reject`, `edit`, `request_info`, or `defer` affordance executable.

## Frame A — review overview

```text
┌──────────────────────────────────────────────────────────────────┐
│ Review preview                                      FIXTURE ONLY │
│ Four fictional items are ready for review. Nothing changes here.│
│                                                                  │
│ Status: Ready for review       Items: 4                          │
│ Privacy check: Reviewed clean  Repository safety: Safe to show  │
│                                                                  │
│ Source material (fictional, shown by safe basename only)         │
│ • example-organizing-circle-notes.fixture.md                     │
│ • example-allocation-draft.fixture.json                          │
│                                                                  │
│ [Review the 4 proposed items]                                    │
└──────────────────────────────────────────────────────────────────┘
```

Required behavior:

- The `FIXTURE ONLY` boundary is text, not a color-only badge.
- “Nothing changes here” appears before the first interactive control.
- The technical contract URNs may appear in an optional details disclosure, never as the primary title.
- Source provenance uses repo-safe basenames and summaries, never private paths or URLs.

## Frame B — proposed-item list and detail

```text
┌──────────────────────────────┬───────────────────────────────────┐
│ Proposed items               │ Item 1 of 4                       │
│                              │                                   │
│ 1 Action item · review       │ Draft a sample agenda for the     │
│ 2 Decision · review          │ next organizing cycle.            │
│ 3 Attendance · needs info    │                                   │
│ 4 Allocation · review        │ Governing body                    │
│                              │ Example Stewards Circle           │
│                              │ Scope                             │
│                              │ Example Working Group — next cycle│
│                              │ Assignee label                    │
│                              │ Example steward                   │
│                              │                                   │
│                              │ Source: example-organizing-…md    │
│                              │ Authority: assigned action item   │
│                              │ Risk: Low                         │
└──────────────────────────────┴───────────────────────────────────┘
```

On a narrow screen, the list precedes the selected row in one column. The selected row is an `<article>` with a heading; the collection is a semantic list. Status and risk always include words. The assignee is explicitly labeled **Assignee label**—never DID—and the surface explains that identity binding is a separate private activation step.

## Frame C — review choices and mutation preview

```text
┌──────────────────────────────────────────────────────────────────┐
│ What could the reviewer decide?                                 │
│ [Approve] [Reject] [Request changes] [Ask for information]      │
│ [Review later] (shown only when this row allows defer)           │
│ Fixture preview: these controls do not record a decision.        │
│                                                                  │
│ Before anything changes                                          │
│ A later, separate endpoint would create one action-item record   │
│ assigned to the “Example steward” label.                         │
│                                                                  │
│ Expected evidence: action-item completion receipt                │
│ No record or receipt exists merely because this preview is open. │
└──────────────────────────────────────────────────────────────────┘
```

The initial implementation uses real disabled `<button>` elements so the affordances are announced correctly without implying they work. `edit` is rendered as **Request changes**, `request_info` as **Ask for information**, and `defer` as **Review later**. Each control is rendered only when its value is present in that row's `review_actions[]`. The raw enum value can live in a `data-review-action` attribute for tests, not in organizer-facing copy.

## Field-to-copy map

| Contract field | Organizer-facing label/copy | Rule |
|---|---|---|
| wrapper `proposed_artifact.title` | Review preview title | Remove fixture prefixes from the visual hierarchy only if the separate fixture boundary remains prominent. |
| wrapper `review_status` | Ready for review / Changes requested / Approved for next step / Rejected | “Approved” must be followed by “for the next review step; no mutation recorded here.” |
| row `plain_summary` | Primary item summary | Show first; do not require raw JSON. |
| row `kind` | Action item / Decision / Attendance / Obligation / Allocation / Settlement / Evidence note / Risk note | Plain labels, never unexplained enum strings. |
| row `status` | Awaiting row review / Approved to stage the row's next step / Row rejected / Row needs changes / Row needs more information | These labels are row-specific and must not be reused for the wrapper status. “Approved to stage” must be followed by “no change is made by this preview.” Status text must not rely on color. |
| row `governing_body_label` | Governing body | Human-readable label only. |
| row `target_scope_label` | Scope | Human-readable label only. |
| row `assignee_label` | Assignee label | Never relabel as identity or DID. |
| row `source_provenance_ref.ref_label` | Source | Safe basename/reference label; no private path. |
| row `review_actions[]` | Disabled review-choice buttons | Affordances only; no event handler performs mutation. |
| row `mutation_preview.summary` | Before anything changes | Consequence appears before any future confirm surface. |
| row `receipt_expected` | Expected evidence | State the category in plain language and say it is expected, not already issued. |

## Accessibility gate for the implementation PR

The implementation is in scope for the [organizer/member accessibility gate](ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md). Its PR must attach the gate checklist and, at minimum, verify:

- semantic headings, list structure, `<article>` detail, and native disabled buttons;
- a plain-language summary before structured detail;
- keyboard reading order matching visual order, with visible focus on enabled navigation;
- status, risk, fixture boundary, and selection conveyed with text rather than color alone;
- 200% zoom and a single-column mobile layout without horizontal scrolling;
- touch targets of at least 44×44 CSS pixels for enabled navigation;
- reduced-motion behavior and no animation required to understand state;
- no raw JSON, URN, hash, DID, or terminal command required for the basic path;
- fictional/fixture status, privacy posture, mutation boundary, and expected receipt category announced in text;
- an automated axe/Playwright pass, while explicitly leaving any real human/assistive-technology pass as separate evidence unless it is actually performed.

## Implementation boundary

The next UI PR should be limited to:

1. a `?mode=demo` **Review preview** entry in the Pilot UI;
2. offline fixture loading for the two existing contract packets;
3. read-only overview/list/detail rendering through DOM APIs and `textContent`;
4. disabled review-choice controls with no mutation event handlers;
5. Playwright coverage for fixture loading, plain-language copy, semantic structure, keyboard reachability of navigation, and the no-mutation boundary;
6. the required service-worker cache-version bump for changed static assets.

It must not add a gateway endpoint, persist a decision, bind a holder label to a DID, create an ActionCard, issue a receipt, export evidence, or alter authentication/authorization.

## Nonclaims

- Not an organizer-ready or organizer-validated surface.
- Not a production deployment or formal NYCN pilot.
- Not live federation, cloud sync, or private-data handling.
- Not a mutation API and not a decision record.
- Not holder-label-to-DID activation or entity-aware authorization enforcement.
- Not evidence that an ActionCard or receipt exists; the surface only describes what a later documented step would create and which receipt category would be expected.
- Does not complete #1726, #1727, #1728, #1746, #2041, #2082, or Phase 2.
