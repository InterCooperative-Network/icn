# ICN Agent Context Spine Plan

This is the design and roadmap for the **Agent Context Spine** — a repo-owned, evidence-grounded
context substrate for ICN agents. v0 is implemented (see
[`agent-context-spine.md`](agent-context-spine.md)); this document records why it exists, the node/edge
model, and the follow-up sequence.

## Purpose

Give an ICN agent a truth-grounded starting map so it can orient from repo-owned artifacts instead of
crawling the tree and guessing. The spine should answer, for a file/crate/subsystem:

```
Where am I?               What subsystem owns this?     What docs describe this?
What tests verify this?   What routes/surfaces could this affect?
What invariants apply?    What truth source is canonical?
What skill/agent helps?   What public claims could this affect?
```

It is an **orientation artifact, not a canonical source of truth.** It bridges existing truth systems;
it does not replace them, and it never asserts production/live/pilot readiness.

## Existing context systems

ICN already has several context/truth surfaces. The spine composes them rather than competing:

- **`ops/state/truth/*.json`** — canonical fact ownership: `sources.json` (which file owns which
  fact), `agents.json`, `skills.json`, `policy.json`.
- **`docs/reference/project-index/`** — ~24 hand-maintained orientation maps + `generated/`
  artifacts (`route-inventory.md`, `icn-file-record.json`), all `Canonical: no`.
- **`docs/scripts/route_inventory.py`** — the model for `--write`/`--check`, generated frontmatter,
  and drift normalization.
- **`icn/Cargo.toml`** — the authoritative crate list (`[workspace].members`).
- **`AGENTS.md`** — the five ICN invariants.
- **The `icn-agent-pack` plugin** — skills/agents, including the `icn-navigator` that pointed toward a
  *future* generated knowledge graph under `docs/reference/project-index/generated/`.

## Current gaps

1. No single artifact maps a path/crate to its subsystem, docs, tests, routes, invariants, claim
   surfaces, and skills.
2. The Navigator's generated substrate was a promise (`repo_knowledge_graph.py`), not a file.
3. The two truth systems (fact ownership vs. mechanical scans) were not bridged into one queryable map.
4. Claim-surface knowledge lived in prose, not as something an agent could mechanically follow.

## Proposed v0 artifact

A single committed JSON file:

```
docs/reference/project-index/generated/agent-context-spine.json
```

with top-level metadata (`schema`, `status: generated`, `canonical: false`, `generated`,
`source_commit`, `generator`, `regenerate`, `check`, `node_types`, `edge_types`, `counts`) plus
`nodes` and `edges`. Deterministic output; `--check` ignores volatile fields.

## Node model

Types: `crate`, `subsystem`, `doc`, `route_surface`, `skill`, `agent`, `mcp_tool`, `script`,
`invariant`, `claim_surface`, `truth_source`, `generated_artifact`.

Each node: `id`, `type`, `name`, `source_of_truth`, non-empty `evidence` (`[{source, kind}]`), plus
optional `path`, `subsystem`, `status`, `description`. Sources:

- **crate** — parsed from `icn/Cargo.toml` members (membership + path only; **no module-graph parse**).
- **subsystem** — curated seed from `CLAUDE.md` core subsystems (honestly marked `curated-seed`).
- **doc** — canonical/context docs (`STATE.md`, `PHASE_PROGRESS.md`, `CLAUDE.md`, `AGENTS.md`,
  `source-of-truth-map`, `show-readiness-map`, `proof-level-taxonomy`).
- **route_surface** — a single pointer to `generated/route-inventory.md` (no per-route nodes in v0).
- **skill/agent** — globbed from the plugin's `skills/` and `agents/`.
- **mcp_tool** — the eight `icn-ops` diagnostics.
- **script** — the generator, the validator, the plugin checks, `route_inventory.py`,
  `generate_repo_record.py`.
