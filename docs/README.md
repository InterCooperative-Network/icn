# ICN Documentation

This directory contains comprehensive documentation for the ICN project.

**→ Start here: [Documentation Index (INDEX.md)](INDEX.md)** - Complete navigation to all documentation.

## Quick Links

- **[Getting Started](GETTING_STARTED.md)** - Quick start for new developers
- **[Architecture Overview](ARCHITECTURE.md)** - System architecture (comprehensive)
- **[Developer Guides](guides/developer/README.md)** - Development guides
- **[Operations Guides](guides/operations/README.md)** - Deployment and ops
- **[Security Documentation](security/FINAL_SECURITY_STATUS.md)** - Security overview
- **[Current Status](status/PROJECT_STATE_2026-02-09.md)** - Project state snapshot

## Directory Structure

```
docs/
├── INDEX.md                    # 📋 Complete documentation index (START HERE)
├── README.md                   # This file
├── ARCHITECTURE.md             # Comprehensive architecture (160KB+)
├── GETTING_STARTED.md          # Quick start guide
├── PHASE_HISTORY.md            # Development phase history
├── STATE.md                    # Current project state
├── TODO.md                     # Active work items
├── glossary.md                 # Terminology definitions
│
├── architecture/               # 🏗️ Architecture & design decisions
├── design/                     # 📐 Feature designs & proposals
│   ├── economics/             # Economic system design
│   ├── governance/            # Governance system design
│   └── sdis/                  # SDIS design docs
├── spec/                       # 📜 Formal specifications
│
├── reference/                  # 📖 Technical references
│   ├── api/                   # API documentation
│   └── config/                # Configuration references
│
├── guides/                     # 📚 User & developer guides
│   ├── developer/             # Development guides
│   ├── operations/            # Operations guides
│   └── user/                  # End-user guides
│
├── development/                # 🛠️ Development resources
│   ├── sprints/               # Sprint planning & tracking
│   └── testing/               # Testing documentation
│
├── security/                   # 🔒 Security & threat models
├── sdis/                       # 🆔 SDIS documentation
│
├── internal/                   # 🔬 Internal planning
│   ├── pilots/                # Pilot program docs
│   └── status/                # Status tracking
├── status/                     # 📊 Current status reports
│
├── onboarding/                 # 🎓 Contributor onboarding
├── adr/                        # 📝 Architecture Decision Records
├── templates/                  # 📄 Documentation templates
├── vision/                     # 🌟 Vision documents
│
├── deployment/                 # 🚀 Deployment configs
├── operations/                 # 🔧 Operational procedures
├── ops/                        # 📟 Runbooks & procedures
├── performance/                # ⚡ Performance docs
├── observability/              # 📈 Metrics & monitoring
├── mobile/                     # 📱 Mobile app docs
│
├── archive/                    # 📦 Historical documentation
│   └── 2025/                  # Year-based archives
│
├── api/                        # Legacy API docs
├── ci/                         # CI/CD documentation
├── demo/                       # Demo session notes
├── examples/                   # Examples & policies
├── features/                   # Feature-specific docs
├── migration-guides/           # Migration guides
├── phases/                     # Phase planning docs
├── pilots/                     # Pilot documentation
├── sprints/                    # Sprint documentation
└── testing/                    # Testing guides
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

### Development Sessions
The [development/sessions/](./development/sessions/) directory contains narrative entries documenting:
- What was built in each phase
- Technical decisions and rationale
- Problems encountered and solutions
- Test results
- Reflections and learnings

**Format:** One entry per significant milestone (phase completion, major feature, etc.)

**Example:** `2025-11-10-phase-0-bootstrap.md`

### Architecture Decision Records
The [adr/](./adr/) directory contains architecture decision records for specific architectural choices.

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
3. **After milestone:** Create a development session entry documenting what was built

### For Users
1. **Understanding the system:** Start with [ARCHITECTURE.md](./ARCHITECTURE.md)
2. **Learning the history:** Read development session entries chronologically
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
