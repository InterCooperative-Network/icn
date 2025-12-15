# RFC: ICN Commons Evolution

**Document ID**: ICN-RFC-COMMONS-01
**Status**: IMPLEMENTED (v0.3.0 Foundation)
**Author**: Matthew Faherty
**Created**: 2025-12-14
**Updated**: 2025-12-14
**Target Version**: v0.3.0+

### Implementation Status (2025-12-14)

**v0.3.0 Foundation - COMPLETE**:
- PersonhoodAnchor, CommonsHolderRecord, Charter structs
- Gateway REST APIs (`/v1/commons/*`, `/v1/charter/*`)
- SDIS enrollment creates PersonhoodAnchor + CommonsHolderRecord
- CLI commands (`icnctl commons anchor`, `icnctl charter show/list`)
- Integration tests for full enrollment → anchor → holder flow

**v0.4.0 Stewardship - COMPLETE**:
- StewardRecord management in CommonsManager with full lifecycle
- Gateway REST API (`/v1/steward/*`) with 13 endpoints
- POP attestation workflow integrated with steward validation
- Reputation tracking (attestations issued, disputes, disputes won)
- Term and bond management (extend term, add/slash bond)
- CLI commands (`icnctl steward info/list/attesters/register/retire`)
- 14 steward integration tests

**v0.5.0 Membership & Rights - COMPLETE**:
- RevocationRecord and RevocationRegistry for tracking revocations with appeals
- Membership management API (`/v1/membership/*`) with 15 endpoints
- Full membership lifecycle: apply, approve, promote, suspend, reinstate, exit
- Capability management: grant, revoke, check capabilities per jurisdiction
- Role management: add/remove roles for members
- Ban/revoke with appeal process and due process support
- Rights enforcement through revocation checks before actions

---

## Abstract

This RFC proposes evolving ICN from a "cooperative coordination tool" to a "global commons infrastructure." The core insight: **Commons Holder is the root identity; membership is scoped capability grants under that identity.**

This enables:
- Portable identity across jurisdictions
- Baseline rights that can't be revoked locally
- Constitutional governance at network scale
- True "global participation" without centralized authority

---

## Motivation

### Current Model (v0.2.x)

```
Person → creates DID → joins Coop → joins Federation
         (identity)    (membership)  (inter-coop)
```

**Limitations:**
- Identity is siloed to cooperative context
- No baseline rights independent of membership
- No formal distinction between "person" and "key"
- No constitutional layer above federations

### Proposed Model (v0.3.x+)

```
Person → PersonhoodAnchor → CommonsHolder → Affiliations
         (proof-of-person)   (baseline rights)  (coop/community/federation)
```

**Benefits:**
- Identity survives key rotation and organizational changes
- Baseline rights (voice, due process, exit) are network-level
- Cooperatives become "jurisdictions" within a commons
- Constitutional governance enables network-wide decisions

---

## Conceptual Framework

### The Three Layers

| Layer | Concept | Persistence | Revocable By |
|-------|---------|-------------|--------------|
| **Layer 0** | PersonhoodAnchor | Permanent | Network governance only |
| **Layer 1** | CommonsHolderRecord | Long-lived | Network governance (with appeal) |
| **Layer 2** | Affiliation/Membership | Jurisdiction-scoped | Jurisdiction governance |

### Key Principle

> A cooperative can revoke your membership.
> A cooperative cannot revoke your personhood.

This separation protects individuals from local power abuse while preserving organizational autonomy.

---

## New Domain Types

Current ICN uses flat domain identifiers. This RFC introduces hierarchical domains:

```
network:commons          # Global commons layer
network:governance       # Network-level constitutional governance
network:stewards         # Steward registry and oversight

federation:<id>          # Regional/sectoral federations
coop:<id>                # Economic cooperatives
community:<id>           # Civic communities (mutual aid, advocacy)
```

### Domain Relationships

```
network:commons
    ├── network:governance
    ├── network:stewards
    ├── federation:pacific-nw
    │   ├── coop:food-portland
    │   ├── coop:housing-seattle
    │   └── community:mutual-aid-tacoma
    └── federation:northeast
        ├── coop:worker-boston
        └── community:timebank-nyc
```

