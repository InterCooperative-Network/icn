---
id: "0000"
title: "RFC Process"
status: "accepted"
created: "2026-04-26"
updated: "2026-04-26"
authors: ["Matt Faherty"]
reviewers: []
related_adr_candidates: []
related_issues: []
supersedes: []
superseded_by: []
---

# RFC 0000: RFC Process

## Status

`accepted` — this RFC defines its own process and is the seed RFC for the system. Future amendments are made by drafting a successor RFC.

## The one-line model

> **RFCs explore. ADRs decide. Issues build. Tests prove. The website claims only what the proof supports.**

## Pipeline

```text
                 ┌─────────────────┐
   design        │ rfc_candidates  │  ops/coordination/rfc_candidates.yaml
   space         │     .yaml       │  unresolved design spaces, no decision yet
                 └────────┬────────┘
                          │  promote
                          ▼
                 ┌─────────────────┐
   exploration   │      RFC        │  docs/rfcs/RFC-NNNN-*.md
                 │ draft → active  │  options, tradeoffs, alternatives
                 └────────┬────────┘
                          │  accept (project converges)
                          ▼
                 ┌─────────────────┐
   decision      │ adr_candidates  │  ops/coordination/adr_candidates.yaml
   queued        │     .yaml       │  rows updated to point at the accepted RFC
                 └────────┬────────┘
                          │  draft ADR
                          ▼
                 ┌─────────────────┐
   decision      │      ADR        │  docs/adr/ADR-NNNN-*.md
                 │ proposed →      │  records the decision and rationale
                 │ accepted        │
                 └────────┬────────┘
                          │  open issue(s)
                          ▼
                 ┌─────────────────┐
   commitment    │   GitHub issue  │  links back to ADR + RFC
                 └────────┬────────┘
                          │  implement
                          ▼
                 ┌─────────────────┐
   evidence      │   tests / proof │  crates/*/tests/, docs, observability
                 └────────┬────────┘
                          │  proof passes
                          ▼
                 ┌─────────────────┐
   status update │ ADR.implementation_status →
                 │   not_started → partial → implemented → verified
                 └────────┬────────┘
                          │  evidence is public
                          ▼
                 ┌─────────────────┐
   public claim  │     website     │  website/src/
                 │  claims only    │  evidence-linked maturity band
                 │  what's proven  │
                 └─────────────────┘
```

The pipeline is one-way in spirit. Anyone can write an RFC; not every RFC reaches `accepted`; not every accepted RFC produces an ADR; not every ADR is implemented; not every implementation is publicly claimed. Each arrow above filters out designs that should not advance.

## When to write an RFC

Write an RFC when:

- The design space is **non-obvious**. Multiple viable approaches exist; tradeoffs are real; picking one without writing them down would lose information.
- The change crosses **multiple subsystems** or **kernel/app boundaries**. Anything that touches the meaning firewall benefits from explicit exploration.
- The change has **accessibility, conflict-resolution, or compute-authority** consequences. These are first-class concerns; an RFC forces them to be addressed in writing.
- The project is **forming a north star**. Forward-direction architecture (e.g., CCL as institutional process language; cross-institution mandate recognition) needs design exploration before decisions are made.
- A **public claim is at stake**. If the website would describe a capability differently after the change, an RFC keeps the truth boundary honest.
- A **reviewer can't make sense of an ADR** because the alternatives are missing. The fix is to write the RFC the ADR was implicitly depending on.

## When NOT to write an RFC

Do not write an RFC when:

- The change is a **bug fix** with a single correct answer.
- The change is a **straightforward implementation** of an already-accepted ADR.
- The design is **constrained by existing invariants** to a single path. (Write a short ADR instead, citing the invariant.)
- The change is **scoped to one file**, has no cross-cutting effects, and would be reviewable in a normal PR.
- The decision is **already made** elsewhere and the work is to record it. (Write an ADR back-fill instead.)
- The design space is **so unsettled** that a candidate row in [`rfc_candidates.yaml`](../../ops/coordination/rfc_candidates.yaml) is the right artifact and the RFC itself can wait.

If in doubt, prefer a candidate row first; promote to an RFC when the design space is concrete enough to compare options.

## How RFCs move to ADRs

1. RFC drafted → `status: draft`. Author writes alone.
2. RFC opened for review → `status: active`. Reviewers comment, propose alternatives, surface invariants.
3. Project converges on an option (or rejects all). RFC moves to `status: accepted` (or `rejected`). The `Outcome` section names the choice and the rationale.
4. The follow-up ADR(s) are drafted. They reference the accepted RFC. ADR `status` lifecycle then takes over per [ADR-0018](../adr/ADR-0018-adr-lifecycle-and-canonical-decision-index.md).
5. [`adr_candidates.yaml`](../../ops/coordination/adr_candidates.yaml) rows are updated to point at the accepted RFC and the new ADR file paths.

An RFC may produce **zero, one, or multiple** ADRs. A complex RFC (e.g., obligation/allocation/settlement) routinely produces three or four. A narrow RFC may produce one. A rejected RFC produces zero.

## How RFCs relate to issues

