---
name: worktree
description: "Manage ICN git worktrees: create, status, cleanup, rebase. Usage: /worktree [status|create <name>|cleanup|rebase <name>]"
disable-model-invocation: true
---

Manage worktrees in `/home/ubuntu/projects/icn-wt/`. These are full checkouts of the `icn` repo on separate branches.

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
| 1084-names-gateway-a   | feat/1084   | 2           | Agent1 (naming)   |
| main                   | main        | current     | —                 |
| 1120-auth-semantics-b  | feat/1120   | 15  ⚠️      | —                 |
```

Flag ⚠️ if behind main > 10 commits.

If MCP unavailable, fall back to:
```bash
ls /home/ubuntu/projects/icn-wt/
```
Then for each directory:
```bash
git -C /home/ubuntu/projects/icn-wt/<name>/icn rev-parse --abbrev-ref HEAD 2>/dev/null
git -C /home/ubuntu/projects/icn-wt/<name>/icn log -1 --format="%cr: %s" 2>/dev/null
```

---

## `/worktree create <name>`

Creates a new worktree and branch for feature work.

```bash
git -C /home/ubuntu/projects/icn worktree add \
  /home/ubuntu/projects/icn-wt/<name>/icn \
  -b feat/<name>
```

On success, report:
```
Created worktree: /home/ubuntu/projects/icn-wt/<name>/icn
Branch: feat/<name>
To start working: cd /home/ubuntu/projects/icn-wt/<name>/icn
```

On failure (e.g. branch exists), show the git error and suggest: `git -C /home/ubuntu/projects/icn worktree list` to see existing worktrees.

---

## `/worktree cleanup`

Remove stale worktrees that are no longer needed.

**Step 1**: Check what's stale
```bash
git -C /home/ubuntu/projects/icn worktree list
```

**Step 2**: For each worktree, check if its branch is merged into main:
```bash
git -C /home/ubuntu/projects/icn branch --merged origin/main | grep feat/
```

**Step 3**: For each merged branch that has a worktree, ask for confirmation before removing:
"Remove worktree `<name>` (branch `feat/<name>` is merged to main)? [y/N]"

If confirmed:
```bash
git -C /home/ubuntu/projects/icn worktree remove /home/ubuntu/projects/icn-wt/<name>/icn
```

**Step 4**: Prune stale worktree references:
```bash
git -C /home/ubuntu/projects/icn worktree prune
```

Report: N worktrees removed, N pruned.

**Safety**: Never remove a worktree that has uncommitted changes or that is claimed by an active session (check MCP `list_sessions` first if available).

---

## `/worktree rebase <name>`

Rebase a worktree's branch onto latest origin/main.

```bash
git -C /home/ubuntu/projects/icn-wt/<name>/icn fetch origin
git -C /home/ubuntu/projects/icn-wt/<name>/icn rebase origin/main
```

Report:
- ✅ "Rebased N commits onto origin/main"
- ❌ "Conflicts detected in: [file list]. Resolve manually, then `git rebase --continue`"

If the worktree has uncommitted changes, warn and stop:
```bash
git -C /home/ubuntu/projects/icn-wt/<name>/icn status --porcelain
```
"⚠️ Worktree has uncommitted changes — stash or commit before rebasing."
