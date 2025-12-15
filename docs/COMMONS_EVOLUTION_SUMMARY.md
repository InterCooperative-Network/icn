# Commons Evolution Implementation Summary

**Document ID**: ICN-DEV-COMMONS-SUMMARY
**Version**: 0.8.0
**Date**: 2025-12-15
**Author**: ICN Development Team

---

## Overview

The Commons Evolution transforms ICN from a "cooperative coordination tool" into a "global commons infrastructure." This document summarizes the implementation completed over versions 0.3.0 through 0.8.0.

**Core Insight**: Commons Holder is the root identity; membership is scoped capability grants under that identity.

---

## Implementation Timeline

| Version | Milestone | Commits | Lines Added |
|---------|-----------|---------|-------------|
| v0.3.0 | Commons Foundation | 7 | ~2,500 |
| v0.4.0 | Stewardship | 1 | ~1,200 |
| v0.5.0 | Membership & Rights | 1 | ~2,400 |
| v0.6.0 | Constitutional Governance | 1 | ~3,800 |
| v0.7.x | Integration & Testing | 1 | ~500 |
| v0.8.x | Production Hardening | 4 | ~1,200 |
| **Total** | | **15** | **~11,600** |

---

## Layer Architecture

### Layer 0: PersonhoodAnchor (v0.3.0)
**Purpose**: Cryptographic proof that a unique human exists behind the keys.

**Files**:
- `icn-identity/src/personhood.rs` - Core types
- `icn-identity/src/personhood_store.rs` - Storage layer

**Key Types**:
```rust
PersonhoodAnchor {
    id: [u8; 32],              // Unique anchor ID
    underlying_anchor: Anchor,  // SDIS anchor with VUI
    status: AnchorStatus,       // Active/Suspended/Revoked
    attestations: Vec<POPAttestation>,
    current_key: PublicKey,
    key_history: Vec<KeyRotationRecord>,
}

POPAttestation {
    method: POPMethod,          // InPerson/Sponsored/Biometric/Ceremony/VideoCall
    level: POPLevel,            // Weak/Strong/Verified
    attester_did: Did,
    timestamp: u64,
}
```

**Capabilities**:
- Create anchor from enrollment
- Add POP attestations (multiple methods strengthen identity)
- Track key rotations with full audit trail
- Status lifecycle (Active → Suspended → Revoked)
- Query by DID or public key

### Layer 1: CommonsHolderRecord (v0.3.0)
**Purpose**: A human participant's record in the global commons with baseline rights.

**Files**:
- `icn-identity/src/commons.rs` - Core types
- `icn-identity/src/commons_store.rs` - Storage layer

**Key Types**:
```rust
CommonsHolderRecord {
    id: [u8; 32],
    anchor_id: [u8; 32],        // Links to PersonhoodAnchor
    holder_did: Did,
    status: HolderStatus,
    personhood_level: POPLevel,
    affiliations: Vec<Affiliation>,
    baseline_rights: CommonsRights,
}

CommonsRights {
    vote_in_commons: bool,      // Network-level governance
    petition: bool,             // Constitutional changes
    minimal_fuel_floor: bool,   // Civic participation
    due_process: bool,          // Before suspension/revocation
    exit_freely: bool,          // Leave any jurisdiction
    audit_access: bool,         // Public records
}

Affiliation {
    jurisdiction_id: JurisdictionId,
    membership_status: MembershipStatus,  // Candidate→Provisional→Member→Suspended→Exited→Banned
    roles: Vec<String>,
    capabilities: Vec<MembershipCapability>,
    joined_at: u64,
}
```

**Key Principle**: "A cooperative can revoke your membership. A cooperative cannot revoke your personhood."

### Layer 2: Charter (v0.3.0)
**Purpose**: Founding document that creates a jurisdiction (coop, community, federation).

**Files**:
- `icn-governance/src/charter.rs` - Core types
- `icn-governance/src/charter_store.rs` - Storage layer

