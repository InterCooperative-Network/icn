# Source Review — map external sources to typed records

A source review captures what exists today in Drive / Sheets / external
SaaS / private notes — and what should happen with each item.

> **Drive and Sheets are bootstrap source material, not canonical ICN
> architecture.** A source review is the artifact that maps that
> material to typed records, privacy classes, and promotion decisions.

Use this template when an idea is `needs_source_review`, or when a
package import (NYCN bootstrap, Drive folder, external SaaS export)
needs to be planned without committing private data to Git.

## Outline

```markdown
# {source} — source review

**Idea card:** ops/ideas/ideas.yaml#idea-NNNN
**Source:** Drive folder / Sheet name / SaaS export / etc.
**Reviewer / session:** ...
**Date:** YYYY-MM-DD

## What this source is

One paragraph. What lives at this source today? Who maintains it? Why
does it exist?

## Items

| Item | Keep as | Future ICN object | Privacy level | Action |
|---|---|---|---|---|

Examples:

| 2026 Planning | NYCN bootstrap source | SummitCycle / Activity (planned) | public structure / private contact | promote to NYCN package as `summit/2026/...` |
| Budget | private allocation plan | governed allocation (planned) | private | summarize publicly only via approved governance act |
| Fundraising | sponsor agreements | obligation + relationship records (planned) | public commitments only; contacts private | split: public sponsor commitments → repo; contacts → private overlay |
| Attendee list | private registration import | attendee record (planned) | private | never expose; bridge import via `BridgeImportReceipt` |
| Evaluation summary | redacted cycle learning | feedback aggregate (planned) | aggregate only, no PII | summarize as design response (no raw rows) |
| Marketing plan | consent campaign / outreach plan | outreach plan + consent receipts (planned) | public framing; per-contact data private | promote framing to NYCN; contacts to private overlay |
| Passwords / secrets | private scoped vault | (none — secrets never enter ICN typed records) | private | move to private overlay; never commit |

## Privacy classes (vault scopes)

Each row above must classify into one of:

- `public-archive` — safe to publish.
- `*-internal` — visible to specific role-holders.
- `*-restricted` — visible to a small named scope; aggregate
  publication forbidden.
- `private-overlay` — never enters Git.

## Boundary check

- ICN-side action: is anything here a generic ICN primitive that
  should land in ICN docs? If yes, open a separate framing brief.
- NYCN-side action: which items become NYCN package material? Cite
  the target file (e.g. `summit/2026/...`).
- Private-overlay-side action: which items must never be in any repo?

## Promotion proposals

For each item, a proposed promotion:

- `promoted_package_task` for NYCN repo content.
- `promoted_learning_task` for icn-learn material.
- `promoted_issue` for ICN runtime work (rare; usually framed first).
- `parked` for items not actionable.
- `private_overlay` for never-Git items.

## Risks

- Private data leak (PII, contacts, secrets).
- Premature pinning of stale Drive structure as architecture.
- Inventing ICN primitives to match Drive shape (anti-pattern — Drive
  shape is bootstrap, not target).

## Forbidden

A source review must not:

- Commit private data to Git.
- Promote raw spreadsheet rows into the public website.
- Treat Drive structure as canonical ICN architecture.
- Introduce new ICN primitives without a framing brief and promotion
  review.
```

## Discipline

- Drive shape is **bootstrap**. The target ICN typed records are
  designed in ICN, not back-derived from a Drive folder.
- Where Drive holds private data, the review's job is to **prevent**
  it from entering the repo, not to find a clever way to import it.
- A source review can promote multiple items in one pass — but each
  promotion target should be tracked as its own row in
  `ops/ideas/ideas.yaml` so promotion review and validation work.
