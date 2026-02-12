# SDIS & Steward System - Completion Roadmap

**Date:** 2025-12-12  
**Status Snapshot:** Foundation complete ✅  
**Next Phase:** Integration & User-Facing Features

---

## 🎯 Current State Assessment

### ✅ What's Complete (Phases S1-S3)

#### Cryptographic Foundations (Phase S1)
- ✅ **icn-crypto-pq** crate (51 tests passing)
  - Hybrid signatures (Ed25519 + ML-DSA)
  - ML-KEM key encapsulation
  - Threshold secret sharing for VUI computation
  - Blind signatures for unlinkable credentials
  - KDF for hybrid key derivation

#### Identity Extensions (Phase S2)
- ✅ **Anchor/KeyBundle** separation in icn-identity
  - Permanent anchor IDs
  - Rotatable key bundles
  - VUI (Verifiable Unique Identifier) support
  - Multi-device identity support
  - Recovery mechanisms

#### Steward Network (Phase S3)
- ✅ **icn-steward** crate (66 tests passing)
  - StewardActor for ceremony coordination
  - Token issuance and verification
  - VUI registry with Bloom filters
  - Enrollment ceremonies
  - Recovery ceremonies
  - Threshold VUI computation

#### Zero-Knowledge Proofs (Phase S4 - Partial)
- ✅ **icn-zkp** crate (42 tests passing)
  - Age proofs (prove > 18 without revealing age)
  - Non-revocation proofs
  - Cryptographic accumulators
  - Proof verification
  - Nonce-based replay protection

---

## 🚧 What Needs Completion

### Priority 1: Integration with Gateway API (2-3 weeks)

SDIS components are now exposed through gateway route wiring, but this roadmap tracks productization hardening and user-facing completion work.

#### Tasks:

**1. Gateway SDIS Endpoints**
```
POST /v1/sdis/enrollment/start            - Start enrollment ceremony
GET  /v1/sdis/status/{enrollment_id}      - Check enrollment status
POST /v1/sdis/vouch/{enrollment_id}       - Steward vouch action
POST /v1/sdis/reject/{enrollment_id}      - Steward reject action
POST /v1/sdis/enrollment/complete         - Complete enrollment

GET  /v1/sdis/anchor/{anchor_id}          - Get anchor info
POST /v1/sdis/anchor/rotate-keys          - Rotate keys (keep anchor)
POST /v1/sdis/anchor/devices/add          - Add trusted anchor device
GET  /v1/sdis/anchor/{anchor_id}/devices  - List anchor devices

POST /v1/sdis/recovery/start              - Start recovery ceremony
GET  /v1/sdis/recovery/{id}               - Check recovery status
POST /v1/sdis/recovery/{id}/approve       - Steward recovery approval (admin-gated)
POST /v1/sdis/recovery/{id}/complete      - Complete recovery

POST /v1/sdis/verify/level1               - Level-1 verification
POST /v1/sdis/verify/level2               - Level-2 verification
POST /v1/sdis/ephemeral/generate          - Generate ephemeral proof
```

**2. Steward Discovery**
- Gossip topic: `sdis:steward:discovery`
- Allow nodes to find stewards in their region
- Trust-weighted steward selection

**3. SDIS Storage**
- Add anchor → DID mapping to ledger
- Store VUI registry checkpoints
- Persist enrollment/recovery ceremonies

---

### Priority 2: User-Facing Features (3-4 weeks)

Make SDIS usable by end users.

#### Tasks:

**1. Enrollment Wizard (Pilot UI)**
```
Screens:
- Choose pathway (gov ID, org sponsor, web of trust)
- Submit verification documents
- Wait for steward verification
- Receive anchor + initial keybundle
- Backup recovery codes
```

**2. Recovery Flow (Pilot UI)**
```
Screens:
- Enter anchor ID or VUI
- Submit identity verification
- Wait for steward threshold
- Receive new keybundle
- Update all devices
```

**3. Identity Viewer**
```
Show:
- Your anchor ID (permanent)
- Current keybundle version
- Enrollment pathway
- Recovery stewards
- Trusted devices
- Attestations received
```

**4. Steward Dashboard (Admin UI)**
```
Features:
- Pending enrollment requests
- Verification workflows
- Recovery ceremonies in progress
- VUI registry stats
- Steward performance metrics
```

---

### Priority 3: Mobile Integration (2-3 weeks)

Add SDIS to CoopWallet mobile app.

#### Tasks:

**1. Enrollment Screen**
- Camera for ID scanning
- Document upload
- Selfie verification
- Biometric (Face ID / fingerprint)

**2. Credential Presentation**
- QR code generation (tier 1)
- NFC presentation (tier 2)  
- Network verification (tier 3)

**3. Proof Generation UI**
- "Prove I'm over 18"
- "Prove I'm a member"
- "Prove I'm from [region]"
- Selective attribute disclosure

**4. Recovery Backup**
- Export recovery shares
- Print recovery codes
- Secure share distribution

---

### Priority 4: Advanced Features (4-6 weeks)

Enhanced SDIS capabilities.

#### Tasks:

