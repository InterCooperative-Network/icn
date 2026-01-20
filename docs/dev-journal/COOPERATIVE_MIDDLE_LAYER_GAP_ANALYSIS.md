# Cooperative Middle Layer: Gap Analysis

---
**Status**: RECONCILED
**Created**: 2025-12-23
**Updated**: 2026-01-20
**Authors**: fahertym, Claude Code
**Decision Makers**: TBD (pending review)

---

**Related**:
- [ICN_SDIS_INTEGRATED_VISION.md](ICN_SDIS_INTEGRATED_VISION.md)
- [ROADMAP.md](ROADMAP.md) - Reconciled roadmap with this vision
- [STRATEGIC_ALIGNMENT_AUDIT_2026-01-20.md](STRATEGIC_ALIGNMENT_AUDIT_2026-01-20.md) - Audit that reconciled documents

## Executive Summary

This document identifies gaps between ICN's current implementation and the "Cooperative Middle Layer" vision where ICN is itself cooperatively governed infrastructure.

**2026-01-20 Update**: A strategic alignment audit found that implementation is significantly more advanced than this document originally stated. The gap analysis below has been updated to reflect actual implementation status.

### Gap Categories

| Category | Current State | Vision State | Priority | Status |
|----------|--------------|--------------|----------|--------|
| **Recursive Entity Model** | ✅ Implemented (icn-entity) | Unified CooperativeEntity | CRITICAL | **INTEGRATION ONLY** |
| **Protocol Governance** | Hardcoded parameters | Democratically adjustable | CRITICAL | Needs Phase 22 |
| **SDIS Integration** | Phase S1-S6 complete | Full anchor-based identity | HIGH | Needs Phase 24 |
| **Inter-Coop Economics** | Treasury + basic ledger | Clearing agreements, group purchasing | HIGH | Needs Phase 25 |
| **Federation Hierarchy** | ✅ 90% implemented | Recursive federation | MEDIUM | Needs Phase 28 |
| **Subsidiarity** | Not implemented | Decision scoping by level | MEDIUM | Needs Phase 28 |

---

## Gap 1: Unified CooperativeEntity Model

**Status**: ✅ **IMPLEMENTED** (Integration remaining)

### Current State (2026-01-20 Update)

**The entity model is fully implemented:**
- `icn-entity`: 4,624 LOC, 72 tests, **100% complete**
- `icn-coop`: 4,125 LOC, 93 tests, **95% complete** (not spawned in supervisor)
- `icn-governance`: 25,438 LOC, 347 tests, entity-scoped proposals
- `icn-federation`: 14,732 LOC, 101 tests, multi-level federation

**What's implemented:**
- `CooperativeEntity` type with EntityId (`entity:icn:<type>:<identifier>`)
- Entity types: Individual, Cooperative, Federation, Community
- Recursive membership tracking
- Entity-scoped governance
- SledEntityRegistry for persistence

### Remaining Work (Phase 19)

| Task | Status | Effort |
|------|--------|--------|
| Spawn CoopActor in supervisor | ❌ Not done | 8 hours |
| Add entity gossip topic | ❌ Not done | 4 hours |
| Wire SledEntityRegistry | ❌ Not done | 4 hours |
| Update gateway endpoints | ⚠️ Partial | 8 hours |

**Total remaining: ~2 weeks** (not 6-8 weeks)

### Tracking Issue

See GitHub Issue #738 for the integration work.

---

## Gap 2: Protocol Governance (ICN Governs Itself)

### Current State
- Protocol parameters hardcoded in configuration
- No mechanism for democratic parameter changes
- No concept of "Protocol Commons" as an entity

### Vision State
- ICN Protocol Commons is a CooperativeEntity at the top level
- Protocol parameters are governable via proposals
- Different levels can customize within constraints

### Gap Analysis

| Aspect | Current | Needed |
|--------|---------|--------|
| Parameter storage | Config files, code constants | Governance-controlled store |
| Change mechanism | Code deployment | Proposal → vote → apply |
| Scope separation | None | Clear which level controls what |
| Constraints | None | Higher levels constrain lower |

### Implementation Path

1. **Define ProtocolParameter type** with scope, constraints
2. **Create protocol governance domain** in icn-governance
3. **Build parameter store** that tracks changes via proposals
4. **Define immutable vs. mutable parameters**
5. **Implement constraint propagation** (higher constrains lower)

### Estimated Effort: 4-6 weeks

---

## Gap 3: SDIS Full Integration

### Current State (Phase S1-S6 Complete)
- Anchor-based identity structure defined
- KeyBundle with Ed25519 + ML-DSA hybrid signatures
- VUI commitment for uniqueness
- Basic credential types

### Vision State
- Sybil-resistant voting (ZK proofs)
- Cooperative enrollment ceremonies
- Steward network as cooperative
- Full L0-L3 credential ecosystem

### Gap Analysis

| Feature | Current | Needed |
|---------|---------|--------|
| Proof of Personhood | VUI structure defined | Steward verification flows |
| ZK Voting | Not implemented | STARK proofs for anonymous voting |
| Coop Anchors | Not implemented | Threshold ceremony for coop identity |
| Credential Ecosystem | Basic types | Full L2/L3 attestation flows |
| Steward Network | Not implemented | Stewards as cooperative entity |

### Implementation Path

1. **Implement ZK voting circuits** (Phase S7)
2. **Define cooperative anchor creation** ceremony
3. **Build steward network governance**
4. **Create credential presentation API**
5. **Integrate with icn-governance** for sybil-resistant voting

### Estimated Effort: 8-12 weeks

---

## Gap 4: Inter-Cooperative Economics

### Current State
- Individual mutual credit ledger
- Treasury management (budgets, spending rules)
- Basic credit policy

