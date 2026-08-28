# GitHub Copilot Instructions for ICN

A **provider adapter**. It says how Copilot reaches this repository's contract and where its
provider-specific surfaces live. It does not restate the contract, and it owns no project facts.

Everything this file used to carry — project overview, repository structure, architecture
patterns, build commands, testing patterns, current status — had registered owners elsewhere, and
the copies here rotted. A provider surface that answers "what is this project" is a second
handbook, and the second handbook is the one that goes stale silently.

## Start here, every session

1. **[`AGENTS.md`](../AGENTS.md)** — the provider-neutral operating contract: the five ICN
   invariants, scope and mutation discipline, and how verification is chosen from the changed
   paths. Read it before non-trivial work. It outranks this file.
2. **[`ops/state/truth/sources.json`](../ops/state/truth/sources.json)** — the truth-ownership map.
   Resolve the question to its owner before loading any broad document. Load only the owners the
   task needs.
3. **Live state is queried live.** Branch, head, PR, reviews, checks, issues, branch protection.
   Never from this file, a handoff, a cached prompt, or model memory.

If this file ever disagrees with `AGENTS.md` or a registered owner, this file is the stale layer.

## Reviewing and finishing a pull request

The pull-request delivery lifecycle is owned by
**[`ops/state/truth/delivery.json`](../ops/state/truth/delivery.json)**. Load it. It defines the
lifecycle states, the finding dispositions (BLOCKER, FOLLOW_UP, QUESTION, NOT_A_FINDING), the
blocker predicate, the FULL/DELTA review distinction, freeze and refreeze, and which decisions
belong to the maintainer alone.

Three consequences that matter to an automated reviewer:

- **Comprehensive review is bounded.** A normal pull request gets one comprehensive generation.
  A push does not reset it.
- **Severity is advisory.** A P1/P2/critical label is evidence about your confidence. It is not
  authority, and it never breaks a freeze on its own.
- **A frozen pull request stays frozen** unless a finding satisfies every blocker condition.
  Valid observations below that threshold become follow-up work, not a reopened pull request.

Do not define blocker, severity, or freeze semantics here or in any reviewer prompt. That is
enforced by `scripts/check-delivery-lifecycle.py`.

## Provider surfaces

- **Agents**: [`.github/agents/`](agents/) — `@icn-orchestrator` routes multi-subsystem requests.
  `icn-code-reviewer` is bound to the delivery lifecycle above and mirrors its Claude counterpart.
- **Path-specific instructions**: [`.github/instructions/`](instructions/) — Rust, web, SDK, docs.
- **Skills**: canonical paths come from
  [`ops/state/truth/skills.json`](../ops/state/truth/skills.json), not from whichever directory a
  provider happens to load. `ship-pr` finishes a pull request; `merge-pr` is the narrow merge
  primitive.

## Rules that are not negotiable here

1. **Never weaken safety to make something pass.** Not validation, trust gates, signature checks,
   encoding rules, determinism requirements, tests, or invariant checks.
2. **Verify before claiming.** Show the command and its output. "Implemented" is not "integrated",
   "deployed", or "adopted" — `AGENTS.md` §9 owns that distinction.
3. **Scope is a hard boundary.** A failing check is not permission for unrelated cleanup.
4. **Do not silently upgrade toolchains or dependencies.**
5. **Docs must match code.** If you change semantics, update the owner in the same pull request.

## Verification

Choose it from the changed paths, the domain owner, and the repository's merge policy — the
procedure is in `AGENTS.md` §5, which also records the documentation-control entrypoints once, so
they are not duplicated into provider adapters like this one.

Merge requirements come from [`ops/state/truth/policy.json`](../ops/state/truth/policy.json) plus
live branch protection. A fixed list of required checks written into a prompt is stale by
construction.
