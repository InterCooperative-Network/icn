# Phase 1 checkpoint: reading and contributing

> **Optional learning exercise.** This checkpoint helps you practice the skills in Phase 1. It is **not** a gate for making contributions, opening PRs, reviewing ideas, or joining project discussions. No mentor approval or badge is required.

If your goal is to help right away, use [Getting Started](../../../GETTING_STARTED.md) instead.

## What this checkpoint is for

After [Environment](01-environment.md), [Rust Through ICN's Lens](02-rust-through-icn.md), and [Errors and Tracing](03-errors-and-tracing.md), try to show that you can:

- Find the part of the repository that owns a behavior.
- Run one relevant command and interpret its result.
- Explain one design or runtime boundary without confusing an intended invariant with what current code actually enforces.
- Share an observation or bounded change clearly enough for another contributor to use.

The exercises below are optional. Choose the ones that help you learn.

## Exercise A: map a behavior

Pick one behavior you care about: identity creation, gateway authorization, governance proposals, gossip, or an actual test.

1. Locate its entry point in the current checkout.
2. Trace two or three relevant functions or modules.
3. Sketch the relationships and name each source path.
4. Note which facts you verified in code and which remain design intentions or questions.

**Useful artifact:** a short diagram or paragraph with source links. Don't force every crate into a simplistic kernel/app/domain chart; consult the current ownership map and applicable architectural rules.

## Exercise B: run a focused test

From a clean checkout, follow the [technical quickstart](../../../GETTING_STARTED.md#3-technical-path-get-a-development-environment-working).

1. Record the checkout revision and toolchain pin.
2. Run one scoped test (such as the quickstart's `icn-kernel-api` library test).
3. Write down what the result establishes and what it does **not** establish.
4. If it fails, preserve enough information to reproduce the failure.

**Useful artifact:** the command and outcome, or a clear issue/Discussion describing an unexpected result. You do not have to make an entire workspace green to finish this exercise.

## Exercise C: explain one error or safety rule

Find a `Result` or explicit error in a crate you are reading. Explain:

- What input or state triggers the error?
- Who receives it, and what happens next?
- What would go wrong if the error were silently ignored?

If you want coding practice, reproduce a safe, local error in an isolated lab rather than deliberately breaking the main repository. Do not weaken production validation for an exercise.

**Useful artifact:** a short explanation with file and test references.

## Exercise D: make a small real contribution

Choose one:

- Correct an onboarding step that you actually verified was wrong.
- Improve documentation that overstates what a subsystem currently does.
- Add a focused regression test for a reproduced defect.
- Record a useful cooperative or UX scenario in a Discussion, with clear actors and expected behavior.

**Useful artifact:** a focused PR, a reproducible issue, or a specific Discussion. A PR is not mandatory for this learning checkpoint, and no arbitrary line-count limit defines a good contribution.

For code and documentation changes, read [CONTRIBUTING.md](../../../../CONTRIBUTING.md) and [AGENTS.md](../../../../AGENTS.md), work on a branch, and run the checks relevant to your change. Keep established security and architectural invariants intact.

## Reflect and continue

Can you explain what you tried, what you learned, what evidence supports it, and what you would do next? That's a useful checkpoint.

You can continue to [Phase 2](../phase-2-architecture/05-the-meaning-firewall.md), choose a [hands-on lab](../../labs/), or work on another [first contribution](../../../GETTING_STARTED.md#1-pick-a-first-contribution). There is no required sequence for contributing to ICN.
