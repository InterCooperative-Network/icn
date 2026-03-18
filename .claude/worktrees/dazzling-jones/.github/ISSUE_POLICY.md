# ICN Issue Taxonomy

This document defines the issue management system for ICN. All contributors and agents MUST follow this contract.

---

## 1. Label System (19 labels total)

Every issue gets **exactly one epic + exactly one type**. Trust issues also get **exactly one tier**. That's it.

### A) Epic (REQUIRED, exactly one)

| Label | Scope |
|-------|-------|
| `epic:kernel-separation` | Kernel/App boundary extraction (#856) |
| `epic:arch-invariants` | Architecture invariant enforcement, waves 1-6 |
| `epic:trust-hardening` | Trust correctness, observability, performance |
| `epic:service-discovery` | Service discovery pipeline |
| `epic:commons-compute` | Commons resource pool and compute |
| `epic:kernel-perf` | Kernel performance and security |

**Invariant**: Every open issue MUST have exactly one `epic:*` label.

### B) Type (REQUIRED, exactly one)

| Label | Meaning |
|-------|---------|
| `type:spec` | Design/specification work |
| `type:impl` | Implementation (new feature or change) |
| `type:refactor` | Structural change, behavior preserved |
| `type:test` | Test coverage |
| `type:doc` | Documentation |

**Invariant**: Every open issue MUST have exactly one `type:*` label.

### C) Tier (REQUIRED for trust, exactly one)

Only for issues with `epic:trust-hardening`:

| Label | Meaning |
|-------|---------|
| `tier:1-correctness` | Correctness fixes — do first |
| `tier:2-observability` | Observability — after correctness |
| `tier:3-perf` | Performance/testing — last |

**Invariant**: Trust issues MUST have exactly one `tier:*`. Non-trust issues MUST NOT.

### D) System Labels (do not assign manually)

| Label | Used by |
|-------|---------|
| `bug` | True defects only |
| `duplicate` | Closure hygiene |
| `priority:critical` | Emergency: blocks correctness/security/trunk |
| `dependencies` | Dependabot PRs (automated) |
| `perf-regression-ok` | CI benchmark gate acknowledgment |

### E) Dependencies

Dependencies go in the **issue body**, not labels:

```markdown
## Depends on
- [ ] #1007
- [ ] #856
```

---

## 2. Issue Title Standard

```
<type>(<domain>): <action>
```

Examples:
- `feat(ledger): Enforce credit limits server-side`
- `fix(gossip): Remove blocking operations from async path`
- `refactor(governance): Extract shared validation logic`
- `test(identity): Add rotation integration tests`

---

## 3. Definition of "Triaged"

An issue is triaged when it has:
- [ ] Exactly one `epic:*`
- [ ] Exactly one `type:*`
- [ ] If `epic:trust-hardening`, exactly one `tier:*`
- [ ] Dependencies in body checklist (not labels)

---

## 4. Issue Hierarchy (Three Levels)

### Level 0 — Epics / Umbrellas
- Explains why work exists
- Contains ratchets and acceptance criteria
- Never directly worked

### Level 1 — Work Items
- Single responsibility
- Independently completable
- Produces concrete artifact
- MUST belong to exactly one epic

### Level 2 — Sub-tasks (inside issue body only)
- Expressed as checklist items in the parent issue body
- NOT separate issues unless they grow large enough

---

## 5. Agent Behavior Rules

### When Creating Issues
1. Search existing issues for duplicates first
2. Apply exactly one `epic:*` and one `type:*`
3. Add `tier:*` if trust epic
4. No new labels without explicit human approval

### When Closing
- Duplicates: comment "Superseded by #XYZ", add `duplicate`, close
- Merges: copy acceptance criteria into keeper issue as checklist, close with link

### Non-Negotiables
- No creation of new labels without human approval
- Never change issue numbers or delete history
- `priority:critical` burns off quickly or gets decomposed
