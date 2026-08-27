---
name: merge-pr
description: Merge one PR through the trusted ordinary-merge executable. Evaluate, report, get authorization, then merge or refuse.
argument-hint: "[PR number]"
user-invocable: true
allowed-tools: "Bash"
truth_contract:
  canonical_sources:
    - ops/state/truth/policy.json       # merge requirements; consumed by the executable, not here
  live_load_required:
    - "icn-merge-pr check <N>"          # the program loads every live signal itself
  examples_only: []
  never_hardcode:
    - required checks, merge strategy, or readiness rules — this skill owns none of them
    - PR number or branch name
---

# merge-pr

Merge one PR by calling the trusted executable and reporting what it said.

Merge semantics live in `icn-merge-pr`, not here. This skill resolves a PR number, runs the
program, shows the structured outcome, and gets a human's word before the form that mutates.
It does not interpret GitHub state, required checks, branch protection, review threads, or merge
strategy: a copy of any of those here would be a second owner, and the copy is the one that
silently rots (icn#2651).

## Input
- `$1` = PR number. If omitted: `gh pr view --json number --jq .number`.

## Routine

1. **Evaluate.** This mutates nothing.
   ```bash
   icn-merge-pr check "$PR"
   ```
   If the command is not found, **stop** — see *When the executable is missing*. Do not
   substitute another merge path.

2. **Report** the `outcome` and every `reasons[].detail` verbatim. The outcome code is the
   verdict. Do not soften it, re-derive it, or check anything it already decided.

3. **If the outcome is not `READY`**, stop. The reasons say what has to change.

4. **If the outcome is `READY`**, merging still requires
   explicit, per-PR maintainer authorization. Ask for it and wait for the answer, unless it has
   already been given in this interaction.

5. **Merge**, only once authorized:
   ```bash
   icn-merge-pr merge "$PR" --authorize
   ```
   The program re-reads every signal immediately before acting, so a `READY` from step 1 is a
   report, not a promise. A refusal here means the state genuinely changed; it is not a glitch to
   retry.

6. **Report** the final outcome. `MERGED` carries the merge commit SHA. Every other outcome means
   the PR did not merge — say so plainly and quote the code.

## When the executable is missing

Refuse the merge and hand the operator the install command. There is no fallback:

```bash
python3 tools/icn-merge-pr/install.py
```

Installation refuses unless its source checkout is the repository's default branch, clean, and at
the current remote head — so a checkout of the PR under review cannot install the program that
would merge it. The installed program lives under `~/.local`, outside every worktree.
`icn-merge-pr provenance` reports which commit is installed.

## Boundaries

- Do not decide readiness here, or re-check anything the program decided.
- Do not escalate to a privileged merge when the program refuses. A privileged merge is a human
  decision owned by the ADR that `ops/state/truth/policy.json` names, and is outside this
  workflow.
- Do not arrange for a merge to happen later. The primitive completes or refuses.
- Do not retry a refusal with different arguments.
