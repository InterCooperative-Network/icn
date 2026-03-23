# ADR-0010: Admin Merge Exception Policy for GitHub Runner Pool Saturation

**Date**: 2026-03-23
**Status**: accepted
**Tags**: ci, governance, deployment

## Context

ICN uses GitHub branch protection with four required status checks:
`Build Release`, `Test`, `Clippy`, `Format Check`.

All four run on `ubuntu-latest` (GitHub-hosted runners). The self-hosted `ci-runner`
(VM 446, 10.8.30.46, labels `homelab,k3s`) handles Docker build/deploy
(`docker-build-deploy.yml`) only — it is not in the required-check path.

Required jobs queue at `pending / 0s` when the GitHub-hosted runner pool for the
repository is saturated. The principal cause is `benchmark.yml` lacking a concurrency
group: every Rust-touching commit to a PR queues a new `Compare Against Base` job
(two full Rust builds, ~30-60 min each). Without cancellation, these pile up and
exhaust concurrent runner slots, leaving required CI jobs waiting indefinitely.
The branch protection rule then blocks merges even though the blocking condition is
infrastructure scarcity, not test failure.

This produces a structural mismatch:

- Policy says required checks gate merges.
- Infrastructure says those checks may not run in bounded time.
- Practice: maintainers substitute judgment for policy by using `--admin`.

Doing this silently normalizes admin override and turns branch protection into
ceremonial theater. This ADR names the exception explicitly so it remains a
*conscious exception*, not a habit.

## Decision

`gh pr merge --admin` is **permitted** when ALL of the following conditions hold:

1. **All completed required checks are green.** Any required check that has started
   and finished must have passed. Only checks that are `pending at 0s` (queued, not
   yet assigned a runner) may be bypassed. If any check has completed with failure or
   a timeout result, do not admin-merge — that is evidence, not queue starvation.

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
- The benchmark concurrency fix (2026-03-23) addresses the root cause of pool
  saturation. If admin merges recur frequently after that fix, a different saturation
  source exists and should be identified before normalizing the exception further.
- Admin merges bypass `--strict` up-to-date enforcement. The branch must be rebased
  onto current main before local verification to ensure correctness.
- Any future contributor who sees admin merges in history without knowing this policy
  may incorrectly infer that required checks are optional.

## Alternatives Considered

| Alternative | Why rejected |
|-------------|-------------|
| Wait indefinitely for the stalled runner pool | Correct but operationally untenable for solo/small-team projects |
| Add concurrency group to benchmark.yml | **Correct root-cause fix; applied 2026-03-23.** Cancels stale Compare Against Base jobs when new commits arrive, freeing runner slots. |
| Add a second self-hosted runner | Mitigates `docker-build-deploy.yml` throughput; does NOT help required-check latency (which uses GitHub-hosted runners). |
| Remove required-check jobs | Removes safety signal. Test and Build Release should remain required. |

## Infrastructure Debt Named by This ADR

The root cause of this exception is **`benchmark.yml` lacking a concurrency group**,
which allows stale `Compare Against Base` jobs to pile up and saturate the
GitHub-hosted runner pool. This has been fixed (concurrency group added 2026-03-23).

Remaining items:

1. Monitor whether admin-merge exceptions recur after the benchmark concurrency fix.
   If they do, the runner pool exhaustion has a different cause.
2. Deciding whether `Security Audit` should be promoted to required once the
   inherited CVE backlog is cleared.
3. Adding a second `ci-runner` (VM 447) would benefit `docker-build-deploy.yml`
   (build/deploy parallelism) but does NOT affect required-check latency, which
   runs on GitHub-hosted infrastructure.

These are not Sprint 26 tasks. They are named here so they are visible when
capacity allows.
