# ICN learning ladder

The Reader → Fixer → Builder → Maintainer ladder is an **optional way to organize technical learning**, not a system of project ranks, permissions, or prerequisites for contributing.

**If you are new and want to help today, start with [Getting Started](../../GETTING_STARTED.md).** You can make a real contribution before starting any of these exercises.

Actual code ownership, reviews, security-sensitive changes, and merge authority follow the project's live repository policies and governance, **not** completion of a course or possession of a badge.

## Choose a skill to practice

| Learning stage | Useful capability | Example exercise |
| --- | --- | --- |
| **Reader** | Find code, reproduce behavior, explain what you observed | Follow a call path; run a focused test; report a confusing instruction |
| **Fixer** | Identify a bounded defect and preserve system invariants | Reproduce a bug; add a regression test; explain the boundary it protects |
| **Builder** | Design and test interactions between components | Implement a scoped change across an API and its consumers |
| **Maintainer practice** | Review, operate, and explain a subsystem responsibly | Review a PR against its claimed evidence and source-of-truth owners |

These are capabilities, not titles granted by this curriculum. People can learn them in a different order, contribute in other disciplines, or already have relevant experience.

## Reader practice

Start from [Phase 1](../path/phase-1-foundations/01-environment.md).

- Find one subsystem and its real entry point in the current code.
- Run a relevant command and interpret success or failure.
- Trace an error path or safety rule.
- Share a small change, reproduction, or clear documentation observation.

The [Phase 1 checkpoint](../path/phase-1-foundations/gate-1.md) offers optional activities. A PR is useful practice but is not required to demonstrate learning.

## Fixer practice

Use the [Phase 2 Meaning Firewall lesson](../path/phase-2-architecture/05-the-meaning-firewall.md) and [firewall lab](../labs/lab-04-firewall-oracle/).

- Separate the cause of a defect from its symptoms.
- Determine which crate or domain owns the semantics.
- Write a regression test for a bounded fix.
- Explain why the change does not weaken authorization, determinism, or other invariants.

The lesson's idealized diagrams must be checked against current implementation and the owners named by [AGENTS.md](../../../AGENTS.md).

## Builder practice

Use [Phase 3](../path/phase-3-systems/07-identity-and-crypto.md).

- Work through a well-scoped feature or integration problem.
- Define expected behavior, failure cases, and test evidence before changing code.
- Review another person's design or PR and explain which claims are supported and which are not.
- Help make the subsystem understandable to someone who did not build it.

A cross-crate implementation is not required simply to make a useful contribution.

## Maintainer practice

Use [Phase 4](../path/phase-4-ownership/10-federation-and-ops.md).

- Understand a subsystem's source-of-truth owner and operational limits.
- Trace an upgrade, restart, or recovery claim back to evidence.
- Evaluate risks, CI results, review comments, and out-of-scope observations.
- Help someone else complete a bounded contribution.

Completing these exercises does not grant review or merge authority. Deployment, production operations, and architectural changes require appropriate authorization.

## Suggested study plans

The [Foundations](foundations.md) and [Accelerated](accelerated.md) tracks include example multiweek pacing. They are **not** commitments, approvals, or requirements, and may need updating as the code changes.

The [path lessons](../path/) and [labs](../labs/) are teaching resources. Their checkpoints should be treated as practice prompts, not tests of whether someone is permitted to participate.

## Using the ladder with real project work

1. Choose a current issue, verified gap, or concrete use case.
2. Read the [contribution guide](../../../CONTRIBUTING.md) and [AGENTS.md](../../../AGENTS.md).
3. Do one bounded piece of work and verify the claims you make about it.
4. Share the result for review, and decide what to learn next.

A useful review or bug report is a contribution even if it doesn't involve code. [Discussions](https://github.com/InterCooperative-Network/icn/discussions) and [Discord](https://discord.gg/sAEFCnEahn) are valid ways to start.
