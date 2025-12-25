# ICN Issue Taxonomy & Triage System

This document defines the canonical issue management system for ICN. All contributors and agents MUST follow this contract when creating, editing, or reorganizing issues.

## Mission

Keep the issue tracker queryable and pilot-oriented by enforcing:
- One priority axis
- One issue type axis
- At least one domain axis
- Optional concern axes
- Controlled phase tags

---

## 1. Label Dimensions and Invariants

### A) Priority (REQUIRED, exactly one)

| Label | Meaning |
|-------|---------|
| `priority:critical` | Blocks pilot OR security invariant OR consensus correctness OR safe deploy |
| `priority:high` | Required for pilot success but not a hard blocker |
| `priority:medium` | Important, schedulable, not a pilot stopper |
| `priority:low` | Polish / convenience / future work |

**Invariant**: Every open issue MUST have exactly one `priority:*` label.

**Forbidden (deprecated)**: `critical`, `high`, `medium`, `low`, `P0-critical`, `P1-high`

### B) Issue Type (REQUIRED, exactly one)

Every issue MUST have exactly one of:
- `bug` - Something isn't working
- `enhancement` - New feature or request
- `design` - Architecture/design discussion
- `documentation` - Docs improvements
- `testing` - Test coverage improvements
- `refactor` - Code restructuring without behavior change

### C) Domain / Subsystem (REQUIRED for code work)

At least one domain label if the issue touches code:

| Domain | Scope |
|--------|-------|
| `core` | Runtime, supervisor, actor lifecycle |
| `identity` | DIDs, keypairs, keystore |
| `ledger` | Double-entry accounting, Merkle-DAG |
| `governance` | Proposals, voting, parameters |
| `treasury` | Budgets, spending rules |
| `gateway` | REST/WebSocket API |
| `federation` | Inter-coop protocols |
| `sdk` | TypeScript/React Native SDKs |
| `cli` | icnctl commands |
| `ccl` | Contract language |
| `gossip` | P2P message propagation |
| `networking` | QUIC/TLS, sessions |
| `protocol` | Wire formats, message types |
| `storage` | Sled, persistence |
| `web-ui` | Web interface |
| `k8s` | Kubernetes manifests/config |
| `ci` | GitHub Actions, pipelines |
| `release` | Signing, SBOMs, artifacts |
| `infrastructure` | General ops/platform |

**Invariant**: Every code-impacting issue MUST have >=1 domain label.

### D) Concern / Motivation (OPTIONAL, many allowed)

Use only when materially relevant:
- `security` - Security implications
- `performance` - Performance impact
- `scalability` - Scaling considerations
- `observability` - Metrics, tracing, logging
- `compliance` - Regulatory/policy
- `error-handling` - Error paths
- `integration` - Cross-boundary work
- `i18n` - Internationalization

**Rule**: Concerns answer "why/what risk," not "where."

### E) Lifecycle / Meta (OPTIONAL)

- `duplicate` - Already exists elsewhere
- `invalid` - Not a real issue
- `wontfix` - Won't be addressed
- `help wanted` - Extra attention needed
- `good first issue` - Good for newcomers

### F) Phase Tags (OPTIONAL, controlled)

- `phase-21` (or other explicitly defined phase tags)

**Rule**: Phase tags are only for roadmap grouping. Never use them as priority.

---

## 2. Label Normalization Rules

When reorganizing, normalize labels per this mapping:

### Priority Merge Mapping
| Deprecated | Canonical |
|------------|-----------|
| `critical` | `priority:critical` |
| `P0-critical` | `priority:critical` |
| `high`, `P1-high` | `priority:high` |
| `medium` | `priority:medium` |
| `low` | `priority:low` |

### Other Merges
| Deprecated | Canonical |
|------------|-----------|
| `tech-debt` | `refactor` |
| `technical-debt` | `refactor` |
| `monitoring` | `observability` |
| `ops` | `infrastructure` |

---

## 3. Issue Title Standard

Use consistent prefixes:

```
<type>(<domain>): <action>
```

