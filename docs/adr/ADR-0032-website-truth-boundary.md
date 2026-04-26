---
id: "0032"
title: "Website Truth Boundary"
status: "accepted"
date: "2026-04-26"
deciders: ["Matt Faherty"]
tags: ["website", "truth-boundary", "maturity", "claims", "back-fill"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "implemented"
references:
  - "ADR-0018 (ADR Lifecycle and Canonical Decision Index)"
  - "ADR-0033 (Public Maturity Claims and Evidence Links)"
  - "website/src/pages/whats-real-now.astro"
  - "website/src/components/MaturityBadge.astro"
  - "website/src/data/roadmap.json"
---

# ADR 0032: Website Truth Boundary

## Status

**Accepted (2026-04-26).** Back-fill of an editorial convention the public site already follows. Records the rule so it does not drift across PRs.

## Context

The public website at intercooperative.network is the truth boundary between ICN and the world. The site already follows a maturity-band convention anchored on `whats-real-now.astro`: every subsystem claim is explicitly banded as `strong`, `advancing`, `maturing`, `behind`, or `not-yet`. A `MaturityBadge` component is used inline; `MaturityCard` and `MaturityGrid` provide structured bands; `roadmap.json` carries banded stages.

The convention works. Reviewers can read any claim on the site, find its band, and ask "is the implementation status worthy of this band?" Without an ADR, the convention is editorial folklore — easily lost in a future redesign.

The convention's core rule is **honesty over polish**. The website's job is to give a member, contributor, partner, or auditor an accurate picture of what is real today, what is advancing, and what is still aspiration. This is a cooperative-substrate-specific rule: a project that overclaims to attract partners destroys the trust it needs to function. Honesty is itself a feature.

## Decision

The ICN public website is a truth boundary subject to the following rules.

### Maturity bands (closed taxonomy)

- **strong** — implemented, hardened, on the production path; ICN is good at this today.
- **advancing** — implemented in core, gaps remain; significant new work landing regularly.
- **maturing** — implementation exists, public surfaces and integrations are still catching up; honest about the asymmetry.
- **behind** — recognized as the part of the system most visibly trailing the substrate underneath; the gap is named, not hidden.
- **not-yet** — designed, not yet implemented; explicitly aspirational with a path forward.

### Claim discipline

1. **No claim sits without a band** on member-facing surfaces (`how-it-works`, `whats-real-now`, `for-cooperatives`, `for-developers`, `roadmap`, `index` lead).
2. **No band overstates the implementation.** A `strong` band requires shipped, tested, deployed code. A `maturing` band requires real implementation behind partial public surface. `not-yet` requires explicit "not implemented" signaling.
3. **No autonomous-compute-decision claims.** Per ADR-0030, compute is constrained execution, never autonomous decision; the website never implies otherwise.
4. **No payment / wallet / balance / currency vocabulary** for ICN economic primitives. ICN uses position, settlement, obligation, allocation. ADR-0004, ADR-0005, ADR-0031 enforce the vocabulary in code; this ADR enforces it on the public surface.
5. **No mention of specific institution packages** in core public content beyond the demo system — and even there, generic "reference institution" framing only. The website serves ICN, not a particular institution.
6. **`whats-real-now` carries a `Reviewed:` date.** That date is the freshness anchor. Other pages may be undated; the truth anchor is dated.
7. **Implementation gaps are named, not buried.** Member-facing experience trailing the substrate is part of the public story, not a polished omission.

### Editorial process

- Material claim changes (anything moving a band, adding a subsystem, changing vocabulary) get reviewed against this ADR.
- Drift is caught at review time, not at runtime. There is no production accessibility scanner that auto-fails the site for overclaim.
- Update of `whats-real-now`'s `Reviewed:` date is a load-bearing edit; do not bump the date without re-checking each claim.

### Boundaries

- The website is **not** the source of truth for implementation status. The repo is. The website cites the repo.
- The website is **not** a replacement for [docs/STATE.md](../STATE.md) or [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md). Those are internal; the website is for outsiders.
- The website **may** lag the repo. Claims must not exceed the repo; claims may trail it.

## Consequences

- The maturity-band convention is locked. New pages and components inherit the rule; redesigns that abandon it are an ADR-supersession event.
- Future evidence-linking work (ADR-0033) plugs into a written truth boundary.
- Trade-off: the floor is opinionated. Some marketing pressure to soften "behind" or to drop bands entirely will arise; this ADR is the reference for refusing.
- Trade-off: editorial discipline is process, not automation. Reviewers carry the load.

## Implementation status

Implemented as editorial convention. Evidence:

- `whats-real-now.astro` carries a `Reviewed:` date and explicit bands per subsystem.
- `MaturityBadge`, `MaturityCard`, `MaturityGrid` components encode the band taxonomy.
- `roadmap.json` carries banded stages.
- Existing audit (April 2026) confirmed zero NYCN/Summit/reference-federation mentions in `website/src/`; vocabulary discipline (no payment/wallet/balance) is honored.

Future work, captured in ADR-0033: a build-time linter that verifies every `MaturityBadge` usage carries a citation to an ADR or issue.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Let pages set their own claim discipline | Produces drift across pages; the truth anchor breaks. |
| Adopt a marketing-style framing without bands | The cooperative substrate use case requires honesty over polish. A site without bands is easier to read and easier to mistrust. |
| Move maturity bands to a separate "developer" page only | Members and partners benefit from bands too. The whole site is the truth boundary. |
| Let the band taxonomy grow per page need | Bands lose meaning when each page invents its own. The closed taxonomy is the contract. |
