# Module 14: Governance and CCL Deep Dive

## Overview
This module explains how ICN encodes cooperative governance and how CCL (the
Cooperative Contract Language) enables enforceable agreements. The focus is on
the "how" and "why" of policy creation, voting, and capability enforcement.

## Objectives
- Understand ICN's governance flow from proposal to enforcement
- Learn how CCL models rules and capabilities
- Understand how governance decisions affect runtime behavior
- Map governance and CCL concepts to the codebase

## Prerequisites
- Module 2 (Architecture Overview)
- Module 6 (Ledger and Contracts)

## Key Reading
- `icn/crates/icn-governance/` - Governance engine
- `icn/crates/icn-ccl/` - Contract language implementation
- `docs/governance-primitives.md` - Governance system reference
- `icn/crates/icn-governance/tests/governance_integration.rs`

---

## Walkthrough

### 1. Governance Lifecycle

Governance provides structured change without destabilizing trust. A typical
lifecycle includes:

1. **Proposal creation**: A member proposes a change
2. **Deliberation**: Members review and discuss
3. **Voting**: A decision is recorded
4. **Activation**: Policy is applied and enforced

Why it exists:
- Cooperative systems need transparent change management
- Governance ensures the system reflects human values

### 2. Policy Scopes and Enforcement

Policies can apply at different scopes (community, coop, federation). This
allows local governance while preserving interoperability.

### 3. CCL: The Cooperative Contract Language

CCL is deterministic and capability-based. It focuses on:

- Clear rule definitions
- Explicit effects on ledger and trust
- Capability limits that bound what contracts can do

### 4. Governance and CCL Together

Governance produces policies; CCL enforces them in executable form. This keeps
human decisions aligned with system behavior.

---

## Exercises

1. **Trace a proposal type**
   - Open `icn/crates/icn-governance/`
   - Identify the proposal types and what they change

2. **Read a governance integration test**
   - Open `icn/crates/icn-governance/tests/governance_integration.rs`
   - Summarize the test flow and what it validates

3. **Find capability checks**
   - Search `icn/crates/icn-ccl/` for capability enforcement
   - List the capabilities that affect ledger operations

4. **Map policy to runtime**
   - Identify where policy configuration is read by runtime components
   - Describe how changes propagate to nodes

---

## Checkpoints

- [ ] You can explain the governance lifecycle from proposal to activation
- [ ] You can describe how CCL models rules and capabilities
- [ ] You understand how governance decisions are enforced
- [ ] You can map key governance concepts to code locations

---

## Notes and gotchas

- Governance changes can affect security boundaries; audit carefully.
- Capability scopes should be minimal to avoid policy overreach.
- Determinism is critical for reproducible contract outcomes.
