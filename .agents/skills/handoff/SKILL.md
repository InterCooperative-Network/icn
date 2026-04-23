---
name: handoff
description: Write a session handoff to docs/dev/ using the structured template with truth-plane labeling.
argument-hint: "[--push]"
user-invocable: true
allowed-tools: "Bash, Read, Write, Grep"
truth_contract:
  canonical_sources:
    - docs/dev/HANDOFF_TEMPLATE.md        # structured handoff template
    - docs/ai/ICN_SESSION_FRAME_TEMPLATE.md  # session frame (include in handoff)
  live_load_required:
    - "git branch --show-current"
    - "git log --oneline -10"
    - "gh pr list --json number,title,headRefName --limit 5"
  examples_only: []
  never_hardcode:
    - sprint number
    - PR numbers or branch names
    - session date (read from system)
---

Write a session handoff note so the next session (or next agent) can resume without context loss.

## Steps

### 1. Gather state

Run in parallel:
```bash
git branch --show-current
git log --oneline -10
git status --short
git stash list
```

Also:
```bash
# Open PRs on this branch (if gh available)
gh pr list --head $(git branch --show-current) --json number,title,state 2>/dev/null || echo "gh not available"
# Uncommitted TODOs added in this session
git diff origin/main...HEAD 2>/dev/null | grep '^+.*// TODO' | head -20
```

### 2. Identify open threads

- List any tests that are failing or skipped
- Note any `// TODO`, `// FIXME`, or `// HACK` lines added on this branch
- Note any files edited but not committed
- Note any decisions made this session (summarize from recent tool use context)

### 3. Write the handoff file

Target: `docs/dev/handoff-YYYY-MM-DD.md`

Where `YYYY-MM-DD` = today's date (`date +%Y-%m-%d`).

If a file for today already exists, use suffix: `handoff-YYYY-MM-DD-b.md`.

**Use the structured template from `docs/dev/HANDOFF_TEMPLATE.md`.** Each section explicitly labels its truth type. At minimum include:

- **Final State (Verified)** — only facts confirmed by commands
- **What Changed** — execution truth from this session
- **What's Open** — known incomplete work
- **Unsafe Assumptions** — anything relied on but not verified (do NOT skip this)
- **Next Move** — exact sequence for next session
- **Truth-Plane Notes** — which truth types were relied on, any known conflicts

### 4. Clean up

- Remind: `git stash list` should be empty — stash or commit before ending
- If `$ARGUMENTS` includes `--push` AND working tree is clean: run `/push` to push the branch

### 5. Report

Print:
```
Handoff written to docs/dev/handoff-YYYY-MM-DD.md
Branch: <name> | Commits: <n> | Open PRs: <n> | Open threads: <n>
```

## Important

- Do NOT commit the handoff file automatically. User decides.
- If `docs/dev/` doesn't exist, create it.
- Keep notes concise — this is for the next session, not a full PR description.
- The "Unsafe Assumptions" section is the most important section. If you skip one, do not skip that one.