**1. Credential Issuance**
```rust
// Gateway issues verifiable credentials
POST /v1/sdis/credentials/issue
{
  "holder_anchor": "anchor_abc123",
  "credential_type": "membership",
  "attributes": {
    "coop_id": "test-coop",
    "role": "member",
    "since": 1702425600
  },
  "validity_period": 31536000  // 1 year
}
```

**2. Revocation**
```rust
// Revoke compromised credentials
POST /v1/sdis/credentials/revoke
{
  "credential_id": "cred_xyz789",
  "reason": "key_compromise"
}

// Add to cryptographic accumulator
// ZK proofs can prove non-revocation
```

**3. Tiered Presentation**

**Tier 1: QR Code (Low Security)**
- Static QR with anchor + proof
- No network required
- For low-value transactions

**Tier 2: NFC (Medium Security)**
- Challenge-response protocol
- Proximity verification
- For physical access control

**Tier 3: Network (High Security)**
- Full SDIS verification
- Real-time revocation checks
- For financial transactions

**4. Steward Governance**
```rust
// Cooperative votes on steward appointments
POST /v1/governance/proposals
{
  "type": "appoint_steward",
  "nominee_did": "did:icn:...",
  "region": "North America",
  "duration_days": 365
}

// Steward performance tracking
GET /v1/sdis/stewards/:did/metrics
{
  "enrollments_processed": 142,
  "recovery_ceremonies": 23,
  "avg_response_time_hours": 8.5,
  "trust_score": 0.92
}
```

---

## 📋 Detailed Task Breakdown

### Phase 1: Gateway Integration (Weeks 1-3)

#### Week 1: Basic Endpoints
- [ ] Create `icn-gateway/src/api/sdis.rs`
- [ ] Add enrollment initiation endpoint
- [ ] Add enrollment status endpoint
- [ ] Wire StewardActor to Gateway
- [ ] Add anchor info endpoint
- [ ] Write integration tests

#### Week 2: Recovery & Key Rotation
- [ ] Add recovery ceremony endpoints
- [ ] Add key rotation endpoint
- [ ] Implement anchor → DID mapping
- [ ] Add VUI registry persistence
- [ ] Add ceremony state persistence

#### Week 3: ZK Proofs
- [ ] Add proof generation endpoint
- [ ] Add proof verification endpoint
- [ ] Implement proof caching
- [ ] Add proof types (age, membership, location)
- [ ] Add rate limiting for proofs

---

### Phase 2: Pilot UI Features (Weeks 4-7)

#### Week 4: Enrollment Wizard
- [ ] Create EnrollmentWizard component
- [ ] Add pathway selection screen
- [ ] Add document upload
- [ ] Add progress tracker
- [ ] Add backup generation

#### Week 5: Recovery Flow
- [ ] Create RecoveryFlow component
- [ ] Add anchor/VUI input
- [ ] Add verification submission
- [ ] Add steward coordination UI
- [ ] Add success/failure handling

#### Week 6: Identity Viewer
- [ ] Create IdentityViewer component
- [ ] Display anchor info
- [ ] Display current keybundle
- [ ] Display enrollment pathway
- [ ] Display trusted devices
- [ ] Display attestations

#### Week 7: Steward Dashboard
- [ ] Create StewardDashboard component
- [ ] Add enrollment queue
- [ ] Add verification workflows
- [ ] Add recovery management
- [ ] Add metrics dashboard

---

### Phase 3: Mobile Integration (Weeks 8-10)

#### Week 8: Enrollment
- [ ] Create EnrollmentScreen.tsx
- [ ] Add camera integration
- [ ] Add biometric auth
- [ ] Add document upload
- [ ] Add API integration

#### Week 9: Credential Presentation
- [ ] Create PresentationScreen.tsx
- [ ] Add QR code generation
- [ ] Add NFC support (Android/iOS)
- [ ] Add network verification
- [ ] Add proof selection UI

#### Week 10: Recovery & Backup
- [ ] Create RecoveryScreen.tsx
- [ ] Add backup export
- [ ] Add recovery share generation
- [ ] Add secure storage integration
- [ ] Add recovery testing

---

### Phase 4: Advanced Features (Weeks 11-16)

#### Week 11-12: Credential Issuance
- [ ] Design credential schema
- [ ] Implement issuer role
- [ ] Add credential storage
- [ ] Add verification logic
- [ ] Add expiration handling

#### Week 13-14: Revocation
- [ ] Implement accumulator integration
- [ ] Add revocation endpoint
- [ ] Add revocation gossip
- [ ] Add non-revocation proofs
- [ ] Add revocation UI

#### Week 15-16: Steward Governance
- [ ] Add steward proposal types
- [ ] Add steward voting
- [ ] Add performance tracking
- [ ] Add steward rotation
- [ ] Add regional coordination

---

## 🎯 Success Criteria

### Minimum Viable SDIS (MVP)

**User Can:**
- ✅ Enroll through web UI
- ✅ Choose enrollment pathway
- ✅ Receive permanent anchor
- ✅ View their anchor info
- ✅ Generate basic ZK proofs
- ✅ Recover lost keys

