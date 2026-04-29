# Promotion Review — gate before any RFC / ADR / issue / package / learning / website target

A promotion review is the artifact that decides whether an idea moves
from the refinery into another pipeline (RFC candidates, ADR
candidates, GitHub issues, NYCN package tasks, icn-learn packets, or
website claims) — or is parked or rejected.

> **Capture is cheap; promotion has thresholds.** A promotion review
> is the explicit gate where those thresholds are checked.

## When to open one

- An idea is `framed` or `classified` and someone wants to act on it.
- A framing brief, source review, or dogfood slice has produced
  enough shape to decide on a destination.
- An idea has been `parked` and a new signal makes it actionable.

## Outline

```markdown
# {idea title} — promotion review

**Idea card:** ops/ideas/ideas.yaml#idea-NNNN
**Reviewer / session:** ...
**Date:** YYYY-MM-DD

## Idea summary

One sentence (copy from the idea card's `one_sentence`).

## Current artifacts

- Framing brief: path or none
- Source review: path or none
- Dogfood slice: path or none
- Other: prior PRs, ADRs, RFCs, related issues

## Proposed promotion target

Pick exactly one:

- [ ] RFC candidate (`rfc_candidates.yaml`)
- [ ] ADR candidate (`adr_candidates.yaml`)
- [ ] GitHub issue
- [ ] NYCN package task
- [ ] icn-learn packet
- [ ] Website claim
- [ ] Park (real but not actionable)
- [ ] Reject (will not be pursued)
- [ ] Supersede (cite replacing idea)

## Threshold check

Mark the row that applies. All boxes must be checked for the
selected target.

### → RFC candidate

- [ ] Design space is unresolved.
- [ ] Multiple viable options exist with real tradeoffs.
- [ ] Decision would affect generic ICN architecture (not just NYCN).
- [ ] The framing brief enumerates options, not just a name.

### → ADR candidate

- [ ] Decision is clear enough to record.
- [ ] Scope is generic, or back-fills actual implementation.
- [ ] Consequences are understood.
- [ ] Implementation status will be tracked separately on the ADR.

### → GitHub issue

- [ ] Build slice is clear.
- [ ] Acceptance criteria are clear.
- [ ] Affected files / crates / docs are identifiable.
- [ ] Validation / proof path is known.

### → NYCN package task

- [ ] Institution-specific meaning belongs in NYCN.
- [ ] Generic ICN substrate exists or is explicitly marked planned.
- [ ] No private data in the proposed change.
- [ ] Boundary check: ICN does not absorb the institution-specific
      meaning.

### → icn-learn packet

- [ ] Canonical source exists or is explicitly linked.
- [ ] Teaching material does not define doctrine.
- [ ] Cross-role vs role packet is decided (per icn-learn README).

### → Website claim

- [ ] Backed by state docs, tests, ADR `implementation_status:
      implemented` or `verified`, or shipped runtime.
- [ ] Maturity band is honest (per ADR-0033).
- [ ] Evidence link is specific (test, receipt, ADR — not a generic
      doc reference).

## Cross-repo effect

What other repos does this promotion touch?

- ICN: ...
- NYCN: ...
- icn-learn: ...
- public website: ...

If multiple repos are affected, the merge order is fixed (per
[`PR_STACK_PROTOCOL.md`](../../coordination/PR_STACK_PROTOCOL.md)):
ICN canonical first, NYCN application second, ICN Academy teaching
third.

## Decision

- Outcome: promote / park / reject / supersede.
- Target artifact reference (filled at promotion):
  - `rfc_candidates.yaml#NNNN`
  - `adr_candidates.yaml#NNNN`
  - `issues#NNNN`
  - NYCN repo path
  - icn-learn repo path
  - website path

## Idea card update

After the decision, update `ops/ideas/ideas.yaml`:

- `status` → one of `promoted_*`, `parked`, `rejected`, `superseded`.
- `promoted_target` → reference to the target artifact.
- `next_transform` → a short description of the next step in the
  target's own pipeline (not in the refinery).
```

## Discipline

- A promotion review is a **gate**, not a celebration. Most ideas
  should land in `parked`, `rejected`, or one of the lighter
  destinations (NYCN package task / icn-learn packet) — not RFCs.
- An RFC candidate that is just a renamed idea card should be
  rejected at this gate. RFCs require enumerated options and
  tradeoffs.
- A website claim that is not backed by ADR
  `implementation_status: implemented | verified` or a receipt-bearing
  test should be rejected at this gate.
- The promotion decision is recorded in `ops/ideas/ideas.yaml`. The
  refinery is the audit trail of what the project considered and
  what it decided to do with it.
