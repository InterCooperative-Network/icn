---
Status: template
Authority: process
Canonical: no
Last verified: 2026-04-15
---

# ICN Session Frame Template

Fill this frame before beginning non-trivial work. Do not code until it is complete.

Trivial work (single-file typo, one-line fix) may skip the frame. Everything else gets one.

---

```markdown
## Session Frame

**Task:**
<one-sentence statement of the actual task>

**Truth layers loaded:**
- Declared project truth: <docs/STATE.md, docs/PHASE_PROGRESS.md — loaded? current?>
- Implementation truth: <relevant code inspected? what was verified?>
- Execution truth: <latest handoff loaded? branch state checked?>
- Narrative truth: <strategy docs consulted if relevant?>

**Branch:** <current branch>

**Working tree:** <clean/dirty + notable uncommitted files>

**Current canonical phase:** <from docs/STATE.md and docs/PHASE_PROGRESS.md>

**Current execution target:** <what this branch/session is actually trying to land>

**Touched subsystem(s):** <governance / ledger / federation / compute / member surface / workflow / docs / etc.>

**Main risk:** <the most likely drift, architectural violation, or truth-plane mismatch>

**Smallest high-leverage next move:** <the move that improves closure without expanding scope>

**Verification commands:**
<exact commands to run before claiming success>

**Canonical-doc impact:**
- [ ] None
- [ ] Synchronization edit likely (aligning canon to verified reality)
- [ ] Governance edit proposal likely (changing project truth classification)

**Public-claim impact:**
- [ ] None
- [ ] Internal narrative only
- [ ] Public narrative must be updated
```

---

## Usage Notes

- The frame is for grounding, not ceremony. Keep it short.
- If truth layers conflict, name the conflict in the frame before proceeding.
- If the session discovers that the frame was wrong, update it — don't pretend it was right.
- The frame becomes part of the handoff record at session end.
