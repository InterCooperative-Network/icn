# CooperativeEntity Type Specification

---
**Status**: IMPLEMENTED
**Created**: 2025-12-24
**Updated**: 2025-12-24
**Phase**: 19 - Cooperative Entity Foundation
**Authors**: fahertym, Claude Code
**Crate**: `icn-entity` (32 tests passing)

---

**Related**: [COOPERATIVE_MIDDLE_LAYER_GAP_ANALYSIS.md](COOPERATIVE_MIDDLE_LAYER_GAP_ANALYSIS.md)

## Implementation Status

| Component | Status | File |
|-----------|--------|------|
| EntityId | Complete | `entity.rs` |
| EntityType | Complete | `entity.rs` |
| CooperativeEntity | Complete | `entity.rs` |
| EntityStatus | Complete | `entity.rs` |
| AccountId | Complete | `entity.rs` |
| Membership | Complete | `membership.rs` |
| MembershipRole | Complete | `membership.rs` |
| MembershipCapability | Complete | `membership.rs` |
| MembershipStatus | Complete | `membership.rs` |
| EntityRegistry trait | Complete | `registry.rs` |
| InMemoryRegistry | Complete | `registry.rs` |
| EntityRegistryHandle | Complete | `registry.rs` |
| LifecycleEvent | Complete | `lifecycle.rs` |
| EntityLifecycle trait | Complete | `lifecycle.rs` |

**Note**: The implementation uses a simplified model compared to the original spec below. Key differences:
- EntityId format: `entity:icn:<type>:<identifier>` (vs `entity:<type>:<namespace>:<local_id>`)
- Three entity types: Individual, Cooperative, Federation (vs Person, WorkingGroup, Cooperative, Federation, Commons)
- No EntityAnchor yet (deferred to full SDIS integration)
- No GovernanceConfig/EconomicConfig/TrustConfig embedded (linked via domain_id instead)

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

### EntityId (Implemented)

A universally unique identifier for any cooperative entity.

```rust
/// Unique identifier for a cooperative entity
/// Format: "entity:icn:{type}:{identifier}"
/// Examples:
///   - "entity:icn:individual:z5TrA8Qk..." (wraps DID public key)
///   - "entity:icn:cooperative:food-coop-2024"
///   - "entity:icn:federation:midwest-fed"
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(String);

impl EntityId {
    /// Create from an existing DID (for individuals)
    pub fn from_did(did: &Did) -> Self;

    /// Create a cooperative entity ID from slug
    pub fn cooperative(slug: &str) -> Result<Self>;

    /// Create a federation entity ID from slug
    pub fn federation(slug: &str) -> Result<Self>;

    /// Get entity type from the ID
    pub fn entity_type(&self) -> EntityType;

    /// Convert back to DID (only for individuals)
    pub fn to_did(&self) -> Option<Did>;

    /// Get the raw identifier portion
    pub fn identifier(&self) -> &str;
}
```

**Slug Validation Rules**:
- 3-64 characters
- Lowercase letters, numbers, hyphens only
- Must start with a letter
- No consecutive hyphens

### EntityType (Implemented)

The level in the cooperative hierarchy.

```rust
/// The type/level of a cooperative entity
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityType {
    /// Individual backed by a DID
    Individual,

    /// Cooperative organization
    Cooperative,

    /// Federation of cooperatives
    Federation,

    /// Unknown type (parsing fallback)
    Unknown,
}
```

**Design Decision**: The implementation uses three core types (Individual, Cooperative, Federation) rather than the five originally proposed. WorkingGroup can be modeled as a Cooperative with a parent_id, and Commons can be modeled as a top-level Federation.

### CooperativeEntity (Implemented)

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

    /// Entity lifecycle state
    pub status: EntityStatus,

    /// Parent entity (if any)
    pub parent_id: Option<EntityId>,

    /// Link to governance domain (if governance enabled)
    pub governance_domain_id: Option<String>,

    /// Link to treasury account (if treasury enabled)
    pub treasury_account: Option<AccountReference>,

    /// Creation timestamp (Unix seconds)
    pub created_at: u64,

    /// Last modification timestamp (Unix seconds)
    pub updated_at: u64,

    /// Human-readable description
    pub description: Option<String>,

    /// Arbitrary key-value metadata
    pub metadata: HashMap<String, String>,
}

