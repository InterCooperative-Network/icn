# Gap Remediation Roadmap - ICN Architecture Completion

**Date:** 2025-12-17  
**Status:** Priority Action Plan  
**Goal:** Close critical gaps identified in implementation audit

---

## 🎯 Priority Matrix

| Component | Impact | Effort | Priority | Target |
|-----------|--------|--------|----------|--------|
| Upgrade Coordination | HIGH | MEDIUM | P0 | Week 1 |
| Dispute Resolution | HIGH | LOW | P0 | Week 1 |
| SDIS Integration Tests | MEDIUM | LOW | P1 | Week 2 |
| Trust-Adaptive Credits | MEDIUM | MEDIUM | P1 | Week 3 |
| Charter Enforcement | MEDIUM | HIGH | P2 | Week 4 |
| Federation Protocol | LOW | HIGH | P3 | Week 5 |
| Distributed Snapshots | LOW | MEDIUM | P3 | Week 6 |

---

## Sprint 1: Critical Infrastructure (Week 1-2)

### 1.1 Upgrade Coordination Protocol ⚠️ CRITICAL

**Why:** Without this, any protocol change breaks the network permanently.

**Tasks:**
- [ ] Add `ProtocolVersion` struct to `icn-net/src/protocol.rs`
- [ ] Implement version negotiation in QUIC handshake
- [ ] Add feature flags for backward compatibility
- [ ] Create upgrade gossip topic for coordination
- [ ] Test mixed-version cluster (v1 + v2 nodes)

**Files:**
```
icn/crates/icn-net/src/protocol.rs         // Add ProtocolVersion
icn/crates/icn-net/src/handshake.rs        // Version negotiation
icn/crates/icn-core/src/supervisor.rs      // Feature flag loading
icn/crates/icn-gossip/src/topics.rs        // Add "icn:upgrade" topic
icn/tests/upgrade_test.rs                  // Integration test
```

**Definition of Done:**
- [ ] Nodes reject incompatible protocol versions
- [ ] Mixed-version clusters can coexist during rollout
- [ ] Upgrade path documented in `/docs/UPGRADE-PROTOCOL.md`

---

### 1.2 Dispute Resolution Mechanism ⚠️ CRITICAL

**Why:** Ledger quarantine is a black hole without resolution logic.

**Tasks:**
- [ ] Add `DisputeDomain` to governance system
- [ ] Create `Dispute` proposal type
- [ ] Link dispute proposals to quarantined ledger entries
- [ ] Implement arbitrator voting (3-of-5 stewards)
- [ ] Add dispute resolution gossip topic
- [ ] Test: Create dispute → Vote → Resolve entry

**Files:**
```
icn/crates/icn-governance/src/domain.rs    // Add DisputeDomain
icn/crates/icn-governance/src/proposal.rs  // Add DisputeProposal
icn/crates/icn-ledger/src/quarantine.rs    // Link to governance
icn/crates/icn-gateway/src/api/governance.rs // Add dispute endpoints
icn/tests/dispute_resolution_test.rs       // Integration test
```

**Definition of Done:**
- [ ] Quarantined entries can be voted on
- [ ] Winning vote releases or permanently quarantines entry
- [ ] UI shows dispute status in dashboard
- [ ] Documentation in `/docs/DISPUTE-RESOLUTION.md`

---

### 1.3 SDIS End-to-End Integration Tests 🔧

**Why:** SDIS UI/API exists, but no tests prove steward enrollment/recovery works.

**Tasks:**
- [ ] Create 5-node test cluster with 3 designated stewards
- [ ] Test: Enroll identity with 3 stewards
- [ ] Test: Lose device, recover with 2-of-3 steward approval
- [ ] Test: Proof generation and verification
- [ ] Test: Anchor creation and gossip propagation
- [ ] Add CI test for SDIS flow

**Files:**
```
icn/tests/sdis_enrollment_test.rs          // Full enrollment flow
icn/tests/sdis_recovery_test.rs            // 2-of-3 recovery
icn/tests/sdis_proof_verification_test.rs  // ZK proof validation
docs/SDIS-ARCHITECTURE.md                  // Document steward selection
```

**Definition of Done:**
- [ ] All SDIS flows tested end-to-end
- [ ] Tests run in CI
- [ ] Steward selection algorithm documented
- [ ] Recovery flow proven in multi-node environment

---

## Sprint 2: Economic Hardening (Week 3-4)

### 2.1 Trust-Adaptive Credit Limits 💰

**Why:** Static credit limits allow abuse by coordinated attackers.

**Tasks:**
- [ ] Query trust graph during transaction validation
- [ ] Implement trust-weighted credit limit formula
- [ ] Add decay function for low-trust paths
- [ ] Create governance override mechanism
- [ ] Test: High-trust user gets higher limit
- [ ] Test: Low-trust user restricted

**Files:**
```
icn/crates/icn-ledger/src/validation.rs    // Query trust graph
icn/crates/icn-trust/src/credit_weights.rs // Credit limit calculation
icn/crates/icn-governance/src/overrides.rs // Manual adjustments
docs/ECONOMIC-SAFETY.md                    // Document model
```

**Definition of Done:**
- [ ] Credit limits scale with trust scores
- [ ] Governance can override automatic limits
- [ ] Tests show abuse prevention
- [ ] Documentation explains formula

---

### 2.2 Charter Enforcement via CCL 📜

**Why:** Charters are currently descriptive, not enforceable.

