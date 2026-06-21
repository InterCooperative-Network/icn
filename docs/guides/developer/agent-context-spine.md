# Agent Context Spine (v0)

The **Agent Context Spine** is a generated, repo-owned orientation map that lets an ICN agent answer,
from evidence rather than guesswork:

> Where am I? What subsystem owns this? What docs describe it? What tests or scripts verify it? What
> routes or public surfaces could this affect? What invariants apply? What truth source is canonical?
> What skill or agent should I invoke?

It is a small node/edge graph committed at:

```
docs/reference/project-index/generated/agent-context-spine.json
```

## What it is — and is not

- **Generated, not hand-written.** Produced mechanically by
  `scripts/generate-agent-context-spine.py`. Do not hand-edit the JSON; rerun the generator.
- **Non-canonical.** The artifact carries `"canonical": false`. It is an *orientation* layer that
  **bridges** ICN's existing truth systems — it does not replace them. When the spine and a canonical
  source disagree, the canonical source wins. Precedence lives in
  [`source-of-truth-map.md`](../../reference/project-index/source-of-truth-map.md).
- **Evidence-grounded.** Every node and every edge carries an `evidence` pointer to a path that
  exists on disk. The validator rejects any node/edge whose evidence path is missing. If a
  relationship cannot be evidenced, it is omitted, not guessed.
- **Not a readiness or liveness signal.** Structure is not runtime liveness. The spine asserts **no**
  production readiness, **no** live federation, and **no** pilot status. Claim-sensitive surfaces are
  modeled as `claim_surface` nodes that point you toward the `truth-sync` / `docs-truth-auditor`
  workflows — they never assert a status.

## Which truth systems it bridges

| Existing system | What it owns | How the spine references it |
|---|---|---|
| `ops/state/truth/*.json` | Fact ownership (repo topology, agents, skills, invariants, …) | `truth_source` nodes mirror each domain; `canonical_source` edges point to them. Never mutated by the spine. |
| `icn/Cargo.toml` `[workspace].members` | The authoritative crate list | `crate` nodes (membership + path only; no module-graph parse) |
| `docs/reference/project-index/generated/` | Mechanical artifacts (route inventory, file record) | `generated_artifact` and `route_surface` nodes point at them; v0 does **not** re-derive routes |
| `AGENTS.md` | The five ICN invariants | `invariant` nodes |
| The `icn-agent-pack` plugin | Skills and agents | `skill` / `agent` nodes (globbed from the plugin) |

## Node and edge model

Node types: `crate`, `subsystem`, `doc`, `route_surface`, `skill`, `agent`, `mcp_tool`, `script`,
`invariant`, `claim_surface`, `truth_source`, `generated_artifact`, `path_guidance` (the path-prefix
rules that back the code-quality brief).

Edge types: `owned_by_subsystem`, `documents`, `verifies`, `generates`, `exposes`, `requires_skill`,
`canonical_source`, `touches_claim_surface`, `depends_on`.

Each node has `id`, `type`, `name`, `source_of_truth`, and a non-empty `evidence` list. Each edge has
`from`, `to`, `type`, and `evidence.source`. Some metadata (the curated subsystem seed, the
crate→subsystem map) is honestly hand-seeded; its evidence points at `CLAUDE.md`, and the
`status` field marks it `curated-seed`.

## Regenerate and validate

```bash
python3 scripts/generate-agent-context-spine.py --write   # regenerate the artifact
python3 scripts/generate-agent-context-spine.py --check    # exit 1 if the committed artifact is stale
python3 scripts/check-agent-context-spine.py               # validate integrity, evidence, truth discipline
```

The generator is Python standard-library only and follows the same conventions as
`docs/scripts/route_inventory.py` (deterministic output, a generated timestamp + source commit, and a
`--check` mode that normalizes volatile fields before comparison). The validator additionally checks:
unique node ids, resolvable edge endpoints, on-disk evidence/paths, that every `claim_surface` carries
`asserts_readiness: false`, and that no overclaim language ("production ready", "live federation",
"formal pilot", "partner deployment", …) appears anywhere in the artifact.

> CI wiring for these checks is intentionally **not** part of v0 — it is a follow-up PR.

## Path briefs — the immediate use case (path → code-quality brief)

The highest-value query is the **changed-path brief**: given the files a change touches, get an
actionable first-pass checklist *before* editing or reviewing. The brief is derived from
`path_guidance` nodes (path-prefix rules) layered with crate→subsystem / crate→claim-surface graph
edges, then deduplicated.

```bash
# brief for the files in a diff
git diff --name-only origin/main...HEAD | xargs python3 scripts/generate-agent-context-spine.py --brief
# or explicit paths
python3 scripts/generate-agent-context-spine.py --brief ops/mcp/src/tools/agent-ops.ts docs/INDEX.md
```

Worked example — changed:

```
- ops/mcp/src/tools/agent-ops.ts
- docs/guides/developer/agent-context-spine.md
```

the brief returns (abridged):

- **areas:** `icn-ops MCP server (TypeScript)`, `Documentation (truth / claim discipline)`
- **review_focus:** read-only/no-mutation, clear failure modes, ICN root/path portability; truth-source
  alignment, no production/live/pilot overclaims
- **verification_commands:** `npm --prefix ./ops/mcp run build`, `npm --prefix ./ops/mcp test`,
  `python3 scripts/check-mcp-portability.py`, `python3 docs/scripts/doc_control_check.py`
- **claim_surfaces:** `claim_surface:public-website-claims`, `…:production-readiness`, …
- **recommended_skills:** `skill:doctor`, `skill:navigator`, `skill:truth-sync`
- **recommended_agents:** `agent:icn-ops`, `agent:icn-code-reviewer`, `agent:icn-docs-truth-auditor`

An unknown path returns a clear `note: "no direct guidance match"` with a navigator fallback rather
than nothing. The brief is **advisory orientation, not a gate** — confirm against the source before
asserting anything.

## Read it from MCP

The `icn-ops` MCP server exposes the spine read-only:

- **Tool:** `icn_ops_agent_context_spine` — `paths=[...]` returns the per-path + combined **code-quality
  brief** (takes precedence); no arguments returns a summary (counts by node/edge type); `node=<id>`
  returns one node plus its incident edges (or a contains-match list); `type=`, `subsystem=`, and
  `path=` filter the node list. It never executes commands or mutates files, and fails clearly if the
  artifact is missing or malformed.
- **Resource:** `icn-ops://project-index/agent-context-spine` — the raw artifact JSON.

## Scope and follow-ups

v0 is deliberately small and reviewable. It does **not** parse the Rust module graph, does **not**
create per-route nodes, and does **not** wire CI. Planned follow-ups (CI gate, route/API/doc drift
edges, crate/module-graph enrichment, Navigator consuming the spine directly) are tracked in
[`agent-context-spine-plan.md`](agent-context-spine-plan.md).

## Related

- [`agent-context-spine-plan.md`](agent-context-spine-plan.md) — design, gaps, and PR sequence
- [`claude-code-plugin.md`](claude-code-plugin.md) — the `icn-agent-pack` plugin (the Navigator consumes this spine)
- [`agent-mcp-tooling.md`](agent-mcp-tooling.md) — the `icn-ops` MCP diagnostics layer
- [`source-of-truth-map.md`](../../reference/project-index/source-of-truth-map.md) — canonical precedence (the spine is `Canonical: no`)
