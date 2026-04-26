---
id: "0034"
title: "ADR Candidate Registry as Architectural Memory"
status: "accepted"
date: "2026-04-26"
deciders: ["Matt Faherty"]
tags: ["adr", "registry", "coordination", "architectural-memory"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "implemented"
references:
  - "ADR-0018 (ADR Lifecycle and Canonical Decision Index)"
  - "ops/coordination/adr_candidates.yaml"
  - "ops/coordination/README.md"
  - "docs/strategy/ICN_CONSTITUTIONAL_ROADMAP.md"
---

# ADR 0034: ADR Candidate Registry as Architectural Memory

## Status

**Accepted (2026-04-26).** Back-fills the artifact created in this PR. The registry exists; the ADR records the decision to keep it.

## Context

ICN's architecture spans many domains: identity, governance, federation, economics, compute, member shell, accessibility, conflict resolution, package model, website truth, project governance. Decisions are recorded in [docs/adr/](../adr/); the lifecycle for those decisions is governed by ADR-0018.

What ADR-0018 does not address is the **pre-decision** layer — the long arc of architectural intent that has not yet been written as an ADR but that the project needs to remember. Without a coordination artifact for that intent, the long arc tends to fade between sessions: contributors and agents re-discover it ad hoc, and pieces get accidentally designed out.

This PR introduces [ops/coordination/adr_candidates.yaml](../../ops/coordination/adr_candidates.yaml) and the companion [docs/strategy/ICN_CONSTITUTIONAL_ROADMAP.md](../strategy/ICN_CONSTITUTIONAL_ROADMAP.md) as that coordination artifact. This ADR records the decision to maintain them.

## Decision

ICN maintains a machine-readable ADR candidate registry at [ops/coordination/adr_candidates.yaml](../../ops/coordination/adr_candidates.yaml) and a human-readable companion at [docs/strategy/ICN_CONSTITUTIONAL_ROADMAP.md](../strategy/ICN_CONSTITUTIONAL_ROADMAP.md).

### Registry properties

1. **Each candidate row** describes an ADR ICN intends to write, including its target id, title, proposed status (`proposed | accepted`), the domain it addresses, why it matters, runtime surface, accessibility implications, conflict path, compute boundary, website truth implication, existing issues, missing issues, dependencies, priority horizon, and tranche.
2. **Registry state** progresses `candidate → drafted → merged`. Drafted rows reference their `adr_path`. Merged rows are pruned periodically as cleanup.
3. **The registry is not authoritative.** It is a planning index. ADR-0018 governs lifecycle once a row is drafted as an ADR file.
4. **Tranche numbering** groups ADRs by drafting order: tranche 1 (foundational, this PR), tranche 2 (federation/economics), tranche 3 (member shell/accessibility/conflict), tranche 4 (project governance/release/kernel-dispatch detail).

### Roadmap properties

1. **Eleven domains** match the registry's domain taxonomy.
2. **Each domain section** answers four questions: why it matters, ADR candidates that belong here, existing issues, priority horizon.
3. **Non-doctrine** by explicit declaration. Decisions live in `docs/adr/`.
4. **Modifiable.** Forward-direction items can be modified, refused, or superseded once written as ADRs.

### Maintenance

- A row flips from `candidate` to `drafted` when the ADR is written and merged. The roadmap section is updated to link the ADR.
- Rows that are decided to be unnecessary are removed (not silently kept as zombies).
- New rows are added as new architectural concerns emerge; the registry is not closed.

## Consequences

- The long arc of design becomes legible. Contributors and agents can pick up tranches without rediscovery cost.
- Work prioritization is visible. "What's next?" has a written answer at the architectural layer.
- Trade-off: maintenance cost. The registry must be kept current or it becomes worse than nothing.
- Trade-off: the registry could be misread as binding. ADR-0018's lifecycle is the actual binding mechanism; the registry is a coordination artifact only.

## Implementation status

Implemented in this PR.

Evidence:
- [ops/coordination/adr_candidates.yaml](../../ops/coordination/adr_candidates.yaml) — 60+ candidate rows across 11 domains.
- [ops/coordination/README.md](../../ops/coordination/README.md) — conventions and limits.
- [docs/strategy/ICN_CONSTITUTIONAL_ROADMAP.md](../strategy/ICN_CONSTITUTIONAL_ROADMAP.md) — human-readable companion.
- Tranche 1 ADRs (0021–0034) drafted concurrently with the registry.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Keep architectural intent in scattered design docs | What we had through April 2026. Scattered intent fades; long arcs get re-discovered. |
| Use issues alone as the planning surface | Issues are work units; they fragment the architectural arc. A row for "CCL institutional process language" is one architectural concern; the issues that fall out of it are many. |
| Make the registry an ADR rather than a separate artifact | The registry is meta to ADRs. Making it an ADR would mean every row update is an ADR amendment — too heavy for a coordination artifact. |
| Embed the registry in the roadmap document only | Machine-readable separation lets future tooling (a "next tranche" suggestor) consume it. The roadmap is the human view; the YAML is the machine view. |
