# Social Recovery Design

**Feature**: M-of-N Social Recovery for Lost Devices
**Phase**: Week 1-2 of MVC Track
**Status**: Design → Implementation
**Date**: 2025-01-15

## Problem Statement

Multi-device identity (Phase 11) allows users to have multiple devices, but what happens when:
- User loses ALL devices (laptop stolen, house fire, etc.)
- User loses access to all backups (encrypted backup but forgot passphrase)
- User's only device breaks and keystore is corrupted

**Current State**: User loses their identity permanently. All economic history, trust relationships, and cooperative memberships are gone forever.

**Required State**: User can recover identity with help from trusted friends/colleagues.

## Solution: M-of-N Social Recovery

### High-Level Design

**Recovery Trustees**: User designates N trusted individuals (friends, colleagues, family)
**Recovery Threshold**: User sets M (e.g., 3-of-5 trustees must agree)
**Recovery Process**:
1. User loses all devices, creates new DID on new device
2. User contacts M trustees out-of-band (phone, in-person)
3. Each trustee signs a recovery attestation linking old DID → new DID
4. Once M signatures collected, old DID is marked as recovered, new DID inherits identity
5. Trust graph, ledger, and contracts automatically recognize new DID

### Technical Design

#### 1. DID Document Extension

Extend Phase 11 DID Document v2 with recovery configuration:

```rust
// icn-identity/src/did_document.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// Trustee DIDs who can help with recovery
    pub trustees: Vec<DID>,

    /// Number of trustees required for recovery (M)
    pub threshold: usize,

    /// Optional: Recovery delay period (seconds)
    /// Allows old owner to cancel fraudulent recovery
    pub delay_period: Option<u64>,
}

// Add to DIDDocument
pub struct DIDDocument {
    pub id: DID,
    pub verification_method: Vec<VerificationMethod>,
    pub authentication: Vec<String>,
    pub created: u64,
    pub updated: u64,
    pub rotation_events: Vec<RotationEvent>,
    pub recovery: Option<RecoveryConfig>,  // NEW
}
```

#### 2. Recovery Event Chain

Track recovery events separately from rotation events:

```rust
// icn-identity/src/recovery.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAttestation {
    /// Trustee's DID
    pub trustee: DID,

    /// Old DID being recovered
    pub old_did: DID,

    /// New DID to inherit identity
    pub new_did: DID,

    /// Trustee's signature over (old_did, new_did, timestamp)
    pub signature: Signature,

    /// When attestation was created
    pub timestamp: u64,

    /// Optional: Evidence of identity verification
    /// (e.g., "met in person", "video call", "shared secret")
    pub verification_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryEvent {
    /// Old DID being recovered
    pub old_did: DID,

    /// New DID inheriting identity
    pub new_did: DID,

    /// M attestations from trustees
    pub attestations: Vec<RecoveryAttestation>,

    /// When recovery was initiated
    pub initiated_at: u64,

    /// When recovery was finalized (after delay period)
    pub finalized_at: Option<u64>,

    /// Recovery status
    pub status: RecoveryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryStatus {
    /// Collecting attestations (need M signatures)
    Pending,

    /// Waiting for delay period to expire
    WaitingForDelay { expires_at: u64 },

    /// Recovery finalized, new DID active
    Finalized,

    /// Recovery cancelled by old owner (fraudulent attempt)
    Cancelled { cancelled_by: DID, reason: String },
}
```

#### 3. Recovery Protocol

**Step 1: Configure Recovery (One-Time Setup)**
```bash
# User sets up recovery trustees
icnctl identity recovery setup \
  --trustees did:icn:alice,did:icn:bob,did:icn:carol,did:icn:dave,did:icn:eve \
  --threshold 3 \
  --delay-period 86400  # 24 hours

# This updates DID Document and broadcasts via gossip
```

**Step 2: Initiate Recovery (After Device Loss)**
```bash
# On new device, create new identity
icnctl id init

# Initiate recovery for old DID
icnctl identity recovery initiate \
  --old-did did:icn:user123 \
  --new-did did:icn:newkey456

# System broadcasts recovery request to trustees
# User contacts trustees out-of-band: "I lost my device, please sign recovery"
```

**Step 3: Trustees Attest (Out-of-Band Verification)**
```bash
# Each trustee verifies user's identity (phone call, video, in-person)
# Then signs recovery attestation

icnctl identity recovery attest \
  --old-did did:icn:user123 \
  --new-did did:icn:newkey456 \
  --verification "Video call - confirmed identity via shared memories"

# System broadcasts attestation to network
```

