# ADR-0016: Admin Merge Exception Policy for GitHub Runner Pool Saturation

**Date**: 2026-03-23
**Status**: accepted
**Tags**: ci, governance, deployment
**Note**: Originally filed as ADR-0010 in `ops/state/decisions/` (collided with `ADR-0010-app-topology` in `docs/adr/`). Renumbered to 0016 when ADRs were canonicalized under `docs/adr/`.

## Context

At acceptance in March 2026, the repository had four required status checks:
`Build Release`, `Test`, `Clippy`, `Format Check`.

**The repository's required-check set may expand independently of this ADR, and has.**
This ADR does not enumerate the canonical current set — `ops/state/truth/policy.json#merge.required_checks`
owns that. The scope of this exception is defined in the Decision below, and is deliberately
narrower than "the required checks".

All four run on `ubuntu-latest` (GitHub-hosted runners). The self-hosted `ci-runner`
(VM 446, operator-supplied host, labels `homelab,k3s`) handles Docker build/deploy
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

### Scope

**The queue-starvation exception is scoped only to required checks for which this ADR explicitly
defines equivalent local verification.** Currently those are:

- `Format Check`
- `Clippy`
- `Test`
- `Build Release`

A different required check that is pending — including a future required check added after this
ADR — is **NOT** bypassable under this exception, unless this ADR is explicitly amended to define
equivalent verification for it. If such a check is what is blocking the merge, wait or fix.

Scope stated explicitly 2026-08-27 (icn#2651/#2658 review). The canonical set had grown from four
to eleven while condition 3 below still named only the four `cargo` commands, so a maintainer
following this ADR literally could have admin-merged past a stalled `Meaning Firewall Check`,
`TypeScript SDK` or `Accessibility Tests` having verified none of them. The exception was always
about the checks it knows how to substitute for; that is now written down rather than inferred.

### Conditions

`gh pr merge --admin` is **permitted** when ALL of the following conditions hold:

0. **The stalled check is in scope.** Every required check being bypassed is one of the four named
   under Scope. A stalled required check outside that list ends the exception — no local
   verification defined here substitutes for it.

1. **All completed required checks are green.** Any required check that has started
   and finished must have passed. Only checks that are `pending at 0s` (queued, not
   yet assigned a runner) may be bypassed. If any check has completed with failure or
   a timeout result, do not admin-merge — that is evidence, not queue starvation.

2. **Blocking condition is queue starvation, not failure.** The required jobs must
   be `pending` at 0s duration, not `failure` or `timed out`. A job that has started
   and failed is not starvation — it is evidence. Do not admin-merge past evidence.

3. **Local verification matches the in-scope required checks.** These four commands are the
   equivalence this ADR defines; they are the reason the Scope list is what it is.
   - `cargo fmt --check` (mirrors Format Check)
   - `cargo clippy -p <changed-crates> -- -D warnings` (mirrors Clippy)
   - `cargo test -p <changed-crates>` or equivalent integration tests (mirrors Test)
   - `cargo check --workspace` or `cargo build` for structural soundness (mirrors Build Release)

4. **Non-required check failures are pre-existing.** Any non-required check that
   fails (e.g., Security Audit with inherited CVEs, benchmark regression flags) must
   be verifiably pre-existing on `main`, not introduced by the branch.

5. **The merge is documented.** The commit/PR description must note: "admin merge —
   required runner jobs queue-stalled; local verification complete."

When all conditions hold, admin merge is a legitimate operational decision, not a policy bypass.
When any condition is absent — including condition 0, the scope check — wait or fix.

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
