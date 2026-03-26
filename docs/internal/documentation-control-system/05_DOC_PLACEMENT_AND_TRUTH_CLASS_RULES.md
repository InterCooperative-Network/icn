# Doc Placement and Truth-Class Rules

This file condenses the strongest parts of the existing `DOCUMENTATION_MAINTENANCE.md` into a simpler operating reference.

## Root-level docs allowed in `docs/`

Only durable, project-wide entry docs should live directly under `docs/`.

Recommended:
- `README.md`
- `INDEX.md`
- `ARCHITECTURE.md`
- `GETTING_STARTED.md`
- `STATE.md`
- `TODO.md`
- `glossary.md`
- docs-system spec
- registry references

---

## Directory rules

| Directory | What belongs there |
|---|---|
| `docs/architecture/` | Architecture deep dives, invariants, boundary docs |
| `docs/design/` | Designs and proposals not yet frozen |
| `docs/spec/` | Exact behavioral contracts |
| `docs/planning/` or `docs/plans/` | Sequencing, milestones, tranche plans |
| `docs/strategy/` | Vision, roadmap strategy, operability framing |
| `docs/guides/` | User/developer/operator guides |
| `docs/deployment/` | Deployment procedures and environment guidance |
| `docs/security/` | Security architecture, hardening, threat models |
| `docs/status/` | Dated or current status notes |
| `docs/development/sessions/` | Session archaeology and work summaries |
| `docs/archive/YYYY/` | Historical / superseded docs |
| `docs/internal/` | Internal planning not yet canon |

---

## Truth header recommendation

Each important doc should declare something like:

```yaml
---
Status: normative
Canonical: yes
Last Reviewed: 2026-03-25
Role: architecture
---
```

Or, if frontmatter is not desired, a top-of-file metadata block with the same fields.

---

## Truth-class rules

### normative
How the system should work.

### descriptive
What currently exists.

### operational
How to run or verify it.

### historical
Preserved context only.

### draft
Proposal not yet frozen.

---

## Supersession rule

When a doc is replaced:
- do not silently leave it in place
- mark it superseded
- link the replacement
- preserve historical value when useful