**Key Types**:
```rust
Charter {
    charter_id: CharterId,
    org_type: OrgType,          // Cooperative/Community/Federation
    domain_id: String,          // e.g., "coop:food-portland"
    name: String,
    founders: Vec<FounderSignature>,
    governance: GovernanceConfig,
    membership_policy: MembershipPolicy,
    economic_policy: Option<EconomicPolicy>,
    dispute_policy: DisputePolicy,
    status: CharterStatus,      // Draft→Active→Suspended→Dissolved
    amendments: Vec<AmendmentRef>,
}
```

---

## Stewardship Layer (v0.4.0)

**Purpose**: Trusted operators who verify personhood and maintain network integrity.

**Files**:
- `icn-governance/src/steward.rs` - Core types
- `icn-governance/src/steward_store.rs` - Storage layer

**Key Types**:
```rust
StewardRecord {
    id: StewardId,
    holder_did: Did,
    anchor_id: [u8; 32],
    status: StewardStatus,      // Candidate→Active→Suspended→Retired→Revoked
    jurisdiction: Option<JurisdictionId>,
    term_start: u64,
    term_end: Option<u64>,
    bond_amount: u64,
    attestations_issued: u64,
    disputes_against: u64,
    disputes_won: u64,
}
```

**Capabilities**:
- Register as steward (requires Strong POP level)
- Issue POP attestations
- Track reputation (attestations, disputes, wins)
- Manage term and bond
- Suspend/reinstate/retire/revoke lifecycle

**Gateway API**: 13 endpoints at `/v1/steward/*`

---

## Membership & Rights (v0.5.0)

**Purpose**: Full membership lifecycle with capabilities, roles, and revocation with due process.

**Files**:
- `icn-identity/src/revocation.rs` - Revocation types
- `icn-identity/src/revocation_store.rs` - Revocation registry
- `icn-gateway/src/api/membership/mod.rs` - API endpoints

**Key Types**:
```rust
RevocationRecord {
    revocation_id: [u8; 32],
    target_type: RevocationType,  // Anchor/Holder/Membership/Credential
    target_id: String,
    scope: RevocationScope,       // Global/Federation/Jurisdiction
    authority: Did,
    reason: CommonsRevocationReason,
    evidence_refs: Vec<[u8; 32]>,
    appeal_deadline: u64,
    appeal_status: AppealStatus,
    effective_at: u64,
}

MembershipCapability {
    Vote, Propose, Transact, HoldOffice, AccessPrivate, Sponsor
}
```

**Membership Lifecycle**:
```
Apply → Candidate → Approved → Provisional → Promoted → Member
                                    ↓
                              Suspend ↔ Reinstate
                                    ↓
                         Exit / Ban (with appeal window)
```

**Gateway API**: 15 endpoints at `/v1/membership/*`

---

## Constitutional Governance (v0.6.0)

**Purpose**: Amendment process with multi-level ratification and appeal mechanisms.

**Files**:
- `icn-governance/src/amendment.rs` - Amendment types
- `icn-governance/src/appeal.rs` - Appeal types
- `icn-gateway/src/api/constitutional/mod.rs` - API endpoints

### Amendment System

```rust
Amendment {
    id: AmendmentId,
    amendment_type: AmendmentType,  // Charter/Constitutional/Policy/Economic/Governance
    scope: AmendmentScope,          // Jurisdiction/Federation/Network
    status: AmendmentStatus,
    title: String,
    description: String,
    changes: Vec<AmendmentChange>,
    proposer: Did,
    sponsors: Vec<Did>,
    requirements: RatificationRequirements,
    ratifications: Vec<Ratification>,
}
```

**Amendment Lifecycle**:
```
Draft → Submitted → UnderReview → Voting → Ratified/Rejected/Withdrawn/Expired
```

**Ratification Requirements by Scope**:

| Scope | Quorum | Approval | Review | Voting | Min Jurisdictions | Min Federations |
|-------|--------|----------|--------|--------|-------------------|-----------------|
| Jurisdiction | 50% | 67% | 7 days | 14 days | - | - |
| Federation | 60% | 67% | 14 days | 21 days | 3 | - |
| Network | 75% | 80% | 21 days | 30 days | 10 | 3 |
| Critical | 80% | 90% | 30 days | 45 days | 20 | 5 |

