---
Status: template
Authority: agent process
Canonical: no
Last verified: 2026-08-19
---

# ICN Session Frame Template

A short grounding record for non-trivial work. It should take minutes, not become a planning artifact of its own.

The frame records **what was checked before mutation**. It is not a truth source and is not carried forward as current state.

```markdown
## Session Frame

**Task**
<one sentence>

**Scope boundary**
- In scope: <paths/behavior/outcome>
- Out of scope: <adjacent work intentionally excluded>

**Checkout**
- repo root: <git rev-parse --show-toplevel>
- branch: <git branch --show-current>
- HEAD: <git rev-parse HEAD>
- working tree: <clean / concise dirty-state note>
- origin/main observed: <sha, if freshness matters>

**Truth domains resolved**
| Claim/domain needed | Owner consulted | Freshness/evidence |
|---|---|---|
| <e.g. identity semantics> | <resolved from ops/state/truth/sources.json> | <what was read/verified> |

**Live execution state checked**
- issue/control surface: <live query or not applicable>
- PR/reviews/checks: <live query or not applicable>

**Implementation evidence checked**
- <smallest code/test/schema evidence proving the current gap>

**Path context**
- Agent Context Spine brief: <paths or not needed>
- scoped instructions loaded: <paths>

**Invariant / compatibility risk**
- <which AGENTS.md invariant or compatibility surface could be affected>

**Main uncertainty**
- <the most important thing not yet proven>

**Bounded plan**
1. <smallest mutation>
2. <verification>
3. <stop boundary>

**Verification**
- <commands/evidence appropriate to the touched surface>

**Authorization boundary**
- [ ] analysis/review only
- [ ] code/docs mutation authorized
- [ ] merge authorized separately
- [ ] deploy/release/migration authorized separately
```

## Rules

- Do not fill a "current canonical phase" field unless the task actually depends on a domain that owns such a concept.
- Do not load `docs/STATE.md`, `docs/PHASE_PROGRESS.md`, or the latest handoff by default. Load them only when they are relevant evidence for the question being asked.
- Resolve current PR/CI/issue state live.
- If implementation evidence and a registered semantic owner conflict, write the conflict into the frame before editing.
- Update the frame if the premise changes materially during the session. Do not preserve a disproven starting assumption for appearances.