- **invariant** — the five `AGENTS.md` invariants.
- **claim_surface** — `production-readiness`, `live-federation`, `pilot-readiness`,
  `public-website-claims`, `route-api-docs-drift` (each carries `asserts_readiness: false`).
- **truth_source** — one per `ops/state/truth/sources.json` domain (read-only; never mutated).
- **generated_artifact** — the spine itself, the route inventory, the file record.

## Edge model

Types: `owned_by_subsystem`, `documents`, `verifies`, `generates`, `exposes`, `requires_skill`,
`canonical_source`, `touches_claim_surface`, `depends_on`. Every edge carries `evidence.source`. Edges
are only emitted when evidenced; uncertain edges are omitted. The one coarse `depends_on` layer is a
manifest-only check (does a crate's `Cargo.toml` declare `icn-kernel-api`?), not a module-graph parse.

## Generator approach

`scripts/generate-agent-context-spine.py`, Python standard-library only, mirroring
`docs/scripts/route_inventory.py`: `--write` / `--check`, git `HEAD` as `source_commit`, deterministic
ordering, and volatile-field normalization for drift detection. Skill/agent descriptions are read from
**frontmatter only**, so editing a skill's body never drifts the artifact.

## MCP exposure

`ops/mcp/src/diagnostics/agent-context-spine.ts` provides a read-only view; registered in
`ops/mcp/src/tools/agent-ops.ts` as `icn_ops_agent_context_spine` (summary by default;
`node`/`type`/`subsystem`/`path` filters) and exposed as the passive resource
`icn-ops://project-index/agent-context-spine`. It only reads the artifact; it never regenerates,
mutates, or executes. Root `.mcp.json` / `.cursor/mcp.json` are untouched.

## Plugin integration

Minimal: the `navigator` skill, its `reference.md`, and the `icn-navigator` agent are updated to point
at the real artifact and its commands (body only — frontmatter unchanged). No other skill/agent is
broadly rewritten.

## Validation

`scripts/check-agent-context-spine.py` (stdlib only) verifies: artifact exists and parses; required
top-level fields; per-node `id`/`type`/`name`/`source_of_truth`/non-empty `evidence`; unique ids;
per-edge `from`/`to`/`type`/`evidence.source`; edge endpoints resolve to known nodes; node `path` and
every `evidence.source` exist on disk; every `claim_surface` carries `asserts_readiness: false`; no
overclaim language anywhere; and (via the generator) no drift.

## Security and truth-discipline constraints

- The spine is `Canonical: no`. It defers to `source-of-truth-map.md`.
- Every node/edge must carry evidence that exists on disk.
- It must not assert production/live/pilot readiness; claim-sensitive surfaces route to
  `truth-sync` / `docs-truth-auditor`.
- It never mutates `ops/state/truth/*`, `.claude/`, root `.mcp.json`, or `.cursor/mcp.json`.

## Proposed PR sequence

- **PR A (this PR): Agent Context Spine v0** — generator, validator, artifact, MCP tool + resource,
  docs, Navigator pointers.
- **PR B: CI gate** — wire `check-claude-plugin*.py` and the spine `--check`/validator into CI
  (non-blocking warning first, like the route-inventory check).
- **PR C: Truth-map MCP resources** — expose `sources.json` and project-index maps as MCP resources.
- **PR D: Route/API/doc drift edges** — per-route nodes and route↔OpenAPI↔doc drift edges.
- **PR E: Rust crate/module-graph enrichment** — real `depends_on` from `cargo metadata`.
- **PR F: Navigator consumes the spine** — the skill/agent read the graph programmatically.
- **PR G: Project-scoped activation** — optional mirror/activate of the plugin per repo.

## Out of scope for v0

Full Rust module-graph parse; per-route nodes; CI wiring; broad truth-map MCP resources; Navigator
*consuming* the graph beyond a pointer; any readiness/liveness assertion; any change to `.claude/`,
root `.mcp.json`, `.cursor/mcp.json`, or `ops/state/truth/*`.
