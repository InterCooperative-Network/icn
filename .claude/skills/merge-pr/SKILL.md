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
    - "gh api repos/InterCooperative-Network/icn/branches/$BASE/protection --jq '.required_status_checks.contexts'"
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

The procedure has **two mutually exclusive paths**, and which one you are on is decided *before*
any pending-check handling, not discovered part-way through:

```
resolve PR / head / base  ->  load policy + readiness evidence
                                        |
                    is ADMIN escalation explicitly authorized?
                          |                              |
                        yes                              no
                          v                              v
              step 4: evaluate the COMPLETE      step 5: ordinary path
              admin gate, then merge or REFUSE     - all required green -> merge
              (no fallback to step 5)              - protected pending  -> auto, STOP
                                                   - policy-only pending -> wait, STOP
```

Until icn#2656 round 6 the admin branch sat *after* the pending handler. Since the admin
exception requires a pending required check, step 3 consumed every qualifying state before the
bypass was ever evaluated, and the `--admin` command was unreachable in exactly the situation it
existed for.

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
   gh pr view <N> --json number,title,headRefName,baseRefName,headRefOid,state,isDraft,mergeable,mergeStateStatus
   ```

   Keep two values from this snapshot: `HEAD_OID=<headRefOid>` and `BASE=<baseRefName>`. Every
   later command is pinned to them — the checks you inspect and the commit you merge must be the
   same commit, and the protection you check must be the branch you are actually merging into.

   With no number in `$ARGUMENTS`, resolve the current branch's PR once and use its number from
   then on. Note `mergeable` and `mergeStateStatus` are **different fields with different
   enums** — `MERGEABLE` is a value of the former only.

3. Gather the readiness evidence. Both paths need it, so it is collected once, before the branch:

   - `gh pr checks <N>`, cross-referenced against `.merge.required_checks`, and confirm the live
     set with
     `gh api "repos/InterCooperative-Network/icn/branches/${BASE}/protection" --jq '.required_status_checks.contexts'`
     — `${BASE}`, not a hardcoded `main`: a stacked PR targets another branch, whose required
     set can differ, and merging with `--admin` against the wrong protection would bypass a
     check unique to the real base.
   - A red or pending check that is not required does not block. When
     `.merge.unstable_is_mergeable` is true, `mergeStateStatus: UNSTABLE` is mergeable provided
     every required check is green.
   - Classify each required check as green, pending, or failing. Use `bucket`, which normalises
     every pending spelling — queued, in-progress, waiting, requested, expected — into one
     value. Matching a single state string instead misses the rest, the same
     incomplete-enumeration fail-open that `.requires.required_check_pending_allowlist` exists
     to prevent.

     ```bash
     PENDING=$(gh pr checks <N> --json name,bucket --jq '[.[]|select(.bucket=="pending")|.name]')
     POLICY=$(jq -r '.merge.required_checks' ops/state/truth/policy.json)
     LIVE=$(gh api "repos/InterCooperative-Network/icn/branches/${BASE}/protection" \
              --jq '.required_status_checks.contexts')
     # (PENDING ∩ POLICY) is the set the paths below reason about
     ```

   **Now choose the path.** Admin escalation is taken only when it was explicitly authorized for
   *this* escalation — see step 4. Otherwise go to step 5. Never arrive at the admin path by
   discovering that a check is stalled.

4. **ADMIN OVERRIDE — only on explicit, escalation-specific authorization.**

   Enter this step **only** if one of the following is true. Authorization to merge is *not*
   authorization to override branch protection, and a generic earlier `"yes"`, `"merge it"` or
   `"go ahead"` is **never** retroactively read as admin authorization:

   1. `$ARGUMENTS` includes `--admin` — the user invoked the escalation themselves; or
   2. the ordinary path was blocked and you obtained a **fresh confirmation** whose wording said
      that this action will use **administrator privileges to override branch protection** for
      **this PR**. Solicited for this escalation, in this invocation, naming this PR number.

   That authorization is scoped to this PR and this invocation. It does not carry to another PR,
   to a later invocation, or to a different head. If neither holds, **there is no admin path**:
   go to step 5.

   Authorization is *necessary and not sufficient*. Check `.merge.admin_bypass` mechanically
   against live state — every field, in this order, stopping at the first that does not hold:

   1. `.merge.admin_bypass.allowed` is `true`. This is the owner's off switch: if it is `false`,
      **there is no admin path at all** and the remaining requirements are not consulted — human
      authorization does not lift it.
   2. `mergeable` equals `.requires.mergeable` (`MERGEABLE`).
   3. `mergeStateStatus` is one of `.requires.merge_state_status_in`.
   4. The gates `--admin` would *also* bypass are independently clear — it overrides **every**
      branch protection, not just the check gate, so these mirror the top-level
      `readiness_definition`: `isDraft` equals `.requires.is_draft`, `reviewDecision` is in
      `.requires.review_decision_allowlist` (`null` is listed there explicitly), and the
      unresolved review-thread count equals `.requires.unresolved_review_threads`. If any cannot
      be loaded, that is missing evidence — do not bypass.
      ```bash
      gh pr view <N> --json isDraft,reviewDecision
      gh api graphql --paginate -f query='query($n:Int!,$endCursor:String){repository(owner:"InterCooperative-Network",name:"icn"){
        pullRequest(number:$n){reviewThreads(first:100,after:$endCursor){
          pageInfo{hasNextPage endCursor} nodes{isResolved}}}}}' -F n=<N>
      ```
      `--paginate` with `$endCursor`/`pageInfo` because a single page caps at 100: an
      unresolved thread on page two is invisible to an unpaginated query, and invisible is not
      resolved. If pagination cannot complete, that is missing evidence — do not bypass.
   5. **Every** required check is either concluded with a value in
      `.requires.required_check_conclusion_allowlist`, or pending with a state in
      `.requires.required_check_pending_allowlist`. These are allowlists: a check in any other
      state — `FAILURE`, `TIMED_OUT`, `CANCELLED`, `ACTION_REQUIRED`, `STALE`,
      `STARTUP_FAILURE`, a legacy `ERROR`, or anything GitHub adds later — blocks the bypass.
   6. At least one required check has been pending longer than
      `.requires.stalled_required_check.min_pending_minutes` with
      `.requires.stalled_required_check.elapsed_seconds` duration.

   Only when authorization holds **and** every item above holds, perform the bypass —
   head-pinned like any other merge:

   ```bash
   gh pr merge <N> --match-head-commit "${HEAD_OID}" --admin --"${STRATEGY}"
   ```

   If **any** requirement fails, **refuse the escalation and stop.** Do not fall back to step 5,
   to auto-merge, or to any weaker implicit bypass: a failed admin gate is a refusal, not a
   downgrade. `.merge.admin_bypass.fail_closed` governs everything else: if a field is absent,
   `UNKNOWN`, or ambiguous, do not bypass. "Branch protection blocked the merge" is not on its
   own a qualifying condition — `BLOCKED` is also what a genuinely failing required check looks
   like. `.merge.admin_bypass.never_for` forbids bypassing that, and no human authorization
   lifts it.

5. **ORDINARY PATH** — no admin escalation. Nothing in this step may produce a command
   containing `--admin`; discovering a stalled check here is a reason to report, never to
   escalate.

   **5a · Every required check green.** Merging is **authorized per PR by a human**, and that
   authorization covers an ordinary merge only. Green required checks are a precondition; they
   are not permission. With that authorization:

   ```bash
   gh pr merge <N> --match-head-commit "${HEAD_OID}" --"${STRATEGY}"
   ```

   `--match-head-commit` on **every** merge invocation. Without it a commit pushed between
   inspection and merge is merged instead, carrying none of the gates you just verified.

   The single exception is the one `.merge.exception` describes; state the reason explicitly
   when you invoke it.

   **5b · A required check is pending and GitHub will wait on it.** `--auto` waits for
   **branch protection's** requirements, not for `.merge.required_checks`, so a policy-required
   check that is not a live context on `${BASE}` would not hold the merge and the PR could land
   without it. Every member of `(PENDING ∩ POLICY)` from step 3 must also appear in `LIVE`. When
   it does:

   ```bash
   gh pr merge <N> --match-head-commit "${HEAD_OID}" \
     $(jq -r '.merge.auto_merge.gh_flags|join(" ")' ops/state/truth/policy.json) --"${STRATEGY}"
   ```

   **Then STOP.** `--auto` *enables* a future merge and returns; it does not merge. The PR is
   still open. Do not run the post-merge steps and do not report a merge. Report the live state:
   auto-merge armed, which checks are outstanding, and that the PR has not merged.

   **5c · A pending policy-required check is missing from `LIVE`.** The two authorities
   disagree, and only the union of both being green is readiness. Do **not** enable auto-merge.
   Report and wait.

6. **Confirm the merge actually happened before doing anything post-merge.** A returned
   `gh pr merge` is not proof — with `--auto` it never was, and a direct merge can still be
   refused:

   ```bash
   gh pr view <N> --json state,mergedAt,mergeCommit
   ```

   Only when `state` is `MERGED` and `mergedAt` is non-null: `git checkout "${BASE}" && git pull`.
   Otherwise report the live state and stop.

## Output

Exactly one of:

- **Merged** — PR #, the merge strategy used, the merge commit SHA (from `mergeCommit`, not
  assumed), and any follow-ups.
- **Auto-merge armed** — PR #, the outstanding required checks, and an explicit statement that
  the PR has **not** merged.
- **Not merged** — PR #, and which gate stopped it.

Never report a merge that `state: MERGED` has not confirmed.
