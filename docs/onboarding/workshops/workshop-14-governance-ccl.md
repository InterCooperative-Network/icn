# Workshop 14: Governance and CCL Deep Dive

## Learning Objectives

By the end of this workshop, you will be able to:

1. **Trace governance flow** - Follow a proposal from creation to activation
2. **Read governance tests** - Understand integration test expectations
3. **Identify capability enforcement** - Locate CCL capability checks

## Goal
Connect governance concepts to the implementation and understand how CCL
enforces cooperative policy decisions.

## Prerequisites
- Completed [Module 14: Governance and CCL](../modules/module-14-governance-ccl-deep-dive.md)
- Familiarity with Module 6 (Ledger and Contracts)

## Estimated time
1-2 hours

## Related Materials
- `icn/crates/icn-governance/`
- `icn/crates/icn-ccl/`
- `docs/governance-primitives.md`

---

## Part 1: Proposal Lifecycle

### Steps
1. Open `icn/crates/icn-governance/`
2. Identify proposal types and their states
3. Map state transitions to governance actions

### Questions
1. Where are proposal status transitions enforced?
2. How is voting outcome recorded?

### Checkpoint
- [ ] You can describe proposal lifecycle states

---

## Part 2: Governance Integration Test

### Steps
1. Open `icn/crates/icn-governance/tests/governance_integration.rs`
2. Identify the setup, action, and assertion phases

### Questions
1. What is the test verifying about policy changes?
2. What dependencies are required for the test to run?

### Checkpoint
- [ ] You can summarize the governance integration test

---

## Part 3: CCL Capability Enforcement

### Steps
1. Search `icn/crates/icn-ccl/` for capability checks
2. Identify where ledger write permissions are validated

### Questions
1. Which capabilities can mutate ledger state?
2. How is capability scope limited?

### Checkpoint
- [ ] You can locate capability enforcement code

---

## Summary

After completing this workshop you should be able to:
- Explain how proposals move through governance
- Read and interpret governance integration tests
- Identify where CCL enforces capabilities

## Next steps
Revisit the manual sections on Governance and CCL for system-level context