Examples:
- `feat(ledger): Enforce credit limits server-side`
- `fix(gossip): Remove blocking operations from async path`
- `refactor(governance): Extract shared validation logic`
- `docs(ops): Add troubleshooting runbooks`
- `test(identity): Add rotation integration tests`

**Rule**: The `<type>` prefix MUST match the issue type label.

---

## 4. Issue Hierarchy (Three Levels Only)

### Level 0 — Meta / Roadmap (RARE)
- Explains why work exists
- Very few (≤5 total)
- Never directly worked
- Contains links to Level 1 epics

### Level 1 — Execution Epics (PRIMARY CONTROL SURFACE)
- Represents a coherent system capability
- Completable in ≤1–2 sprints
- Owns a checklist of Level 2 sub-issues
- No code merged directly for the epic

### Level 2 — Atomic Work Items (WHERE WORK HAPPENS)
- Single responsibility
- Limited surface area
- Independently completable and reviewable
- Produces concrete artifact (code, test, doc)
- MUST belong to exactly one execution epic
- MUST NOT contain further sub-tasks

---

## 5. Epic Structure

Every execution epic should have sub-issues in these categories (where applicable):

| Category | Purpose |
|----------|---------|
| **Core Logic** | Primary functionality, data structures, state machines |
| **Validation / Safety** | Server-side enforcement, invariant checks |
| **Integration** | Wiring to gateway/SDK/other subsystems |
| **Testing** | Integration tests, property tests |
| **Observability** | Metrics, logging, alerts (if applicable) |
| **Docs** | API docs, architecture notes (if externally visible) |

**Limit**: No epic may have more than ~10 sub-issues. If it does, split the epic.

---

## 6. Epic / Duplicate Handling

### Superseding an Epic
When a new epic supersedes an old one:
1. Add `duplicate` label to old issue
2. Comment: "Superseded by #XYZ"
3. Close the old issue

### Duplicate Issues
When two issues describe the same work:
1. Keep the newer or more detailed one as canonical
2. Add `duplicate` to the non-canonical
3. Cross-link and close

---

## 7. Agent Behavior Rules

### When Creating Issues
1. Search existing issues for semantic duplicates first
2. If duplicate exists, comment + link instead of creating
3. If creating:
   - Apply exactly one `priority:*`
   - Apply exactly one type label
   - Apply >=1 domain label
   - Add concerns only if meaningful
4. If "future idea," set `priority:low` and label `design`

### When Reorganizing
Follow this order:

**Pass 1 — Priority normalization**
- Ensure exactly one `priority:*` per issue
- Migrate and remove deprecated priority labels

**Pass 2 — Type enforcement**
- Ensure exactly one type label per issue

**Pass 3 — Domain assignment**
- Ensure >=1 domain label for code issues

**Pass 4 — Concern cleanup**
- Apply concerns only when supported by content
- Merge duplicates per mapping rules

**Pass 5 — Epic consolidation**
- Identify duplicate epics
- Supersede/close older ones

**Pass 6 — Audit report**
Output a report with:
- Counts by priority
- Issues missing type/domain labels
- Deprecated labels still in use
- Suspected duplicates needing human confirmation

### Non-Negotiables
- No creation of new labels without explicit human approval
- Never change issue numbers or delete history
- Never downgrade `priority:critical` without stating reason in comment
- Don't relabel closed issues unless marking duplicate/invalid/wontfix

---

## 8. Quick Reference

Every issue MUST have:
- Exactly one `priority:*`
- Exactly one type label
- At least one domain label (if it touches code)

Everything else is optional.

---

## 9. Execution Epics Template

When creating an execution epic, use this template:

```markdown
## Summary
[1-2 sentence description of the capability]

## Scope
[What's included and what's explicitly out of scope]

## Sub-Issues
- [ ] feat(domain): Core logic description
- [ ] feat(domain): Integration with X
- [ ] test(domain): Integration tests
- [ ] docs(domain): API documentation (if applicable)

## Acceptance Criteria
- [ ] Criterion 1
- [ ] Criterion 2

## Dependencies
- Depends on: #XXX (if any)
- Blocks: #YYY (if any)
```
