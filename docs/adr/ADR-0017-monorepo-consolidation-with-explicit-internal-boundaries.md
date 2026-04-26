---
id: "0017"
title: "Monorepo Consolidation with Explicit Internal Boundaries"
status: "accepted"
date: "2026-04-26"
deciders: ["Matt Faherty"]
tags: ["topology", "monorepo", "orchestration", "architecture", "boundaries", "kernel-app-separation"]
supersedes: ["ADR-0001", "ADR-0010"]
superseded_by: []
amends: ["ADR-0002"]
implementation_status: "implemented (physical layout in main ICN repo)"
references:
  - "ADR-0001 (Orchestration Plane Architecture — superseded by this ADR)"
  - "ADR-0002 (MCP Server Registration — amended by this ADR)"
  - "ADR-0010 (App topology — superseded by this ADR)"
  - "ADR-0011 (Canonical Truth Ownership)"
  - "docs/architecture/KERNEL_APP_SEPARATION.md"
  - "docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md"
---

# ADR 0017: Monorepo Consolidation with Explicit Internal Boundaries

## Status

**Accepted (2026-04-26).** This ADR records the topology that already exists in `main` and replaces the multi-repo assumptions in ADR-0001 and the dual-app-roots ambiguity in ADR-0010 with a single canonical layout.

This ADR makes no behavior changes to running code. It is a topology statement: where work belongs, and what the boundary lines are.

## Context

Earlier ADRs assumed a multi-repo topology:

- `icn/` — substrate runtime workspace.
- `icn-website/` — public website.
- `icn-ops/` — orchestration plane (MCP, automation, ops state).

That topology was not retained. The website source, MCP/orchestration tooling, deploy assets, docs, and ADRs all moved into the main ICN repo. ADR-0001 still describes the multi-repo arrangement; ADR-0010 still describes "two app roots" without resolving which is canonical.

The risk has shifted:

- **Old risk** (multi-repo era): cross-repo drift. Dependencies, conventions, and decisions diverged between `icn-ops/`, `icn-website/`, and `icn/` because nothing forced them to converge.
- **New risk** (monorepo era): semantic-layer collapse. Everything sits next to everything else; physical proximity invites the assumption that the boundaries dissolved with the directory walls. They did not.

**The kernel/app boundary, the substrate/app/website boundary, the docs/state boundary, and the institution-package/runtime boundary are unchanged. Only their physical location changed.**

## Decision

ICN's main repo is the canonical physical home for substrate runtime, app surfaces, website/docs, MCP/orchestration tooling, operational state, deploy assets, and any in-tree institution-package scaffolding. Physical consolidation does not collapse architectural boundaries.

### Canonical roots

| Path | Layer | Owns | Must not |
|---|---|---|---|
| `icn/` | substrate / Rust workspace root | `Cargo.toml`, kernel crates (`icn-core`, `icn-kernel-api`, `icn-identity`, `icn-net`, `icn-gossip`, `icn-store`, `icn-encoding`, `icn-time`, `icn-testkit`, `icn-protocol`, `icn-services`, `icn-crypto`, `icn-obs`, `icn-security`, `icn-authz`, `icn-http-kit`, `icn-naming`), domain crates that the kernel depends on (`icn-governance`, `icn-ledger`, `icn-trust`, `icn-ccl`, `icn-federation`, `icn-compute`, `icn-privacy`, `icn-snapshot`, `icn-crypto-pq`, `icn-steward`, `icn-zkp`, `icn-coop`, `icn-community`, `icn-entity`, `icn-api`, `icn-gateway`, `icn-rpc`), bins (`icnd`, `icnctl`, `icn-console`) | hold website assets, MCP tools, deploy YAML, or institution-package fixtures |
| `icn/apps/` | runtime-wired app crates (PolicyOracle implementations, app lifecycle wiring) | `governance`, `ledger`, `membership`, `charter` — the app crates that are part of the workspace and integrate with `icnd` | accumulate domain-only logic that should live in a kernel crate, or accumulate institution-specific vocabulary |
| `apps/` (top-level) | external/standalone app prototypes and demonstrators | `echo`, `governance`, `ledger`, `trust` (demonstrators / standalone surfaces, not part of the Rust workspace) | be conflated with `icn/apps/`. New runtime-wired apps go in `icn/apps/`, not here. This root remains for legacy prototypes and external demonstrators only. |
| `website/` | public/narrative/docs/discovery surface | landing pages, public docs, narrative/strategy content, public roadmap | hold proprietary or pre-publication operational state |
| `ops/mcp/` | MCP/orchestration tooling | TypeScript MCP server, decision/sprint/repo tools, `dist/` build artifact | become a parallel decision-doc tree (see ADR-0018) |
| `ops/state/` | operational machine state | `truth/sources.json`, `sprint/current.json`, `config/repo-map.json`, MCP SQLite (gitignored) | hold human-readable docs (those live under `docs/`) |
| `docs/` | human-readable documentation root | `STATE.md`, `ARCHITECTURE.md`, `architecture/`, `design/`, `spec/`, `reference/`, `guides/`, `planning/`, `strategy/`, `dev/`, `dev-journal/`, etc. | duplicate state owned by `ops/state/` |
| `docs/adr/` | canonical Architecture Decision Records | `ADR-NNNN-slug.md`, `template.md` | be parallel to any other ADR location (see ADR-0018) |
| `deploy/` | deployment manifests and runbooks | `k8s/`, helm charts, deploy READMEs | hold runtime code |
| `sdk/` | language SDKs | `typescript/`, `react-native/`, generated API types | hold runtime code |
| `web/` | in-repo web UIs | `pilot-ui/` (PWA), `dashboard/` (static) | be conflated with `website/` (which is the public surface, not pilot tooling) |

