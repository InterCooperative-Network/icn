---
name: navigator
description: ICN repo navigator / knowledge graph. This skill should be used when the user explicitly invokes "/icn-agent-pack:navigator", or asks to "map the repo", "build/refresh the knowledge graph", "trace this concept to its source", or "show the conceptual map / impact map". Begins the living repository knowledge-graph and conceptual-map workflow, grounded in the icn-ops MCP tools and (future) generated graph artifacts.
disable-model-invocation: true
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
argument-hint: "[concept | crate | subsystem to map]"
---

Begin the **living repository knowledge graph**: a conceptual map of ICN's crates, subsystems, claims, and the source that backs them, plus source-to-claim and impact tracing. Build or refresh a map **only when explicitly invoked** — this skill does not auto-run and does not write generated artifacts unless the user asks for a refresh.

Arguments: `$ARGUMENTS` may name a concept, crate, or subsystem to focus the map (e.g., "trust oracle", "icn-entity", "auth spine"). If empty, produce a top-level orientation map.

## Ground the map in current evidence (MCP first)

Prefer the `icn-ops` MCP tools over re-deriving structure by hand — they are read-mostly and reflect the current repo:

- `icn_ops_agent_context_spine` — the Agent Context Spine. **When you have changed/target paths, run the path brief first** (`icn_ops_agent_context_spine({ paths: [...] })`, or `python3 scripts/generate-agent-context-spine.py --brief <paths>`) before broad repo search: it returns subsystem ownership, invariants, docs, verification commands, claim surfaces, and recommended skills/agents from repo-owned data.
- `icn_ops_repo_map` — crate/app/binary topology and boundaries
- `icn_ops_state_index` — index of declared state / truth documents
- `icn_ops_agent_brief` — current orientation brief for agents
- `icn_ops_verification_plan` — suggested verification steps for a change area

If the MCP server is not connected, fall back to reading `docs/reference/project-index/` maps and the
crate tree directly, and say which evidence source you used.

## What a map produces

1. **Nodes**: crates / apps / subsystems / key concepts (from `icn_ops_repo_map` + the project-index maps).
2. **Edges**: dependencies, kernel/app boundaries, meaning-firewall lines, claim→source links.
3. **Source-to-claim trace**: for a claim or concept, the file(s) that back it — pair with `/icn-agent-pack:truth-sync` for claim safety and `/icn-agent-pack:route-impact` for API surface.
4. **Impact map**: for a proposed change area, the crates/docs/claims likely affected (cross-check with `icn_ops_verification_plan`).

## Refresh discipline

A generated, non-canonical, evidence-grounded **Agent Context Spine** now exists at
`docs/reference/project-index/generated/agent-context-spine.json` (the v0 foundation of this map).
Read it first; it answers "what subsystem/crate/docs/tests/invariants/claims/skills apply here" from
repo-owned data. To refresh or check it (only when the user asks):

```bash
python3 scripts/generate-agent-context-spine.py --write   # regenerate
python3 scripts/generate-agent-context-spine.py --check    # fail if stale
python3 scripts/check-agent-context-spine.py               # validate integrity + evidence
```

You can also read it live via the `icn-ops` MCP tool `icn_ops_agent_context_spine` (summary, or
`node`/`type`/`subsystem`/`path` filters). The spine is v0: it does not yet parse the Rust module
graph or enumerate per-route nodes — for anything it does not cover, produce the map inline (markdown
+ optional mermaid) and never fabricate a generated-file path that does not exist.

## Output

A focused conceptual map for the requested scope: node list, edges (as a mermaid diagram when it
helps), the source-to-claim trace, and — if a change area was given — an impact list. Note which
evidence sources (MCP tools vs. project-index docs) backed the map.

See `reference.md` for the node/edge taxonomy, the boundaries to respect, and the future-generator notes.
