# ICN Claude Project Rules

Compatibility layer for Claude Code.

The provider-neutral operating contract is `AGENTS.md`. The agent workflow architecture is `docs/ai/WORKFLOW_ARCHITECTURE.md`. Canonical fact ownership is registered in `ops/state/truth/sources.json`.

This file intentionally owns **no protocol semantics, current project state, merge policy, deployment facts, identity model, or subsystem-specific invariants**.

## Claude-specific rules

- Run `/icn-preflight` for non-trivial work when available.
- Resolve specialist routing from `ops/state/truth/agents.json` rather than a duplicated agent list.
- Resolve canonical skill paths from `ops/state/truth/skills.json` before editing a skill.
- Load scoped `.claude/rules/*.md` only when their path/domain is relevant.
- Hooks are implementation enforcement. Follow `.claude/hooks/HOOKS.md` when modifying them.
- Prefer structured `gh --json` queries for live GitHub state.
- Treat handoffs and model memory as historical context, never current authority.

## Conflict rule

If this file or another Claude-facing prompt conflicts with `AGENTS.md`, a registered domain owner, current implementation evidence, or live Git/GitHub state, the Claude-facing prompt is stale. Report and repair the adapter instead of propagating the old assumption.
