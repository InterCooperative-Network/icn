---
Status: normative
Authority: Codex adapter process
Canonical: no
Last verified: 2026-08-19
---

# Codex Workflow for ICN

Codex adapter for the provider-neutral ICN agent workflow.

Read `AGENTS.md` first. The workflow architecture is `docs/ai/WORKFLOW_ARCHITECTURE.md`. This file describes Codex-specific usage only and does not own project truth.

## Start

For non-trivial work:

1. verify the checkout and dirty state;
2. read `ops/state/truth/sources.json` and resolve the domain owner(s) needed for the task;
3. query relevant GitHub state live;
4. use the Agent Context Spine path brief when useful;
5. record the compact session frame from `docs/ai/ICN_SESSION_FRAME_TEMPLATE.md`;
6. verify the claimed gap before mutation.

Do not load the latest handoff or a universal project-state narrative by default. Use a handoff only when resuming history that is relevant to the task, and reverify its volatile claims.

## Tool use

Prefer repository-native and structured tools:

- Git for checkout/history/diffs;
- GitHub/`gh` for current PR, issue, review, and CI state;
- `rg`/repository search for implementation discovery;
- scripts in this repository for generated truth/orientation;
- Cargo/SDK/tooling only for affected paths.

Do not hardcode machine paths, toolchain versions, current branches, active issues, or required check sets in Codex prompts.

## Skills and routing

Canonical skill locations are registered in `ops/state/truth/skills.json`. Agent routing is registered in `ops/state/truth/agents.json`.

If `.codex/skills/` or a Codex-specific adapter exists, treat it as a provider surface unless the registry explicitly names it canonical. Do not create a second editable copy of ICN semantics inside Codex configuration.

## Planning and execution

- State the requested outcome and stop boundary.
- Read only the registered owners and path context relevant to that outcome.
- Prefer one bounded semantic change per PR.
- Inspect exact CI failures before changing code.
- Preserve the five invariants in `AGENTS.md`.
- If implementation appears to require changing a settled contract, stop and report the contract conflict.

## Verification

Verification is derived from changed paths, relevant owners, the Agent Context Spine, and live merge policy.

Do not keep a permanent command matrix here. In particular, do not assume every Rust task requires whole-workspace clippy/test/build or that a docs-only task can never affect generated/public surfaces.

Before a merge-readiness claim, read `ops/state/truth/policy.json` and live branch protection/check state.

## Shipping boundary

Committing/pushing a branch, opening/updating a PR, merging, deploying, releasing, and migrating are distinct actions. Authorization for one does not silently authorize the others.

A final report should distinguish analysis/test/library-only implementation/integration/migration/deployment and name unresolved external failures or assumptions.

## Conflict rule

If this file or any Codex-specific prompt conflicts with `AGENTS.md`, a registered domain owner, current implementation evidence, or live Git/GitHub state, the Codex adapter is the stale layer. Repair it rather than teaching the repository the adapter's old belief.
