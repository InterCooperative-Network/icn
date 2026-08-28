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
   If the caller supplied the exact head the authorization was given for, pass it through
   unchanged:
   ```bash
   icn-merge-pr merge "$PR" --authorize --expected-head "$AUTHORIZED_HEAD"
   ```
   **Forward it verbatim or not at all.** Do not read the live head to fill it in: a value derived
   here is a statement about now, and the whole point of the option is that it comes from whatever
   established the head was fit to merge. Do not substitute, shorten, or re-derive it, and do not
   drop it and merge anyway when it does not match — the program owns that decision.

   The program re-reads every signal immediately before acting, so a `READY` from step 1 is a
   report, not a promise. A refusal here means the state genuinely changed; it is not a glitch to
   retry. `REFUSED_EXPECTED_HEAD` means the pull request moved off the head that was authorized:
   whatever justified the merge covered a commit that is no longer the head.

6. **Report** the final outcome, quoting the code. Each one describes **this invocation**, not
   the pull request's history.
   - `MERGED` — this invocation completed a confirmed merge. It carries the merge commit SHA.
   - `REFUSED_*` — this invocation did not complete a confirmed ordinary merge. That is a
     statement about the invocation and not about the pull request, which may already have been
     merged or closed by someone else. Report the structured reason and observed state exactly as
     step 2 requires, and infer nothing beyond them.
   - `MERGE_UNCONFIRMED` — a merge request went out and its result could not be established. The
     status is **unknown** and needs a human to establish what happened. Never report it as "did
     not merge", and do not run anything else against this PR first.

## When the executable is missing

Refuse the merge. There is no fallback merge path, and **do not run the installer that is sitting
in the working tree** — a change under review can edit its own installer, so an installer taken
from the checkout being reviewed vouches for nothing, including itself.

Hand the operator this instead: take the installer from the trusted default-branch ref, and point
it at a checkout that is on that branch.

```bash
git -C <repo> fetch origin
d="$(mktemp -d)"
git -C <repo> show origin/<default-branch>:tools/icn-merge-pr/install.py > "$d/install.py"
python3 "$d/install.py" --source <a checkout on the default branch>
```

`mktemp -d` matters: a fixed path under a world-writable directory can be pre-created as someone
else's symlink, or replaced between the write and the run, which would execute their code with the
operator's credentials.

Resolve `<default-branch>` from GitHub, not from a file in the repository. The installer then
refuses unless that source checkout is clean and at the current remote head. The installed program
lives under `~/.local`, outside every worktree. `icn-merge-pr provenance` reports which commit is
installed.

## Boundaries

- Do not decide readiness here, or re-check anything the program decided.
- Do not escalate to a privileged merge when the program refuses. A privileged merge is a human
  decision owned by the ADR that `ops/state/truth/policy.json` names, and is outside this
  workflow.
- Do not arrange for a merge to happen later. The primitive completes or refuses.
- Do not retry a refusal with different arguments.
