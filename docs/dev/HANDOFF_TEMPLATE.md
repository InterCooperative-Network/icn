---
Status: template
Authority: process
Canonical: no
Last verified: 2026-04-15
---

# ICN Handoff Template

Use this template when writing session handoffs to `docs/dev/handoff-YYYY-MM-DD.md`.

Each section explicitly labels its truth type. Do not blend verified facts with assumptions.

---

```markdown
# Session Handoff — YYYY-MM-DD

## Session Goal
<one-sentence statement of what this session set out to accomplish>

## Decisive Test
<the institutional operability question this work is measured against>

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD
`<sha> <commit message>`

### Open PRs
| PR | Branch | State | CI Status | Blocker |
|----|--------|-------|-----------|---------|

### Branches
- `<branch>` — <status, head sha>

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. <change summary>
<what was done, which commits, which PRs>

### 2. <change summary>
...

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [ ] <description of unfinished work>
- [ ] <decision that needs follow-up>

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- <assumption about X — not confirmed because Y>
- <assumption about Z — believed true but not tested>

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. <first action>
2. <second action>
3. ...

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes (if any were made or ratified) -->

1. <decision and rationale>
2. ...

---

## Verification Commands
<!-- Commands the next session should run to confirm starting state -->

```bash
<exact commands>
```

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- Declared project truth: <loaded from STATE.md / PHASE_PROGRESS.md — current as of?>
- Implementation truth: <what was verified from code?>
- Execution truth: <branch/PR/CI state confirmed?>
- Narrative truth: <any strategy doc claims that may be stale?>
- Known conflicts: <any divergence between layers discovered this session?>
```

---

## Usage Notes

- Handoff files go in `docs/dev/handoff-YYYY-MM-DD.md`.
- If multiple handoffs occur on the same day, append a suffix: `handoff-YYYY-MM-DD-b.md`.
- Do NOT commit the handoff file automatically — the user decides.
- Keep notes concise. This is for the next session, not a PR description.
- The "Unsafe Assumptions" section is the most important section. If you skip one section, do not skip that one.
