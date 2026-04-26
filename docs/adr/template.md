---
id: "NNNN"
title: "Short Decision Title"
status: "proposed"
date: "YYYY-MM-DD"
deciders: ["Name 1", "Name 2"]
tags: ["topic1", "topic2"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "proposed"
references:
  - "ADR-NNNN (relationship description)"
  - "docs/path/to/related.md"
---

# ADR-NNNN: Short Decision Title

## Status

`proposed` — until the project decides; then `accepted`. Subsequent
amendments do not change the lifecycle status to `proposed` again;
they add an amendment note and (if accepted on review) flip the
status to `amended`. Supersession is recorded by adding a successor
ADR and updating both `status` and `superseded_by` here.

Decision status (`status` field) is independent from implementation
status (`implementation_status` field). See ADR-0018 for the
lifecycle vocabulary and rules.

## Context

What's the situation? What forces are at play? What problem are we
solving?

## Decision

What did we decide? Be specific — include the key choice and any
important constraints that shaped it.

## Consequences

What are the trade-offs? What becomes easier? What becomes harder
or more risky?

## Implementation status

If code already proves out part of this decision, list:

- evidence paths (file path or PR number)
- which sub-claims are proven and which are not
- what would have to land to call this `implemented`

If this ADR is purely doctrinal (no code change), say so.

## Alternatives Considered

What else did we evaluate and why did we not choose it?

| Alternative | Why rejected |
|-------------|-------------|
| Option A    | … |
| Option B    | … |

---

## Notes

- For *amendments*, add an `## Amendments` section with dated
  entries naming the amending ADR; do not rewrite the body.
- For *supersession*, add a header `> Update (YYYY-MM-DD)` block
  pointing to the successor ADR; preserve the original body
  unchanged below it.
- Metadata format above is YAML frontmatter. Existing ADRs may use
  classic markdown (`**Date**:` style) or bullet metadata; the MCP
  indexer parses all three. New ADRs SHOULD use the frontmatter
  form.