**Tasks:**
- [ ] Define charter rule AST in CCL
- [ ] Add charter validation hook in ledger
- [ ] Implement "before_transaction" CCL trigger
- [ ] Add charter violation quarantine
- [ ] Test: Transaction violates charter → rejected
- [ ] UI shows charter rule violations

**Files:**
```
icn/crates/icn-ccl/src/charter_rules.rs    // Charter AST
icn/crates/icn-ledger/src/validation.rs    // Add charter check
icn/crates/icn-cooperative/src/charter.rs  // Charter storage
icn/crates/icn-gateway/src/api/charter/    // Charter endpoints
docs/CHARTER-ENFORCEMENT.md                // Documentation
```

**Definition of Done:**
- [ ] Transactions must pass charter rules
- [ ] Charter violations logged and quarantined
- [ ] UI shows enforcement status
- [ ] Example charter rules documented

---

## Sprint 3: Federation & Advanced Features (Week 5-6)

### 3.1 Federation Protocol 🌐

**Why:** Multi-cooperative networks need bridge nodes.

**Tasks:**
- [ ] Document federation trust model
- [ ] Implement federated gossip topics (namespaced)
- [ ] Add cross-federation trust attestation
- [ ] Create bridge node role
- [ ] Test: Two coops communicate via bridge
- [ ] Add federation dashboard to UI

**Files:**
```
icn/crates/icn-federation/src/bridge.rs    // Bridge node logic
icn/crates/icn-federation/src/protocol.rs  // Federation protocol
icn/crates/icn-gossip/src/federation.rs    // Federated topics
icn/tests/federation_bridge_test.rs        // Integration test
docs/FEDERATION-PROTOCOL.md                // Documentation
```

**Definition of Done:**
- [ ] Two isolated coops can communicate
- [ ] Bridge nodes enforce trust boundaries
- [ ] Tests prove message routing
- [ ] Documentation complete

---

### 3.2 Distributed Snapshot Coordination 📸

**Why:** Current snapshots are node-local only.

**Tasks:**
- [ ] Implement Chandy-Lamport snapshot algorithm
- [ ] Add snapshot negotiation gossip topic
- [ ] Coordinate snapshot across all nodes
- [ ] Test: Cluster-wide consistent snapshot
- [ ] Add snapshot verification logic

**Files:**
```
icn/crates/icn-snapshot/src/coordinator.rs // Snapshot coordination
icn/crates/icn-snapshot/src/chandy_lamport.rs // Algorithm
icn/crates/icn-gossip/src/topics.rs        // Add snapshot topic
icn/tests/distributed_snapshot_test.rs     // Integration test
docs/SNAPSHOT-PROTOCOL.md                  // Documentation
```

**Definition of Done:**
- [ ] Cluster-wide snapshots are consistent
- [ ] Snapshots include all actor state
- [ ] Tests prove correctness
- [ ] Restore from snapshot works

---

## Documentation Cleanup Sprint (Parallel)

### Tasks:
- [ ] Remove claims about unimplemented features from README
- [ ] Update ARCHITECTURE.md to reflect reality
- [ ] Mark aspirational features as "Roadmap" not "Implemented"
- [ ] Add implementation status badges to all docs
- [ ] Create KNOWN-LIMITATIONS.md

**Files to Update:**
```
README.md
docs/ARCHITECTURE.md
docs/GETTING_STARTED.md
ROADMAP.md
```

---

## Success Metrics

### Phase 1 Complete When:
- [ ] Mixed-version clusters survive upgrades
- [ ] Disputes can be resolved democratically
- [ ] SDIS flows tested in CI

### Phase 2 Complete When:
- [ ] Trust scores affect credit limits
- [ ] Charter violations are caught automatically
- [ ] Economic abuse scenarios fail in tests

### Phase 3 Complete When:
- [ ] Two coops can federate
- [ ] Snapshots are cluster-consistent
- [ ] All integration tests pass

---

## Risk Mitigation

### High-Risk Items:
1. **Upgrade coordination** - Breaking change to handshake
   - Mitigation: Test with 3-node cluster (v1, v1, v2)
   
2. **Charter enforcement** - Could break existing transactions
   - Mitigation: Feature flag, opt-in per cooperative
   
3. **Federation** - Complex trust boundaries
   - Mitigation: Start with trusted bridge nodes only

### Low-Risk Items:
- Dispute resolution (additive, no breaking changes)
- SDIS tests (validation only)
- Snapshot coordination (opt-in feature)

---

## Deployment Strategy

### Week 1-2: Critical Fixes
- Deploy upgrade coordination to testnet
- Enable dispute resolution in pilot

### Week 3-4: Economic Features
- Roll out trust-adaptive limits gradually
- Charter enforcement behind feature flag

### Week 5-6: Advanced Features
- Federation for pilot cooperatives
- Snapshot coordination in production

---

## Open Questions

1. **Upgrade Coordination:** Should we use semantic versioning or custom protocol versions?
2. **Dispute Resolution:** Should disputes require stake (prevent spam)?
3. **Federation:** Should bridge nodes be elected or permissioned?
4. **Snapshots:** Should snapshots be triggered manually or automatically?

---

## Next Steps

1. Review this roadmap with team
2. Assign owners to each sprint
3. Create GitHub issues for each task
4. Begin Sprint 1 implementation
5. Update project board

**Roadmap Finalized: Ready for implementation.**
