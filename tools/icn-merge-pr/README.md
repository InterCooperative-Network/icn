# icn-merge-pr

The trusted ordinary-merge executable for ICN. Merge semantics live here, in code, rather than in
a Markdown procedure a model re-interprets each time (icn#2651).

It has exactly two mutation outcomes: **MERGED** or **REFUSED**. It never admin-merges, never arms
auto-merge, never enqueues, and never leaves a future merge armed.

## Install

Take the installer from the trusted default-branch ref, not from the working tree — a change under
review can edit its own installer, and one taken from the checkout being reviewed vouches for
nothing, including itself:

```bash
git -C <repo> fetch origin
d="$(mktemp -d)"
git -C <repo> show origin/<default-branch>:tools/icn-merge-pr/install.py > "$d/install.py"
python3 "$d/install.py" --source <a checkout on the default branch>
```

`mktemp -d` matters: a fixed path under a world-writable directory can be pre-created as someone
else's symlink, or replaced between the write and the run, which would execute their code with the
operator's credentials.

The installer also refuses to run out of a checkout that is not on the default branch, which
catches the honest accident of running the copy in front of you. That check cannot bind a tampered
installer — a pull request editing the file deletes the check too — which is exactly why the
instruction above starts from the ref rather than the tree.

Installation **refuses** unless its source checkout is provably the live default-branch tip: the
default branch and its head OID come from GitHub, the checkout must be on that branch, `git fetch`
runs, local `HEAD` must equal the externally reported OID, and every file being installed must be
clean. There is no `--force`; a flag that skipped those checks would be the whole vulnerability
wearing a respectable name.

The program installs to `~/.local/lib/icn/merge-pr/` with a launcher at `~/.local/bin/icn-merge-pr`
— outside every ICN worktree, because a program executed from the PR's own worktree is
candidate-controlled.

**Closed-tree integrity.** The launcher runs `python3 -I -B`, and `__main__.py` verifies the
install tree *before* putting it on the import path or importing any package module: every
recorded file must be a regular file with its recorded digest, and the tree must be **closed** —
the install root holds only the record and the package directory, the package directory holds only
the recorded files, and nothing is a symlink. Without this, an unrecorded top-level `json.py`
executed during `import icn_merge_pr.cli`, ahead of any provenance check, and the digest check
never noticed because it only inspected paths the record named.

Call this what it is: **integrity, not authentication.** A local actor who can rewrite the whole
installation *and* its record is outside this program's threat model — they already hold the
credentials it would use. What is closed off is narrower and worth having on its own: a tree that
has merely gained an unexpected importable file no longer gets to run it.

## Use

```bash
icn-merge-pr check <PR>              # evaluate; mutates nothing
icn-merge-pr merge <PR> --authorize  # re-read everything, then merge once, or refuse
                                     # (only an INSTALLED copy may mutate)
icn-merge-pr provenance              # which commit is installed
```

Results are JSON on stdout with a stable `outcome` code (`icn_merge_pr/codes.py`); a summary goes
to stderr.

Exit 0 = READY or MERGED. Exit 2 = bad invocation. **Exit 1 means refused _or_ `MERGE_UNCONFIRMED`,
and those are not the same thing**: a refusal means nothing happened, while `MERGE_UNCONFIRMED`
means a merge request went out and its result could not be established. A consumer must read the
structured `outcome` — re-issuing a merge on the strength of exit 1 alone is exactly the mistake
the outcome vocabulary exists to prevent.

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
   queue, auto-merge. Unavailable evidence is not ready, and a thread count larger than the threads
   actually readable is unavailable evidence.
5. Refuse unless **no server-side bypass path exists at all** on the target branch: protection
   must apply to administrators (`enforce_admins`), classic pull-request bypass allowances must be
   absent or empty, and every ruleset actively in force on that branch must have zero bypass
   actors. Rulesets are enumerated through GitHub's own applicability endpoint, so inherited
   organisation and enterprise rules are covered without needing `admin:org`; if any of that
   evidence cannot be read, the answer is refuse.

   This deliberately does **not** ask whether the current caller matches a particular grant. That
   would mean resolving user, team, app and custom-role membership — an authorization engine this
   primitive has no business holding. The existence of an active bypass path is what makes the
   merge non-ordinary, whoever it belongs to. The ordinary merger is intentionally incompatible
   with configured bypass actors: if a repository needs them, that is a privileged authority
   design and must not arrive quietly inside ordinary merge.
6. Match each required check by **name and producer**. Where branch protection pins a check to a
   GitHub App, only that App's runs are consulted — a green result of the right name from another
   source is as good as absent.
7. Evaluate the structured `merge.ready_when` gate. Fail closed on any drift between the pinned
   policy and live branch protection.
8. Before mutating, run the **same** loader again, refuse if any pinned identity moved, and
   re-evaluate every gate.
9. Merge once, with the expected head SHA pinned in the request. Success is a fresh read
   reporting `merged == true`, not what the merge call returned. A refusal is reported only when a
   fresh read confirms the PR is not merged; once a request has been dispatched, an outcome that
   cannot be established is `MERGE_UNCONFIRMED` — never "it did not merge".

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
