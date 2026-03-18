# Contributing to ICN

## Architectural Guardrails

### 1. Interfaces First

Before extracting domain logic:

1.  Introduce trait boundaries.
2.  Land a minimal abstraction PR.
3.  *Then* move the implementation.

**Guideline**: No extraction PR should introduce new interfaces and move 40 files at once.

### 2. No Drive-By Refactors

Extraction PRs must:
*   Move code
*   Adjust wiring
*   Fix compilation
*   Add minimal tests

They do **NOT**:
*   Rename unrelated types
*   Redesign logic
*   Improve style

**Principle**: Stability > Elegance.

### 3. The Meaning Firewall

*   **Kernel enforces**: Identity primitives, trust mechanisms, state primitives, auditability, resource accounting.
*   **Apps define**: Governance, membership semantics, economics, federation agreements.

**Goal**: No domain language in the kernel.

---

## Issue Labels

Every issue gets **exactly one `epic:*` + exactly one `type:*`**. Trust issues also get one `tier:*`. No exceptions.

See **[.github/ISSUE_POLICY.md](.github/ISSUE_POLICY.md)** for the full label system, triage rules, and agent behavior contract.
