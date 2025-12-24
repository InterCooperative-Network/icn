# CooperativeEntity Type Specification

**Date**: 2025-12-24
**Phase**: 19 - Cooperative Entity Foundation
**Status**: DRAFT
**Related**: [COOPERATIVE_MIDDLE_LAYER_GAP_ANALYSIS.md](COOPERATIVE_MIDDLE_LAYER_GAP_ANALYSIS.md)

## Overview

This document specifies the `CooperativeEntity` type - a unified recursive model for all participants in the ICN ecosystem. The goal is to replace the current fragmented types (DIDs for individuals, CooperativeInfo for coops, implicit federation relationships) with a single composable type that works at every scale.

## Design Principles

1. **Recursive Composition**: Entities contain entities - individuals form coops, coops form federations
2. **Uniform Interface**: Same governance, economic, and trust APIs work at every level
3. **Identity Anchoring**: Each entity has a cryptographic anchor (DID or threshold-derived)
4. **Explicit Membership**: Relationships between entities are first-class, not implicit
5. **Audit Trail**: All entity state changes are logged with timestamps and attestations

---

## Core Types

### EntityId

A universally unique identifier for any cooperative entity.

```rust
/// Unique identifier for a cooperative entity
/// Format: "entity:{type}:{namespace}:{local_id}"
/// Examples:
///   - "entity:person:icn:did:icn:z123..."
///   - "entity:coop:food-network:sunshine-coop"
///   - "entity:federation:regional:pacific-northwest"
///   - "entity:commons:global:icn-protocol"
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(String);

impl EntityId {
    pub fn person(did: &Did) -> Self {
        Self(format!("entity:person:icn:{did}"))
    }

    pub fn coop(namespace: &str, local_id: &str) -> Self {
        Self(format!("entity:coop:{namespace}:{local_id}"))
    }

    pub fn federation(namespace: &str, local_id: &str) -> Self {
        Self(format!("entity:federation:{namespace}:{local_id}"))
    }

    pub fn commons(local_id: &str) -> Self {
        Self(format!("entity:commons:global:{local_id}"))
    }

    pub fn entity_type(&self) -> EntityType {
        // Parse from string
    }
}
```

### EntityType

The level in the cooperative hierarchy.

```rust
/// The type/level of a cooperative entity
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityType {
    /// Individual person with SDIS anchor
    Person,

    /// Working group within a cooperative
    WorkingGroup,

    /// A cooperative organization
    Cooperative,

    /// Federation of cooperatives
    Federation,

    /// Meta-federation or global commons (e.g., ICN Protocol itself)
    Commons,
}

impl EntityType {
    /// Returns the parent type in the hierarchy
    pub fn parent_type(&self) -> Option<EntityType> {
        match self {
            EntityType::Person => Some(EntityType::Cooperative),
            EntityType::WorkingGroup => Some(EntityType::Cooperative),
            EntityType::Cooperative => Some(EntityType::Federation),
            EntityType::Federation => Some(EntityType::Commons),
            EntityType::Commons => None,
        }
    }

    /// Returns child types that can be members
    pub fn allowed_member_types(&self) -> Vec<EntityType> {
        match self {
            EntityType::Person => vec![],
            EntityType::WorkingGroup => vec![EntityType::Person],
            EntityType::Cooperative => vec![EntityType::Person, EntityType::WorkingGroup],
            EntityType::Federation => vec![EntityType::Cooperative],
            EntityType::Commons => vec![EntityType::Federation],
        }
    }
}
```

### CooperativeEntity

The core entity type representing any participant at any level.

```rust
/// A cooperative entity at any level of the hierarchy
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CooperativeEntity {
    /// Unique identifier for this entity
    pub id: EntityId,

    /// Human-readable name
    pub name: String,

    /// Type/level in the hierarchy
    pub entity_type: EntityType,

    /// Cryptographic identity anchor
    pub anchor: EntityAnchor,

    /// Parent entity (if any)
    pub parent_id: Option<EntityId>,

    /// Entity lifecycle state
    pub state: EntityState,

    /// Governance configuration
    pub governance: GovernanceConfig,

    /// Economic configuration
    pub economics: EconomicConfig,

    /// Trust configuration
    pub trust: TrustConfig,

    /// Creation timestamp
    pub created_at: Timestamp,

    /// Last modification timestamp
    pub updated_at: Timestamp,

    /// Metadata (tags, descriptions, external links)
    pub metadata: EntityMetadata,
}
```

### EntityAnchor

The cryptographic anchor for entity identity.

