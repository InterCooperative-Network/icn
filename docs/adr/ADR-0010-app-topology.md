# ADR-0010: App topology and canonical app roots

- Status: Superseded
- Superseded by: ADR-0017
- Date: 2026-02-09
- Decision drivers:
  - Reduce contributor confusion
  - Preserve kernel/app separation invariant
  - Make app ownership and migration paths explicit

> **Update (2026-04-26):** this ADR opened the question of canonical
> app roots but never closed it (it was filed `Proposed` and not
> ratified). [ADR-0017](ADR-0017-monorepo-consolidation-with-explicit-internal-boundaries.md)
> answers the question:
>
> - `icn/apps/` is the **runtime-wired** location and the home for
>   any new app crate that participates in `icnd`.
> - `apps/` (top-level) holds **prototypes and external
>   demonstrators** that are not part of the Rust workspace.
> - Neither root may accumulate institution-specific vocabulary.
>
> The ambiguity this ADR documented is real and is now resolved by
> ADR-0017's canonical-roots table. Body preserved as the historical
> motivation; current rule lives in ADR-0017.

## Context

The repo currently contains two app roots:

- `apps/` (top-level): echo, governance, ledger, trust, etc.
- `icn/apps/` (workspace): governance, membership, etc.

`icnd` wiring references top-level app crates in places, while the Cargo workspace includes `icn/apps/*`.

This is workable but creates ambiguity:

- Which root is canonical?
- Which layer do these represent?
- Where should new app services live?

Ambiguity increases architectural drift and erodes boundary hardening goals.

## Decision

Canonical rule:

- `icn/apps/` is the canonical location for app crates that integrate with the daemon runtime.
- `apps/` is deprecated as a runtime app root and will be migrated into `icn/apps/` or reclassified as external examples/tools.

Allowed meanings going forward:

- `icn/apps/*`: app-layer services that bind policy semantics and implement domain behavior, injected into the kernel runtime by the supervisor/daemon.
- `apps/*`: one of:
  1. examples/demos/experimental tools not wired into the canonical runtime
  2. temporary migration targets marked with a migration ticket and removal date

## Consequences

### Positive

- Contributors have one clear place to add app crates.
- Boundary hardening becomes enforceable (fewer ghost layers).
- `icnd` wiring can be made consistent.

### Negative / costs

- Requires migration effort for existing top-level app crates.
- Short-term churn: path updates, docs updates, workspace membership review.

## Migration plan

1. Freeze: no new runtime-wired app crates in `apps/` after 2026-02-09.
2. Inventory: for each `apps/*`, classify as:
   - migrate to `icn/apps/*`
   - reclassify as example/tool
   - delete
3. Move: migrate one crate per PR (or small set), updating:
   - Cargo workspace members
   - `icnd` wiring imports
   - docs references
4. Enforce: add a CI check that fails if runtime wiring references `apps/*` after a grace period.

## Definition of done

- This ADR merged.
- `AGENTS.md` and architecture docs reflect the rule.
- A tracking issue exists with per-crate migration tasks and deadlines.
