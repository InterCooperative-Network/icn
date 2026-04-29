# Idea Card — minimal capture

Use this template when capturing a new idea into `ops/ideas/ideas.yaml`.
Capture is cheap; promotion has thresholds (see
[`ops/ideas/README.md`](../README.md)).

> **An idea card is not a decision and not a backlog commitment.** It
> is a captured observation that we will then triage, frame, classify,
> decompose, park, promote, or reject.

## Required fields

```yaml
- id: idea-NNNN                    # zero-padded, four digits
  title: short-noun-phrase
  status: raw                      # raw, captured, framed, classified,
                                   # decomposed, needs_source_review,
                                   # needs_dogfood, promotion_review,
                                   # promoted_*, parked, rejected,
                                   # superseded
  kind: architecture_concept       # see ops/ideas/README.md
  source: "session 2026-04-28; conversation with X; Drive folder Y"
  one_sentence: "Single sentence stating the idea."
  problem: |
    Two or three sentences. What is the actual problem this idea is
    trying to address? What is wrong today? What patterns does it want
    to break?

  belongs_to: icn                  # icn, nycn, icn-learn, website,
                                   # private_overlay, google_drive,
                                   # external, unknown
  layer: substrate                 # informal: substrate / runtime /
                                   # institutional / package / learning /
                                   # public

  current_artifact: "(none)"       # what exists today: a sentence in
                                   # CLAUDE.md, a Drive doc, a
                                   # conversation note, an ADR sketch,
                                   # a stale comment, etc.
  proposed_next_artifact: framing_brief   # idea_card / framing_brief /
                                          # source_review / dogfood_slice /
                                          # promotion_review / rfc_candidate /
                                          # adr_candidate / issue / package_task /
                                          # learning_packet / website_claim
  promoted_target: null            # filled at promotion: e.g.
                                   # "rfc_candidates.yaml#NNNN" or
                                   # "issues#NNNN" or "nycn:summit/2026/..."

  proposed_objects:
    - "(strings only — names, not commitments)"
  requires_rfc: false              # true / false / unknown
  likely_adr: false                # true / false / unknown
  implementation_ready: false      # true requires a proof path field
  public_claim_ready: false        # true requires evidence_required + evidence

  evidence_required: []            # list of evidence types needed
                                   # before public claim or
                                   # implementation_ready: true
  privacy_risk: low                # low / medium / high
  boundary_risk: low               # low / medium / high — risk of
                                   # leaking institution-specific
                                   # meaning into ICN core or
                                   # private data into the public repo
  risks:
    - "list of concrete risks"

  next_transform: |
    What needs to happen to move this idea to its next artifact.
    Be concrete. Name a person/role only if a human owner is required.
    Otherwise name the artifact and the question it must answer.
```

## Field discipline

- `id` is unique across all idea-cards. The validator enforces this.
- `title` is a short noun phrase. Imperative phrasing ("Implement X",
  "Add Y") belongs on issues, not idea cards.
- `source` is human-readable and citation-style. It is not a typed
  reference.
- `proposed_objects` is a list of name candidates only. Naming an
  object here does not commit to building it.
- `requires_rfc` and `likely_adr` are best-guess at capture time and
  may change at framing or promotion review.
- `implementation_ready: true` requires the idea to have a proof path
  (a `dogfood_slice` template, an existing test fixture, or a
  receipt-bearing flow).
- `public_claim_ready: true` requires `evidence_required` to be
  populated and have at least one entry satisfied.

## Status transitions

- `raw` → `captured`: a one-sentence problem and a source are
  recorded.
- `captured` → `framed`: a framing brief is written.
- `framed` → `classified`: `kind`, `belongs_to`, and `layer` are
  finalized.
- `classified` → `decomposed`: the idea is broken into smaller ideas
  if needed (each gets its own card).
- Any state → `needs_source_review`: a Drive/external source must be
  reviewed before promotion.
- Any state → `needs_dogfood`: a NYCN dogfood slice is required.
- Any state → `promotion_review`: a promotion review is opened.
- `promotion_review` → `promoted_*` or `parked` or `rejected`.

## What an idea card is not

- Not an RFC. RFCs require enumerated options and tradeoffs.
- Not an ADR. ADRs record decisions.
- Not an issue. Issues require buildable acceptance criteria.
- Not a backlog item. Capture does not commit to delivery.