**Step 4: Finalize Recovery (After M Signatures)**
```bash
# Once M attestations collected, recovery enters delay period
# After delay expires (or immediately if no delay configured):

icnctl identity recovery finalize \
  --recovery-id <hash>

# System updates trust graph, ledger references, contract permissions
# Old DID marked as "recovered" → points to new DID
# New DID inherits all relationships and history
```

#### 4. Fraud Prevention

**Delay Period**: 24-72 hour window where original owner can cancel

```bash
# If someone tries to steal your identity via fraudulent recovery:
icnctl identity recovery cancel \
  --recovery-id <hash> \
  --reason "I still have my device, this recovery is fraudulent"

# Requires signature from old DID's active device
# Immediately cancels recovery and alerts network
```

**Attestation Requirements**:
- Trustees should verify identity out-of-band (phone, video, in-person)
- Include verification method in attestation
- Community can audit attestations for suspicious patterns

#### 5. Gossip Integration

Recovery events propagate via gossip on `identity:recovery` topic:

```rust
// icn-gossip topic for recovery events
Topic: "identity:recovery"
Access Control: Public (anyone can read recovery events)

Message Types:
1. RecoveryRequest { old_did, new_did, initiated_at }
2. RecoveryAttestation { trustee, old_did, new_did, signature }
3. RecoveryFinalized { old_did, new_did, finalized_at }
4. RecoveryCancelled { old_did, recovery_id, reason }
```

#### 6. Storage Schema

**In icn-store (Sled database)**:

```rust
// Key: "recovery:<old_did>"
// Value: RecoveryEvent (serialized)

// Key: "recovery_config:<did>"
// Value: RecoveryConfig (serialized)

// Key: "did_mapping:<old_did>"
// Value: new_did (after recovery finalized)
```

#### 7. Trust Graph Integration

When recovery is finalized, update all trust edges:

```rust
// icn-trust/src/graph.rs

impl TrustGraph {
    /// Update all references to old_did to point to new_did
    pub fn apply_recovery(&mut self, old_did: &DID, new_did: &DID) {
        // For each trust edge (A → old_did), update to (A → new_did)
        for edge in self.edges_to(old_did) {
            self.add_edge(edge.from, new_did.clone(), edge.weight);
            self.remove_edge(&edge.from, old_did);
        }

        // For each trust edge (old_did → B), update to (new_did → B)
        for edge in self.edges_from(old_did) {
            self.add_edge(new_did.clone(), edge.to, edge.weight);
            self.remove_edge(old_did, &edge.to);
        }

        // Store mapping for historical queries
        self.did_recovery_map.insert(old_did.clone(), new_did.clone());
    }
}
```

#### 8. Ledger Integration

Ledger must recognize new DID as continuation of old DID:

```rust
// icn-ledger/src/ledger.rs

impl Ledger {
    /// Get balance, checking for DID recovery
    pub fn balance(&self, did: &DID) -> Result<i64> {
        // Check if this DID has been recovered
        let effective_did = self.resolve_did_recovery(did)?;

        // Use effective DID for balance lookup
        self.accounts.get(&effective_did)
            .map(|account| account.balance)
            .ok_or(Error::AccountNotFound)
    }

    fn resolve_did_recovery(&self, did: &DID) -> Result<DID> {
        // If DID has been recovered, return new DID
        // Otherwise return original DID
        match self.recovery_map.get(did) {
            Some(new_did) => Ok(new_did.clone()),
            None => Ok(did.clone()),
        }
    }
}
```

## Security Considerations

### Attack Vectors

**1. Sybil Attack on Trustees**
- **Attack**: Attacker creates 5 fake identities, user sets them as trustees
- **Mitigation**: Trustees must have trust edges from other community members
- **Recommendation**: UI should warn if trustees aren't well-connected in trust graph

**2. Collusion Attack**
- **Attack**: 3 malicious trustees collude to steal user's identity
- **Mitigation**: Delay period allows victim to cancel fraudulent recovery
- **Recommendation**: Choose trustees who don't know each other (geographic/social diversity)

**3. Coercion Attack**
- **Attack**: Attacker forces user to initiate recovery and hand over new device
- **Mitigation**: Hard to prevent (rubber-hose cryptanalysis)
- **Recommendation**: Document risk, recommend trustee diversity