---

## New Core Data Structures

### 1. PersonhoodAnchor (Layer 0)

The cryptographic root of a human's participation, independent of key rotation.

```rust
/// Proof of personhood anchor - persistent across key rotations
pub struct PersonhoodAnchor {
    /// Unique anchor ID (hash of initial attestation bundle)
    pub anchor_id: Hash,

    /// Current active public key (rotatable)
    pub current_key: PublicKey,

    /// Key rotation history
    pub key_history: Vec<KeyRotationEvent>,

    /// Proof-of-personhood attestations
    pub pop_attestations: Vec<POPAttestation>,

    /// Uniqueness proofs (cross-anchor deduplication)
    pub uniqueness_proofs: Vec<UniquenessProof>,

    /// Anchor creation timestamp
    pub created_at: Timestamp,

    /// Current status
    pub status: AnchorStatus,
}

pub enum AnchorStatus {
    Active,
    Suspended { reason: String, until: Option<Timestamp> },
    Revoked { reason: String, evidence: Vec<Hash> },
}

/// Proof-of-personhood attestation from a steward or organization
pub struct POPAttestation {
    /// Issuer (steward or organizational DID)
    pub issuer_did: DID,

    /// Verification method used
    pub method: POPMethod,

    /// Strength level of verification
    pub level: POPLevel,

    /// When attestation was issued
    pub timestamp: Timestamp,

    /// Optional expiry (some methods require renewal)
    pub expiry: Option<Timestamp>,

    /// Cryptographic signature
    pub signature: Signature,
}

pub enum POPMethod {
    /// In-person verification by steward
    InPerson,
    /// Sponsored by existing verified holder
    Sponsored { sponsor_did: DID },
    /// Biometric verification (privacy-preserving)
    Biometric,
    /// Community ceremony (e.g., key signing party)
    Ceremony { witnesses: Vec<DID> },
}

pub enum POPLevel {
    /// Basic - single attestation, limited capabilities
    Weak,
    /// Standard - multiple attestations or strong method
    Strong,
    /// High assurance - multiple methods, periodic renewal
    Verified,
}
```

### 2. CommonsHolderRecord (Layer 1)

A human participant's record in the global commons.

```rust
/// Record of a Commons Holder - a human participant with baseline rights
pub struct CommonsHolderRecord {
    /// The holder's current DID
    pub holder_did: DID,

    /// Link to underlying PersonhoodAnchor
    pub anchor_id: Hash,

    /// Holder status
    pub status: HolderStatus,

    /// Current personhood verification level
    pub personhood_level: POPLevel,

    /// Affiliations with jurisdictions
    pub affiliations: Vec<Affiliation>,

    /// Baseline rights (network-level, non-revocable by jurisdictions)
    pub baseline_rights: CommonsRights,

    /// When holder entered the commons
    pub created_at: Timestamp,

    /// Last governance review (for periodic re-verification)
    pub last_review_at: Option<Timestamp>,
}

pub enum HolderStatus {
    Active,
    Suspended { reason: String, appeal_deadline: Timestamp },
    Exited { reason: String, exit_timestamp: Timestamp },
    Revoked { reason: String, evidence: Vec<Hash> },
}

/// Affiliation with a jurisdiction (coop, community, or federation)
pub struct Affiliation {
    /// The jurisdiction domain ID
    pub jurisdiction_id: DomainId,

    /// Membership status within this jurisdiction
    pub membership_status: MembershipStatus,

    /// Roles held in this jurisdiction
    pub roles: Vec<Role>,

    /// When affiliation began
    pub joined_at: Timestamp,
}

/// Baseline rights held by all Commons Holders
/// These CANNOT be revoked by individual jurisdictions
pub struct CommonsRights {
    /// Right to vote in commons-level governance
    pub vote_in_commons: bool,

    /// Right to petition for constitutional changes
    pub petition: bool,

    /// Guaranteed minimum fuel for civic participation
    pub minimal_fuel_floor: bool,

    /// Right to due process before suspension/revocation
    pub due_process: bool,

    /// Right to exit any jurisdiction freely
    pub exit_freely: bool,

    /// Right to audit public records
    pub audit_access: bool,
}
```

