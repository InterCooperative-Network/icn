# ADR-0010: Admin Merge Exception Policy for Self-Hosted Runner Starvation

**Date**: 2026-03-23
**Status**: accepted
**Tags**: ci, governance, deployment

## Context

ICN uses GitHub branch protection with four required status checks:
`Build Release`, `Test`, `Clippy`, `Format Check`.

All four run on a single self-hosted CI runner (`ci-runner`, 10.8.30.46). When this
runner is busy with a long-running job (e.g., benchmark comparison, multi-hour test
suites), required jobs queue at `pending / 0s` indefinitely. The branch protection
rule then blocks merges even though the blocking condition is infrastructure scarcity,
not test failure.

This produces a structural mismatch:

- Policy says required checks gate merges.
- Infrastructure says those checks may not run in bounded time.
- Practice: maintainers substitute judgment for policy by using `--admin`.

Doing this silently normalizes admin override and turns branch protection into
ceremonial theater. This ADR names the exception explicitly so it remains a
*conscious exception*, not a habit.

## Decision

`gh pr merge --admin` is **permitted** when ALL of the following conditions hold:

1. **Non-runner required checks are green.** `Clippy` and `Format Check` run on
   GitHub-hosted infrastructure and are not subject to runner starvation. Both must
   pass before an admin merge is considered.

2. **Blocking condition is queue starvation, not failure.** The required jobs must
   be `pending` at 0s duration, not `failure` or `timed out`. A job that has started
   and failed is not starvation — it is evidence. Do not admin-merge past evidence.

3. **Local verification matches required scope.**
   - `cargo fmt --check` (mirrors Format Check)
   - `cargo clippy -p <changed-crates> -- -D warnings` (mirrors Clippy)
   - `cargo test -p <changed-crates>` or equivalent integration tests (mirrors Test)
   - `cargo check --workspace` or `cargo build` for structural soundness (mirrors Build Release)

4. **Non-required check failures are pre-existing.** Any non-required check that
   fails (e.g., Security Audit with inherited CVEs, benchmark regression flags) must
   be verifiably pre-existing on `main`, not introduced by the branch.

5. **The merge is documented.** The commit/PR description must note: "admin merge —
   required runner jobs queue-stalled; local verification complete."

When all five conditions hold, admin merge is a legitimate operational decision,
not a policy bypass. When any condition is absent, wait or fix.

## Consequences

**Easier**: Refactor and documentation PRs are no longer indefinitely blocked by
runner queue depth when local verification is clean.

**Harder / riskier**:
- The single-runner bottleneck is papered over rather than fixed. This ADR should
  not reduce pressure to add a second runner or move required jobs to GitHub-hosted
  runners.
- Admin merges bypass `--strict` up-to-date enforcement. The branch must be rebased
  onto current main before local verification to ensure correctness.
- Any future contributor who sees admin merges in history without knowing this policy
  may incorrectly infer that required checks are optional.

## Alternatives Considered

| Alternative | Why rejected |
|-------------|-------------|
| Wait indefinitely for the stalled runner | Correct but operationally untenable for small solo/team projects with one runner |
| Move all required checks to GitHub-hosted runners | Correct long-term fix; not immediately viable due to build time on free tier |
| Remove runner-dependent jobs from required checks | Removes safety signal; Security Audit and benchmark jobs can be non-required but Test and Build Release should remain required |
| Add a second self-hosted runner | Correct; see infrastructure debt note below |

## Infrastructure Debt Named by This ADR

The root cause of this exception is **one self-hosted runner gating all required
checks**. This should be resolved by:

1. Adding a second `ci-runner` (VM on Hyperion or node-2) to reduce starvation risk.
2. Evaluating whether `Build Release` and `Test` can run on GitHub-hosted runners
   for branches, reserving self-hosted for main-merge jobs only.
3. Deciding whether `Security Audit` should be promoted to required once the
   inherited CVE backlog is cleared.

These are not Sprint 26 tasks. They are named here so they are visible when
capacity allows.