**Admin Can:**
- ✅ Process enrollment requests
- ✅ Verify documents
- ✅ Coordinate recovery ceremonies
- ✅ Monitor steward performance
- ✅ Revoke compromised credentials

**System Can:**
- ✅ Compute VUI via threshold
- ✅ Store anchors securely
- ✅ Rotate keys without changing anchor
- ✅ Verify ZK proofs
- ✅ Check revocation status
- ✅ Gossip steward updates

---

## 🔧 Technical Considerations

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Gateway API                            │
│  ┌───────────────────────────────────────────────────────┐ │
│  │  /v1/sdis/*  endpoints                                │ │
│  │  - Enrollment, Recovery, Proofs, Credentials          │ │
│  └───────────────┬───────────────────────────────────────┘ │
└──────────────────┼─────────────────────────────────────────┘
                   │
         ┌─────────┼─────────┐
         │         │         │
         ▼         ▼         ▼
    ┌────────┐ ┌────────┐ ┌────────┐
    │Steward │ │  ZKP   │ │ Ledger │
    │ Actor  │ │ Verify │ │ Store  │
    └────────┘ └────────┘ └────────┘
         │
         ▼
    ┌────────────────────────────────┐
    │     Gossip Protocol            │
    │  - steward:discovery           │
    │  - sdis:enrollment             │
    │  - sdis:recovery               │
    │  - sdis:revocation             │
    └────────────────────────────────┘
```

### Storage Requirements

**New Collections:**
- `anchors` - Anchor ID → Anchor data
- `anchor_did_map` - Anchor ID → current DID
- `vui_registry` - VUI → Anchor ID (privacy-preserving)
- `ceremonies` - Enrollment/recovery ceremony state
- `credentials` - Issued credentials
- `revocations` - Revoked credential IDs

**Estimated Storage:**
- Anchor: ~200 bytes
- Ceremony: ~2KB
- Credential: ~500 bytes
- Revocation: ~32 bytes

For 10,000 users: ~25MB

---

## 📊 Metrics to Track

### Enrollment Metrics
- Enrollment requests per day
- Avg time to complete enrollment
- Enrollment success rate by pathway
- Documents rejected (by reason)

### Recovery Metrics
- Recovery ceremonies initiated
- Recovery success rate
- Avg steward response time
- Failed recovery attempts

### Proof Metrics
- Proofs generated per day (by type)
- Proof verification success rate
- Avg proof generation time
- Avg proof verification time

### Steward Metrics
- Active stewards by region
- Enrollments per steward
- Steward response times
- Trust scores over time

---

## 🚀 Quick Start: What to Build First?

### Option 1: End-to-End Enrollment (Recommended)
**Priority:** Make it possible for a user to enroll and get an anchor.

**Tasks:**
1. Add Gateway endpoints for enrollment
2. Create enrollment wizard in Pilot UI
3. Wire up steward actor
4. Test full flow

**Time:** 1-2 weeks  
**Impact:** High (enables all other features)

---

### Option 2: Proof Generation UI
**Priority:** Let users generate and verify ZK proofs.

**Tasks:**
1. Add proof endpoints to Gateway
2. Create proof generation UI
3. Add verification UI
4. Add common proof types

**Time:** 1 week  
**Impact:** Medium (demos SDIS capability)

---

### Option 3: Mobile Enrollment
**Priority:** Enable mobile enrollment with camera/biometrics.

**Tasks:**
1. Create EnrollmentScreen in CoopWallet
2. Add camera integration
3. Add biometric auth
4. Wire to Gateway API

**Time:** 1-2 weeks  
**Impact:** High (mobile-first users)

---

## 🎯 Recommended Next Steps

**Immediate (This Week):**
1. Add Gateway SDIS endpoints (enrollment + proof generation)
2. Create basic enrollment wizard in Pilot UI
3. Wire up existing StewardActor

**Short-term (Next 2 Weeks):**
1. Add recovery flow
2. Create identity viewer
3. Add proof generation UI

**Medium-term (Next Month):**
1. Mobile enrollment screen
2. Credential issuance
3. Steward dashboard

**Long-term (Next Quarter):**
1. Advanced credential types
2. Multi-region stewards
3. Governance integration

---

## 📖 Documentation Needed

- [ ] SDIS User Guide (end-user perspective)
- [ ] Steward Handbook (how to be a steward)
- [ ] API Documentation (all SDIS endpoints)
- [ ] Integration Guide (for developers)
- [ ] Security Audit Report (before production)

---

## ✅ Definition of Done

SDIS is "complete" when:

1. ✅ User can enroll through web/mobile UI
2. ✅ User receives permanent anchor
3. ✅ User can recover lost keys
4. ✅ User can generate ZK proofs
5. ✅ Credentials can be issued
6. ✅ Credentials can be revoked
7. ✅ Stewards can process enrollments
8. ✅ System passes security audit
9. ✅ Documentation is complete
10. ✅ Integration tests pass

---

**Status:** Ready to begin Gateway integration! 🚀

Which component would you like to work on first?