impl CooperativeEntity {
    /// Create an individual entity from a DID
    pub fn individual(did: &Did, name: &str) -> Self;

    /// Create a cooperative entity (builder pattern)
    pub fn cooperative(slug: &str, name: &str) -> Result<Self>;

    /// Create a federation entity (builder pattern)
    pub fn federation(slug: &str, name: &str) -> Result<Self>;

    /// Builder methods
    pub fn with_description(self, description: &str) -> Self;
    pub fn with_governance_domain(self, domain_id: &str) -> Self;
    pub fn with_metadata(self, key: &str, value: &str) -> Self;
}
```

**Design Decision**: Governance/Economic/Trust configurations are linked via IDs rather than embedded. This reduces coupling and allows the entity crate to remain lightweight.

### EntityAnchor (Deferred)

The cryptographic anchor for entity identity. **Not yet implemented** - deferred to full SDIS integration in Phase 22.

The original design proposed:
- `Personal`: Individual anchor with VUI commitment and ceremony proof
- `Cooperative`: Threshold-derived anchor from member signatures
- `Federation`: Anchor derived from member cooperatives
- `Genesis`: For the ICN Protocol Commons

**Current Implementation**: Individuals link to DIDs via `EntityId::from_did()`. Cooperatives and federations have no cryptographic anchor yet - they are identified by their EntityId slug. Multi-party signing will be added when threshold signatures are implemented.

### EntityStatus (Implemented)

The lifecycle state of an entity.

```rust
/// Entity lifecycle status
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EntityStatus {
    /// Entity is being formed (pre-charter ratification)
    #[default]
    Forming,

    /// Entity is active and operational
    Active,

    /// Entity is suspended (frozen)
    Suspended {
        reason: String,
        suspended_at: u64,
    },

    /// Entity is in the process of dissolving
    Dissolving {
        started_at: u64,
    },

    /// Entity has been dissolved
    Dissolved {
        dissolved_at: u64,
    },

    /// Entity has merged into another entity
    Merged {
        into: EntityId,
        merged_at: u64,
    },

    /// Entity has split into multiple entities
    Split {
        into: Vec<EntityId>,
        split_at: u64,
    },
}

impl EntityStatus {
    /// Check if entity is operational
    pub fn is_operational(&self) -> bool;

    /// Check if entity lifecycle has ended
    pub fn is_terminated(&self) -> bool;
}
```

**Valid State Transitions**:
| From | To |
|------|-----|
| Forming | Active |
| Active | Suspended, Dissolving, Merged, Split |
| Suspended | Active, Dissolving |
| Dissolving | Dissolved, Merged, Split |

**Design Decision**: FormationStep tracking is deferred. The `Forming` state is simple for now; detailed formation requirements can be tracked externally or added later.

---

## Membership Model (Implemented)

### Membership

Represents the relationship between an entity and its parent.

```rust
/// Membership of an entity within another entity
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Membership {
    /// The member entity (individual OR cooperative)
    pub member_id: EntityId,

    /// The containing entity (parent)
    pub parent_id: EntityId,

    /// Role within the parent entity
    pub role: MembershipRole,

    /// Current membership status
    pub status: MembershipStatus,

    /// When membership was created (Unix timestamp)
    pub joined_at: u64,

    /// When membership was last updated
    pub updated_at: u64,

    /// Voting shares (for weighted voting, 0 = no vote)
    pub shares: u64,

    /// Capabilities granted by this membership
    pub capabilities: Vec<MembershipCapability>,

    /// Optional notes
    pub notes: Option<String>,
}

impl Membership {
    /// Create a new pending membership
    pub fn new(member_id: EntityId, parent_id: EntityId, role: MembershipRole) -> Self;

