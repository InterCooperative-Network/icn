---
name: ci-status
description: One-pass PR merge-readiness verdict — required checks (vs non-required noise), unresolved review threads, mergeable + mergeStateStatus. Read-only, never merges.
argument-hint: "[PR number]"
user-invocable: true
allowed-tools: "Bash"
truth_contract:
  canonical_sources:
    - ops/state/truth/policy.json       # required_checks, non_blocking_checks
  live_load_required:
    - "gh pr view <N> --json state,isDraft,headRefOid,mergeable,mergeStateStatus"
    - "gh api repos/InterCooperative-Network/icn/branches/main/protection/required_status_checks --jq '.contexts'"
    - "gh api graphql … pullRequest.reviewThreads(first:100){ totalCount nodes{ isResolved } }"
  examples_only: []
  never_hardcode:
    - required check set or count (always read live from branch protection / policy.json)
    - PR number or branch name
---

# ci-status

Give a single, accurate merge-readiness verdict for an ICN PR in one pass. The **required** check
set (branch protection / `ops/state/truth/policy.json`) is authoritative — load it live, never
hardcode it. Non-required reds never block. Read-only — never merges, reruns, or resolves threads.

## Input
- `$1` = PR number. If omitted: `gh pr view --json number --jq .number`.
- Repo: `InterCooperative-Network/icn`.

## Routine
1. **HEAD + mergeability** — `mergeStateStatus` is part of the verdict, not just `mergeable`:
   ```bash
   gh pr view "$PR" --json state,isDraft,headRefOid,mergeable,mergeStateStatus \
     --jq '{state,isDraft,headRefOid,mergeable,mergeStateStatus}'
   ```
2. **Required checks** — load the set live, count `pass` / `pending` / `fail` **separately**. Do NOT
   swallow `gh` failures into a false green: if the check fetch errors or returns no rows, report
   **UNVERIFIABLE**, never READY.
   ```bash
   REQ="$(gh api repos/InterCooperative-Network/icn/branches/main/protection/required_status_checks --jq '.contexts | join("|")')"
   RAW="$(gh pr checks "$PR")"                      # capture; gh exits 8 on mixed (not an error)
   [ -z "$RAW" ] && { echo "UNVERIFIABLE: no check data (auth/repo resolution failed)"; exit 0; }
   printf '%s\n' "$RAW" | awk -F'\t' -v reqs="$REQ" '
     BEGIN{n=split(reqs,R,"|");for(i=1;i<=n;i++)req[R[i]]=1}
     req[$1]{printf "  %-34s %s\n",$1,$2;
             if($2=="pass")p++; else if($2=="pending"||$2=="")q++; else f++}
     END{print "  required: pass="p+0" pending="q+0" fail="f+0}'
   ```
3. **Unresolved review threads** — request `totalCount`; **fail closed if >100** (the `first:100`
   page can't confirm a clean state beyond that):
   ```bash
   gh api graphql -f query='{ repository(owner:"InterCooperative-Network",name:"icn"){ pullRequest(number:'"$PR"'){
     reviewThreads(first:100){ totalCount nodes{ isResolved } } } }}' \
     --jq '.data.repository.pullRequest.reviewThreads
           | {total:.totalCount, unresolved:([.nodes[]|select(.isResolved==false)]|length),
              truncated:(.totalCount>100)}'
   ```

## Verdict
- **READY** — required `fail=0 pending=0`, `unresolved=0` (and not `truncated`), `mergeable=MERGEABLE`,
  AND `mergeStateStatus` is `CLEAN`, **or** `UNSTABLE` with only non-required checks outstanding.
- **PENDING** — required `pending>0` (still running/queued), or `mergeStateStatus=UNKNOWN` (GitHub
  still computing — re-check).
- **BLOCKED** — any of: required `fail>0`; `unresolved>0`; `mergeable!=MERGEABLE` (`CONFLICTING`);
  `mergeStateStatus` is `BLOCKED` (failed required gate / unresolved thread / branch-protection rule),
  `DIRTY` (conflict), `BEHIND` (out of date — needs `gh pr update-branch`), or `DRAFT` (mark ready
  first). Name the exact blocker(s).
- **UNVERIFIABLE** — the check or thread fetch failed; do not assert readiness.

> Note: `mergeable=MERGEABLE` alone is NOT sufficient — a PR can be MERGEABLE yet `BLOCKED`/`BEHIND`.

## Flake classification (before calling a required failure "real")
- Non-required benchmark checks (e.g. `Compare Against Base`, `Save Benchmark Baseline`) → noise; ignore.
- A failing job whose step shows `conclusion: null` (`gh api repos/InterCooperative-Network/icn/actions/jobs/<id>`)
  = runner kill/eviction, not a test failure (the #1955 sled-flock class). Re-run that job **once**
  (`gh run rerun <run-id> --failed`) before treating it as real.
- Advisory checks (`claude`) `skipping` → ignore (never block).

## Boundaries
- Read-only. To merge, use the `merge-pr` skill after explicit authorization. Never add
  `perf-regression-ok`. Watching to completion = a bounded, observable loop, never blind polling.
