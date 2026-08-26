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

**Never shell-eval a value read from `policy.json`.** Every field consumed below is a discrete
value or list substituted into a command *you* write. There is no command string in the policy
file to execute.

## Steps

1. **Load the policy first**, before touching the PR:

   ```bash
   jq '.merge | {strategy: .default_strategy, exception,
                 required: .required_checks, non_blocking: .non_blocking_checks,
                 unstable_is_mergeable, admin_bypass, auto_merge}' ops/state/truth/policy.json
   ```

   Keep `STRATEGY=$(jq -r '.merge.default_strategy' ops/state/truth/policy.json)` to hand — every
   merge command below is built from it.

2. Resolve the PR **explicitly**. If `$ARGUMENTS` names a number, pass it to every `gh` call;
   a bare `gh pr view` resolves the current branch's PR instead, which is how you merge the
   wrong one:

   ```bash
   gh pr view <N> --json number,title,headRefName,baseRefName,state,isDraft,mergeable,mergeStateStatus
   ```

   With no number in `$ARGUMENTS`, resolve the current branch's PR once and use its number from
   then on. Note `mergeable` and `mergeStateStatus` are **different fields with different
   enums** — `MERGEABLE` is a value of the former only.

3. Check the **required** checks against the actual head commit — not every check:
   - `gh pr checks <N>`, cross-referenced against `.merge.required_checks`, and confirm the live
     set with
     `gh api repos/InterCooperative-Network/icn/branches/main/protection --jq '.required_status_checks.contexts'`
   - A red or pending check that is not required does not block. When
     `.merge.unstable_is_mergeable` is true, `mergeStateStatus: UNSTABLE` is mergeable provided
     every required check is green.
   - If a required check is still *pending*, prefer auto-merge over waiting or polling. Compose
     it from the structured fields — the policy carries flags and a strategy pointer, not a
     command to run:
     ```bash
     gh pr merge <N> $(jq -r '.merge.auto_merge.gh_flags|join(" ")' ops/state/truth/policy.json) --"${STRATEGY}"
     ```

4. Merging is **authorized per PR by a human**, not by green checks. Green required checks are a
   precondition; they are not permission. With that authorization, merge using
   `.merge.default_strategy`:

   ```bash
   gh pr merge <N> --"${STRATEGY}"
   ```

   The single exception is the one `.merge.exception` describes; state the reason explicitly
   when you invoke it.

5. **`--admin` is not a way past a failing gate.** It is permitted only for a queue-stalled
   runner. Check `.merge.admin_bypass.requires` mechanically against live state — every field,
   not the summary:
   - `mergeable` equals the required value (`MERGEABLE`);
   - `mergeStateStatus` is one of `merge_state_status_in`;
   - **no** required check has concluded with any value in `no_required_check_concluded`;
   - at least one required check has been pending longer than `min_pending_minutes` with
     `elapsed_seconds` duration.

   `.merge.admin_bypass.fail_closed` governs everything else: if a field is absent, `UNKNOWN`,
   or ambiguous, **do not bypass**. "Branch protection blocked the merge" is not on its own a
   qualifying condition — `BLOCKED` is also what a genuinely failing required check looks like.
   If one has actually failed, stop and report; `.merge.admin_bypass.never_for` forbids
   bypassing it, and no user "yes" lifts that.

6. After merge: `git checkout main && git pull`

## Output

Report: merged PR #, the merge strategy used, commit SHA, and any follow-ups needed.
