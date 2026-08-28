---
name: ship-pr
description: Finish a pull request. Read its lifecycle state, run one bounded review generation, batch blocker fixes, freeze an exact head, then hand over to the merge primitive.
argument-hint: "[PR number]"
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
truth_contract:
  canonical_sources:
    - ops/state/truth/delivery.json     # lifecycle, dispositions, blocker predicate, freeze
    - ops/state/truth/skills.json       # which skill owns the merge primitive
  live_load_required:
    - "gh pr view <N> --json state,isDraft,mergeable,baseRefName,headRefOid,body"
    - "gh api graphql  # review threads and their resolved state"
  examples_only: []
  never_hardcode:
    - lifecycle states, dispositions, or the blocker predicate — load delivery.json
    - required checks, merge strategy, or readiness rules — this skill owns none of them
    - PR number, branch name, or head SHA
---

# ship-pr

Take one pull request from where it is to merged, without letting review run forever.

The governing rule, owned by `ops/state/truth/delivery.json`:
**automated review is evidence, not unbounded veto authority.**

This skill decides *when* a PR is ready to hand over. It never decides *whether* the merge is
permitted. That belongs to the merge primitive and its executable, and a second implementation
here would be a second owner of merge semantics — with the copy being the one that rots.

## Input
- `$1` = PR number. If omitted: `gh pr view --json number --jq .number`.

## Load first, every time

```bash
python3 -c "import json;print(json.dumps(json.load(open('ops/state/truth/delivery.json')),indent=2))"
```

Do not work from memory of this file. The lifecycle states, the finding dispositions, the blocker
predicate, the freeze rules and the lane definitions are all owned there and may have changed.

## 1. Read the live state

Query the PR. Read the lifecycle block from the PR body — the delimiters are named by
`lifecycle.state_surface` in the policy. The PR body is where lifecycle state lives; a repository
file claiming to know a PR's current state is stale by construction.

If there is no block, the PR is in the first state of `lifecycle.states`. Write the block.

## 2. Confirm the lane

`lanes.default` unless the block already names one. The DEEP lane requires the maintainer to
select it — do not choose it yourself.

## 3. Establish the acceptance contract

The contract is what the PR claims to deliver, plus its explicit non-goals. It comes from the PR
body; `pr-create` puts it there. If it is missing, write it from the diff and the linked issue and
say so. Everything downstream — what a FULL review may inspect, and what makes a finding a
blocker — is measured against this contract, so an unstated contract makes the lane unbounded.

## 4. Run the review generation

One comprehensive generation per lane. Request it once, when the implementation is complete and
verification is green — not after every push.

Record the generation in the block. **A push does not reset it.** If fixing findings reopened
discovery, then fixing would be the act that triggers the next unbounded search, and no PR that
fixes anything could ever converge.

## 5. Classify every finding

Use the dispositions in `finding_dispositions` and the predicate in `blocker_predicate`. A finding
is a blocker only when **every** condition in `blocker_predicate.all_must_hold` is satisfied.

A reviewer's own severity label is advisory evidence about that reviewer's confidence. It is not
authority, and it never satisfies the predicate by itself.

Reply to every thread with its disposition and the evidence, then resolve it. Server-side
conversation resolution still applies to a frozen PR; dispositioning a thread is not reopening
review.

## 6. Handle blockers as one batch

Fix the known blockers together. For each: the smallest local correction plus only the regression
coverage that directly proves it closed. Do not inspect sibling areas for more things to fix —
`blocker_predicate.no_sibling_sweep` says why.

## 7. Defer everything else

Valid observations that are not blockers go to one follow-up ledger issue for this PR — reuse it,
do not open one per comment. Each entry states the observation, why it was deferred, and the fix,
with a link back to its thread. Reply to the thread saying it is valid but outside the frozen
delivery contract, link the ledger, and resolve.

## 8. Verify

Run the verification appropriate to the changed paths. After a blocker batch this is DELTA
verification: the changed delta and whether it addresses the known findings. Not a restart.

## 9. Freeze

When `freeze.entry_conditions` are all met, name the exact head and write the block:

```
ICN DELIVERY LIFECYCLE
State:                FROZEN
Lane:                 <lane>
Acceptance contract:  <one line, or a pointer into the body>
Review generation:    CLOSED
Freeze head:          <exact 40-hex head>
Known blockers:       0
Follow-up ledger:     <issue>
```

After this, a new automated review comment does not reopen the PR. A late finding breaks the
freeze only if it satisfies every blocker condition; then make one targeted correction, run delta
verification, update the freeze head, and return to FROZEN. Do not run another comprehensive
review. If a late observation would need a substantial design change, it is a follow-up and a
maintainer decision, not a widening of the frozen scope.

## 10. Wait only on live merge gates

Use the repository's bounded wait primitive, `ops/scripts/icn-wait`. Never an ad-hoc polling loop.
Read which gates actually matter from the merge owner and live branch protection — this skill does
not know them and must not learn them.

## 11. Hand over

```bash
icn-merge-pr check "$PR"
```

Report the structured outcome verbatim. If it is not ready, stop: the reasons say what has to
change. If it is ready, the mutation is the `merge-pr` skill's, under explicit per-PR maintainer
authorization. Move the block to the merging state, then to the final state with the merge commit.

Do not re-derive, second-guess, or work around anything the executable decided, and do not
substitute another path when it refuses.

## 12. Report

The final head, the merge commit, the disposition counts, and the follow-up ledger.

## Boundaries

- Do not evaluate merge requirements here. Do not read branch protection to decide readiness.
- Do not request another comprehensive review generation. Only the maintainer may.
- Do not escalate a follow-up into a blocker, or reopen discovery, on your own.
- Do not drop a valid observation instead of recording it.
- Do not put lifecycle state in a repository file.
