# navigator — reference

Node/edge taxonomy, boundaries, and future-generator notes for the `navigator` skill.

## Node taxonomy

- **Kernel crates** — domain-agnostic (e.g. `icn-kernel-api`, `icn-core`, `icn-net`, `icn-gossip`,
  `icn-store`, `icn-encoding`). These enforce constraints without understanding meaning.
- **App / domain crates** — domain-specific (e.g. `icn-ledger`, `icn-ccl`, `icn-governance`,
  `icn-trust`, `icn-entity`, `icn-federation`). These translate meaning into constraints.
- **Apps** — `apps/governance`, `apps/ledger`, `apps/membership`, `apps/charter`.
- **Binaries** — `icnd`, `icnctl`, `icn-console`.
- **Concepts** — Meaning Firewall, PolicyOracle, ConstraintSet, entity-authority spine, trust class, CCL.
- **Claims / surfaces** — public claims, runtime surface, route inventory, readiness/show maps.

## Edge taxonomy

- **depends-on** — crate dependency (apps may depend on kernel, never the reverse).
- **meaning-firewall** — the boundary where domain semantics become generic constraints.
- **policy-oracle** — app implements `PolicyOracle`; kernel consumes the decision blindly.
- **claim→source** — a public/docs claim and the file(s) that back it.
- **route→artifact** — a gateway route and its OpenAPI / SDK / inventory descendants.

## Boundaries to respect when mapping

- **Never imply a reverse meaning-firewall edge** (kernel importing a domain crate) — that is a
  violation, not a relationship. If the map would draw one, flag it as a defect.
- **Declared ≠ live.** Map nodes/edges from source and the project-index maps are evidence of
  structure, not of runtime liveness. Keep liveness claims out of the structural map; route them to
  `truth-sync`.

## Evidence sources, in order of preference

1. `icn_ops_repo_map`, `icn_ops_state_index`, `icn_ops_agent_brief`, `icn_ops_verification_plan`
   (the `icn-ops` MCP server) — current, read-mostly.
2. `docs/reference/project-index/*.md` — source-of-truth, runtime-surface, website-truth, show-readiness maps.
3. The crate tree (`icn/crates/`, `icn/apps/`, `icn/bins/`) and `CLAUDE.md` topology — fall back only
   when MCP and project-index are unavailable; say so in the output.

## Future generated artifacts (do not fabricate yet)

A future `repo_knowledge_graph.py` is expected to emit a machine-readable graph under
`docs/reference/project-index/generated/` (e.g. `repo-knowledge-graph.json` and a rendered
`repo-knowledge-graph.md`). Until that generator is committed:

- Produce maps **inline** (markdown node/edge lists + mermaid diagrams).
- Do not write to a generated path or claim a generated file exists.
- When the generator lands, this skill should prefer reading/refreshing its output over re-deriving.

## Suggested mermaid skeleton

```mermaid
graph TD
  subgraph Kernel
    KAPI[icn-kernel-api]
    CORE[icn-core]
  end
  subgraph Apps/Domain
    GOV[icn-governance]
    ENT[icn-entity]
  end
  GOV -- policy-oracle --> KAPI
  ENT -- depends-on --> CORE
  %% never draw Kernel --> Apps/Domain (reverse meaning-firewall = defect)
```
