---
name: merge-pr
description: Merge one PR with an ordinary merge once current evidence says it is ready. No privileged bypass, no auto-merge.
argument-hint: "[PR number]"
user-invocable: true
allowed-tools: "Bash"
truth_contract:
  canonical_sources:
    - ops/state/truth/policy.json@$BASE_OID   # default_strategy, required_checks, ready_when,
                                              # read at the PINNED BASE COMMIT, never the worktree
  live_load_required:
    - "gh pr view <N> --json headRefOid,baseRefName,baseRefOid,isDraft,mergeable,mergeStateStatus,reviewDecision"
    - "gh api repos/InterCooperative-Network/icn/contents/ops/state/truth/policy.json?ref=$BASE_OID -H 'Accept: application/vnd.github.raw'"
    - "gh api repos/InterCooperative-Network/icn/branches/$BASE_ENC/protection --jq '.required_status_checks.contexts'"
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

1. Resolve the PR **explicitly**, to exactly one number, before anything else reads it.

   If `$ARGUMENTS` names a number, that is `<N>`. If it does not — the argument is optional —
   resolve the current branch's PR **once**, and use the number it returns for every call after:

   ```bash
   N=$(gh pr view --json number --jq .number)
   ```

   This is the **only** unaddressed `gh pr view` in the procedure, and its only job is to produce
   the number. Every later call names `<N>` explicitly, because a bare `gh pr view` re-resolves
   the current branch each time — and a branch that changes underneath you is how you inspect one
   PR and merge another. Resolve once, then address everything.

   ```bash
   gh pr view <N> --json number,title,headRefName,baseRefName,baseRefOid,headRefOid,state,isDraft,mergeable,mergeStateStatus,reviewDecision
   ```

   Keep three values: `HEAD_OID=<headRefOid>`, `BASE=<baseRefName>` and
   `BASE_OID=<baseRefOid>`. Every later command is pinned to them — the checks you inspect and the
   commit you merge must be the same commit, the protection you read must belong to the branch you
   are actually merging into, and the policy you apply must be the revision already on that branch.

   `baseRefOid` is a `GitObjectID!` on `PullRequest` — confirmed against the live schema, not
   assumed. It is what makes the policy read *pinned*: a branch name is a moving target, so
   `?ref=main` re-resolves to whatever `main` is at the moment of the call, and a policy that can
   change between reading it and merging is not a pinned policy (icn#2656 review).

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

2. **Load the policy from the PR's base**, never from the working tree:

   ```bash
   POLICY_JSON=$(gh api \
     "repos/InterCooperative-Network/icn/contents/ops/state/truth/policy.json?ref=${BASE_OID}" \
     -H "Accept: application/vnd.github.raw") || POLICY_JSON=UNAVAILABLE
   if [ "${POLICY_JSON}" = "UNAVAILABLE" ]; then
     echo "STOP: cannot read merge policy at ${BASE_OID} — report Not merged, do not continue"
   else
     STRATEGY=$(jq -r '.merge.default_strategy' <<<"${POLICY_JSON}")
     jq '.merge | {strategy: .default_strategy, exception,
                   required: .required_checks, non_blocking: .non_blocking_checks,
                   unstable_is_mergeable, ready_when}' <<<"${POLICY_JSON}"
   fi
   ```

   **Which revision of the rules admits this change is not the change's decision.** Reading
   `ops/state/truth/policy.json` from the working tree means reading it from whatever is checked
   out — which, in this repo's per-branch worktree layout, is normally the PR branch itself. A PR
   that edits `policy.json` would then supply the strategy and the readiness gates that admit it,
   and `--match-head-commit` does not help: it pins the remote head, not the local policy
   revision (icn#2656 review). Pinning the read to `${BASE_OID}` makes the rules the ones already
   in force on the branch being merged into. That is also why step 1 comes first: the base has to
   be known before the policy can be pinned to it.

   **A commit, not a branch name.** `?ref=${BASE}` re-resolves on every call, so the policy could
   change between the read and the merge and nothing would notice — pinning to a name is not
   pinning. `${BASE_OID}` is immutable, so the blob this step reads is the blob that was evaluated,
   and step 5 refuses if `baseRefOid` has moved since. Running the skill from `main` is not the fix
   either: that makes the read correct by accident of where you happened to stand, and the next
   invocation from a worktree is wrong again.

   If the base's policy has no `.merge.ready_when`, **stop.** The gate must already be in force on
   the branch you are merging into; a PR that introduces the gate cannot be admitted by it. That
   is not a limitation to work around — it is the same rule, applied to itself.

   `${STRATEGY}` builds the one merge command below.

   The single documented departure is `.merge.exception`, and it is a **strategy** selection,
   not an authority one — both values are ordinary merges. It applies only when the operator
   states that this PR is the category `.merge.exception.applies_to` names; whether a PR is a
   subtree import is not mechanically derivable, so it is never inferred. When they do, take
   that strategy from the policy as well, and state the reason explicitly at merge time:

   ```bash
   STRATEGY=$(jq -r '.merge.exception.strategy' <<<"${POLICY_JSON}")
   ```

   Both branches read the strategy from the base's `policy.json`. Neither value is ever typed
   into a command — before icn#2656 this step printed the exception as a sentence while
   `STRATEGY` stayed unconditionally `default_strategy`, so the exempt category was
   squash-merged anyway and the documented exception could not be applied at all.

3. **Gather the current evidence, once.** Every load below is required. An unsuccessful load is
   missing evidence, not an absent requirement:

   ```bash
   CHECKS=$(gh pr checks <N> --json name,state,bucket)
   POLICY=$(jq -c '.merge.required_checks' <<<"${POLICY_JSON}")
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

   First confirm the PR is still the one the evidence describes, **and that the gates only this
   skill enforces are still green**:

   ```bash
   gh pr view <N> --json headRefOid,baseRefName,baseRefOid,isDraft,mergeable,mergeStateStatus,reviewDecision
   gh api graphql --paginate -f query='query($n:Int!,$endCursor:String){repository(owner:"InterCooperative-Network",name:"icn"){
     pullRequest(number:$n){reviewThreads(first:100,after:$endCursor){
       pageInfo{hasNextPage endCursor} nodes{isResolved}}}}}' -F n=<N>
   CHECKS=$(gh pr checks <N> --json name,state,bucket)
   REQUIRED_STATE=$(jq -n --argjson checks "${CHECKS}" --argjson policy "${POLICY}" \
                          --argjson live "${LIVE}" '
     ($policy + $live | unique) as $required
     | INDEX($checks[]; .name) as $seen
     | $required | map({name: .,
                        state:  ($seen[.].state  // "ABSENT"),
                        bucket: ($seen[.].bucket // "absent")})')
   ```

   All three identity values must still equal `${HEAD_OID}`, `${BASE}` and `${BASE_OID}`. If any
   moved, the evidence is stale: **refuse and start over.** `baseRefOid` is included because the
   base branch advancing — someone else merging into it — changes the policy revision your gate was
   computed from without changing the base's *name*. A retarget and a base that merely moved are
   both staleness, and neither is visible to `--match-head-commit`, which pins only the head. Then **re-evaluate step 4 items 3, 4, 5 and 6**
   against what was just read, and stop on any that no longer holds, exactly as step 4 does.

   **The whole gate is re-evaluated, not a remembered subset of it.** The rule is not "refresh the
   checks" and not "refresh the fields someone listed here" — it is: *every operative field of
   `.merge.ready_when` has an evidence source, and all of them are reloaded and the full gate
   recomputed.* Today that mapping is

   | `ready_when` field | evidence reloaded above |
   |---|---|
   | `mergeable`, `merge_state_status_in` | `gh pr view … mergeable,mergeStateStatus` |
   | `is_draft` | `gh pr view … isDraft` |
   | `review_decision_allowlist` | `gh pr view … reviewDecision` |
   | `unresolved_review_threads` | the paginated `reviewThreads` query |
   | `required_check_conclusion_allowlist` | `CHECKS` → `REQUIRED_STATE` |

   **If a field is added to `.merge.ready_when`, this step must reload its evidence too**; the
   invariant test derives the field list from the policy itself and fails until it does, so the
   table cannot fall behind the gate. An earlier revision refreshed `gh pr checks` and nothing
   else, so a review flipping to `CHANGES_REQUESTED`, a thread reopened, or a PR converted to
   draft after step 3 still merged — this repo reports `required_approving_review_count: 0`, so
   GitHub enforces none of the review gates and they are this skill's rules alone (icn#2656
   review).

   Then require the **full** `.merge.ready_when` gate to hold again on the reloaded values, exactly
   as step 4 evaluated it the first time. Any field that no longer holds, or any evidence that
   cannot be reloaded, refuses — it does not fall through to the merge.

   This **narrows** the window to the merge call itself. It does not eliminate it: GitHub offers no
   atomic evaluate-and-merge, so a gate can still change between this read and the request landing.
   Where GitHub enforces a gate it re-checks at merge time and refuses; where it does not, this is
   the closest honest approximation, and the residual race is stated rather than papered over.

   The refresh **reassigns** `CHECKS` and **rebuilds** `REQUIRED_STATE`. An earlier revision ran
   `gh pr checks` here without assigning it, so the command printed the new JSON and discarded it
   while item 6 was re-evaluated against the stale step-3 table — a refresh that refreshed nothing
   (icn#2656 review). Its test asserted the command's presence rather than its effect, which is
   why it passed. Re-running a read is not the same as replacing what you reason about.

   This is the same construction as step 3, deliberately repeated rather than referenced so that
   what runs here is visible where it runs; a test asserts the two are character-for-character
   identical, so they cannot drift apart the way `merge-pr` and `policy.json` originally did.

   Re-reading the checks matters for a specific subset. For a check GitHub itself requires, GitHub
   re-evaluates protection at merge time and refuses — that backstop is real, and it is why an
   ordinary merge needs no race-free gate of its own. But this procedure enforces the **union** of
   `POLICY` and `LIVE`, and for a policy-required check that is *not* a live protection context,
   nothing but this skill enforces it: if it is rerun and fails between step 3 and here, an
   ordinary merge succeeds anyway and the gate was decorative (icn#2656 review). `POLICY \ LIVE`
   is empty on `main` today, so this does not currently reproduce — the union exists precisely
   because the two sets *can* diverge, and `agent_tooling_check_note` records that this one did.

   Re-reading **narrows** the window to the merge call itself; it does not close it, because there
   is no atomic evaluate-and-merge. That is an honest limit, not a gap to keep patching — the
   answer to an unclosable race is not more authority, and where GitHub can enforce, it does. `--match-head-commit` pins the head and **nothing pins the base** —
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
