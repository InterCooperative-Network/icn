---
name: ci-status
description: One-pass PR merge-readiness verdict — required checks (vs non-required noise), unresolved review threads, mergeable state. Read-only, never merges.
argument-hint: "[PR number]"
user-invocable: true
allowed-tools: "Bash"
truth_contract:
  canonical_sources:
    - ops/state/truth/policy.json       # required_checks, non_blocking_checks
  live_load_required:
    - "gh pr view <N> --json state,isDraft,headRefOid,mergeable,mergeStateStatus"
    - "gh api repos/InterCooperative-Network/icn/branches/main/protection/required_status_checks --jq '.contexts'"
    - "gh api graphql … pullRequest.reviewThreads.nodes[].isResolved"
  examples_only: []
  never_hardcode:
    - required check set or count (always read live from branch protection / policy.json)
    - PR number or branch name
---

# ci-status

Give a single, accurate merge-readiness verdict for an ICN PR in one pass. The **required** check
set (branch protection / `ops/state/truth/policy.json`) is authoritative — load it live, never
hardcode it. Non-required reds never block. Read-only — this skill never merges, reruns, or
resolves threads.

## Input
- `$1` = PR number. If omitted: `gh pr view --json number --jq .number`.
- Repo: `InterCooperative-Network/icn`.

## Routine
1. **HEAD + mergeability** (verify the live HEAD, not a stale run):
   ```bash
   gh pr view "$PR" --json state,isDraft,headRefOid,mergeable,mergeStateStatus \
     --jq '{state,isDraft,headRefOid,mergeable,mergeStateStatus}'
   ```
2. **Required checks** — load the set live, then report each (tab-separated; multi-word names need `-F'\t'`):
   ```bash
   REQ="$(gh api repos/InterCooperative-Network/icn/branches/main/protection/required_status_checks --jq '.contexts | join("|")')"
   gh pr checks "$PR" 2>/dev/null | awk -F'\t' -v reqs="$REQ" '
     BEGIN{n=split(reqs,R,"|");for(i=1;i<=n;i++)req[R[i]]=1}
     req[$1]{printf "  %-34s %s\n",$1,$2; if($2!="pass")bad++}
     END{print "  REQUIRED_NOT_PASS="bad+0}'
   ```
3. **Unresolved review threads** (`required_conversation_resolution` blocks on any open thread):
   ```bash
   gh api graphql -f query='{ repository(owner:"InterCooperative-Network",name:"icn"){ pullRequest(number:'"$PR"'){
     reviewThreads(first:100){ nodes{ isResolved } } } }}' \
     --jq '{unresolved: ([.data.repository.pullRequest.reviewThreads.nodes[]|select(.isResolved==false)]|length)}'
   ```

## Verdict
- **READY** — all required `pass` (REQUIRED_NOT_PASS=0) AND unresolved=0 AND `mergeable=MERGEABLE`.
  `mergeStateStatus: UNSTABLE` is fine here if the only non-`pass` checks are non-required.
- **PENDING** — a required check is still `pending`/`""`.
- **BLOCKED** — a required check failed, OR unresolved>0, OR `mergeable!=MERGEABLE`. Name the blocker(s).

## Flake classification (before calling a required failure "real")
- Non-required benchmark checks (e.g. `Compare Against Base`, `Save Benchmark Baseline`) → noise; ignore.
- A failing job whose step shows `conclusion: null` (`gh api repos/InterCooperative-Network/icn/actions/jobs/<id>`)
  = runner kill/eviction, not a test failure (the #1955 sled-flock class). Re-run that job **once**
  (`gh run rerun <run-id> --failed`) before treating it as real.
- Advisory checks (`claude`) `skipping` → ignore (never block).

## Boundaries
- Read-only. To merge, use the `merge-pr` skill after explicit authorization. Never add
  `perf-regression-ok`. Watching to completion = a bounded, observable loop, never blind polling.