### Appeal System

```rust
Appeal {
    id: AppealId,
    appeal_type: AppealType,        // Revocation/Suspension/GovernanceDecision/etc.
    scope: AppealScope,
    status: AppealStatus,
    appellant: Did,
    respondent: Option<Did>,
    grounds: Vec<AppealGrounds>,
    statement: String,
    requested_remedy: AppealRemedy,
    evidence: Vec<AppealEvidence>,
    responses: Vec<AppealResponse>,
}
```

**Appeal Lifecycle**:
```
Filed → UnderReview → Hearing → Resolved/Dismissed/Withdrawn
```

**Appeal Outcomes**:
- **Upheld**: Original decision overturned + remedy
- **Denied**: Original decision stands
- **PartiallyUpheld**: Modified remedy
- **Remanded**: Sent back for reconsideration

**Gateway API**: 15 endpoints at `/v1/constitutional/*`

---

## Gateway API Summary

### Commons Endpoints (`/v1/commons/*`)
- Anchor CRUD and attestations
- Holder CRUD and affiliations
- Status management

### Charter Endpoints (`/v1/charter/*`)
- Charter CRUD
- Founder signatures
- Ratification and status changes

### Steward Endpoints (`/v1/steward/*`)
- Registration and lifecycle
- Reputation tracking
- Term and bond management

### Membership Endpoints (`/v1/membership/*`)
- Application and approval workflow
- Capability and role management
- Ban/revoke with appeals

### Constitutional Endpoints (`/v1/constitutional/*`)
- Amendment CRUD and ratification
- Appeal CRUD and resolution
- Evidence and response management

---

## Test Coverage

| Crate | Module | Tests |
|-------|--------|-------|
| icn-identity | personhood | 19 |
| icn-identity | personhood_store | 15 |
| icn-identity | commons | 24 |
| icn-identity | commons_store | 14 |
| icn-identity | revocation | 10 |
| icn-identity | revocation_store | 10 |
| icn-governance | charter | 15 |
| icn-governance | charter_store | 10 |
| icn-governance | steward | 12 |
| icn-governance | steward_store | 12 |
| icn-governance | amendment | 17 |
| icn-governance | appeal | 18 |
| **Total** | | **~176** |

---

## CommonsManager

Central coordinator in `icn-gateway/src/commons_mgr.rs` providing:

**Storage** (in-memory with indexes):
- PersonhoodAnchors (by ID, by DID, by key)
- CommonsHolderRecords (by ID, by DID, by anchor)
- Charters (by ID, by domain)
- Stewards (by ID, by DID)
- RevocationRegistry
- Amendments (by ID)
- Appeals (by ID)

**Operations**:
- Anchor creation from enrollment
- Holder creation from anchor
- POP attestation workflow
- Membership lifecycle
- Capability/role management
- Revocation with appeals
- Amendment lifecycle
- Constitutional appeals

---

## Integration Points

### SDIS Integration
- Enrollment completion creates PersonhoodAnchor + CommonsHolderRecord
- Steward validation for POP attestations
- Device proof verification

### Governance Integration
- Charter creates governance domain
- Amendments link to governance proposals
- Appeals can challenge governance decisions

### Ledger Integration (Future)
- Membership status affects transaction permissions
- Capability `Transact` required for ledger operations
- Revocation can freeze accounts

---

## What's NOT Included (v1.0.0 Scope)

1. **Persistent Storage**: Currently in-memory; needs Sled backend
2. **Full Steward Network**: Multi-steward ceremonies, steward discovery
3. **Multiple POP Pathways**: Only Sponsored implemented; need InPerson, Biometric, Ceremony
4. **Economic Layer**: Fuel system, contribution credits, demurrage
5. **CLI Commands**: Constitutional commands not yet wired
6. **Integration Tests**: End-to-end flows not yet tested
7. **Production Hardening**: Rate limiting, caching, monitoring for Commons endpoints

---

## Next Steps: Prioritized Roadmap