```rust
/// Cryptographic anchor for entity identity
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EntityAnchor {
    /// Individual anchor (SDIS personal identity)
    Personal {
        /// The person's DID
        did: Did,
        /// VUI commitment for uniqueness
        vui_commitment: [u8; 32],
        /// Creation ceremony proof
        ceremony_proof: Option<CeremonyProof>,
    },

    /// Cooperative anchor (threshold-derived from member signatures)
    Cooperative {
        /// Derived DID for the cooperative
        did: Did,
        /// Threshold (k of n) for signing
        threshold: Threshold,
        /// Current key holders (member DIDs with signing authority)
        key_holders: Vec<Did>,
        /// Creation ceremony proof
        ceremony_proof: CeremonyProof,
    },

    /// Federation anchor (derived from member cooperatives)
    Federation {
        /// Derived DID for the federation
        did: Did,
        /// Member coops that contribute to anchor
        anchor_members: Vec<EntityId>,
        /// Threshold for federation-level signing
        threshold: Threshold,
    },

    /// Genesis anchor (for ICN Protocol Commons)
    Genesis {
        /// The genesis DID
        did: Did,
        /// Genesis block hash
        genesis_hash: [u8; 32],
    },
}

/// Threshold configuration for multi-party anchors
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Threshold {
    /// Required signatures
    pub k: u32,
    /// Total signers
    pub n: u32,
}
```

### EntityState

The lifecycle state of an entity.

```rust
/// Entity lifecycle state
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityState {
    /// Entity is being formed (not yet operational)
    Forming {
        /// Required steps to become active
        pending_steps: Vec<FormationStep>,
    },

    /// Entity is active and operational
    Active,

    /// Entity is suspended (governance action)
    Suspended {
        reason: String,
        suspended_at: Timestamp,
        suspended_by: EntityId,
    },

    /// Entity is being dissolved
    Dissolving {
        reason: String,
        dissolution_started_at: Timestamp,
        assets_transferred_to: Option<EntityId>,
    },

    /// Entity is dissolved (historical record only)
    Dissolved {
        dissolved_at: Timestamp,
        final_state_hash: [u8; 32],
    },
}

/// Steps required during entity formation
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormationStep {
    /// Minimum number of founding members
    MinimumMembers { required: u32, current: u32 },

    /// Charter ratification
    CharterRatification { ratified: bool },

    /// Initial treasury funding
    TreasuryFunding { minimum: i64, current: i64, currency: String },

    /// Anchor ceremony completion
    AnchorCeremony { completed: bool },

    /// Parent entity approval (if applicable)
    ParentApproval { approved: bool },
}
```

---

## Membership Model

### Membership

Represents the relationship between an entity and its parent.

```rust
/// Membership of an entity within another entity
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Membership {
    /// The member entity
    pub member_id: EntityId,

    /// The containing entity (parent)
    pub parent_id: EntityId,

    /// Type of membership
    pub membership_type: MembershipType,

    /// Roles assigned to this member
    pub roles: Vec<Role>,

    /// Membership state
    pub state: MembershipState,

    /// When membership was established
    pub joined_at: Timestamp,

    /// Sponsor who vouched for this member (if applicable)
    pub sponsored_by: Option<EntityId>,

    /// Contribution credits (for credit limit calculations)
    pub contribution_score: u64,
}

/// Type of membership
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembershipType {
    /// Full voting member
    Full,

    /// Probationary member (limited voting, time-limited)
    Probationary { expires_at: Timestamp },

    /// Associate member (no voting, economic participation only)
    Associate,

    /// Observer (read-only access)
    Observer,
}

/// Membership lifecycle state
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembershipState {
    /// Application pending approval
    Pending { application_id: String },

    /// Active member
    Active,

    /// Suspended (can be reactivated)
    Suspended { reason: String, until: Option<Timestamp> },

    /// Voluntarily withdrawn
    Withdrawn { at: Timestamp },

    /// Expelled (cannot rejoin without appeal)
    Expelled { at: Timestamp, reason: String },
}

/// Role within an entity
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    pub name: String,
    pub permissions: Vec<Permission>,
    pub granted_at: Timestamp,
    pub expires_at: Option<Timestamp>,
}

/// Permissions a role can grant
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    /// Can vote on proposals
    Vote,
    /// Can create proposals
    Propose,
    /// Can approve new members
    ApproveMembership,
    /// Can sign on behalf of entity
    Sign,
    /// Can manage treasury
    Treasury,
    /// Can manage governance parameters
    Governance,
    /// Full administrative access
    Admin,
}
```

---

## Governance Configuration

