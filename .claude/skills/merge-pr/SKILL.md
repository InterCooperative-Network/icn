---
name: merge-pr
description: Merge one PR with an ordinary merge once current evidence says it is ready. No privileged bypass, no auto-merge.
argument-hint: "[PR number]"
user-invocable: true
allowed-tools: "Bash"
truth_contract:
  canonical_sources:
    - ops/state/truth/policy.json       # default_strategy, required_checks, ready_when
  live_load_required:
    - "gh pr view <N> --json mergeable,mergeStateStatus,statusCheckRollup"
    - "gh api repos/InterCooperative-Network/icn/branches/$BASE/protection --jq '.required_status_checks.contexts'"
  examples_only: []
  never_hardcode:
    - required check list (always query live or read policy.json)
    - merge strategy and readiness conditions (policy.json owns both)
    - PR number or branch name
---

Merge one PR with an **ordinary merge**, or report why it cannot be merged. Nothing else.

`ops/state/truth/policy.json` owns the merge strategy, the required-check set and the structured
readiness gate (`.merge.ready_when`). This skill does not restate any of them: restating is how
the three of them drifted apart in the first place (icn#2651). Read them at run time, and if this
prose and `policy.json` ever disagree, `policy.json` wins.

**Never shell-eval a value read from `policy.json`.** Every field consumed below is a discrete
value or list substituted into a command *you* write. There is no command string in the policy
file to execute.

## Authority

This skill performs the merge any collaborator could perform. It asks GitHub to merge; **GitHub**
decides whether branch protection is satisfied. There is no privileged path and no deferred path:

- It never merges with administrator privileges. The `--admin` escalation is not in this
  skill's vocabulary, it is never offered, and the authorization that would unlock one is never
  solicited or inferred.
- It never arms auto-merge. This skill merges now or it stops; it never leaves a merge armed to
  happen later, when none of the evidence it gathered will still be current.
- Uncertainty is a stop condition. Pending, failing, `UNKNOWN`, ambiguous and unloadable
  evidence all end the same way: report exactly what stopped it, and stop.

Why the authority is deliberately this small (icn#2656): a privileged merge overrides **every**
branch protection, not only the gate under discussion. A skill that used one would have to prove
that the entire live protection set is safe to override — including protections it cannot
enumerate, such as a required deployment, which never appears in
`.required_status_checks.contexts` — and prove it *again* at the instant of the merge, because
every gate it checked is mutable in between. Review demonstrated it could do neither. Reducing
the authority discharges both obligations instead of approximating them. Each repair round that
tried to model one more bypass condition was evidence that the abstraction was wrong.

`docs/adr/ADR-0016-admin-merge-exception-policy.md` and `.merge.admin_bypass` still describe when
a **human maintainer** may admin-merge a queue-stalled PR. That remains a human decision under
that ADR. This skill does not execute it, evaluate it, or route to it — including when it
observes exactly the queue-stalled state the ADR is about. Reporting the stall is the whole of
its job there.

## Steps

1. **Load the policy first**, before touching the PR:

   ```bash
   jq '.merge | {strategy: .default_strategy, exception,
                 required: .required_checks, non_blocking: .non_blocking_checks,
                 unstable_is_mergeable, ready_when}' ops/state/truth/policy.json
   STRATEGY=$(jq -r '.merge.default_strategy' ops/state/truth/policy.json)
   ```

   `${STRATEGY}` builds the one merge command below.

   The single documented departure is `.merge.exception`, and it is a **strategy** selection,
   not an authority one — both values are ordinary merges. It applies only when the operator
   states that this PR is the category `.merge.exception.applies_to` names; whether a PR is a
   subtree import is not mechanically derivable, so it is never inferred. When they do, take
   that strategy from the policy as well, and state the reason explicitly at merge time:

   ```bash
   STRATEGY=$(jq -r '.merge.exception.strategy' ops/state/truth/policy.json)
   ```

   Both branches read the strategy from `policy.json`. Neither value is ever typed into a
   command — before icn#2656 this step printed the exception as a sentence while `STRATEGY`
   stayed unconditionally `default_strategy`, so the exempt category was squash-merged anyway
   and the documented exception could not be applied at all.

2. Resolve the PR **explicitly**. If `$ARGUMENTS` names a number, pass it to every `gh` call;
   a bare `gh pr view` resolves the current branch's PR instead, which is how you merge the
   wrong one:

   ```bash
   gh pr view <N> --json number,title,headRefName,baseRefName,headRefOid,state,isDraft,mergeable,mergeStateStatus,reviewDecision
   ```

   Keep `HEAD_OID=<headRefOid>` and `BASE=<baseRefName>`. Every later command is pinned to them —
   the checks you inspect and the commit you merge must be the same commit, and the protection
   you read must belong to the branch you are actually merging into.

   `baseRefName` is the **sole authority** for branch identity. Encode it as one path component
   before it ever appears in an API path — with a real encoder, never a hand-written
   `/` → `%2F` substitution:

   ```bash
   BASE_ENC=$(jq -rn --arg s "${BASE}" '$s|@uri')
   ```

   This matters for a ref whose *literal name contains a percent escape*. `feat%2Fnext` is a
   valid Git ref (`git check-ref-format refs/heads/feat%2Fnext` accepts it) and GitHub decodes
   `%2F` in that path segment, so interpolating `${BASE}` raw would load **`feat/next`'s**
   protection for a PR based on **`feat%2Fnext`**. Encoding keeps the two distinct:
   `feat/next` → `feat%2Fnext`, `feat%2Fnext` → `feat%252Fnext`. A slashed base resolves in that
   path either way — verified against the live API — so the slash-only concern did not
   reproduce; the percent case is a different defect and does.

   Note `mergeable` and `mergeStateStatus` are **different fields with different enums**;
   `MERGEABLE` is a value of the former only. `.merge.admin_bypass.field_note` records the
   distinction and how to re-verify it against the live schema.

3. **Gather the current evidence, once.** Every load below is required. An unsuccessful load is
   missing evidence, not an absent requirement:

   ```bash
   CHECKS=$(gh pr checks <N> --json name,state,bucket)
   POLICY=$(jq -c '.merge.required_checks' ops/state/truth/policy.json)
   LIVE=$(gh api "repos/InterCooperative-Network/icn/branches/${BASE_ENC}/protection" \
            --jq '.required_status_checks.contexts') || LIVE=UNAVAILABLE
   ```

   Branch on that sentinel **before** the table is built, so the unavailable path never reaches
   `--argjson`, and build **one row per required check** on the other side — a check GitHub did
   not report becomes a value rather than a gap:

   ```bash
   if [ "${LIVE}" = "UNAVAILABLE" ]; then
     echo "STOP: base protection unavailable for ${BASE} — report Not merged, do not continue"
   else
     REQUIRED_STATE=$(jq -n --argjson checks "${CHECKS}" --argjson policy "${POLICY}" \
                            --argjson live "${LIVE}" '
       ($policy + $live | unique) as $required
       | INDEX($checks[]; .name) as $seen
       | $required | map({name: .,
                          state:  ($seen[.].state  // "ABSENT"),
                          bucket: ($seen[.].bucket // "absent")})')
   fi
   ```

   `if`/`else`, not `[ ... ] && echo`. A trailing `&&` list makes the **healthy** path the failing
   one: with `LIVE` holding real JSON the test is false, the whole block exits `1`, and an agent
   applying this skill's own fail-closed rule would refuse a merge that was actually ready —
   verified, healthy `1` / unavailable `0`, exactly inverted (icn#2656 review). Both branches of
   the `if` exit `0`; which one ran is carried by the message, as everywhere else here.

   `UNAVAILABLE` is not JSON, so falling through would hand `--argjson` a parse error instead of
   the clean refusal step 4 promises — and an error is a worse way to learn you have no evidence
   than being told you have none. A protected branch that genuinely declares **no** required
   contexts is a different case and is fine: `.required_status_checks.contexts` is `null` there,
   and `null` is the identity for `+` in jq, so the union is just `POLICY`.

   **Filtering for the states you expect loses the ones you did not.** `gh pr checks` sorts every
   check into one of *five* buckets — `pass`, `fail`, `pending`, `skipping`, `cancel` — so
   collecting only the pending and failing names drops the other three, and a required check that
   GitHub **cancelled** would appear in neither list and read as green (icn#2656 review). A check
   absent from the rollup entirely reads the same way. The table above has a row for every member
   of `POLICY ∪ LIVE` and an explicit `ABSENT` for anything unreported, so step 4 compares a
   value in every case. For the report, the outstanding ones are
   `jq -r '[.[]|select(.bucket=="pending")|.name]' <<<"${REQUIRED_STATE}"`.

   `${BASE_ENC}`, not a hardcoded `main`: a stacked PR targets another branch whose required set
   can differ. A 404 (`Branch not protected`), a permissions error and a genuinely empty context
   list are three different facts that all leave `LIVE` looking empty — so a load that did not
   succeed is `UNAVAILABLE` and stops the skill at step 4. **An unsuccessful load is missing
   evidence, not "no requirements".**

   Classify by `bucket`, which normalises every pending spelling — queued, in-progress, waiting,
   requested, expected — into one value. Matching a single state string instead misses the rest.

   Then the review threads, paginated:

   ```bash
   gh api graphql --paginate -f query='query($n:Int!,$endCursor:String){repository(owner:"InterCooperative-Network",name:"icn"){
     pullRequest(number:$n){reviewThreads(first:100,after:$endCursor){
       pageInfo{hasNextPage endCursor} nodes{isResolved}}}}}' -F n=<N>
   ```

   `--paginate` with `$endCursor`/`pageInfo` because a single page caps at 100: an unresolved
   thread on page two is invisible to an unpaginated query, and invisible is not resolved. If
   pagination cannot complete, that is missing evidence — stop.

   Finally, whether the base defers merges to a **merge queue**:

   ```bash
   gh api graphql -f query='query($o:String!,$r:String!,$b:String!,$n:Int!){repository(owner:$o,name:$r){
     mergeQueue(branch:$b){id} pullRequest(number:$n){isInMergeQueue}}}' \
     -f o=InterCooperative-Network -f r=icn -f b="${BASE}" -F n=<N>
   ```

   `${BASE}` unencoded here on purpose: this is a GraphQL argument, not a URL path component, so
   the encoding that step 2 requires for the protection path would corrupt it.

4. **Decide from `.merge.ready_when`, and from nothing else.** Every field must hold against the
   step-3 evidence. Evaluate in order and stop at the first that does not:

   1. `mergeable` equals `.ready_when.mergeable`. `UNKNOWN` means GitHub has not finished
      computing mergeability; that is missing evidence, not a soft yes.
   2. `mergeStateStatus` is in `.ready_when.merge_state_status_in`. `UNSTABLE` is admitted
      because `.merge.unstable_is_mergeable` is true and non-blocking checks do not gate — and
      it cannot smuggle a red required check past this gate, because item 6 proves the required
      set green independently.
   3. `isDraft` equals `.ready_when.is_draft`.
   4. `reviewDecision` is in `.ready_when.review_decision_allowlist` (`null` is listed there
      explicitly, because it is what this repo reports today).
   5. The unresolved review-thread count equals `.ready_when.unresolved_review_threads`.
   6. **Every row** of `REQUIRED_STATE` — the union of `POLICY` and `LIVE`, so neither authority
      can admit a check the other requires — has a `state` in
      `.ready_when.required_check_conclusion_allowlist`. Every other value stops the merge, and
      because the table has a row per required check, that includes `CANCELLED` and the
      synthetic `ABSENT`: a check nobody reported is not a check that passed.
      `LIVE=UNAVAILABLE` fails here by construction — an unproven union is not a proven one.
   7. The base does **not** defer merges: `mergeQueue` is `null` for `${BASE}` and the PR is not
      already `isInMergeQueue`. A bare `gh pr merge` against a merge-queue base *enqueues* the PR
      rather than merging it, which would leave a merge armed to happen later on evidence that is
      no longer current — exactly what dropping `--auto` was meant to prevent (icn#2656 review).
      Enqueuing is a legitimate outcome; it is not one this skill is shaped to own, so it stops
      and says so.

   **Any other state stops the skill.** Report which gate stopped it and what its live value was,
   then stop:

   - a required check still pending, however long it has been pending, and whether or not the
     runner is stalled;
   - a required check in any state the allowlist does not name — `FAILURE`, `TIMED_OUT`,
     `CANCELLED`, `ACTION_REQUIRED`, `STALE`, `STARTUP_FAILURE`, a legacy `ERROR`, or anything
     GitHub adds later; and equally a required check that is `ABSENT`, which GitHub never
     reported at all;
   - a base whose merges are deferred to a merge queue;
   - `mergeable: UNKNOWN`, a `mergeStateStatus` outside the list (`BLOCKED`, `BEHIND`, `DIRTY`,
     `HAS_HOOKS`), a draft PR, `CHANGES_REQUESTED`, an unresolved thread;
   - any evidence in step 3 that could not be loaded.

   There is no weaker route out of this step. Stopping is the complete and correct outcome for
   every one of these, and none of them is a reason to acquire more authority.

5. **Merge.** Only when step 4 held completely, and merging this PR is authorized by a human —
   green checks are a precondition, not permission.

   First confirm the PR is still the one the evidence describes:

   ```bash
   gh pr view <N> --json headRefOid,baseRefName
   ```

   Both must still equal `${HEAD_OID}` and `${BASE}`. If either moved, the evidence is stale:
   **refuse and start over.** `--match-head-commit` pins the head and **nothing pins the base** —
   a retarget leaves the head SHA untouched, so the flag still succeeds while the merge lands on
   a branch whose required-check set and protection were never inspected, and the gate you passed
   was computed for a different base (icn#2656 review). GitHub still enforces the new base's own
   protection, so this is a defect in the skill's *claim* rather than a way past a gate — but
   reporting a merge as validated when it was validated against another branch is exactly the
   false confidence the rest of this procedure exists to prevent.

   ```bash
   gh pr merge <N> --match-head-commit "${HEAD_OID}" --"${STRATEGY}"
   ```

   `--match-head-commit` on the merge invocation. Without it, a commit pushed between inspection
   and merge is merged instead, carrying none of the gates you just verified.

   If GitHub refuses the merge, **that refusal is the answer.** Report it as returned and stop.
   GitHub is the authority on whether protection is met, and it is enforcing a condition this
   skill either could not see or read differently. Do not re-run the command with additional
   flags, do not arm anything, and do not escalate.

6. **Confirm the merge actually happened before doing anything post-merge.** A returned
   `gh pr merge` is not proof; the merge can still have been refused:

   ```bash
   gh pr view <N> --json state,mergedAt,mergeCommit
   ```

   Only when `state` is `MERGED`, `mergedAt` is non-null and `mergeCommit` is present:
   `git checkout "${BASE}" && git pull`. Otherwise report the live state and stop.

## Output

Exactly one of:

- **Merged** — PR #, the merge strategy used, the merge commit SHA (from `mergeCommit`, not
  assumed), and any follow-ups.
- **Not merged** — PR #, which gate stopped it, and that gate's live value.

Never report a merge that a fresh `state: MERGED` has not confirmed.