### Immediate (v0.7.x) - Integration & Testing ✅ COMPLETE

**1. CLI Commands for Constitutional Governance** ✅
- ✅ `icnctl amendment propose/list/vote/ratify` commands wired
- ✅ `icnctl appeal file/list/respond/resolve` commands wired
- ✅ Gateway authentication fixed (`get_gateway_token` uses correct endpoints)
- Files: `icnctl/src/main.rs`

**2. Integration Tests** ✅
- ✅ End-to-end enrollment → anchor → holder flow
- ✅ Charter creation → activation flow
- ✅ Membership lifecycle (Candidate → Provisional → Member → Suspended → Exited)
- ✅ E2E coop formation test
- Files: `icn-gateway/tests/governance_flows_integration.rs`

**3. SDIS-Commons Bridge Completion** ✅
- ✅ `complete_enrollment` creates PersonhoodAnchor + CommonsHolderRecord
- ✅ Auto-affiliation with enrollment coop_id
- ✅ Auto-approve to Provisional if steward vouched
- ✅ Response includes `coop_id` and `membership_status`
- Files: `icn-gateway/src/api/sdis/simple_enrollment.rs`

### Near-term (v0.8.x) - Production Readiness ✅ COMPLETE

**4. Persistent Storage Migration** ✅
- ✅ `CommonsStoreBackend` trait with pluggable backends
- ✅ `InMemoryCommonsStore` for development/testing
- ✅ `SledCommonsStore` for production (feature: `sled-storage`)
- ✅ LRU caching for all entity types
- ✅ Proper indexes (by_did, by_anchor, by_domain)
- Files: `icn-gateway/src/commons_store.rs`, `icn-gateway/src/commons_mgr.rs`

**5. Production Hardening** ✅
- ✅ **Per-category rate limiting**: Different limits for Read/Write/Governance/Compute endpoints
  - Read: 200 burst, 20 req/sec (high limits for queries)
  - Write: 60 burst, 6 req/sec (moderate for mutations)
  - Governance: 30 burst, 3 req/sec (lower for voting/proposals)
  - Compute: 10 burst, 1 req/sec (strictest for resource-intensive operations)
- ✅ **Audit logging for governance actions**: Structured tracing with target `commons_audit`
  - Covers: anchor/holder creation, membership lifecycle, charter operations, steward operations, amendments, appeals
  - Uses `warn!` for adverse actions (suspensions, revocations), `info!` for normal operations
- ✅ **Metrics for Commons operations**: 50+ metrics in `icn-obs/src/metrics.rs`
  - Counters: anchors created, holders created, memberships joined/exited/approved/promoted/suspended/banned
  - Counters: charters created/activated/suspended/dissolved, amendments, appeals
  - Histograms: operation durations, store operation latency
- Files: `icn-gateway/src/rate_limit.rs`, `icn-gateway/src/commons_mgr.rs`, `icn-obs/src/metrics.rs`

**6. Multiple POP Pathways**
- InPerson verification flow (steward + applicant co-located)
- VideoCall verification flow (WebRTC or similar)
- Ceremony flow (group verification events)
- Files: `icn-identity/src/personhood.rs`, `icn-gateway/src/api/sdis/`

### Medium-term (v0.9.x) - Ecosystem

**7. Full Steward Network**
- Steward discovery protocol
- Multi-steward ceremonies for higher POP levels
- Steward reputation and dispute resolution
- Geographic distribution requirements
- Files: `icn-gossip/src/topics/steward.rs`, `icn-governance/src/steward.rs`

**8. Economic Layer Integration**
- Link CommonsRights to fuel allocation
- Capability-gated transaction permissions
- Membership status affects ledger access
- Files: `icn-ledger/src/`, `icn-gateway/src/api/ledger/`

**9. Federation Support**
- Federation charter creation
- Cross-jurisdiction membership
- Federation-level amendments
- Files: `icn-governance/src/federation.rs`

### Long-term (v1.0.0) - Full Commons

**10. Network-level Governance**
- Network amendments requiring 80% approval
- Global steward council
- Protocol upgrade voting
- Economic parameter changes

