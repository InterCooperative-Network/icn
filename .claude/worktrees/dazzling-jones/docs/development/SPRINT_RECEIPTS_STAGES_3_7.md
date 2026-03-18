# Sprint Receipts: Stages 3–7 (Compute Substrate Vertical Slice)

**Branch:** `feat/compute-substrate-e1-e5`  
**Date:** 2026-02-11  
**Status:** ✅ Complete

---

## Summary

This sprint transformed ICN from a "governance system that runs" to a **coordination protocol that can prove it converges across nodes**.

| Metric | Before | After |
|--------|--------|-------|
| Governance codepaths | 2 (legacy + effect) | 1 (effect only) |
| Legacy handlers | 5,361 lines | **DELETED** |
| Firewall governance refs | 44 | 3 |
| Service-backed domains | 1 | 5 |
| Effect surface contract | implicit | **locked (16 approved)** |
| Federation determinism | unproven | **proven (6 tests)** |

---

## What Was Accomplished

### Stage 3–4: Legacy Deletion + Effect Path Canonicalization

- **Deleted `governance_handlers/`** (5,361 lines)
- Removed `ICN_USE_EFFECT_PATH` env gate
- Effect path is now the **only** execution route
- Added tripwire tests that fail if legacy returns

**Proof:** `tripwire_legacy_governance_handlers_deleted` passes

### Stage 5: Service Wiring + Explicit Failures

- **Wired MembershipService** into production lifecycle
- Fixed Protocol executor "lying success" → explicit failure for unimplemented effects
- Updated tripwire coverage: 6 → 13 complete effects

**Proof:** All translator-emitted effects now either execute or fail explicitly

### Stage 6: Emitted Surface Contract + Integration Tests

- Created `emitted_surface_contract.rs` - golden list of 16 approved effects
- Fixed `Treasury::CreateBudget` provenance propagation
- Added `effect_group_integration.rs` with 10 E2E provenance tests

**Proof:** New emitted effect = test failure until executor/service/test added

### Stage 7: Two-Node Federation Determinism

- Added effect serialization roundtrip tests (9 tests)
- Created `StateHashSet` + verification utilities (15 tests)
- Created `federated_two_node_pilot.rs` proving cross-node determinism (6 tests)
- Added two-phase reload durability test

**Proof:** Same decision → same state hash across independent nodes

---

## Service Coverage

| Service | Trait | Impl | Reload Test | Wired |
|---------|-------|------|-------------|-------|
| LedgerService | ✅ | ✅ | ✅ | ✅ |
| FederationService | ✅ | ✅ | ✅ | ✅ |
| MembershipService | ✅ | ✅ | ✅ | ✅ |
| ControlService | ✅ | ✅ | N/A | ✅ |
| ProtocolParameterStore | ✅ | ✅ | ✅ | ✅ |

---

## Effect Coverage (Translator-Emitted)

| Effect | Status |
|--------|--------|
| Treasury::Spend | ✅ Implemented |
| Treasury::CreateBudget | ✅ Implemented |
| Membership::Add/Remove/Freeze/Unfreeze | ✅ Implemented |
| Protocol::SetParameter/SetGovernanceConfig | ✅ Implemented |
| Protocol::Upgrade/SetSchedulingPolicy | ⚠️ Explicit fail |
| Control::Veto/ForceClose/TextResolution | ✅ Implemented |
| Federation::Join/Vouch | ✅ Implemented |
| NoOp | ✅ Handled |

**13 implemented, 2 explicit fail, 0 silent success**

---

## Key Invariants Proven

### 1. Deterministic Governance Pipeline
```
Decision → Effect → Executor → Service → Durable State
```
Proven by: `pilot_chain_demo.sh`, `effect_group_integration.rs`

### 2. No Lying Success
Every unimplemented effect returns `ExecutionOutcome::Failed` with reason.

Proven by: Protocol executor explicit failure paths

### 3. Federation Convergence
Same inputs → same state hash across independent nodes.

Proven by: `federated_two_node_pilot.rs` (6 tests)

### 4. Reload Durability
Both nodes' state survives restart, hashes still match.

Proven by: `test_two_node_federation_reload_durability`

---

## Roadmap Impact

| ICN Goal | How This Sprint Advances It |
|----------|----------------------------|
| **Verifiable legitimacy** | Decision receipts → ledger entries with provenance |
| **Federation without consolidation** | Two-node determinism proves coordination without shared DB |
| **Trust-native security** | Fewer hidden paths, more machine-checkable invariants |
| **Enforceable economics** | Treasury operations bound to governance decisions |
| **Protocol scalability** | Locked surface contract enables safe growth |

---

## Test Summary

| Test Suite | Count | Status |
|------------|-------|--------|
| effect_path_tripwire | 5 | ✅ |
| emitted_surface_contract | 5 | ✅ |
| effect_group_integration | 10 | ✅ |
| federated_two_node_pilot | 6 | ✅ |
| state_hash module | 15 | ✅ |
| effects serialization | 9 | ✅ |
| meaning_firewall | 12 | ✅ |
| pilot_chain_demo.sh | 1 | ✅ EXIT=0 |

---

## Commits (this sprint)

```
19e4a5be test(s7): add two-node reload durability test
f679825c test(s7): add two-node federated pilot test (5 tests)
5e8d7eea feat(s7): add effect serialization + state hash verification utilities
8e92f68c test(s6): add effect group integration tests (10 tests)
84a51599 feat(s6): emitted surface contract + CreateBudget fix + Protocol decision
1f8aa990 feat: wire MembershipService + Protocol explicit failures + update tripwires
6596958f wire MembershipService into lifecycle
75701b07 delete legacy
f6453a42 feat(services): add MembershipService + executor + runtime tripwire
2abc1d55 feat(effects): wire effect dispatcher into production lifecycle
9cc6c1f9 feat(control): add ControlService trait + executor + tripwire tests
72e45b37 test(federation): add two-phase reload durability tests
99d45e78 feat(federation): implement FederationService with durable state
```

---

## What's Next

Stage 8 candidates:
1. **Dispute layer** - Implement DisputeService for conflict resolution
2. **Resource/commons economics** - Grant/revoke resource access
3. **Gateway boundary tightening** - Reduce domain imports in gateway
4. **Pilot app UX layer** - First cooperative pilot application
5. **Multi-node gossip federation** - Actual network propagation (not simulated)

The execution engine is no longer the risk. The substrate is pilot-ready.
