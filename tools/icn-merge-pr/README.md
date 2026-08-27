# icn-merge-pr

The trusted ordinary-merge executable for ICN. Merge semantics live here, in code, rather than in
a Markdown procedure a model re-interprets each time (icn#2651).

It has exactly two mutation outcomes: **MERGED** or **REFUSED**. It never admin-merges, never arms
auto-merge, never enqueues, and never leaves a future merge armed.

## Install

```bash
python3 tools/icn-merge-pr/install.py
```

Installation **refuses** unless its source checkout is provably the live default-branch tip: the
default branch and its head OID come from GitHub, the checkout must be on that branch, `git fetch`
runs, local `HEAD` must equal the externally reported OID, and every file being installed must be
clean. There is no `--force`; a flag that skipped those checks would be the whole vulnerability
wearing a respectable name.

The program installs to `~/.local/lib/icn/merge-pr/` with a launcher at `~/.local/bin/icn-merge-pr`
— outside every ICN worktree, because a program executed from the PR's own worktree is
candidate-controlled. The launcher runs with `-E -s` and pins `sys.path[0]` to the install root, so
a checkout the operator happens to be standing in cannot supply the code that decides a merge.

## Use

```bash
icn-merge-pr check <PR>              # evaluate; mutates nothing
icn-merge-pr merge <PR> --authorize  # re-read everything, then merge once, or refuse
icn-merge-pr provenance              # which commit is installed
```

Results are JSON on stdout with a stable `outcome` code (`icn_merge_pr/codes.py`); a summary goes
to stderr. Exit 0 = READY or MERGED, 1 = refused, 2 = bad invocation.

Unknown options fail. `--admin`, `--auto` and their privileged or deferred relatives are refused
**by name**, so an operator reaching for the habit is told the primitive has no such mode.

## How it decides

1. Resolve the default branch from **external** GitHub metadata — never from a field inside the
   repository content being evaluated.
2. Read the PR's base. If it is not that default branch, refuse **before** loading any policy:
   a non-default base must never supply the document that defines readiness.
3. Pin the trusted default-branch OID, load `ops/state/truth/policy.json` from it, and validate it
   with `scripts/check-merge-policy-schema.py` (vendored at install time, one owner for the rule).
4. Gather every signal in one snapshot loader — state, draft, mergeability, merge state, reviews,
   *every page* of review threads, *every page* of the check rollup, live branch protection, merge
   queue, auto-merge. Unavailable evidence is not ready.
5. Evaluate the structured `merge.ready_when` gate. Fail closed on any drift between the pinned
   policy and live branch protection.
6. Before mutating, run the **same** loader again, refuse if any pinned identity moved, and
   re-evaluate every gate.
7. Merge once, with the expected head SHA pinned in the request. A GitHub refusal is final. Success
   is a fresh read reporting `merged == true`, not what the merge call returned.

## Layout

| File | Owns |
|---|---|
| `icn_merge_pr/codes.py` | the stable outcome vocabulary |
| `icn_merge_pr/ghclient.py` | the only code that talks to GitHub |
| `icn_merge_pr/policy.py` | pinned policy load + validation, and the closed strategy set |
| `icn_merge_pr/snapshot.py` | the one typed evidence boundary and the trust sequence |
| `icn_merge_pr/evaluate.py` | the gate — pure, no I/O |
| `icn_merge_pr/strategy.py` | the only mapping from a policy value to an API value |
| `icn_merge_pr/merge.py` | the single head-pinned mutation and its proof |
| `icn_merge_pr/run.py` | evaluate → refresh → re-evaluate → merge |
| `install.py` | the provenance gate |

Tests: `scripts/tests/test_icn_merge_pr.py` (behaviour) and
`scripts/tests/test_icn_merge_pr_install.py` (provenance).