**11. Advanced Appeal Mechanisms**
- Arbitration panels
- Precedent system
- Cross-jurisdiction appeals
- Automated remedy enforcement

**12. Sybil Resistance at Scale**
- Multiple uniqueness proof methods
- Trust graph-based duplicate detection
- Economic stake requirements
- Social graph analysis

---

## Recommended Implementation Order

```
v0.7.0: CLI Commands + Basic Integration Tests
        ├── icnctl amendment commands
        ├── icnctl appeal commands
        └── End-to-end test: enrollment → holder

v0.7.1: SDIS-Commons Bridge
        ├── PersonhoodAnchor on enrollment completion
        ├── Mobile SDK updates
        └── QR code updates

v0.8.0: Persistent Storage
        ├── Sled backend for CommonsManager
        ├── Migration tooling
        └── Performance testing

v0.8.1: Production Hardening
        ├── Rate limiting
        ├── Audit logging
        ├── Metrics
        └── Security review

v0.9.0: Multiple POP Pathways
        ├── InPerson verification
        ├── VideoCall verification
        └── Ceremony flows

v0.9.1: Economic Integration
        ├── Fuel allocation
        ├── Capability-gated transactions
        └── Membership-ledger link

v1.0.0: Full Commons Release
        └── Network governance + Sybil resistance
```

---

## Pilot Integration Notes

For the pilot community (Track C), the minimum required functionality:

1. **Working Today** ✅:
   - Enrollment via SDIS (steward-sponsored)
   - CommonsHolderRecord with baseline rights
   - Charter creation for pilot cooperative
   - Membership lifecycle management
   - CLI commands for daily operations (`icnctl amendment`, `icnctl appeal`)
   - Persistent storage via Sled (data survives restarts)
   - Basic monitoring/observability (50+ Commons metrics)
   - Audit logging for governance compliance
   - Per-category rate limiting for API protection

2. **Can Wait Until Post-Pilot**:
   - Multiple POP pathways (InPerson, VideoCall, Ceremony)
   - Federation support
   - Economic layer integration
   - Advanced appeal mechanisms

---

## Commit History

```
# v0.8.x - Production Hardening
6ac5f30 feat(gateway): add per-category rate limiting for endpoint protection
b407042 feat(gateway): add comprehensive audit logging for governance actions
0ea62b6 feat(obs): add Commons Evolution metrics for monitoring
9d1c3a5 feat(gateway): add Sled persistent storage backend for CommonsManager

# v0.7.x - Integration & Testing
b313562 feat(gateway): complete Commons Evolution integration (CLI, storage, enrollment)

# v0.6.x - Constitutional Governance
69c5674 feat(governance): implement Constitutional Governance (v0.6.0)
8d4e57a feat(gateway): implement Membership & Rights (v0.5.0)
b990ef9 feat(gateway): implement Steward Layer (v0.4.0)

# v0.3.x-0.5.x - Foundation
6c0931f docs: update COMMONS_EVOLUTION RFC with implementation status
00d8e95 feat(gateway): implement Commons Evolution API endpoints
90d250a feat(cli): add commons and charter commands to icnctl
e0f9a36 feat(governance): add Charter and Steward storage layers
3d92c1f feat(governance): add Charter for Commons Evolution organization genesis
959e295 feat(identity): add CommonsHolderStore for persistent storage
cd61058 feat(identity): add CommonsHolderRecord for Commons Evolution Layer 1
31a3955 feat(identity): add PersonhoodAnchorStore for persistent storage
fffe0c4 feat(identity): add PersonhoodAnchor for Commons Evolution Layer 0
```

---

## Conclusion

The Commons Evolution core infrastructure is complete. The system provides:

1. **Portable Identity**: PersonhoodAnchor survives key rotation and organizational changes
2. **Baseline Rights**: CommonsRights cannot be revoked by individual jurisdictions
3. **Democratic Governance**: Multi-level amendment process with due process
4. **Due Process**: Appeal mechanisms for challenging decisions

The next phase is integration with pilot workflows and production hardening based on real-world usage.
