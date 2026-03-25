---
description: Sync sprint state, docs, and ops/mcp after a work session — update STATE.md, flag stale docs, note what changed
allowed-tools: Read, Write, Edit, Grep, Glob, Bash(git:*, cargo metadata:*)
---

Sync the ICN project state documents after a work session. This command keeps STATE.md, sprint docs, and ADRs aligned with what actually changed in code.

**Input:** Optional — the user can describe what changed this session. If not provided, use git log to discover changes.

**Step 1: Discover what changed**

```bash
# In the ICN repo
git log --oneline -20
git diff --name-only HEAD~5..HEAD
git status --short
```

Identify:
- Which crates were modified
- Which PRs were merged
- Which issues were closed
- Any new files added (new crates, new docs, new migrations)

**Step 2: Check the current state document**

Read the most recent state file in `~/.claude_launchpad/projects/icn/` (e.g. `icn-state-2026-03-21.md`).

Compare what changed in Step 1 against what the state doc says. Note:
- What the state doc claims is the current sprint (e.g. Sprint 19)
- What the state doc says is blocked
- Whether any blockers were resolved this session
- Whether new blockers emerged

**Step 3: Check for stale docs**

For each crate that was modified, check if its documentation is current:
```bash
git log --oneline docs/ | head -5
git log --oneline crates/<modified-crate>/ | head -5
```

If the docs were last updated more than 2 weeks before the crate code, flag as stale.

**Step 4: Check for missing ADRs**

For each significant architectural change (new crate, changed public trait, new message type, modified wire format), check if an ADR exists:
```bash
ls docs/architecture/ | grep adr
```

If a change needed an ADR and doesn't have one, flag it.

**Step 5: Update state document**

Create or update the state entry. Write a new state file at:
`~/.claude_launchpad/projects/icn/icn-state-<today>.md`

Include:
- Sprint/phase
- Cluster state (pull from icn_icn_status if available)
- What was completed this session
- Current blockers (updated list)
- Next recommended actions
- Open PRs and issues count
- Demo flow readiness

**Step 6: Flag ops/mcp drift**

Check if ops/mcp has uncommitted changes:
```bash
ssh icn-dev "cd ~/projects/icn/ops/mcp && git status --short" 2>/dev/null
```

If yes, list the files and recommend committing before ending the session.

**Step 7: Report**

Present a summary:
```
## ICN State Sync — <date>

### What Changed This Session
- Crates modified: ...
- PRs merged: ...
- Issues closed: ...

### State Document
- Updated: ~/.claude_launchpad/projects/icn/icn-state-<date>.md
- Sprint: Sprint X
- Status: ...

### Stale Docs
- docs/xxx.md — last updated Y days before latest crate change (flag)

### Missing ADRs
- Change: ... (needs ADR)

### ops/mcp
- Status: CLEAN / DIRTY (X files uncommitted)

### Recommended Before Ending Session
1. ...
```
