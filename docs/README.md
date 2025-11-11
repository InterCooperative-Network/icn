# ICN Documentation

This directory contains comprehensive documentation for the ICN project.

## Structure

```
docs/
├── ARCHITECTURE.md          # Comprehensive architectural design document
├── dev-journal/             # Development journal entries (chronological)
│   └── 2025-11-10-*.md     # Daily/milestone entries
├── decisions/               # Architecture Decision Records (ADRs)
│   └── NNN-title.md        # Individual decisions with context
└── README.md               # This file
```

## Documentation Types

### Architecture Document
[ARCHITECTURE.md](./ARCHITECTURE.md) is the **living master document** covering:
- Identity & key management
- Trust graph model
- Network transport
- Ledger design
- Contract execution (CCL)
- Gossip & sync protocols
- Data storage
- Security model
- Performance targets
- Operational considerations

**Status:** Living document, updated as decisions are made and implementation progresses.

### Development Journal
The [dev-journal/](./dev-journal/) directory contains narrative entries documenting:
- What was built in each phase
- Technical decisions and rationale
- Problems encountered and solutions
- Test results
- Reflections and learnings

**Format:** One entry per significant milestone (phase completion, major feature, etc.)

**Example:** `2025-11-10-phase-0-bootstrap.md`

### Architecture Decision Records (Future)
The [decisions/](./decisions/) directory will contain ADRs for specific architectural choices.

**Format:** Lightweight, template-based
```markdown
# NNN: Title

**Status:** Accepted | Superseded | Deprecated
**Date:** YYYY-MM-DD
**Deciders:** Names

## Context
What is the issue we're trying to solve?

## Decision
What did we decide?

## Consequences
What becomes easier or harder?

## Alternatives Considered
What other options did we evaluate?
```

## How to Use

### For Contributors
1. **Before implementing:** Read [ARCHITECTURE.md](./ARCHITECTURE.md) to understand design constraints
2. **During implementation:** Update relevant sections if decisions change
3. **After milestone:** Create dev journal entry documenting what was built

### For Users
1. **Understanding the system:** Start with [ARCHITECTURE.md](./ARCHITECTURE.md)
2. **Learning the history:** Read dev journal entries chronologically
3. **Understanding specific decisions:** Check ADRs (once we have them)

### For Reviewers
- Architecture doc provides context for PR reviews
- Dev journal explains "why" behind implementation choices
- ADRs capture alternatives considered

## Keeping Docs Current

**Principle:** Documentation is code.

- ✅ Update ARCHITECTURE.md when design changes
- ✅ Write journal entry when completing milestones
- ✅ Create ADR when making significant architectural decisions
- ❌ Don't let docs drift from implementation
- ❌ Don't create docs that duplicate code comments

## Contributing to Docs

1. **Architecture changes:** PR to ARCHITECTURE.md with rationale
2. **Journal entries:** Create new file, summarize milestone
3. **ADRs:** Use template, number sequentially

## Questions?

- File an issue: https://github.com/InterCooperative-Network/icn/issues
- Dev discussion: (TBD - Discord/Matrix/Forum)

---

**Document structure inspired by:**
- [Architecture Decision Records](https://adr.github.io/)
- [Docs as Code](https://www.writethedocs.org/guide/docs-as-code/)
- [Living Documentation](https://www.amazon.com/Living-Documentation-Cyrille-Martraire/dp/0134689321)
