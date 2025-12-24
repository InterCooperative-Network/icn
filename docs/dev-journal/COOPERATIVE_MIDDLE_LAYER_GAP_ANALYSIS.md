# Cooperative Middle Layer: Gap Analysis

---
**Status**: DRAFT
**Created**: 2025-12-23
**Updated**: 2025-12-24
**Authors**: fahertym, Claude Code
**Decision Makers**: TBD (pending review)

---

**Related**: [ICN_SDIS_INTEGRATED_VISION.md](ICN_SDIS_INTEGRATED_VISION.md)

## Executive Summary

This document identifies gaps between ICN's current implementation and the "Cooperative Middle Layer" vision where ICN is itself cooperatively governed infrastructure.

### Gap Categories

| Category | Current State | Vision State | Priority |
|----------|--------------|--------------|----------|
| **Recursive Entity Model** | Separate types per level | Unified CooperativeEntity | CRITICAL |
| **Protocol Governance** | Hardcoded parameters | Democratically adjustable | CRITICAL |
| **SDIS Integration** | Phase S1-S6 complete | Full anchor-based identity | HIGH |
| **Inter-Coop Economics** | Treasury + basic ledger | Clearing agreements, group purchasing | HIGH |
| **Federation Hierarchy** | Single-level federation | Recursive federation | MEDIUM |
| **Subsidiarity** | Not implemented | Decision scoping by level | MEDIUM |

---

## Gap 1: Unified CooperativeEntity Model

### Current State
- `icn-identity`: DIDs for individuals/devices
- `icn-governance`: Proposals, votes, delegation (separate from identity)
- `icn-ledger`: Accounts for DIDs (not for coops as first-class entities)
- `icn-trust`: Trust between DIDs (not between coops/federations)

Each subsystem has its own entity model that doesn't compose.

### Vision State
A single `CooperativeEntity` type that:
- Works at every scale (individual → coop → federation → global)
- Composes identity, governance, economics, trust, compute
- Is the spine that all subsystems attach to

### Gap Analysis

| Component | Current | Needed |
|-----------|---------|--------|
| Identity | `Did` (individual focus) | `CooperativeEntity` with anchor |
| Membership | Implicit via trust | Explicit recursive membership |
| Governance | Per-proposal, no scoping | Entity-scoped, subsidiarity |
| Treasury | Coop-level only | At every level |
| Trust | DID-to-DID | Entity-to-entity, domain-aware |

### Implementation Path

1. **Define CooperativeEntity in icn-identity** (or new icn-entity crate)
2. **Migrate icn-governance** to entity-scoped proposals
3. **Migrate icn-ledger** to entity accounts (not just DID accounts)
4. **Migrate icn-trust** to entity trust relationships
5. **Update icn-gateway** APIs to work with entities

### Estimated Effort: 6-8 weeks

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

### Phase 19: Cooperative Entity Foundation (8 weeks)
- [ ] Define CooperativeEntity core type
- [ ] Entity-scoped governance
- [ ] Entity accounts in ledger
- [ ] Entity trust relationships
- [ ] Update gateway APIs

### Phase 20: Protocol Governance (4 weeks)
- [ ] ProtocolParameter types
- [ ] Parameter governance domain
- [ ] Change application mechanism
- [ ] Constraint propagation

### Phase 21: Inter-Coop Economics (8 weeks)
- [ ] InterCoopAgreement implementation
- [ ] Clearing house for settlement
- [ ] Contribution tracking
- [ ] Anti-extraction policies
- [ ] Group purchasing (basic)

### Phase 22: SDIS Completion (8 weeks)
- [ ] ZK voting circuits
- [ ] Cooperative anchor ceremonies
- [ ] Steward network governance
- [ ] Full credential ecosystem

### Phase 23: Recursive Federation (6 weeks)
- [ ] Multi-level hierarchy
- [ ] Recursive trust
- [ ] Multi-level settlement
- [ ] Proposal escalation

### Phase 24: Subsidiarity (4 weeks)
- [ ] Decision scoping
- [ ] Automatic scope detection
- [ ] Constraint enforcement
- [ ] Override mechanisms

---

## Dependencies

```
Phase 19 (Entity Foundation)
    │
    ├──► Phase 20 (Protocol Governance) ──► Phase 24 (Subsidiarity)
    │
    ├──► Phase 21 (Inter-Coop Economics)
    │
    └──► Phase 22 (SDIS Completion)
            │
            └──► Phase 23 (Recursive Federation)
```

## Success Criteria

The cooperative middle layer is complete when:

1. **ICN governs itself**: Protocol parameters changeable via democratic vote
2. **Universal entity model**: Same structure works for individuals, coops, federations
3. **Real coop economics**: Bilateral clearing, group purchasing, contribution tracking
4. **Sybil-resistant democracy**: One person, one vote, enforced by SDIS
5. **Recursive scale**: Federations of federations, unlimited depth
6. **Subsidiarity**: Decisions made at lowest appropriate level

---

## Next Steps

1. **Create GitHub issues** for each phase
2. **Prioritize Phase 19** (Entity Foundation) as critical path
3. **Begin CooperativeEntity design** document
4. **Plan migration path** from current types to unified model
