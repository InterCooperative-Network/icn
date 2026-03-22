---
name: merge-prs
description: Merge one or more PRs with full stack-integration pipeline. Resolves branches, rebases, gates, pulls main.
argument-hint: "[PR numbers...] [--admin] [--dry-run]"
user-invocable: true
allowed-tools: "Bash"
---

Stack-integration merge pipeline. Not a thin `gh pr merge` wrapper. Owns branch resolution,
merge ordering, post-merge rebases, local verification, and main sync.

For single quick merges when you already know the state is clean, this still works. For sprint
batch closes, use `/integrate-pr-stack` instead (same logic, more verbose output).

## Steps

1. **Resolve PRs to merge**:
   - If `$ARGUMENTS` specifies numbers, use those.
   - Otherwise list open PRs: `gh pr list --json number,title,headRefName,mergeable,mergeStateStatus`
   - If zero open PRs, stop.

2. **For each PR, resolve head branch from GitHub** (never trust plan names):
   ```bash
   gh pr view <N> --json number,title,headRefName,baseRefName,mergeable,mergeStateStatus \
     --jq '{pr:.number, head:.headRefName, base:.baseRefName, mergeable:.mergeable, state:.mergeStateStatus}'
   ```

3. **Classify check state** (required gates only):
   ```bash
   gh pr checks <N> --json name,state,conclusion \
     | python3 -c "
   import sys, json
   required = {'Build Release','Test','Clippy','Format Check'}
   checks = json.load(sys.stdin)
   req_states = {c['name']:c['conclusion'] or c['state'] for c in checks if c['name'] in required}
   all_pass = all(v in ('SUCCESS','success') for v in req_states.values())
   print('required:', req_states, '→', 'GREEN' if all_pass else 'NOT READY')
   " 2>/dev/null || echo "checks unavailable"
   ```

   - `UNSTABLE` + required all pass → treat as GREEN, safe to merge
   - `MERGEABLE` + required pending → use `--auto` or wait

4. **Merge** (in order provided; smallest/safest first for batch):
   ```bash
   gh pr merge <N> --merge              # green required checks
   gh pr merge <N> --merge --admin      # if $ARGUMENTS includes --admin (queue-stalled)
   gh pr merge <N> --auto --merge       # pending required checks
   ```

5. **After each merge**:
   ```bash
   git checkout main && git pull
   ```

6. **Rebase remaining PR branches** after each merge to keep them current:
   ```bash
   gh pr view <remaining-N> --json headRefName --jq '.headRefName'   # resolve branch
   git checkout <branch> && git rebase origin/main
   # Run scoped clippy before force-pushing
   cargo clippy -p <affected-packages> --all-targets -- -D warnings
   git push --force-with-lease
   ```

7. **Final**: confirm main state, print one-line summary per PR.

## Output format

```
#1389 merged · rebased [feat/s22-compute-policy-config, feat/s22-obs-attestation-config]
#1391 merged · rebased [feat/s22-obs-attestation-config]
#1390 merged · rebased []
```

## Rules

- **No polling loops.** Use `--auto` for pending checks; use `--admin` for queue-stalled (not failing).
- **No tabular parsing.** Always `--json` with `jq` or `python3`.
- **UNSTABLE ≠ blocked.** Required checks green + `mergeable=MERGEABLE` → merge.
- **Always resolve head branch from GitHub.** Never use plan-doc branch names directly.
- **After each merge, rebase remaining branches.** Verify locally before force-push.

## Required checks (from branch protection)

`Build Release`, `Test`, `Clippy`, `Format Check`

Non-blocking (don't block merge): `Test Coverage`, `Compare Against Base`, `Benchmarks`, `claude-review`