```rust
/// Governance configuration for an entity
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GovernanceConfig {
    /// Governance domain ID (links to icn-governance)
    pub domain_id: GovernanceDomainId,

    /// Voting mechanism
    pub voting: VotingConfig,

    /// Quorum requirements
    pub quorum: QuorumConfig,

    /// Proposal types allowed at this level
    pub allowed_proposal_types: Vec<ProposalType>,

    /// Constraints from parent entity
    pub parent_constraints: Vec<Constraint>,
}

/// Voting configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VotingConfig {
    /// How votes are weighted
    pub weight_model: VoteWeightModel,

    /// Delegation allowed?
    pub delegation_enabled: bool,

    /// Maximum delegation depth
    pub max_delegation_depth: u32,
}

/// Vote weighting models
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteWeightModel {
    /// One member, one vote
    EqualWeight,

    /// Weighted by contribution score
    ContributionWeighted { cap: Option<f64> },

    /// Quadratic voting
    Quadratic { credits_per_period: u64 },

    /// Conviction voting (time-weighted)
    Conviction { decay_rate: f64 },
}

/// Quorum requirements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuorumConfig {
    /// Minimum participation rate (0.0 - 1.0)
    pub participation_threshold: f64,

    /// Approval threshold for passing (0.0 - 1.0)
    pub approval_threshold: f64,

    /// Different thresholds per proposal type
    pub type_overrides: HashMap<ProposalType, (f64, f64)>,
}
```

---

## Economic Configuration

```rust
/// Economic configuration for an entity
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EconomicConfig {
    /// Entity's treasury (if applicable)
    pub treasury: Option<TreasuryConfig>,

    /// Credit policy for members
    pub credit_policy: CreditPolicyConfig,

    /// Currencies supported
    pub currencies: Vec<CurrencyConfig>,

    /// Inter-entity agreements
    pub agreements: Vec<InterEntityAgreement>,
}

/// Treasury configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TreasuryConfig {
    /// Treasury DID (for ledger accounts)
    pub treasury_did: Did,

    /// Spending approval thresholds
    pub spending_thresholds: Vec<SpendingThreshold>,

    /// Budget allocation method
    pub budget_method: BudgetMethod,
}

/// Credit policy for entity members
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreditPolicyConfig {
    /// Base credit limit for new members
    pub base_limit: i64,

    /// Trust-based bonus multiplier (0.0 - 1.0 trust score)
    pub trust_multiplier: f64,

    /// Contribution-based bonus
    pub contribution_bonus_rate: f64,

    /// Ramp period for new members (seconds)
    pub ramp_period_seconds: u64,

    /// Anti-extraction ratio (max debit/credit ratio)
    pub max_debit_credit_ratio: f64,
}

/// Agreement between entities
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InterEntityAgreement {
    /// Agreement ID
    pub id: String,

    /// Parties to the agreement
    pub parties: Vec<EntityId>,

    /// Agreement type
    pub agreement_type: AgreementType,

    /// Terms
    pub terms: AgreementTerms,

    /// State
    pub state: AgreementState,
}

/// Types of inter-entity agreements
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgreementType {
    /// Mutual credit line between entities
    CreditLine,

    /// Clearing/settlement agreement
    Clearing,

    /// Group purchasing participation
    GroupPurchasing,

    /// Federation membership
    FederationMembership,

    /// Service provision
    ServiceAgreement,
}
```

---

## Trust Configuration

```rust
/// Trust configuration for an entity
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustConfig {
    /// Trust policies for different relationship types
    pub policies: Vec<TrustPolicy>,

    /// Minimum trust for various operations
    pub thresholds: TrustThresholds,

    /// Trust decay settings
    pub decay: TrustDecay,
}

/// Trust thresholds for operations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustThresholds {
    /// Minimum trust to join (sponsor trust)
    pub join: f64,

    /// Minimum trust to transact
    pub transact: f64,

    /// Minimum trust to vote
    pub vote: f64,

    /// Minimum trust to propose
    pub propose: f64,

    /// Minimum trust to sign on behalf of entity
    pub sign: f64,
}
```

---

## Entity Registry

The entity registry maintains the global state of all entities.

```rust
/// Entity registry for storing and querying entities
pub trait EntityRegistry: Send + Sync {
    /// Create a new entity
    async fn create(&self, entity: CooperativeEntity) -> Result<EntityId>;

    /// Get entity by ID
    async fn get(&self, id: &EntityId) -> Result<Option<CooperativeEntity>>;

    /// Update an entity
    async fn update(&self, entity: CooperativeEntity) -> Result<()>;

    /// List members of an entity
    async fn list_members(&self, parent_id: &EntityId) -> Result<Vec<Membership>>;

    /// Get membership of an entity within a parent
    async fn get_membership(&self, member_id: &EntityId, parent_id: &EntityId)
        -> Result<Option<Membership>>;

    /// Add a member to an entity
    async fn add_member(&self, membership: Membership) -> Result<()>;

    /// Update membership
    async fn update_membership(&self, membership: Membership) -> Result<()>;

    /// List all entities at a given level
    async fn list_by_type(&self, entity_type: EntityType) -> Result<Vec<CooperativeEntity>>;

    /// Get the entity hierarchy (ancestors)
    async fn get_ancestors(&self, id: &EntityId) -> Result<Vec<CooperativeEntity>>;

    /// Get all descendants of an entity
    async fn get_descendants(&self, id: &EntityId) -> Result<Vec<CooperativeEntity>>;
}
```

