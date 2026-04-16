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

Read `docs/STATE.md` (canonical declared project state).

Compare what changed in Step 1 against what the state doc says. Note:
- What the state doc claims is the current phase
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

Update `docs/STATE.md` directly (the canonical project state file).

**Label each update** per the edit classification in `docs/ai/ICN_CONSTITUTIONAL_CORE.md`:
- `[sync edit]` — aligning canon to verified reality
- `[governance edit proposal]` — changing project status classification (must be reviewed separately)

Include:
- Current phase
- What was completed this session
- Current blockers (updated list)
- Next recommended actions
- Open PRs and issues count

**Step 6: Report**

Present a summary:
```
## ICN State Sync — <date>

### What Changed This Session
- Crates modified: ...
- PRs merged: ...
- Issues closed: ...

### State Document
- Updated: docs/STATE.md
- Edit type: [sync edit] / [governance edit proposal]
- Phase: ...
- Status: ...

### Stale Docs
- docs/xxx.md — last updated Y days before latest crate change (flag)

### Missing ADRs
- Change: ... (needs ADR)

### Recommended Before Ending Session
1. ...
```
