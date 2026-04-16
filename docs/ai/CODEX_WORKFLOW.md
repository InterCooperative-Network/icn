---
Status: normative
Authority: process
Canonical: no
Last verified: 2026-04-15
---

# Codex Workflow for ICN

This guide aligns Codex usage with existing ICN agent guidance from `CLAUDE.md` and the
constitutional core (`docs/ai/ICN_CONSTITUTIONAL_CORE.md`).

## Goals

- Keep Codex behavior consistent with ICN invariants and change-routing rules.
- Reuse existing specialization model from `.github/agents/`.
- Provide a repeatable workflow for planning, implementation, verification, and shipping.

## Required Inputs for Every Task

Before coding, load in this order:

1. `docs/ai/ICN_CONSTITUTIONAL_CORE.md` (reasoning foundation)
2. `AGENTS.md` (operating mode and change routing)
3. `CLAUDE.md` (repo architecture and workflows)
4. Latest handoff in `docs/dev/` (execution state)
5. `docs/ai/ICN_SESSION_FRAME_TEMPLATE.md` (fill before non-trivial work)
6. Relevant path instruction from `.github/instructions/` (if applicable)

See `docs/ai/WORKFLOW_ARCHITECTURE.md` for the full four-doc architecture.

## Codex Task Flow

1. Classify scope:
   - Single-scope: one subsystem (for example, only Rust crate changes).
   - Multi-scope: multiple subsystems (for example, Rust + gateway + docs).
2. Plan first:
   - Goal and success criteria.
   - Files/crates to touch.
   - Verification commands by change-routing rules.
3. Execute with small diffs:
   - Preserve invariants: adversarial-by-default, determinism, canonical encodings, no protocol panics, clean layering.
4. Verify:
   - Run only the checks required by touched areas.
   - Never claim success without command output.
5. Ship:
   - Update docs/specs when semantics change.
   - Ensure branch is clean, commit, push, and open/update PR.

## Skill Mapping (Codex)

If `.codex/skills/` exists in the repo, use those skills. Otherwise, fall back to
`.claude/skills/` which provide equivalent functionality for Claude Code sessions.

## MCP Server Configuration

Use `.codex/mcp/servers.example.json` as the baseline template.

Recommended baseline servers:

- `filesystem` for code/docs access in this repository.
- `git` for history and diffs.
- `github` for PR/issue context.
- `ripgrep` for fast indexed search across the workspace.

Copy the template into your local Codex MCP config path and fill in tokens/paths specific to your environment.

## Routing Rules (Practical)

- Rust crates (`icn/crates/**`): run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and targeted tests.
- Gateway API (`icn-gateway`): run `cargo test -p icn-gateway --features sled-storage`; regenerate OpenAPI and TS types if API changed.
- TypeScript SDK (`sdk/typescript/`): `npm ci && npm run build && npm test && npm run lint`.
- React Native SDK (`sdk/react-native/`): `npm test && npm run build`.
- Pilot UI (`web/pilot-ui/`): `npm run test && npm run test:e2e && npm run test:a11y`.
- Docs-only changes: validate links and terminology consistency.

## Non-Negotiables

- Do not weaken security, trust gates, signatures, or encoding requirements to make tests pass.
- Do not add new tooling unless explicitly requested.
- Keep docs in `docs/` (never repo root).
- Prefer multiple focused PRs over large mixed diffs.