### 3. Charter (Organization Genesis)

The founding document that creates a cooperative, community, or federation.

```rust
/// Charter - founding document for a jurisdiction
pub struct Charter {
    /// Unique charter ID
    pub charter_id: Hash,

    /// Type of organization
    pub org_type: OrgType,

    /// Domain ID for this jurisdiction
    pub domain_id: DomainId,

    /// Human-readable name
    pub name: String,

    /// Hash of founding document (immutable reference)
    pub founding_document_hash: Hash,

    /// Founding signatures
    pub founders: Vec<FounderSignature>,

    /// Governance configuration
    pub governance_profile: GovernanceProfile,

    /// Membership rules
    pub membership_policy: MembershipPolicy,

    /// Economic rules (if applicable)
    pub economic_policy: Option<EconomicPolicy>,

    /// Dispute resolution rules
    pub dispute_policy: DisputePolicy,

    /// Initial network endpoints
    pub bootstrap_endpoints: Vec<Endpoint>,

    /// Charter creation timestamp
    pub created_at: Timestamp,

    /// Amendment history
    pub amendments: Vec<AmendmentRef>,
}

pub enum OrgType {
    /// Economic cooperative (has ledger, treasury)
    Cooperative,
    /// Civic community (mutual aid, advocacy, stewardship)
    Community,
    /// Federation of coops/communities
    Federation,
}

pub struct FounderSignature {
    pub holder_did: DID,
    pub signature: Signature,
    pub timestamp: Timestamp,
}

pub struct MembershipPolicy {
    /// Minimum sponsors required
    pub min_sponsors: u32,
    /// Probation period for new members
    pub probation_duration: Option<Duration>,
    /// Whether membership is open or invite-only
    pub open_enrollment: bool,
    /// Required training/orientation
    pub training_requirements: Vec<TrainingId>,
}
```

### 4. MembershipRecord (Layer 2)

Jurisdiction-scoped membership with capabilities.

```rust
/// Membership record for a specific jurisdiction
pub struct MembershipRecord {
    /// The member's DID
    pub member_did: DID,

    /// The jurisdiction this membership is in
    pub jurisdiction_id: DomainId,

    /// Membership status
    pub status: MembershipStatus,

    /// Capabilities granted by this membership
    pub capabilities: Vec<Capability>,

    /// Sponsors who vouched for this member
    pub sponsors: Vec<SponsorSignature>,

    /// Membership conditions (probation, bonds, etc.)
    pub conditions: MembershipConditions,

    /// When membership began
    pub start_time: Timestamp,

    /// Optional expiry (for term-limited memberships)
    pub expiry: Option<Timestamp>,

    /// If revoked, the reason
    pub revocation_reason: Option<RevocationReason>,
}

pub enum MembershipStatus {
    Candidate,    // Applied, not yet approved
    Provisional,  // Approved, in probation period
    Member,       // Full member
    Suspended,    // Temporarily suspended
    Exited,       // Voluntarily left
    Banned,       // Removed by governance
}

pub enum Capability {
    /// Can vote in jurisdiction governance
    Vote,
    /// Can create proposals
    Propose,
    /// Can participate in economic transactions
    Transact,
    /// Can hold elected/appointed positions
    HoldOffice,
    /// Can access private/internal resources
    AccessPrivate,
    /// Can invite/sponsor new members
    Sponsor,
}

pub struct MembershipConditions {
    pub probation_duration: Option<Duration>,
    pub contribution_threshold: Option<u64>,
    pub bond_requirement: Option<Amount>,
    pub training_requirements: Vec<TrainingId>,
}
```

### 5. StewardRecord

Trusted operators who perform identity verification and infrastructure maintenance.

