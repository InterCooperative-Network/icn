---
Status: descriptive
Canonical: no
Last Reviewed: 2026-08-19
---

# Current Truth Map

There is no single file that owns "everything that is real right now."

This map routes a **current claim** to the source that can actually establish it. Canonical ownership is machine-readable in [`ops/state/truth/sources.json`](../../../ops/state/truth/sources.json). This file is orientation only.

## Route the question, not the whole project

| Question | Current source to inspect |
|---|---|
| What branch/HEAD/dirty state am I on? | Live Git: `git branch --show-current`, `git rev-parse HEAD`, `git status --short` |
| What PRs/issues/checks are live? | Live GitHub API/`gh` for the specific item |
| What is the active sprint/task board? | `ops/state/sprint/current.json` |
| What merge policy applies? | `ops/state/truth/policy.json` plus live branch protection |
| What does the current checkout implement? | Current code, tests, schemas, and reproducible behavior |
| What semantics are allowed for a domain? | Resolve the domain owner through `ops/state/truth/sources.json`, then read that owner |
| What architecture decision was accepted? | Accepted ADR/RFC or the registered domain owner |
| What is the repository/worktree topology? | `ops/state/config/repo-map.json` |
| What is a subsystem's documented readiness assessment? | `docs/status.toml` as a **descriptive assessment**, then recheck its cited evidence and freshness before making a current claim |
| What is live/deployed operationally? | The registered operational/private source for that deployment; never infer liveness from public code/docs alone |
| What happened in a prior session? | Git/PR/issue history and handoffs as historical evidence |

## `STATE.md` and `PHASE_PROGRESS.md`

`docs/STATE.md` and `docs/PHASE_PROGRESS.md` remain useful historical/current-state narratives from an earlier coordination model. They are **not universal truth roots** for fresh agent sessions.

Use them when:

- reconstructing historical project posture;
- understanding the rationale of an older phase model;
- reviewing a claim that explicitly cites one of them.

Do not use them to override a registered domain owner, current implementation evidence, or live Git/GitHub state.

## Current-claim protocol

Before writing "currently," "now," "merged," "implemented," "blocked," "deployed," or "ready":

1. Classify the claim.
2. Resolve its owner through `ops/state/truth/sources.json` when one exists.
3. Query live state when the fact is volatile.
4. Inspect implementation evidence when the claim is about behavior.
5. Check timestamp/freshness when using a descriptive assessment such as `docs/status.toml`.
6. State the evidence level honestly.

## Truth conflicts

A disagreement is not solved by a universal ranking like "code always wins" or "state docs always win."

- Code/tests own **implemented behavior**, not normative intent.
- Registered semantic owners own **allowed/intended meaning**, not proof of implementation.
- Git/GitHub own **volatile execution state**.
- Generated maps own **navigation**, not their inputs.
- Handoffs own **historical memory**, not current state.

If two surfaces make contradictory claims in the same domain, report the conflict and repair the stale layer or ownership map.

## Readiness language

Keep the claim level explicit. Useful classifications include:

- normative/design-only;
- implemented library/type/test primitive;
- integrated into a runtime path;
- exercised in a bounded demo/rehearsal;
- deployed in a named environment with current evidence;
- production-ready/adopted only with evidence specific to that stronger claim.

Do not let one level silently imply the next.

## Agent start

For fresh agent work, use:

1. `AGENTS.md`;
2. `ops/state/truth/sources.json`;
3. the owner(s) relevant to the task;
4. live Git/GitHub where relevant;
5. current code/tests for implementation claims;
6. generated project-index maps only for navigation;
7. handoffs only when historical/resume context is useful.

That sequence replaces the old practice of loading a monolithic state narrative before knowing what question the session is answering.
