---
id: "0033"
title: "Public Maturity Claims and Evidence Links"
status: "proposed"
date: "2026-04-26"
deciders: []
tags: ["website", "truth-boundary", "evidence", "linter", "forward-direction"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "proposed"
references:
  - "ADR-0032 (Website Truth Boundary)"
  - "website/src/components/MaturityBadge.astro"
---

# ADR 0033: Public Maturity Claims and Evidence Links

## Status

**Proposed (2026-04-26).** Forward direction. ADR-0032 records the truth-boundary rule; this ADR proposes the practice that operationalizes it.

## Context

ADR-0032 says every claim on the public site sits inside a maturity band. The next move is to require every banded claim to cite **evidence** — an ADR id, an issue, a code path. This makes claim drift mechanically detectable: a band is no longer worth the band unless its evidence link still resolves to something that supports it.

Today, evidence links are inconsistent. Some pages cite ADRs by number; some cite issues; some carry no link at all. The convention is healthy in spirit, ad-hoc in practice.

## Decision (proposed)

Every maturity-banded claim on the public site carries an evidence reference.

### Required evidence forms (any one suffices)

- **ADR id.** `ADR-0021`, `ADR-0026`, etc. Indicates the decision behind the claim.
- **Issue id.** `#1608`, `#1610`, etc. Indicates open work the claim is tracking.
- **Code path.** A repo-relative path to a file or directory. Indicates the implementation backing the claim.
- **Phase.** A reference to [docs/PHASE_HISTORY.md](../PHASE_HISTORY.md) or [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md). Indicates the milestone backing the claim.

### Build-time check

A new build-time linter verifies that every `MaturityBadge` component usage carries an `evidence` prop pointing to one of the above. The linter:

1. Walks `website/src/` and parses `MaturityBadge`, `MaturityCard`, `MaturityGrid` usage.
2. Verifies the `evidence` prop exists and resolves:
   - ADR id resolves to an existing file under `docs/adr/`.
   - Issue id resolves via GitHub API (cached for build perf).
   - Code path resolves to an existing repo file/directory.
   - Phase reference resolves to a heading in PHASE_HISTORY.md or PHASE_PROGRESS.md.
3. Fails the build on broken evidence; emits a structured report.

### Existing prose claims

The linter applies to component-level claims. Prose claims in page bodies that imply a band ("the federation layer is mature," "compute is shipping today") are reviewer-checked, not lint-checked. ADR-0032 is the policy reference.

### `whats-real-now` reviewed date

The page that anchors the truth surface continues to bear a manually-curated `Reviewed:` date. The linter does not touch the date; reviewers do.

## Consequences

- A claim cannot drift silently. If the underlying ADR is superseded or the underlying issue closed without resolution, the linter catches it on the next build.
- The evidence contract is checkable in CI without requiring human review of every PR.
- Trade-off: linter implementation is real engineering work (Astro/JSX parsing, GitHub API integration, caching).
- Trade-off: prose claims escape the linter. The truth boundary is editorial; linter coverage is structural.

## Implementation status

Proposed. Not implemented.

To call this `implemented`:
- Linter implementation in `website/scripts/`.
- `evidence` prop added to `MaturityBadge`, `MaturityCard`, `MaturityGrid`.
- Existing usages backfilled with evidence references.
- Linter wired into the build (`prebuild` step in `package.json`).
- CI integration that fails on broken evidence.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Keep evidence linking as a review-time editorial check | What we have today. Catches obvious drift; misses subtle drift over time. |
| Require evidence as inline prose only (no component prop) | Prose evidence is human-readable but not machine-checkable. The linter is the point. |
| Use external link checking instead of evidence-typed checking | Generic link checkers verify reachability, not semantic match. An ADR id resolving to a `superseded` ADR is a real claim drift; link reachability does not catch it. |
| Defer until claim drift becomes a visible problem | Claim drift is a slow failure mode. By the time it is visible, repair is expensive. |