```rust
/// Record of a network steward
pub struct StewardRecord {
    /// Steward's operational DID
    pub steward_did: DID,

    /// Must be a Commons Holder
    pub holder_did: DID,

    /// Steward status
    pub status: StewardStatus,

    /// Geographic or sectoral scope (None = global)
    pub jurisdiction: Option<DomainId>,

    /// Term start
    pub term_start: Timestamp,

    /// Term end (stewards have limited terms)
    pub term_end: Timestamp,

    /// Bond posted (stake against misbehavior)
    pub bond_amount: Amount,

    /// Computed reputation from attestation outcomes
    pub reputation_score: f64,

    /// Total attestations issued
    pub attestations_issued: u64,

    /// Disputes filed against this steward
    pub disputes_against: u64,

    /// Governance approval reference
    pub governance_approval: GovernanceRef,
}

pub enum StewardStatus {
    Active,
    Suspended { reason: String },
    Retired,
    Revoked { reason: String },
}
```

### 6. RevocationRecord

Records of revoked credentials, memberships, or anchors.

```rust
/// Record of a revocation action
pub struct RevocationRecord {
    /// Unique revocation ID
    pub revocation_id: Hash,

    /// What type of thing was revoked
    pub target_type: RevocationType,

    /// ID of the revoked item
    pub target_id: Hash,

    /// Scope of revocation
    pub scope: RevocationScope,

    /// Authority that issued revocation
    pub authority: DID,

    /// Reason for revocation
    pub reason: RevocationReason,

    /// Evidence references
    pub evidence_refs: Vec<Hash>,

    /// Appeal deadline
    pub appeal_deadline: Timestamp,

    /// Appeal record (if appealed)
    pub appeal_record: Option<AppealRef>,

    /// When revocation takes effect
    pub effective_at: Timestamp,
}

pub enum RevocationType {
    Anchor,      // PersonhoodAnchor revocation
    Holder,      // CommonsHolderRecord revocation
    Membership,  // Jurisdiction membership revocation
    Credential,  // Specific credential revocation
}

pub enum RevocationScope {
    Global,       // Network-wide
    Federation,   // Within a federation
    Jurisdiction, // Within a single coop/community
}
```

---

## New CLI Commands

### Commons Enrollment

```bash
# Enroll as Commons Holder (requires steward attestation)
icnctl commons enroll --steward did:icn:STEWARD_DID

# View your Commons status
icnctl commons status

# View your affiliations across jurisdictions
icnctl commons affiliations

# Exit the commons (voluntary, irreversible)
icnctl commons exit --reason "Moving to different network"
```

### Charter Management

```bash
# Create a new cooperative
icnctl charter create \
  --type coop \
  --name "My Food Cooperative" \
  --domain-id "coop:my-food-coop" \
  --governance-profile consensus-with-fallback \
  --founders did:icn:FOUNDER1,did:icn:FOUNDER2,did:icn:FOUNDER3

# Create a community
icnctl charter create \
  --type community \
  --name "Mutual Aid Queens" \
  --domain-id "community:mutual-aid-queens" \
  --governance-profile sociocracy-consent

# View charter details
icnctl charter show --domain-id "coop:my-food-coop"

# Propose charter amendment
icnctl charter amend \
  --domain-id "coop:my-food-coop" \
  --amendment-file ./proposed-amendment.md
```

### Membership Management

```bash
# Apply for membership in a jurisdiction
icnctl membership apply --jurisdiction "coop:my-food-coop"

# View membership status
icnctl membership status --jurisdiction "coop:my-food-coop"

# Approve membership application (admin)
icnctl membership approve \
  --jurisdiction "coop:my-food-coop" \
  --applicant did:icn:APPLICANT_DID

# Suspend membership (admin, requires due process)
icnctl membership suspend \
  --jurisdiction "coop:my-food-coop" \
  --member did:icn:MEMBER_DID \
  --reason "Pending dispute resolution" \
  --dispute-id DISPUTE_ID

# Exit membership voluntarily
icnctl membership exit --jurisdiction "coop:my-food-coop"
```

### Steward Operations

```bash
# Issue proof-of-personhood attestation
icnctl steward attest-pop \
  --target did:icn:NEW_HOLDER_DID \
  --method in_person \
  --level strong

# View steward statistics
icnctl steward stats

# List stewards in a jurisdiction
icnctl steward list --jurisdiction "federation:pacific-nw"
```

---

## New CCL Contracts

### commons-core-v1.ccl