    /// Create an active membership (bypass pending state)
    pub fn active(member_id: EntityId, parent_id: EntityId, role: MembershipRole) -> Self;

    /// Check if member can vote
    pub fn can_vote(&self) -> bool;  // requires: active + shares > 0 + Vote capability

    /// Check if member can create proposals
    pub fn can_propose(&self) -> bool;
}
```

### MembershipRole

```rust
pub enum MembershipRole {
    // Individual Roles (for people)
    Founder,             // All capabilities
    Member,              // Vote, Propose
    Worker,              // Vote, Propose
    Consumer,            // Vote, Propose
    Producer,            // Vote, Propose
    BoardMember,         // Vote, Propose, Treasury, Invite
    Officer { title: String },  // Vote, Propose, Treasury

    // Entity Roles (for organizations)
    FederatedMember,     // Vote, Propose, Sign
    AssociateMember,     // Propose
    ObserverMember,      // None
    ProvisionalMember,   // Propose

    // Custom
    Custom { name: String },  // Vote, Propose
}
```

### MembershipStatus

```rust
pub enum MembershipStatus {
    Pending,    // Application pending
    Active,     // In good standing
    Suspended,  // Temporarily frozen
    Inactive,   // Not participating
    Resigned,   // Voluntarily left
    Removed,    // Governance removal
    Expelled,   // Serious violation
}
```

### MembershipCapability

```rust
pub enum MembershipCapability {
    Vote,               // Can vote on proposals
    Propose,            // Can create proposals
    TreasuryAccess,     // Can perform treasury operations
    Invite,             // Can invite new members
    ManageSubEntities,  // Can manage child entities
    Sign,               // Can sign on behalf of entity
    Configure,          // Can modify entity settings
    ViewSensitive,      // Can view confidential info
    Custom(String),     // User-defined capability
}
```

**Key Insight**: Capabilities are explicit, not derived from roles. Each role has default capabilities, but they can be overridden with `membership.with_capabilities()`.

---

## Configuration (Linked, Not Embedded)

The original spec proposed embedding GovernanceConfig, EconomicConfig, and TrustConfig directly in CooperativeEntity. The implementation uses **linked references** instead:

```rust
pub struct CooperativeEntity {
    // ...
    pub governance_domain_id: Option<String>,  // Links to icn-governance
    pub treasury_account: Option<AccountReference>,  // Links to icn-ledger
    // Trust is implicit via the entity's DIDs in the trust graph
}
```

**Benefits**:
- Reduced coupling between crates
- Configuration lives in its natural home (governance in icn-governance, treasury in icn-ledger)
- Entity crate stays lightweight
- Easier to evolve configurations independently

**Deferred to Phase 19+ Integration**:
- GovernanceConfig (voting models, quorum, constraints)
- EconomicConfig (credit policy, inter-entity agreements)
- TrustConfig (entity-level trust thresholds)

See the original spec sections below for the envisioned designs.

---

## Entity Registry (Implemented)

The entity registry maintains the global state of all entities.

```rust
/// Entity registry for storing and querying entities
pub trait EntityRegistry: Send + Sync {
    // Entity CRUD
    fn register(&mut self, entity: CooperativeEntity) -> Result<()>;
    fn get(&self, id: &EntityId) -> Result<Option<CooperativeEntity>>;
    fn update(&mut self, entity: CooperativeEntity) -> Result<()>;
    fn delete(&mut self, id: &EntityId) -> Result<()>;

    // Queries
    fn list_by_type(&self, entity_type: EntityType) -> Result<Vec<EntityId>>;
    fn list_children(&self, parent_id: &EntityId) -> Result<Vec<EntityId>>;

    // Membership operations
    fn add_membership(&mut self, membership: Membership) -> Result<()>;
    fn remove_membership(&mut self, member_id: &EntityId, parent_id: &EntityId) -> Result<()>;
    fn get_membership(&self, member_id: &EntityId, parent_id: &EntityId)
        -> Result<Option<Membership>>;
    fn get_members(&self, parent_id: &EntityId) -> Result<Vec<Membership>>;
    fn get_memberships(&self, member_id: &EntityId) -> Result<Vec<Membership>>;
}
```

### EntityRegistryHandle

Thread-safe async wrapper for concurrent access:

```rust
pub struct EntityRegistryHandle {
    inner: Arc<RwLock<InMemoryRegistry>>,
}

