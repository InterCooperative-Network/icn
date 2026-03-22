---
name: integrate-pr-stack
description: Full sprint-batch or stacked-PR integration pipeline. Owns merge order, rebases, local gates, and main sync.
argument-hint: "[PR numbers...] [--admin] [--dry-run]"
user-invocable: true
allowed-tools: "Bash, Read"
---

Own the full lifecycle of safely integrating a batch of PRs into `main`. This is not a thin wrapper
around `gh pr merge`. It is the standard integration pipeline for ICN sprint closes.

## The Problem This Solves

The transcript improvised rebase sequencing, rediscovered branch names mid-flight, and had to
manually orchestrate the merge order. This skill makes that the default path, not a recovery.

## Steps

### Phase 1: Resolve and classify

For each PR in `$ARGUMENTS` (or all open PRs if none given):

```bash
# Resolve head branches from GitHub — never trust plan docs
gh pr list --json number,title,headRefName,baseRefName,mergeable,mergeStateStatus \
  --jq '.[] | {pr:.number, title:.title, head:.headRefName, base:.baseRefName, mergeable:.mergeable, state:.mergeStateStatus}'
```

For each PR, fetch required check status:
```bash
gh api repos/InterCooperative-Network/icn/branches/main/protection \
  --jq '.required_status_checks.contexts'
```

Then for each PR:
```bash
gh pr checks <N> --json name,state,conclusion \
  | python3 -c "
import sys, json
required = {'Build Release','Test','Clippy','Format Check'}
checks = json.load(sys.stdin)
req = [(c['name'],c['state'],c['conclusion']) for c in checks if c['name'] in required]
opt = [(c['name'],c['state'],c['conclusion']) for c in checks if c['name'] not in required and c['conclusion']=='failure']
print('REQUIRED:', req)
print('NON-BLOCKING FAILURES:', opt)
"
```

Classify each PR:
- **GREEN**: all required checks pass, `mergeable=MERGEABLE`
- **PENDING**: required checks still running
- **BLOCKED**: a required check failed
- **UNSTABLE**: only non-blocking checks failed — treat as GREEN for merge purposes
- **STACKED**: base ≠ main — must be merged after its base PR

### Phase 2: Determine merge order

1. Identify stacked PRs (base ≠ main) and sort them after their dependencies.
2. Among independent PRs, merge smallest/least-risky first.
3. If `--dry-run`, print the plan and stop.

Suggested ICN ordering heuristic (from sprint patterns):
- Security/obs (isolated, small) → compute (isolated) → ledger (medium) → docs/meta (last)

### Phase 3: Integrate loop

For each PR in merge order:

**a. Pre-merge checks**
```bash
gh pr view <N> --json mergeable,mergeStateStatus \
  --jq '"mergeable=\(.mergeable) state=\(.mergeStateStatus)"'
```
Stop if `mergeable != MERGEABLE`.

**b. Merge**
```bash
# Green required checks → direct merge
gh pr merge <N> --merge          # or --merge --admin if $ARGUMENTS includes --admin

# Pending required checks → auto-merge (prefer --auto over waiting in a loop)
gh pr merge <N> --auto --merge
```

**c. Pull main**
```bash
git checkout main && git pull
```

**d. Rebase remaining PR branches**
For each remaining PR branch:
```bash
git checkout <branch> && git rebase origin/main
```

After rebase, run scoped local verification before force-pushing:
```bash
# Use resolve-rust-targets to find the right scope
cargo clippy -p <affected-packages> --all-targets -- -D warnings
```

Only force-push if verification passes:
```bash
git push --force-with-lease
```

**e. Log progress**: `#<N> merged · rebased [branch1, branch2]`

### Phase 4: Finalize

```bash
# Confirm main state
git checkout main && git log --oneline -5

# Check for main CI fallout
gh run list --branch main --limit 3 --json status,conclusion,name,databaseId \
  --jq '.[] | "\(.databaseId) \(.name) \(.status) \(.conclusion)"'
```

Print batch summary:
```
Merged:   #1389, #1391, #1392, #1390
Rebased:  feat/s22-compute-policy-config (after #1389), feat/s22-obs-attestation-config (after #1391)
Main CI:  in_progress (non-blocking only)
Lessons:  none new
```

## Guardrails

- **Never trust branch names from plan docs.** Always resolve from `gh pr view <N> --json headRefName`.
- **UNSTABLE ≠ blocked.** If required checks are green and `mergeable=MERGEABLE`, merge.
- **After every rebase, run scoped local verification before force-pushing.** The rebase may have
  introduced new base-crate changes that affect the branch's compilation.
- **Never skip `git pull` after each merge.** Subsequent rebases need an updated local main.
- **Stacked PRs must merge in dependency order.** If PR B has base = PR A's branch, merge A first,
  then update B's base to main before merging B.
- **`--admin` is for queue-stalled runners only**, not for bypassing genuine failures.
  Confirm required gates are actually green before using `--admin`.

## ICN-specific notes

- Required gates: `Build Release`, `Test`, `Clippy`, `Format Check` (from branch protection API).
- Non-blocking: `Test Coverage`, `Compare Against Base`, `Benchmarks`, `claude-review`.
- Single self-hosted runner (`ci-runner` at 10.8.30.46) means parallel PRs queue up. A PR
  showing `pending / 0s` for >30 min is likely queue-stalled, not failing.
- `mergeStateStatus=UNSTABLE` with `mergeable=MERGEABLE` means only non-blocking checks failed.
  Safe to merge with `--admin`.
- After merging multiple PRs in sequence, expect CI on main to run the full workspace build.
  That's normal — not a sign of regression.
