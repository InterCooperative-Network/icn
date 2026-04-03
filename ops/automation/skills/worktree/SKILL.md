---
name: worktree
description: "Manage ICN git worktrees: create, status, cleanup, rebase. Usage: /worktree [status|create <name>|cleanup|rebase <name>]"
disable-model-invocation: true
truth_contract:
  canonical_sources:
    - ops/state/config/repo-map.json  # worktrees.root
  live_load_required:
    - git worktree list
  examples_only: []
---

Manage ICN git worktrees. Worktrees live at `<REPO_ROOT>/../icn-wt/` (from `repo-map.json#worktrees.root`).

**Always resolve paths dynamically — never assume an absolute path:**
```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
WORKTREE_ROOT="${REPO_ROOT}/../icn-wt"
```

Parse the argument to determine the subcommand. If no argument or unrecognized, default to `status`.

---

## `/worktree status` (default)

Try the MCP tool first:
```
call: worktree_status
```

If MCP is available, format output as:

```
Worktrees (N)
| Name                   | Branch       | Behind Main | Claimed By        |
|------------------------|-------------|-------------|-------------------|
| main                   | main        | current     | —                 |
| 1234-feature-name      | feat/1234   | 2           | —                 |
| 1120-example           | feat/1120   | 15  ⚠️      | —                 |
```

Flag ⚠️ if behind main > 10 commits.

If MCP unavailable, fall back to:
```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
WORKTREE_ROOT="${REPO_ROOT}/../icn-wt"
ls "${WORKTREE_ROOT}/" 2>/dev/null || echo "no worktrees"
```
Then for each directory:
```bash
git -C "${WORKTREE_ROOT}/<name>/icn" rev-parse --abbrev-ref HEAD 2>/dev/null
git -C "${WORKTREE_ROOT}/<name>/icn" log -1 --format="%cr: %s" 2>/dev/null
```

---

## `/worktree create <name>`

Creates a new worktree and branch for feature work.

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
WORKTREE_ROOT="${REPO_ROOT}/../icn-wt"
git -C "${REPO_ROOT}" worktree add "${WORKTREE_ROOT}/<name>/icn" -b feat/<name>
```

On success, report:
```
Created worktree: <WORKTREE_ROOT>/<name>/icn
Branch: feat/<name>
To start working: cd <WORKTREE_ROOT>/<name>/icn
```

On failure (e.g. branch exists), show the git error and suggest:
```bash
git -C "${REPO_ROOT}" worktree list
```

---

## `/worktree cleanup`

Remove stale worktrees that are no longer needed.

**Step 1**: Check what's stale
```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
git -C "${REPO_ROOT}" worktree list
```

**Step 2**: Check merged branches (note: squash-merged branches won't appear here — check PR state via `gh`):
```bash
git -C "${REPO_ROOT}" branch --merged origin/main | grep feat/
```

**Step 3**: For each merged branch with a worktree, confirm before removing:
"Remove worktree `<name>` (branch `feat/<name>` is merged to main)? [y/N]"

If confirmed:
```bash
git -C "${REPO_ROOT}" worktree remove "${WORKTREE_ROOT}/<name>/icn"
```

**Step 4**: Prune stale references:
```bash
git -C "${REPO_ROOT}" worktree prune
```

Report: N worktrees removed, N pruned.

**Safety**: Never remove a worktree with uncommitted changes or claimed by an active session (check MCP `list_sessions` first if available).

---

## `/worktree rebase <name>`

Rebase a worktree's branch onto latest origin/main.

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
WORKTREE_ROOT="${REPO_ROOT}/../icn-wt"
git -C "${WORKTREE_ROOT}/<name>/icn" fetch origin
git -C "${WORKTREE_ROOT}/<name>/icn" rebase origin/main
```

Report:
- ✅ "Rebased N commits onto origin/main"
- ❌ "Conflicts detected in: [file list]. Resolve manually, then `git rebase --continue`"

If the worktree has uncommitted changes, warn and stop:
```bash
git -C "${WORKTREE_ROOT}/<name>/icn" status --porcelain
```
"⚠️ Worktree has uncommitted changes — stash or commit before rebasing."