- An RFC may **cite** existing issues as evidence of the design problem. RFCs do not own implementation; issues do.
- An accepted RFC's `Follow-up implementation issues` section names the issues that should exist (or be opened) to drive implementation. Those issues link back to the RFC and the resulting ADR(s).
- Issues do not change RFC status. Issues track work; RFCs track design.

## How implementation evidence updates ADRs

ADR `implementation_status` is a separate field with the closed taxonomy `not_started | partial | implemented | verified`. It moves only when:

- `partial` requires landed code that exercises the decision in production paths.
- `implemented` requires landed code **and** integration tests proving the decision's invariants.
- `verified` requires implementation **and** evidence the invariant holds under the production deployment (telemetry, fault injection, replay audit, etc.).

**Accepted RFC does not move ADR implementation status.** Accepted ADR does not move implementation status. Only code and tests do. This separation is what keeps the website honest.

## How public website claims depend on evidence

The public website ([website/src/](../../website/src/)) claims only what implementation evidence supports. The maturity bands (`strong | advancing | maturing | behind | not-yet`) are documented in [ADR-0032](../adr/ADR-0032-website-truth-boundary.md). The chain is:

- `not-yet` claim: the RFC may exist; no ADR or no implementation. Public framing must be aspirational.
- `behind` claim: ADR exists; implementation is partial; member-facing surface is missing.
- `maturing` claim: implementation exists; integration paths and public surfaces are still being completed.
- `advancing` claim: implementation exists; gaps are visible; the system can stand behind it for some uses but not all.
- `strong` claim: implementation is complete and hardened; ADR `implementation_status: verified`; the project is willing to stand behind it in production.

[ADR-0033](../adr/ADR-0033-public-maturity-claims-and-evidence-links.md) proposes a build-time linter that verifies every banded claim cites an ADR, issue, code path, or phase reference. Until that linter ships, evidence-linking is editorial discipline.

## How to supersede or withdraw RFCs

**Supersede** when the design space changes meaningfully or a better proposal emerges:

1. Draft the successor RFC.
2. In the successor's frontmatter, set `supersedes: ["NNNN"]`.
3. In the original RFC's frontmatter, set `status: superseded` and `superseded_by: ["MMMM"]`.
4. Add a `> Update (YYYY-MM-DD)` block at the top of the original pointing to the successor.
5. **Do not delete the original body.** Superseded RFCs preserve institutional memory of why the project changed direction.

**Withdraw** when the author or project removes the proposal before a decision:

1. Set `status: withdrawn` in frontmatter.
2. Add a short note at the top explaining why.
3. Preserve the body. A withdrawn RFC is part of the design history.

## How RFCs interact with `ops/coordination/adr_candidates.yaml`

The two registries are complementary, not redundant.

- [`rfc_candidates.yaml`](../../ops/coordination/rfc_candidates.yaml) lists **unresolved design spaces**. A row here means "the project knows this is a place where decisions will eventually need to be made; we have not yet explored the space enough to draft an RFC."
- [`adr_candidates.yaml`](../../ops/coordination/adr_candidates.yaml) lists **likely future decisions**. A row here means "the project knows this decision will need to be recorded; the decision may follow from an RFC, may follow from an obvious-direction back-fill, or may be a forward-state proposal that can be modified later."

Some ADR candidates **should be preceded by an RFC** because the design space is unsettled. Examples in the current registry:

- ADR-0029 (Conflict Resolution Object Model) — broad design space, multiple plausible models. → RFC-0002.
- ADR-0023 (CCL Institutional Process Language) — forward direction with non-trivial language design. → RFC-0004.
- ADR-0027 (Action Card Contract) — multi-stakeholder UX surface. → RFC-0005.
- ADR-0028 (Accessibility Baseline) — five-layer model with explicit floor commitments. → RFC-0003.

Other ADR candidates can proceed without an RFC because they back-fill existing implementation reality (e.g., ADR-0021 CCL determinism, ADR-0030 compute workload manifest). The signal is whether the design is *settled by code* or *unsettled in writing*.

When an RFC moves to `accepted`, update the relevant ADR candidate row(s) to add `related_rfc: NNNN` and the candidate's `priority` may shift. This keeps the two registries in sync without forcing either to become authoritative over the other.

## Numbering

- RFC numbers are sequential, four-digit zero-padded (`RFC-0001`, `RFC-0002`, …).
- Numbers are allocated when an RFC moves from `candidate` to `draft`. The author picks the next unused number; collisions are resolved by bumping.
- Withdrawn or rejected numbers are **not** reused. The number reflects the chronology of the conversation, not the chronology of accepted decisions.
- This RFC is `RFC-0000` because it is the seed for the system. No future RFC takes that number.

## What this RFC is not

- Not a project-governance ADR. ICN's broader project governance is the territory of [ADR-0063 candidate](../../ops/coordination/adr_candidates.yaml) and a future RFC-0009.
- Not a license to bypass ADR review. Accepting an RFC does not skip the ADR step; it just makes the ADR step easier.
- Not a documentation treadmill. RFCs are written when the design space justifies them. Most ICN work does not need an RFC. The pipeline filters at every step for a reason.