```
contract CommonsCore {
    // Commons Holder lifecycle
    rule enroll_holder(anchor: AnchorId, attestations: Vec<POPAttestation>) {
        require attestations.len() >= 2;
        require all_valid_stewards(attestations);
        create_holder_record(anchor, attestations);
    }

    rule suspend_holder(holder: DID, reason: String, evidence: Vec<Hash>) {
        require has_governance_authority(sender);
        require evidence.len() >= 1;
        set_holder_status(holder, suspended);
        create_appeal_window(holder, 30 days);
    }

    // Baseline rights enforcement
    query has_right(holder: DID, right: CommonsRight) -> bool {
        let record = get_holder_record(holder);
        return record.status == active && record.baseline_rights.contains(right);
    }
}
```

### charter-v1.ccl

```
contract CharterManagement {
    rule create_charter(
        org_type: OrgType,
        domain_id: DomainId,
        founders: Vec<DID>,
        document_hash: Hash,
        governance_profile: String
    ) {
        require founders.len() >= 3;
        require all_commons_holders(founders);
        require all_signed(founders, document_hash);
        create_charter_record(org_type, domain_id, founders, document_hash, governance_profile);
        register_domain(domain_id);
    }

    rule amend_charter(domain_id: DomainId, amendment: Amendment) {
        require has_amendment_authority(sender, domain_id);
        require amendment_ratified(domain_id, amendment);
        apply_amendment(domain_id, amendment);
    }
}
```

### membership-v1.ccl

```
contract Membership {
    rule apply(jurisdiction: DomainId, applicant: DID) {
        require is_commons_holder(applicant);
        require jurisdiction_accepting_applications(jurisdiction);
        create_application(jurisdiction, applicant);
    }

    rule approve(jurisdiction: DomainId, applicant: DID, sponsors: Vec<DID>) {
        require has_approval_authority(sender, jurisdiction);
        require meets_sponsorship_requirements(jurisdiction, sponsors);
        create_membership_record(jurisdiction, applicant, sponsors);
        assign_initial_capabilities(jurisdiction, applicant);
    }

    rule revoke(jurisdiction: DomainId, member: DID, reason: String) {
        require has_revocation_authority(sender, jurisdiction);
        require due_process_satisfied(jurisdiction, member, reason);
        set_membership_status(member, jurisdiction, revoked);
        // Note: Does NOT affect Commons Holder status
    }
}
```

### commons-governance-v1.ccl

```
contract CommonsGovernance {
    // Network-level constitutional governance

    rule propose_amendment(amendment: ConstitutionalAmendment) {
        require is_commons_holder(sender);
        require fuel_available(sender, 100);
        create_constitutional_proposal(amendment);
    }

    rule ratify_amendment(proposal_id: ProposalId) {
        require holder_vote_threshold(proposal_id, 0.67);
        require jurisdiction_vote_threshold(proposal_id, 0.67);
        require federation_balance_check(proposal_id);
        apply_constitutional_change(proposal_id);
    }

    rule appoint_steward(candidate: DID, jurisdiction: Option<DomainId>) {
        require is_commons_holder(candidate);
        require steward_requirements_met(candidate);
        require governance_approval(candidate);
        create_steward_record(candidate, jurisdiction);
    }
}
```

---

## Glossary Additions

### New Terms

| Term | Definition |
|------|------------|
| **The Commons** | The shared global coordination layer of ICN: identity, rights, governance, and economic participation held in common by all participants |
| **Commons Holder** | A human participant recognized by the network as holding baseline rights and responsibilities in the commons |
| **Personhood Anchor** | The cryptographic root of a Commons Holder's identity, independent of key rotation; proves a unique human behind the keys |
| **Charter** | The founding document that creates a cooperative, community, or federation as a jurisdiction within the commons |
| **Jurisdiction** | A coop, community, or federation with its own charter, governance, and membership rules; operates within the commons framework |
| **Steward** | A trusted operator who performs identity verification, infrastructure maintenance, or governance functions on behalf of the network |
| **Commons Rights** | The baseline rights held by all Commons Holders: voice, due process, exit, audit access; cannot be revoked by individual jurisdictions |
| **Affiliation** | A Commons Holder's membership relationship with a specific jurisdiction, granting additional capabilities within that scope |
| **Proof-of-Personhood (POP)** | Verification that a Commons Holder represents a unique human, not a bot or duplicate identity |

