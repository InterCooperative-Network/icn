# Multi-Agent Git Worktree Workflow

Git worktrees let multiple agents work in parallel on the same repository without interfering with each other. Each worktree is a separate checkout backed by a **shared `.git` database** — no cloning, no extra disk for object storage.

## How Worktrees Work

- The primary repo at `/home/ubuntu/projects/icn` contains the `.git/` directory.
- Each worktree gets its own directory with a `.git` **pointer file** (not a directory) that references the shared database.
- Each worktree must be on a **different branch** — Git enforces this.
- Commits, branches, and refs are shared instantly across all worktrees.

## Directory Layout

```
/home/ubuntu/projects/
├── icn/                    # Primary repo (main checkout)
│   ├── .git/               # Shared git database
│   ├── icn/                # Cargo workspace
│   └── ...
└── icn-wt/                 # Worktree workspace (sibling directory)
    ├── agent-a/            # Worktree on feat/agent-a
    │   ├── .git            # Pointer file → ../icn/.git/worktrees/agent-a
    │   ├── icn/
    │   └── ...
    ├── agent-b/            # Worktree on feat/agent-b
    └── agent-c/            # Worktree on feat/agent-c
```

## Rules

1. **One agent = one branch = one worktree.** Never share a worktree between agents.
2. **Never commit to `main`.** All work happens on feature branches.
3. **Never checkout `main`** in a worktree — use `origin/main` as a read-only base.
4. **Isolate build output** — set `CARGO_TARGET_DIR` per worktree (see below).

## Rust Build Isolation

By default all worktrees share the same `target/` path resolution, which causes lock contention and corrupted incremental builds when two agents compile simultaneously.

**Fix: set `CARGO_TARGET_DIR` in each worktree.**

```bash
# In each agent's shell (or .envrc if using direnv)
export CARGO_TARGET_DIR="$PWD/target"
```

This gives each worktree its own `target/` directory. Disk cost is higher but builds are safe.

Alternatively, add to `icn/.cargo/config.toml` per-worktree:

```toml
# This file is NOT committed — it's per-worktree only
[build]
target-dir = "/home/ubuntu/projects/icn-wt/agent-a/icn/target"
```

## Commands Reference

### Create a worktree

```bash
# From primary repo
git worktree add ../icn-wt/<agent-name> -b feat/<agent-name> origin/main
```

### List worktrees

```bash
git worktree list
```

### Remove a worktree

```bash
git worktree remove ../icn-wt/<agent-name>
# Then optionally delete the branch
git branch -d feat/<agent-name>
```

### Prune stale worktree references

```bash
git worktree prune
```

### Using the helper script

```bash
# Create
./scripts/worktrees.sh create <agent-name>

# List
./scripts/worktrees.sh list

# Remove
./scripts/worktrees.sh remove <agent-name>
```

## Typical Agent Flow

```bash
# 1. Agent is assigned a worktree (already on its branch)
cd /home/ubuntu/projects/icn-wt/agent-a

# 2. Set build isolation
export CARGO_TARGET_DIR="$PWD/target"

# 3. Sync with latest main
git fetch origin
git rebase origin/main

# 4. Work — edit, build, test
cd icn
cargo build -p icn-gateway
cargo test -p icn-gateway

# 5. Commit (conventional commit format)
git add -A
git commit -m "feat(gateway): add rate limit headers"

# 6. Push and open PR
git push -u origin feat/agent-a
gh pr create --base main --title "feat(gateway): add rate limit headers"
```

## Cleanup Checklist

When an agent's work is merged:

- [ ] `git worktree remove ../icn-wt/<agent-name>` — remove the worktree
- [ ] `git branch -d feat/<agent-name>` — delete the local branch
- [ ] `git push origin --delete feat/<agent-name>` — delete the remote branch (if not auto-deleted by PR merge)
- [ ] `git worktree prune` — clean up stale refs
- [ ] Remove any per-worktree `target/` directory if disk space is needed