impl EntityRegistryHandle {
    pub fn new() -> Self;

    // Async versions of all trait methods
    pub async fn register(&self, entity: CooperativeEntity) -> Result<()>;
    pub async fn get(&self, id: &EntityId) -> Result<Option<CooperativeEntity>>;
    // ... etc
}
```

### InMemoryRegistry

Reference implementation for testing and development:

```rust
pub struct InMemoryRegistry {
    entities: HashMap<String, CooperativeEntity>,
    memberships: Vec<Membership>,
}
```

**Constraints Enforced**:
- Duplicate entity IDs rejected
- Cannot update non-existent entities
- Cannot delete entities with active memberships
- Both member and parent must exist to add membership

---

## Entity Lifecycle (Implemented)

### LifecycleEvent

Events that mark entity state transitions:

```rust
pub enum LifecycleEvent {
    Created {
        entity_id: EntityId,
        created_by: EntityId,
        timestamp: u64,
    },
    Activated {
        entity_id: EntityId,
        charter_hash: String,
        activated_by: EntityId,
        timestamp: u64,
    },
    Suspended {
        entity_id: EntityId,
        reason: String,
        suspended_by: EntityId,
        timestamp: u64,
    },
    Resumed {
        entity_id: EntityId,
        resumed_by: EntityId,
        timestamp: u64,
    },
    DissolutionStarted {
        entity_id: EntityId,
        started_by: EntityId,
        timestamp: u64,
    },
    Dissolved {
        entity_id: EntityId,
        timestamp: u64,
    },
    Merged {
        source_id: EntityId,
        target_id: EntityId,
        merger_proposal_id: Option<String>,
        timestamp: u64,
    },
    Split {
        source_id: EntityId,
        new_entity_ids: Vec<EntityId>,
        split_proposal_id: Option<String>,
        timestamp: u64,
    },
}
```

### EntityLifecycle Trait

```rust
pub trait EntityLifecycle {
    fn create(&mut self, entity: CooperativeEntity, created_by: &EntityId)
        -> Result<LifecycleEvent>;

    fn activate(&mut self, entity_id: &EntityId, charter_hash: String, by: &EntityId)
        -> Result<LifecycleEvent>;

    fn suspend(&mut self, entity_id: &EntityId, reason: String, by: &EntityId)
        -> Result<LifecycleEvent>;

    fn resume(&mut self, entity_id: &EntityId, by: &EntityId)
        -> Result<LifecycleEvent>;

    fn begin_dissolution(&mut self, entity_id: &EntityId, by: &EntityId)
        -> Result<LifecycleEvent>;

    fn dissolve(&mut self, entity_id: &EntityId)
        -> Result<LifecycleEvent>;

    fn merge(&mut self, source_id: &EntityId, target_id: &EntityId, by: &EntityId)
        -> Result<LifecycleEvent>;

    fn split(&mut self, source_id: &EntityId, new_entities: Vec<CooperativeEntity>, by: &EntityId)
        -> Result<Vec<LifecycleEvent>>;
}
```

### State Transition Validation

```rust
pub fn validate_transition(from: &EntityStatus, to: &EntityStatus) -> Result<()>;
```

**Note**: FormationCeremony, Charter, and DissolutionProcess are deferred to full Phase 19 integration. The current implementation provides the event types and trait interface.

---

## Migration Strategy

### AccountId - The Bridge Type

The `AccountId` enum provides backward compatibility:

```rust
pub enum AccountId {
    Did(Did),           // Legacy DID-based accounts
    Entity(EntityId),   // New entity-based accounts
}

