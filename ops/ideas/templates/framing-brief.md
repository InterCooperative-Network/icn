# Framing Brief — refine scope and boundary

A framing brief is the artifact between an idea card and any promotion
to RFC / ADR / issue / package task / learning task / website claim.

> **A framing brief shapes an idea enough that someone can decide what
> it should become next. It is not a design doc and not a decision.**

Use this template when an idea card is ready to be sharpened. Keep it
short. If a framing brief grows beyond two pages, decompose the idea
into multiple cards.

## Outline

```markdown
# {title} — framing brief

**Idea card:** ops/ideas/ideas.yaml#idea-NNNN
**Author / session:** ...
**Date:** YYYY-MM-DD

## What this is

One paragraph. State the idea in plain language. No jargon, no naming
of imagined objects yet.

## Why this is interesting

Two or three sentences. What problem does this address? What pattern
does it want to break? Who feels the problem today?

## Scope

What is in scope and what is not. Be ruthless. The most common framing
mistake is bundling three ideas into one brief.

- In scope:
- Out of scope:
- Adjacent (named, not pursued):

## Boundary check

Where does this belong?

- ICN core (generic primitive)
- ICN app (PolicyOracle / state model)
- NYCN package (institution-specific)
- ICN Academy (teaching)
- Public website (claim)
- Private overlay (operational, not Git)
- Google Drive (source material, not architecture)
- External (outside ICN)

If the answer crosses boundaries, the idea probably needs to decompose.

## Existing surface

What already exists in the repo that this idea touches?

- Crates / apps:
- Docs:
- ADRs / RFCs (cite by id):
- Issues / PRs:
- NYCN / icn-learn material:

## Open questions

The questions a future RFC, ADR, or implementation would have to
answer. If there are no open questions, this might already be an ADR
candidate, not an RFC candidate.

1.
2.
3.

## Privacy and boundary risks

- Does this idea touch private operational data?
- Does it risk leaking institution-specific meaning into ICN core?
- Does it require a Drive / external source review before promotion?

## Proposed next artifact

Pick exactly one:

- [ ] another framing brief (decompose first)
- [ ] source review
- [ ] dogfood slice
- [ ] promotion review → RFC candidate
- [ ] promotion review → ADR candidate
- [ ] promotion review → GitHub issue
- [ ] promotion review → NYCN package task
- [ ] promotion review → icn-learn packet
- [ ] promotion review → website claim
- [ ] park
- [ ] reject

## Receipts / evidence (if relevant)

What evidence would have to exist for this idea to be implemented or
claimed publicly? Cite specific receipt types, ADR
`implementation_status` transitions, test names, or runtime endpoints.
```

## Discipline

- A framing brief is **descriptive**, not normative. It does not
  decide; it makes the idea legible enough for someone else to decide.
- A framing brief that turns into doctrine should be promoted to RFC
  or ADR. Briefs that linger as quasi-architecture are exactly the
  drift this layer exists to prevent.
- A framing brief that names invented objects without a proof path
  should be challenged at promotion review.