---

## Entity Lifecycle Operations

### Formation

```rust
/// Entity formation ceremony
pub struct FormationCeremony {
    /// The entity being formed
    pub entity: CooperativeEntity,

    /// Founding members
    pub founders: Vec<(EntityId, Role)>,

    /// Initial charter (ratified by founders)
    pub charter: Charter,

    /// Cryptographic proofs from the ceremony
    pub ceremony_proofs: Vec<CeremonyProof>,
}

/// Charter defining entity rules
pub struct Charter {
    /// Mission statement
    pub mission: String,

    /// Governance rules
    pub governance_rules: Vec<CharterRule>,

    /// Economic rules
    pub economic_rules: Vec<CharterRule>,

    /// Amendment process
    pub amendment_threshold: f64,

    /// Hash of the charter content
    pub content_hash: [u8; 32],

    /// Signatures from ratifying members
    pub ratifications: Vec<(EntityId, Signature)>,
}
```

### Dissolution

```rust
/// Entity dissolution process
pub struct DissolutionProcess {
    /// Entity being dissolved
    pub entity_id: EntityId,

    /// Reason for dissolution
    pub reason: String,

    /// How assets will be distributed
    pub asset_distribution: AssetDistribution,

    /// Required approvals
    pub required_approvals: Vec<EntityId>,

    /// Current approval status
    pub approvals: Vec<(EntityId, Timestamp, Signature)>,

    /// Cooling-off period end
    pub cooling_off_ends_at: Timestamp,
}

/// How to distribute assets on dissolution
pub enum AssetDistribution {
    /// Transfer all to parent entity
    ToParent,

    /// Distribute proportionally to members
    ToMembers { by_contribution: bool },

    /// Transfer to specific entity
    ToEntity(EntityId),

    /// Contribute to commons
    ToCommons,
}
```

---

## Migration Strategy

### Phase 1: Type Introduction
1. Create `icn-entity` crate with core types
2. No breaking changes - new types exist alongside old

### Phase 2: Subsystem Integration
1. Update `icn-governance` to accept EntityId where it currently uses Did
2. Update `icn-ledger` to support EntityId accounts
3. Update `icn-trust` to support entity-to-entity trust

### Phase 3: Migration Tools
1. Create `Did` → `EntityId` mapping
2. Build migration scripts for existing data
3. Dual-write during transition period

### Phase 4: Deprecation
1. Mark old APIs as deprecated
2. Update gateway to use new types
3. Remove deprecated types after migration complete

---

## Compatibility Notes

### Backward Compatibility

- `Did` remains valid as `EntityId::Person(did)`
- Existing governance domains map to entity-scoped governance
- Existing ledger accounts map to entity accounts
- Existing trust edges map to entity trust relationships

### Wire Protocol

- New entity types use existing gossip topics with extended message types
- Entity announcements extend current `CooperativeInfo` messages
- Membership changes gossiped via new `entity:membership` topic

---

## Open Questions

1. **Anchor Rotation**: How do cooperative anchors rotate when membership changes?
2. **Cross-Federation Trust**: How does trust propagate across federation boundaries?
3. **Conflict Resolution**: When parent and child entity rules conflict, which wins?
4. **Privacy**: Which entity information should be public vs. member-only?

---

## Next Steps

1. [ ] Review spec with stakeholders
2. [ ] Create `icn-entity` crate with core types
3. [ ] Implement EntityRegistry trait
4. [ ] Write migration plan for existing data
5. [ ] Update gateway API spec for entity endpoints

---

## Appendix: Type Summary

| Type | Purpose | Key Fields |
|------|---------|------------|
| `EntityId` | Unique identifier | Type, namespace, local_id |
| `EntityType` | Hierarchy level | Person, Coop, Federation, Commons |
| `CooperativeEntity` | Core entity | anchor, governance, economics, trust |
| `EntityAnchor` | Cryptographic root | DID, threshold, ceremony proof |
| `EntityState` | Lifecycle | Forming, Active, Suspended, Dissolved |
| `Membership` | Parent-child relation | member_id, parent_id, roles, state |
| `GovernanceConfig` | Voting rules | domain, quorum, constraints |
| `EconomicConfig` | Financial rules | treasury, credit policy, agreements |
| `TrustConfig` | Trust rules | thresholds, decay, policies |
