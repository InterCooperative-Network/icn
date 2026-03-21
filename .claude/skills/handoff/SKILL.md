---
name: handoff
description: Write a session summary to docs/dev-journal/ capturing branch state, decisions, open threads, and next steps.
argument-hint: "[--push]"
user-invocable: true
allowed-tools: "Bash, Read, Write, Grep"
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

Target: `docs/dev-journal/session-YYYY-MM-DD.md`

Where `YYYY-MM-DD` = today's date (`date +%Y-%m-%d`).

If a file for today already exists, append to it (don't overwrite).

Format:
```markdown
# Session Handoff — YYYY-MM-DD

## Branch
`<branch-name>`

## Commits this session
- <sha> <message>
- ...

## Open PRs
- #<N>: <title> (<state>)

## Open threads
- [ ] <description of unfinished work>
- [ ] <decision that needs follow-up>

## TODOs added
- `<file>:<line>` — <todo text>

## Uncommitted changes
- <file> (<status>)

## Next steps
1. <first thing to do next session>
2. ...

## Notes
<any decisions, trade-offs, or context worth preserving>
```

### 4. Clean up

- Remind: `git stash list` should be empty — stash or commit before ending
- If `$ARGUMENTS` includes `--push` AND working tree is clean: run `/push` to push the branch

### 5. Report

Print:
```
Handoff written to docs/dev-journal/session-YYYY-MM-DD.md
Branch: <name> | Commits: <n> | Open PRs: <n> | Open threads: <n>
```

## Important

- Do NOT commit the handoff file automatically. User decides.
- If `docs/dev-journal/` doesn't exist, create it.
- Keep notes concise — this is for the next session, not a full PR description.