### Rules

1. **Physical proximity is not a license to collapse layers.** A file inside `icn/` may not import a domain noun from an institution package. A file inside `ops/mcp/` may not be substituted for a doc under `docs/`. The Meaning Firewall, the kernel/app separation, and the institution-package boundary remain enforced exactly as they were across the multi-repo era.

2. **Two app roots are not a bug, but they are different.** `icn/apps/` is the runtime-wired location and the home for new apps that participate in `icnd`. `apps/` (top-level) holds prototypes and external demonstrators that are not part of the Rust workspace. Neither shall accumulate institution-specific vocabulary; that belongs in an institution package (see `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md`).

3. **`ops/state/` is machine state, not documentation.** Anything a human is meant to read first goes under `docs/`. The MCP server and other tooling may *index* docs by reading them, but indexing is not authoring.

4. **`website/` is the public surface; `web/` is the in-repo pilot/dashboard surface.** Do not confuse them.

5. **Cross-cutting tooling lives in `tools/`, `scripts/`, or `ops/`.** Do not park it in `icn/` or `docs/`.

## Consequences

- The "where does this go?" question now has a single canonical answer table. Any ambiguity is a doc bug, not a topology question.
- ADR-0001's separate-repo `icn-ops/` topology no longer matches reality. ADR-0001 stays in the record as the historical decision that motivated the orchestration plane in the first place; this ADR replaces its physical layout. The durable principle from ADR-0001 — that ICN needs an explicit orchestration/state plane — is retained. The implementation form changed; the principle did not.
- ADR-0010's "two app roots" question is answered: both exist, with non-overlapping purposes (workspace-wired vs prototypes/external). New runtime-wired apps go in `icn/apps/`. ADR-0010 is superseded by this ADR.
- ADR-0002's MCP-registration mechanism is unchanged in spirit; the path moved from a separate repo into `ops/mcp/`. ADR-0002 is amended, not superseded — the registration mechanism is still the right one, the address just changed.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Re-split into `icn-ops/`, `icn-website/`, `icn/` | The merge happened for real reasons (cross-cutting work, single-PR review, atomic CI, single source of truth). Re-splitting would re-introduce the drift this ADR's predecessor existed to fight. |
| Leave ADR-0001 / ADR-0010 in place without a successor ADR | Future contributors would read the old ADRs as current truth and re-derive the wrong topology. Recording the supersession explicitly is what makes the ADR record honest. |
| Collapse `apps/` into `icn/apps/` | The two roots have non-overlapping purposes (prototypes vs workspace). Collapsing them now would drag legacy prototype code into the workspace surface, breaking kernel/app separation more, not less. The right move is a clear written rule, not a rename. |
| One mega-ADR that replaces 0001, 0002, 0010, plus authority/bootstrap | Too large to review honestly. This ADR records the topology decision only. Authority/mandate (ADR-0019) and bootstrap activation (ADR-0020) get their own records.|
