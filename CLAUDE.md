# CLAUDE.md

Claude Code adapter for the ICN repository.

**Read [`AGENTS.md`](AGENTS.md) first.** It is the provider-neutral operating contract and owns the five repository agent invariants. This file adds Claude-specific mechanics only. It is deliberately not a second architecture handbook or project-state document.

## Session start

For non-trivial work:

1. Run `/icn-preflight` when the skill is available.
2. Otherwise follow the bootstrap sequence in `AGENTS.md` manually.
3. Resolve fact ownership from `ops/state/truth/sources.json` before loading broad documentation.
4. Query branch/PR/review/CI state live when it matters.
5. Use the Agent Context Spine path brief to narrow the files and checks relevant to the task.

Do not begin from the latest handoff unless the user explicitly asks to resume prior work or the task itself identifies that handoff. Handoffs are memory, not current state.

## Claude tooling model

### Skills

Canonical skill ownership is declared in `ops/state/truth/skills.json`.

Do not infer that `.claude/skills/` is canonical merely because Claude can load it. ICN-level skills are owned where the registry says they are owned; Claude-facing copies are compatibility surfaces and should not become independent sources of truth.

Before editing a skill, resolve its canonical path from the registry.

### Specialist agents

Agent routing is owned by `ops/state/truth/agents.json`.

Specialists are scoped reasoning overlays. They must load domain truth from `ops/state/truth/sources.json` rather than relying on hardcoded architectural summaries in their prompt files. If a specialist prompt conflicts with a registered domain owner, the domain owner wins and the prompt is stale.

### Hooks

Hooks are enforcement/tooling surfaces, not project truth. Follow `.claude/hooks/HOOKS.md` when editing them.

New hooks consume tool input from stdin and parse it explicitly. Do not resurrect deprecated implicit `TOOL_INPUT_*` environment-variable assumptions.

## Repository/root resolution

Never rely on a memorized machine path.

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
RUST_ROOT="${REPO_ROOT}/icn"
```

Repository/worktree topology is owned by `ops/state/config/repo-map.json`. A local path mentioned in an old handoff, prompt, or shell history is not evidence that the checkout is current.

Run Cargo from `${RUST_ROOT}`. Run monorepo scripts, docs tooling, SDK tooling, and `just` recipes from the root or the subdirectory their command declares.

## Claude command discipline

- Prefer structured `gh --json` output over parsing human tables.
- Do not create polling loops when a single live query answers the question.
- Do not use a red CI check as permission to change unrelated code.
- Inspect the exact failing job/log before fixing CI.
- When a task is review-only, do not mutate the tree.
- When the maintainer authorizes a merge, use the merge policy in `ops/state/truth/policy.json` and live branch protection. Do not hardcode a required-check list here.
- Do not upgrade `icn/rust-toolchain.toml` unless explicitly requested.
- If a local incremental Rust cache is demonstrably corrupted, `cargo clean` may be appropriate; do not make it a reflexive first response to ordinary build failures.

## Scope overlays

When a task touches an area with scoped instructions, load the narrowest relevant overlay after `AGENTS.md` and the truth owner. Examples include `.claude/rules/`, path-level `AGENTS.md`/`CLAUDE.md`, and specialist agent definitions.

Scoped instructions may add verification or review concerns. They may not redefine canonical project semantics or live state.

## Writing and documentation

Documentation belongs under `docs/` unless the file is an established root control file such as `README.md`, `CONTRIBUTING.md`, `AGENTS.md`, or this adapter.

Before changing documentation truth:

1. identify the fact domain;
2. resolve its owner in `ops/state/truth/sources.json` and, where applicable, `docs/registry.toml`;
3. edit the owner rather than a downstream projection;
4. regenerate derived artifacts;
5. distinguish synchronization from a genuine semantic/governance change.

Do not update a broad "state" document simply because a session ended. Update the owner of the fact that actually changed.

## Identity warning

Do not use this file for identity semantics. The identity model has changed materially over the life of the repository. Resolve the current identity semantics through `ops/state/truth/sources.json` and the owner it names before reasoning about `Did`, human subjects, institutions, nodes, device principals, continuity, or migration.

A legacy statement such as "a DID is a person" or "human identity is a public key" is not safe to carry forward from old prompts.

## Provider boundary

If this file, a Claude skill, a hook, a specialist prompt, a handoff, or model memory disagrees with:

1. `AGENTS.md` on agent operating invariants;
2. a domain owner named by `ops/state/truth/sources.json` on that domain;
3. current code/tests on what the checkout actually implements; or
4. live Git/GitHub on current execution state;

then the Claude-facing surface is the stale layer. Report the conflict and fix the adapter rather than teaching the repository the adapter's old assumption.