impl AccountId {
    pub fn as_did(&self) -> Option<&Did>;
    pub fn as_entity(&self) -> Option<&EntityId>;
}
```

This allows gradual migration - existing code uses `AccountId::Did`, new code can use `AccountId::Entity`.

### Phase 1: Type Introduction (Current - Sprint 3)
1. ✅ Create `icn-entity` crate with core types
2. ✅ Implement EntityId, CooperativeEntity, Membership
3. ✅ Implement EntityRegistry trait
4. ✅ Implement LifecycleEvent and EntityLifecycle trait
5. No breaking changes - new types exist alongside old

### Phase 2: Subsystem Integration (Phase 19)
1. Add `AccountId` to ledger types (replace `account_id: Did`)
2. Add `entity_id` field to governance domains
3. Update treasury to use `EntityId` for cooperative accounts
4. Add entity registry actor to runtime

### Phase 3: Full Integration (Phase 19+)
1. Update icn-governance to accept EntityId
2. Update icn-ledger to support entity-level accounts
3. Update icn-trust to support entity-to-entity edges
4. Add gateway entity endpoints

### Phase 4: Migration & Cleanup (Future)
1. Create `Did` → `EntityId` mapping for existing data
2. Build migration scripts
3. Dual-write during transition period
4. Deprecate DID-only APIs

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
   - *Deferred to Phase 22 SDIS integration*

2. **Cross-Federation Trust**: How does trust propagate across federation boundaries?
   - *Will require entity-level edges in trust graph*

3. **Conflict Resolution**: When parent and child entity rules conflict, which wins?
   - *Proposed: Subsidiarity principle - local unless explicitly delegated up*

4. **Privacy**: Which entity information should be public vs. member-only?
   - *Current: All public. Future: Add visibility field*

5. **Multi-Federation Membership**: Can a cooperative belong to multiple federations?
   - *Current implementation: Yes. Question: Scope capabilities per-federation?*

6. **Entity Recovery**: What happens if an entity loses all authorized members?
   - *Not yet addressed*

---

## Next Steps (Updated)

1. ✅ Review spec with stakeholders
2. ✅ Create `icn-entity` crate with core types
3. ✅ Implement EntityRegistry trait
4. ✅ Implement LifecycleEvent and EntityLifecycle trait
5. [ ] **Phase 19**: Integrate with icn-ledger (AccountId)
6. [ ] **Phase 19**: Integrate with icn-governance (entity domains)
7. [ ] **Phase 19**: Add entity actor to runtime
8. [ ] **Phase 19**: Add gateway entity endpoints
9. [ ] Write migration plan for existing data

---

## Appendix: Implemented Type Summary

| Type | Purpose | File |
|------|---------|------|
| `EntityId` | Unique identifier (`entity:icn:<type>:<id>`) | `entity.rs` |
| `EntityType` | Individual, Cooperative, Federation | `entity.rs` |
| `CooperativeEntity` | Core entity record | `entity.rs` |
| `EntityStatus` | Lifecycle state (Forming → Active → ...) | `entity.rs` |
| `AccountId` | DID or EntityId (migration bridge) | `entity.rs` |
| `AccountReference` | Link to ledger account | `entity.rs` |
| `Membership` | Entity-to-parent relationship | `membership.rs` |
| `MembershipRole` | Founder, Worker, FederatedMember, etc. | `membership.rs` |
| `MembershipStatus` | Pending, Active, Suspended, etc. | `membership.rs` |
| `MembershipCapability` | Vote, Propose, Treasury, etc. | `membership.rs` |
| `EntityRegistry` | Storage trait | `registry.rs` |
| `InMemoryRegistry` | Reference implementation | `registry.rs` |
| `EntityRegistryHandle` | Async wrapper | `registry.rs` |
| `LifecycleEvent` | State transition events | `lifecycle.rs` |
| `EntityLifecycle` | Lifecycle operations trait | `lifecycle.rs` |
| `EntityError` | Error type | `error.rs` |

---

## Changelog

| Date | Change |
|------|--------|
| 2025-12-24 | Initial implementation complete (Sprint 3) |
| 2025-12-24 | Updated spec to reflect actual implementation |