### Vision State
- Coop-to-coop agreements as first-class objects
- Bilateral clearing with netting
- Group purchasing coordination
- Federation-level settlement
- Contribution-based credit limits

### Gap Analysis

| Feature | Current | Needed |
|---------|---------|--------|
| Coop accounts | Via DID accounts | CooperativeEntity accounts |
| Agreements | Not implemented | InterCoopAgreement type |
| Clearing | Not implemented | Batch settlement, netting |
| Group purchasing | Not implemented | Pooled purchasing coordination |
| Contribution tracking | Partial | Full contribution ledger |
| Anti-extraction | Basic limits | Ratio limits, ramp periods |

### Implementation Path

1. **Define InterCoopAgreement** in icn-ledger
2. **Implement clearing house** for bilateral settlement
3. **Add contribution tracking** to credit policy
4. **Build group purchasing coordinator**
5. **Implement anti-extraction policies**

### Estimated Effort: 8-10 weeks

---

## Gap 5: Recursive Federation Hierarchy

### Current State
- Federation primitives exist (icn-federation types)
- Single-level federation support
- Trust bridging between federations

### Vision State
- Arbitrary depth federation hierarchy
- Meta-federations and global commons
- Trust propagation through hierarchy
- Settlement at each level
- Governance bubbling

### Gap Analysis

| Feature | Current | Needed |
|---------|---------|--------|
| Hierarchy depth | 1 level | Arbitrary recursion |
| Meta-federation | Not implemented | Federation of federations |
| Trust propagation | Direct only | Recursive calculation |
| Multi-level settlement | Not implemented | Netting at each level |
| Proposal bubbling | Not implemented | Escalation rules |

### Implementation Path

1. **Extend federation types** to support parent federation
2. **Implement recursive trust calculation**
3. **Build multi-level settlement** with netting
4. **Create proposal escalation rules**
5. **Define global commons governance**

### Estimated Effort: 6-8 weeks

---

## Gap 6: Subsidiarity Enforcement

### Current State
- No formal decision scoping
- All decisions at proposal level
- No constraint propagation

### Vision State
- Clear decision scopes (personal → global)
- Automatic scope detection for proposals
- Constraint propagation from higher levels
- Local autonomy within federation constraints

### Gap Analysis

| Feature | Current | Needed |
|---------|---------|--------|
| Decision scopes | Not defined | 5-level hierarchy |
| Scope detection | Manual | Automatic based on proposal type |
| Constraints | None | Higher-level constraints on lower |
| Override rules | None | Emergency escalation, appeals |

### Implementation Path

1. **Define DecisionScope enum** and scoping rules
2. **Add scope to ProposalType**
3. **Implement constraint store** per entity level
4. **Build scope enforcement** in governance
5. **Create override/appeal mechanism**

### Estimated Effort: 3-4 weeks

---

## Implementation Roadmap

**Note**: This roadmap has been reconciled with the main [ROADMAP.md](ROADMAP.md). Phase numbers below match the unified roadmap.

### Phase 19: Entity & Coop Integration (2 weeks) ← **NEXT**
- [x] CooperativeEntity core type (DONE - icn-entity)
- [x] Entity-scoped governance (DONE - icn-governance)
- [ ] Spawn CoopActor in supervisor
- [ ] Add entity gossip topic
- [ ] Wire SledEntityRegistry
- [ ] Update gateway APIs

**Tracking**: #738

### Phase 22: Protocol Governance (4 weeks)
- [ ] ProtocolParameter types
- [ ] Parameter governance domain
- [ ] Change application mechanism
- [ ] Constraint propagation

**Tracking**: #267

### Phase 24: SDIS Completion (6 weeks)
- [ ] ZK voting circuits
- [ ] Cooperative anchor ceremonies
- [ ] Steward network governance
- [ ] Full credential ecosystem

**Tracking**: #269

### Phase 25: Inter-Coop Economics (8 weeks)
- [ ] InterCoopAgreement implementation
- [ ] Clearing house for settlement
- [ ] Labor shares and cooperative bonds
- [ ] Contribution tracking
- [ ] Anti-extraction policies
- [ ] Group purchasing coordination

**Tracking**: #268, #386

### Phase 28: Recursive Federation & Subsidiarity (6 weeks)
- [ ] Multi-level hierarchy
- [ ] Recursive trust
- [ ] Multi-level settlement
- [ ] Proposal escalation
- [ ] Decision scoping
- [ ] Constraint enforcement

**Tracking**: #270

---

## Dependencies

```
Phase 19 (Entity Integration)
    │
    ├──► Phase 22 (Protocol Governance)
    │       │
    │       ├──► Phase 24 (SDIS Completion)
    │       │
    │       ├──► Phase 25 (Inter-Cooperative Economics)
    │       │
    │       └──► Phase 28 (Federation & Subsidiarity)
    │               [also depends on Phase 25]
```

See [ROADMAP.md](ROADMAP.md) for complete dependency graph including infrastructure phases.

## Success Criteria

The cooperative middle layer is complete when:

1. **ICN governs itself**: Protocol parameters changeable via democratic vote
2. **Universal entity model**: Same structure works for individuals, coops, federations
3. **Real coop economics**: Bilateral clearing, group purchasing, contribution tracking
4. **Sybil-resistant democracy**: One person, one vote, enforced by SDIS
5. **Recursive scale**: Federations of federations, unlimited depth
6. **Subsidiarity**: Decisions made at lowest appropriate level

---

## Change Log

- **2026-01-20**: Reconciled with Strategic Alignment Audit. Updated Gap 1 to reflect icn-entity implementation. Aligned phase numbers with unified ROADMAP.md.
- **2025-12-24**: Initial draft