**4. Replay Attack**
- **Attack**: Old recovery attestations reused for new recovery attempt
- **Mitigation**: Attestations include timestamp and specific new_did
- **Verification**: Check attestation timestamp is recent (e.g., within 7 days)

### Best Practices

**Trustee Selection**:
- Choose 5-7 trustees (redundancy)
- Set threshold to 3-of-5 or 4-of-7 (balance security vs availability)
- Pick trustees from different social circles (colleagues, friends, family)
- Ensure trustees are active ICN users (not dormant accounts)

**Delay Period**:
- **Low-stakes identities**: 24 hours (faster recovery)
- **High-stakes identities**: 72 hours (more time to detect fraud)
- **Critical identities**: 7 days + community vote (governance integration)

**Verification Methods**:
- **In-person**: Strongest (shared physical token, biometrics)
- **Video call**: Strong (visual confirmation + shared memories)
- **Phone call**: Medium (voice recognition + security questions)
- **Email/chat**: Weak (vulnerable to account compromise)

## Implementation Plan

### Phase 1: Core Recovery Primitives (Week 1)
1. Extend DID Document with RecoveryConfig
2. Implement RecoveryEvent and RecoveryAttestation types
3. Add recovery storage to icn-store
4. Unit tests for recovery data structures

### Phase 2: CLI Integration (Week 1-2)
5. `icnctl identity recovery setup` - Configure trustees
6. `icnctl identity recovery initiate` - Start recovery
7. `icnctl identity recovery attest` - Trustee signs attestation
8. `icnctl identity recovery finalize` - Complete recovery
9. `icnctl identity recovery cancel` - Cancel fraudulent recovery
10. `icnctl identity recovery status` - Check recovery progress

### Phase 3: Gossip Integration (Week 2)
11. Create `identity:recovery` topic
12. Broadcast recovery events (request, attestation, finalized, cancelled)
13. Listen for recovery events and update local state
14. Tests for gossip propagation

### Phase 4: Trust Graph & Ledger Integration (Week 2)
15. Implement trust graph DID mapping
16. Implement ledger DID recovery resolution
17. Integration tests: recover identity, verify balance preserved
18. Integration tests: recover identity, verify trust edges preserved

### Phase 5: Documentation & Testing (Week 2)
19. User guide: "How to set up social recovery"
20. Operator guide: "How to handle recovery requests"
21. End-to-end test: Full recovery flow with 3-of-5 trustees
22. Security audit: Review attack vectors and mitigations

## Success Criteria

- ✅ User can configure 3-of-5 recovery via CLI
- ✅ User can initiate recovery after losing device
- ✅ Trustees can sign attestations via CLI
- ✅ Recovery finalizes after M signatures collected
- ✅ Trust edges automatically transfer to new DID
- ✅ Ledger balance preserved after recovery
- ✅ Fraudulent recovery can be cancelled within delay period
- ✅ All tests pass (unit + integration)
- ✅ Documentation complete (user + operator guides)

## Future Enhancements (Post-MVC)

**Governance Integration**:
- High-stakes recovery requires community vote (not just trustees)
- Cooperative can override individual recovery decisions

**Multi-Factor Recovery**:
- Require N-of-M trustees + backup seed phrase
- Require trustees + hardware security key

**Recovery Analytics**:
- Track recovery success/failure rates
- Detect suspicious recovery patterns (same trustees used repeatedly)

**UX Improvements**:
- Visual trustee selection (show trust graph connections)
- Recovery progress dashboard
- Email/SMS notifications to trustees

## Open Questions

1. **Trustee Incentives**: Should trustees receive compensation for helping with recovery?
   - Pro: Incentivizes responsiveness
   - Con: Creates market for fake attestations
   - **Decision**: Start without compensation, add if pilots request it

2. **Automatic vs Manual Finalization**: Should recovery auto-finalize after delay, or require explicit finalization?
   - **Decision**: Require explicit finalization for security (prevents automatic takeover)

3. **Recovery History Visibility**: Should recovery events be public or private?
   - **Decision**: Public (transparency prevents fraud, enables community oversight)

4. **Cross-Cooperative Recovery**: Can trustees be from other cooperatives?
   - **Decision**: Yes (federated trust is a feature, not a bug)

---

**Document Status**: Design Complete - Ready for Implementation
**Next Step**: Implement Phase 1 (Core Recovery Primitives)
**Estimated Effort**: 2 weeks (matches MVC Week 1-2)
