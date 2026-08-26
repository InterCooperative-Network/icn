---
name: merge-pr
description: Merge a PR after confirming CI is green. Fast, correct, no ceremony.
argument-hint: "[PR number] [--admin]"
user-invocable: true
allowed-tools: "Bash"
truth_contract:
  canonical_sources:
    - ops/state/truth/policy.json       # required_checks, merge_strategy, admin_bypass rules
  live_load_required:
    - "gh pr view <N> --json mergeable,mergeStateStatus,statusCheckRollup"
    - "gh api repos/InterCooperative-Network/icn/branches/main/protection --jq '.required_status_checks.contexts'"
  examples_only: []
  never_hardcode:
    - required check list (always query live or read policy.json)
    - merge strategy and admin-bypass conditions (policy.json owns both)
    - PR number or branch name
---

Merge one PR. Fast, correct, no ceremony.

`ops/state/truth/policy.json` owns the merge strategy, the required-check set and the
conditions under which `--admin` is permitted. This skill deliberately does not restate any of
them: restating is how the three of them drifted apart in the first place (icn#2651). Read them
at run time, and if this prose and `policy.json` ever disagree, `policy.json` wins.

## Steps

1. **Load the policy first**, before touching the PR:

   ```bash
   jq '.merge | {strategy: .default_strategy, exception,
                 required: .required_checks, non_blocking: .non_blocking_checks,
                 unstable_is_mergeable, admin_bypass, auto_merge}' ops/state/truth/policy.json
   ```

2. Confirm the PR and branch:
   - `gh pr view --json number,title,headRefName,baseRefName,state,mergeable,mergeStateStatus`
   - If `$ARGUMENTS` names a PR number, use that one.

3. Check the **required** checks against the actual head commit — not every check:
   - `gh pr checks <N>`, cross-referenced against `.merge.required_checks`, and confirm the live
     set with
     `gh api repos/InterCooperative-Network/icn/branches/main/protection --jq '.required_status_checks.contexts'`
   - A red or pending check that is not required does not block. When
     `.merge.unstable_is_mergeable` is true, `mergeStateStatus: UNSTABLE` is mergeable provided
     every required check is green.
   - If a required check is still *pending*, prefer the policy's `.merge.auto_merge.command`
     over waiting or polling.

4. Merge with the strategy `policy.json` declares in `.merge.default_strategy`:

   ```bash
   gh pr merge <N> --<default_strategy>
   ```

   The single exception is the one `.merge.exception` describes; state the reason explicitly
   when you invoke it.

5. **`--admin` is not a way past a failing gate.** Use it only when the situation
   `.merge.admin_bypass.condition` describes actually holds, and never for what
   `.merge.admin_bypass.never_for` forbids. "Branch protection blocked the merge" is not on its
   own a qualifying condition — verify *why* it blocked. If a required check has genuinely
   failed, stop and report; do not ask for permission to bypass it.

6. After merge: `git checkout main && git pull`

## Output

Report: merged PR #, the merge strategy used, commit SHA, and any follow-ups needed.
