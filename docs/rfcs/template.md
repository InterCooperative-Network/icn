---
id: "NNNN"
title: "Short RFC Title"
status: "draft"
created: "YYYY-MM-DD"
updated: "YYYY-MM-DD"
authors: ["Name or DID"]
reviewers: []
related_adr_candidates: []
related_issues: []
supersedes: []
superseded_by: []
---

# RFC NNNN: Short RFC Title

## Status

`draft` — being written, not yet ready for review.
`active` — open for review and comments.
`accepted` — project has converged. The follow-up ADR(s) record the decision.
`rejected` — project decided against. Body preserved for reference.
`superseded` — a later RFC replaces this one.
`withdrawn` — author or project removed before a decision.

**Accepted RFC does not mean implemented.** Implementation status lives on ADRs and changes only with code/test evidence. See [RFC-0000](RFC-0000-rfc-process.md) for the lifecycle in detail.

## Summary

One paragraph. What problem does this RFC explore, and what is the recommended direction? A reader should be able to stop here and know whether the rest of the document is relevant.

## Problem statement

What is the situation that motivates this RFC? What is missing, broken, ambiguous, or contested? Be specific — name files, ADRs, issues, observed behavior. An RFC that cannot articulate a concrete problem usually should not be an RFC yet.

## Goals

What outcomes would make this RFC "succeed"? List the specific, falsifiable properties the chosen design should achieve.

## Non-goals

What is *not* in scope? RFCs that drift into adjacent territory produce incoherent decisions. Name the territory you are deliberately leaving alone.

## Background / current state

What does the project already know? Summarize the relevant ADRs, prior RFCs, code surfaces, and issues. Cite specific files (`crate/path/file.rs:LINE`) where they ground a claim. Quote sparingly.

## Design options

At least two options. Three is healthy. One option is not an exploration.

### Option A — short name

What is the design? How does it work concretely? What does it look like at the runtime, package, and member-facing layers?

### Option B — short name

Same shape.

### Option C — short name

Same shape. (Optional.)

## Tradeoffs

For each option above, list:

- What it makes easier.
- What it makes harder.
- What invariants it preserves.
- What invariants it stresses.
- What new failure modes it introduces.
- What it costs (engineering, complexity, migration burden).

A table is often the right form here. The point is to make the comparison legible.

## Core/package boundary

How does this design respect the kernel/app separation and the institution-package boundary?
- What lives in ICN core (generic substrate, primitive types, constraint enforcement)?
- What lives in institution packages (local vocabulary, charter content, fixtures)?
- What stays opaque to the kernel?
- Does any option erode the meaning firewall? If so, name the erosion.

See [docs/architecture/KERNEL_APP_SEPARATION.md](../architecture/KERNEL_APP_SEPARATION.md) and [docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md).

## Accessibility implications

How does this design land for low-bandwidth, low-device, non-English, disabled, caregiving, or shift-working members? If the answer is "it doesn't change anything," say so explicitly and explain why. Accessibility is a participation floor; an RFC that ignores it is incomplete.

See ADR-0028 (Accessibility Baseline for Member Interfaces).

## Conflict / dispute path

How are conflicts about this design's outcomes handled?
- Conflicts about effects (post-decision)?
- Conflicts about decisions (process appeal)?
- Relational conflicts between members?

If this RFC is upstream of conflict resolution itself, name the dependency.

See ADR-0029 (Conflict Resolution Object Model).

## Security / privacy implications

What new attack surface does this design introduce or remove? What sensitive data does it touch? What re-disclosure risks does it create? What capability boundaries does it stress?

If the design has no security or privacy implications, say so explicitly and justify the claim.

## Compute / automation boundary

What may compute do under this design? What may compute *not* do?
- Is compute carrying out what authority asked, or making decisions?
- What is governance-grade output vs advisory output?
- Are determinism and fuel bounds preserved?

See ADR-0030 (Compute Workload Manifest and Authority Boundary).

## Website / public truth implications

What can the public website honestly say once this RFC is accepted? What can it *not* say?
- What maturity band would the relevant subsystem occupy?
- What evidence would the website link to?
- What overclaim risk does the design create?

See ADR-0032 (Website Truth Boundary).

## Migration / compatibility

If accepted, what changes for:
- Existing deployments?
- Existing institution packages?
- Existing CCL contracts?
- Existing API consumers?
- Existing test surfaces?

If the design is greenfield with no migration cost, say so.

## Open questions

Specific, unresolved questions that block a decision. Each question should be answerable; vague concerns belong in Tradeoffs.

## Decision criteria

What would tip the project toward one option over another? Be concrete. "We pick A if we observe X; we pick B if we observe Y." A vague decision criterion produces a vague decision.

## Outcome

Filled in when the RFC moves to `accepted` or `rejected`. Names the chosen option (or the rejection rationale) and links to the follow-up ADR(s) that record the decision.

## Follow-up ADRs

If accepted, which ADRs (or ADR candidates) does this RFC produce? Cite by id. Update [ops/coordination/adr_candidates.yaml](../../ops/coordination/adr_candidates.yaml) accordingly.

## Follow-up implementation issues

If accepted, what issues should be opened to drive implementation? Cite existing issues where applicable; name new issues that should be opened.

## Validation / proof plan

How will the project know the implementation matches the decision? List the test surfaces, integration paths, and observability signals that will demonstrate correctness. Implementation status moves only when this proof plan is satisfied.

---

## Notes

- For *amendments* during `draft` or `active`, edit in place and bump `updated`. RFCs in those statuses are living documents.
- For *amendments* after `accepted`, prefer a successor RFC. Editing accepted text rewrites history.
- For *supersession*, add `> Update (YYYY-MM-DD)` block at the top pointing to the successor RFC; preserve the original body unchanged. Update both `status` and `superseded_by` in frontmatter.
- For *withdrawal*, set `status: withdrawn` and add a short note explaining why.
