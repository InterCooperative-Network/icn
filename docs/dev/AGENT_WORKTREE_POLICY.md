# Agent Worktree Policy (icn-dev operating model)

**Status**: Active operating policy for agent sessions on the dev host
**Scope**: Local repository operations. Complements [WORKTREES.md](WORKTREES.md), which documents git-worktree mechanics for the older repo-adjacent `../icn-wt/` layout. This policy governs the host-level `~/icn-dev/` layout that agent sessions must use.

## Why this exists

Parallel agent sessions produced valid work but exposed an orchestration failure: multiple agents with broad prompts each opened locally reasonable PRs with no worktree allocation, no file-lock map, and no single merge lane. The fix is not more review. The fix is a local operating system where isolation and coordination are structural.

## The layout

```text
~/icn-dev/
  repos/                 # bare reference/object stores — NEVER edit here
    icn.git
    nycn.git
    icn-learn.git
    icn-infra.git
    icn-community-bridge.git
    org-github.git       # InterCooperative-Network/.github
  worktrees/
    <repo>/<task-name>/  # one task = one worktree = one branch = one PR
    <repo>/main-readonly/    # detached read-only checkout of origin/main
    <repo>/review-pr-<N>/    # PR review checkouts
  state/
    repo-status.md           # bare-store fetch state
    active-worktrees.yaml    # worktree allocation
    merge-queue.md           # the single merge lane
    file-locks.yaml          # file ownership map
  scripts/
    repo-sync   # fetch all bare stores, refresh main-readonly checkouts
    wt-new      # create a task worktree:   wt-new icn <task-name> <branch>
    wt-status   # list all worktrees across all bare stores
    wt-pr       # create a PR review worktree:  wt-pr icn 2005
```

## Rules

1. **Bare repos are reference/object stores only.** Nothing under `~/icn-dev/repos/` is ever edited, committed to, or checked out in place.
2. **Never edit from `main`.** The bare stores carry no local `main` branch at all — only `origin/main` remote-tracking refs. `main-readonly/` is a detached checkout for reading current main; if it is ever dirty, that is a policy violation.
3. **Every task gets a dedicated worktree.** Create it with `wt-new`. Never share a working tree between tasks or agents, and never reuse a review worktree for new edits.
4. **Declare before editing.** Every session reports repo, worktree path, branch, base ref, and expected files **before** making changes. If any of these is ambiguous, stop and ask.
5. **Check the file-lock map first.** `~/icn-dev/state/file-locks.yaml` records which open PR owns which files. Do not touch a file owned by another open task. Add a lock entry when you claim files; release it when the PR merges or closes.
6. **One merge lane.** `~/icn-dev/state/merge-queue.md` is the ordering truth. Agents do not merge anything unless the current session gives explicit merge authorization and live checks are clean.
7. **Generated docs require explicit sequencing.** PRs that change `docs/registry.toml` usually imply a `docs/DOCUMENT_REGISTRY.md` regeneration, and sometimes `docs/INDEX.generated.md`. Content PR merges first; regeneration follows as its own queued step. Never bundle unrelated regeneration into a content PR.
8. **One PR = one reviewable task.** No broad cleanup PRs that touch unrelated files. If adjacent work looks necessary, note it and queue it as its own task.

## Typical flow

```bash
~/icn-dev/scripts/repo-sync                       # refresh reference stores
~/icn-dev/scripts/wt-new icn my-task docs/my-task # allocate worktree + branch
cd ~/icn-dev/worktrees/icn/my-task
# declare repo/worktree/branch/base/expected files, claim locks, then edit
git add <files> && git commit -m "docs(scope): summary"
git push -u origin docs/my-task && gh pr create --base main
# update state/active-worktrees.yaml, state/file-locks.yaml, state/merge-queue.md
```

## Reviewing a PR

```bash
~/icn-dev/scripts/wt-pr icn 2005
cd ~/icn-dev/worktrees/icn/review-pr-2005
git diff --stat origin/main...HEAD
```

Review worktrees are for inspection and validation only. Fixes to a PR happen on that PR's own branch in its own task worktree, never by piling commits into a review checkout of someone else's lane.

## Relationship to WORKTREES.md

[WORKTREES.md](WORKTREES.md) explains git-worktree mechanics and the older single-clone `../icn-wt/` layout, including Rust build isolation (`CARGO_TARGET_DIR`), which still applies inside any worktree here. This policy supersedes the *operating layout* for agent sessions on the dev host: the shared clone is replaced by bare reference stores plus task worktrees, with coordination state in `~/icn-dev/state/`.
