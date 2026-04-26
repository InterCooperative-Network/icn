# ADRs moved to `docs/adr/`

ICN's Architecture Decision Records are canonical under [`docs/adr/`](../../../docs/adr/). This directory used to hold them; that doubled as a parallel docs tree and caused source-of-truth drift between `docs/adr/` and `ops/state/decisions/`. The canonical home for human-readable docs is `docs/`. Operational state may *index* docs but must not become a second documentation tree.

## What lives here now

Nothing under `ops/state/decisions/` anymore. This README is a redirect for old links and tooling.

## Where to find ADRs

- Read existing ADRs: [`docs/adr/`](../../../docs/adr/)
- Naming: `ADR-NNNN-kebab-case-title.md`
- Template: [`docs/adr/template.md`](../../../docs/adr/template.md)

## Tooling

The MCP `log_decision`, `search_decisions`, and `get_decision` tools (`ops/mcp/src/tools/decisions.ts`) read and write `docs/adr/`. The SQLite `decision_index` is rebuilt on boot from whatever ADR files exist on disk under that directory.

## Canonical-truth pointer

`ops/state/truth/sources.json` records `adr_decisions.owner = "docs/adr/"`.
