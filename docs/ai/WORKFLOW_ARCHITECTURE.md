---
Status: normative
Authority: process architecture
Canonical: no
Last verified: 2026-04-15
---

# ICN Agent Workflow Architecture

This document explains the four-document architecture that governs agent reasoning and session management in the ICN project. Future agents and contributors should read this first to understand which file to consult for which purpose.

## Document Roles

| Purpose | File | When edited |
|---------|------|-------------|
| Stable reasoning rules | `docs/ai/ICN_CONSTITUTIONAL_CORE.md` | Rarely. Changes are governance-level edits requiring explicit ratification. |
| Refreshable project state | `docs/ai/ICN_LIVE_STATE_OVERLAY_TEMPLATE.md` | Template is stable. Agent fills it fresh each session from canonical docs + latest handoff. |
| Per-session grounding | `docs/ai/ICN_SESSION_FRAME_TEMPLATE.md` | Template is stable. Agent fills it before non-trivial work each session. |
| End-of-session record | `docs/dev/HANDOFF_TEMPLATE.md` | Template is stable. Agent writes a concrete handoff at session end. |

## How They Relate

```
Constitutional Core (stable reasoning foundation)
        │
        ▼
Live State Overlay (session-refreshed from canonical docs)
        │
        ▼
Session Frame (filled before non-trivial work)
        │
        ▼
    [ Work ]
        │
        ▼
Handoff (written at session end, becomes input to next session's overlay)
```

## Key Rules

1. **Constitutional Core is not restated elsewhere.** Skills, commands, project_rules, and agent definitions link to it. They do not copy its language.

2. **Live State Overlay is refreshed, not cached.** The template tells the agent which files to load. The loaded content — not the template itself — is the live state.

3. **Session Frame is mandatory for non-trivial work.** Trivial = single-file typo fix. Everything else gets a frame.

4. **Handoffs use truth-plane labeling.** Each section identifies whether its content is verified fact, execution state, open work, or an unsafe assumption.

## Canonical State vs. This Architecture

This architecture governs agent workflow. It does not replace canonical state documents:

- `docs/STATE.md` — declared project state (canonical)
- `docs/PHASE_PROGRESS.md` — phase tracking (canonical)
- `CLAUDE.md` — root operational guidance (unchanged by this architecture)

The constitutional core supplements CLAUDE.md; it does not replace it.

## Edit Classification

Any edit to these workflow documents falls into one of two categories:

- **Process edit**: Changes how agents work (template structure, loading order, required steps). Normal PR review.
- **Governance edit**: Changes what agents believe about project truth (phase meaning, completion criteria, constitutional principles). Must be surfaced explicitly and ratified separately.