### Updated Terms

| Term | Old Definition | New Definition |
|------|----------------|----------------|
| **Membership** | Relationship with a cooperative | A Commons Holder's scoped affiliation with a specific jurisdiction, granting additional capabilities beyond baseline rights |
| **Governance Domain** | Decision-making context | A jurisdiction or the commons itself, with defined governance rules and decision authority |

---

## Migration Path

### Phase 1: v0.2.x → v0.3.0 (Non-Breaking)

1. Add new structs as optional/parallel to existing
2. Existing DIDs continue to work unchanged
3. New `commons enroll` command available but not required
4. Charter creation optional for existing coops

### Phase 2: v0.3.x → v0.4.0 (Soft Migration)

1. Steward network bootstrapped
2. POP attestation available
3. Existing members can opt-in to Commons Holder status
4. New members encouraged to enroll via steward

### Phase 3: v0.5.0+ (Full Commons)

1. Commons enrollment becomes standard path
2. Baseline rights enforcement active
3. Constitutional governance operational
4. Legacy DID-only mode deprecated (but still functional)

---

## Implementation Roadmap

| Version | Milestone | Status | Key Deliverables |
|---------|-----------|--------|------------------|
| **v0.3.0** | Commons Foundation | **COMPLETE** | PersonhoodAnchor, CommonsHolderRecord, Charter structs; Gateway APIs; SDIS integration; CLI commands |
| **v0.4.0** | Stewardship | **COMPLETE** | StewardRecord, POP attestation workflow, steward governance, reputation tracking, bond/term management |
| **v0.5.0** | Membership & Rights | **COMPLETE** | RevocationRecord/Registry, 15 membership API endpoints, full lifecycle management, capability/role system |
| **v0.6.0** | Constitutional Governance | Planned | Amendment process, multi-level ratification, appeal mechanisms |
| **v1.0.0** | Production Commons | Planned | Full steward network, multiple POP pathways, complete economic layer |

---

## Security Considerations

### Steward Trust Model

Stewards are trusted to verify personhood honestly. Mitigations:
- **Multiple attestations required**: No single steward can create a Commons Holder
- **Term limits**: Stewards serve fixed terms, preventing entrenchment
- **Bonding**: Stewards post bonds that can be slashed for misbehavior
- **Reputation tracking**: Historical accuracy affects future trust
- **Governance oversight**: Network can remove rogue stewards

### Sybil Resistance

The POP system must prevent one human from having multiple Commons Holder identities:
- **Cross-attestation requirements**: Different stewards or methods
- **Uniqueness proofs**: Cryptographic commitments that can detect duplicates
- **Social graph analysis**: Unusual patterns flag review
- **Periodic renewal**: Some verification methods expire

### Jurisdiction Abuse

A jurisdiction cannot:
- Revoke baseline Commons Rights
- Prevent a holder from exiting
- Block access to commons-level governance
- Interfere with other jurisdiction affiliations

A jurisdiction can:
- Set its own membership criteria
- Revoke membership (with due process)
- Define internal capabilities and roles
- Establish economic policies for its scope

---

## Open Questions

1. **POP Privacy**: How do we verify personhood without creating a surveillance database?
2. **Steward Bootstrap**: Who are the first stewards, and how are they selected?
3. **Cross-Network**: Can a Commons Holder have identity in multiple ICN networks?
4. **Legal Entity Mapping**: How do jurisdictions map to legal cooperatives?
5. **Recovery**: If PersonhoodAnchor is compromised, what's the recovery path?

---

## References

- [ARCHITECTURE.md](ARCHITECTURE.md) - Current system architecture
- [governance-primitives.md](governance-primitives.md) - Existing governance layer
- [multi-device-identity-design.md](multi-device-identity-design.md) - Key rotation patterns
- [threat-model.md](threat-model.md) - Security considerations

---

## Changelog

- **2025-12-14**: Initial RFC draft
