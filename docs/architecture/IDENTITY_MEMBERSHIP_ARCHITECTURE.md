# Identity and Membership Architecture for ICN

**Status**: Design Document
**Date**: 2025-12-25
**Author**: Architecture Review

## Executive Summary

This document proposes a unified identity and membership architecture for ICN that addresses:
- Individual identity independent of infrastructure nodes
- Multi-device support with key rotation and recovery
- Membership lifecycle for cooperatives and communities
- Federation identity for cross-entity recognition
- Flexible node topology supporting full nodes, light clients, and gateway access

The design adheres to cooperative principles of democratic control and voluntary membership while maintaining cryptographic security.

---

## Table of Contents

1. [Current State Analysis](#1-current-state-analysis)
2. [Individual Identity Model](#2-individual-identity-model)
3. [Membership Lifecycle](#3-membership-lifecycle)
4. [Federation Identity](#4-federation-identity)
5. [Node Topology](#5-node-topology)
6. [Implementation Phases](#6-implementation-phases)
7. [Security Considerations](#7-security-considerations)
8. [Migration Strategy](#8-migration-strategy)

---

## 1. Current State Analysis

### 1.1 Identity Layer (icn-identity)

The current implementation provides a robust foundation:

#### DID Types
```
did:icn:<base58-ed25519-pubkey>     # Legacy: key-derived DID
did:icn:<base58-anchor-id>          # SDIS: anchor-derived DID
```

#### Key Components

| Component | Location | Purpose |
|-----------|----------|---------|
| `KeyPair` | `lib.rs` | Ed25519 signing with optional PQ hybrid (ML-DSA) |
| `DidDocument` | `multi_device.rs` | Multi-device with verification methods and capabilities |
| `AgeKeyStore` | `keystore.rs` | Encrypted storage (v4 format with SDIS support) |
| `Anchor` | `anchor.rs` | Permanent identity root with VUI commitment |
| `PersonhoodAnchor` | `personhood.rs` | Commons layer with POP attestations |
| `RecoveryEvent` | `recovery.rs` | M-of-N social recovery protocol |

#### Keystore Format Progression
- **v1**: Basic Ed25519 keypair
- **v2**: Added TLS binding for DID-TLS security
- **v2.1**: Added X25519 encryption keys
- **v3**: Added DID Document and multi-device support
- **v4**: Added SDIS Anchor and KeyBundle (hybrid PQ signatures)

### 1.2 Cooperative Membership (icn-cooperative)

Current cooperative membership model:

```rust
pub struct Member {
    pub did: String,              // Member's DID
    pub joined_at: DateTime<Utc>,
    pub tier: MembershipTier,     // Worker, Consumer, etc.
    pub capital_contribution: u64,
    pub active: bool,
}

pub enum CooperativeType {
    Worker, Consumer, Producer, MultiStakeholder, Platform, Housing, CreditUnion
}

pub enum CooperativeStatus {
    Forming, Active, Suspended, Dissolving, Dissolved
}
```

**Membership Actions**: Apply, Approve, Reject, Remove, ChangeTier, Suspend, Reinstate

### 1.3 Community Membership (icn-community)

Current community membership model:

```rust
pub enum MemberType {
    Individual(String),   // DID
    Cooperative(String),  // CooperativeId
}

pub struct Member {
    pub id: MemberId,
    pub member_type: MemberType,
    pub joined_at: DateTime<Utc>,
    pub voting_weight: u32,
    pub active: bool,
}

pub enum CommunityType {
    Geographic, Interest, Solidarity, Ecosystem
}
```

Communities support both individual and organizational members.

### 1.4 Federation (icn-federation)

Federation operates at the cooperative level:

```rust
pub struct CooperativeInfo {
    pub coop_id: String,
    pub name: String,
    pub public_did: Did,           // Institutional DID
    pub gateway_endpoints: Vec<String>,
    pub federation_policy: FederationPolicy,
    pub currencies: Vec<CurrencyInfo>,
    pub capabilities: Vec<String>,
}

pub enum FederationPolicy {
    Open,
    Vouched { min_vouches: u8 },
    Closed,
}
```

### 1.5 Current Gaps

1. **Identity-Node Coupling**: Node identity currently equals user identity
2. **No Individual-Coop Separation**: An individual joining multiple coops needs multiple nodes
3. **Limited Delegation**: Cannot delegate authority to other devices/services
4. **Gateway Access**: Light clients have no first-class support
5. **Cross-Coop Identity**: Same person in multiple coops has no identity linking

---

## 2. Individual Identity Model

### 2.1 Design Principles

1. **Identity Independence**: Individuals have DIDs independent of any node
2. **Multi-Device Native**: Design assumes multiple devices from the start
3. **Recoverable**: All identities must have a recovery path
4. **Privacy-Preserving**: No PII in DIDs or public data
5. **Progressive Trust**: Identity strength grows with attestations

### 2.2 Proposed Identity Architecture

```
                        ┌──────────────────────────────────────────────────┐
                        │               PersonhoodAnchor                   │
                        │  (Permanent, survives all key rotations)         │
                        │  ┌────────────────────────────────────────────┐  │
                        │  │  anchor_id: H(VUI || genesis)              │  │
                        │  │  vui_commitment: H(VUI)                    │  │
                        │  │  pop_attestations: Vec<POPAttestation>     │  │
                        │  │  status: Active | Suspended | Revoked      │  │
                        │  └────────────────────────────────────────────┘  │
                        └──────────────────────────────┬───────────────────┘
                                                       │
                        ┌──────────────────────────────┴───────────────────┐
                        │                  DID Document                     │
                        │  (Versioned, synchronized via gossip)             │
                        │  ┌────────────────────────────────────────────┐  │
                        │  │  id: did:icn:<anchor-id>                   │  │
                        │  │  verification_method: Vec<Device>          │  │
                        │  │  recovery: RecoveryConfig                  │  │
                        │  └────────────────────────────────────────────┘  │
                        └──────────────┬─────────────┬─────────────────────┘
                                       │             │
            ┌──────────────────────────┴──┐       ┌──┴──────────────────────────┐
            │        Device 1              │       │        Device 2             │
            │  ┌────────────────────────┐  │       │  ┌────────────────────────┐ │
            │  │ id: "phone"            │  │       │  │ id: "laptop"           │ │
            │  │ ed25519_key: [pub]     │  │       │  │ ed25519_key: [pub]     │ │
            │  │ x25519_key: [pub]      │  │       │  │ x25519_key: [pub]      │ │
            │  │ capabilities: [Sign,   │  │       │  │ capabilities: [Sign,   │ │
            │  │   AddDevice, Recover]  │  │       │  │   AddDevice]           │ │
            │  └────────────────────────┘  │       │  └────────────────────────┘ │
            └─────────────┬────────────────┘       └──────────────┬──────────────┘
                          │                                       │
                          │     ┌──────────────────────────┐      │
                          └─────►       Full Node          │◄─────┘
                                │  (may be shared/hosted)  │
                                │  Stores local keychain   │
                                └──────────────────────────┘
```

### 2.3 Individuals Independent of Nodes

**Question**: Should individuals have DIDs independent of nodes?
**Answer**: **Yes, absolutely.**

#### Rationale

1. **Real-World Mapping**: People are not machines. An individual should have a persistent identity across:
   - Multiple personal devices (phone, laptop, tablet)
   - Shared cooperative infrastructure
   - Gateway-accessed web applications

2. **Key Rotation**: Keys get compromised, devices get lost. The identity must survive.

3. **Cooperative Mobility**: Members may leave one coop and join another. Their identity and reputation should be portable.

#### Implementation

```rust
/// Individual identity structure
pub struct IndividualIdentity {
    /// Permanent anchor (survives key rotation)
    pub anchor: PersonhoodAnchor,

    /// Current DID Document
    pub did_document: DidDocument,

    /// Membership records across organizations
    pub memberships: Vec<MembershipRecord>,

    /// Trust attestations from other identities
    pub trust_attestations: Vec<TrustAttestation>,
}

/// Record of membership in an organization
pub struct MembershipRecord {
    /// Type of organization
    pub org_type: OrganizationType,
    /// Organization identifier
    pub org_id: String,
    /// DID of the organization
    pub org_did: Did,
    /// Role within the organization
    pub role: String,
    /// Join timestamp
    pub joined_at: u64,
    /// Status
    pub status: MembershipStatus,
}

pub enum OrganizationType {
    Cooperative,
    Community,
    Federation,
}
```

### 2.4 Multi-Device Support

The existing `DidDocument` with `VerificationMethod` array already supports this. Enhancements:

```rust
/// Enhanced device capabilities
pub enum Capability {
    /// Can sign messages on behalf of this DID
    Sign,
    /// Can add new devices to this DID
    AddDevice,
    /// Can revoke devices (including self)
    RevokeDevice,
    /// Can rotate keys
    RotateKey,
    /// Can participate in recovery
    Recover,
    /// Can encrypt/decrypt messages
    Encrypt,
    /// NEW: Can delegate authority temporarily
    Delegate,
    /// NEW: Can act as recovery guardian for others
    Guardian,
    /// NEW: Can access cooperative services
    CoopAccess { coop_id: String },
}

/// Device classification for trust scoring
pub enum DeviceClass {
    /// Hardware security module or secure enclave
    HardwareSecured,
    /// Full-featured device with local keystore
    FullDevice,
    /// Browser-based with encrypted local storage
    WebClient,
    /// Delegated access through gateway
    GatewayDelegated,
}
```

### 2.5 Key Rotation and Recovery

#### Proactive Rotation

```rust
pub struct RotationPolicy {
    /// Maximum key age before forced rotation
    pub max_key_age_days: u32,
    /// Whether to allow automatic rotation
    pub auto_rotate: bool,
    /// Notification period before rotation
    pub notify_days_before: u32,
}
```

#### Recovery Options

The existing `RecoveryConfig` supports M-of-N social recovery. Extensions:

```rust
pub enum RecoveryMethod {
    /// M-of-N social recovery (existing)
    Social { m: u8, n: u8 },

    /// Encrypted backup seed (existing)
    BackupSeed,

    /// NEW: Cooperative-assisted recovery
    CooperativeAssisted {
        /// Cooperative that can assist
        coop_did: Did,
        /// Required steward signatures
        steward_threshold: u8,
    },

    /// NEW: Time-locked backup
    TimeLocked {
        /// Backup recovery seed (encrypted)
        backup_hash: [u8; 32],
        /// Time lock duration
        lock_duration_days: u32,
    },

    /// No recovery (accept total loss risk)
    None,
}
```

### 2.6 Identity Synchronization

DID Documents are synchronized via gossip (existing `identity:updates` topic):

```rust
/// Enhanced identity update for multi-org context
pub struct IdentityUpdateMessage {
    /// The DID this update applies to
    pub did: Did,
    /// The rotation event describing the change
    pub event: RotationEvent,
    /// Current DID Document version
    pub new_version: u64,
    /// Timestamp
    pub timestamp: u64,
    /// NEW: Affected organization scopes
    pub affected_orgs: Vec<String>,
}
```

---

## 3. Membership Lifecycle

### 3.1 Design Principles

1. **Democratic Control**: Membership decisions follow cooperative governance
2. **Voluntary Membership**: Easy exit without losing identity
3. **Role Clarity**: Clear capability mapping per membership tier
4. **Auditability**: All membership changes are recorded

### 3.2 Unified Membership Model

```rust
/// Universal membership structure (works for Coops, Communities, Federations)
pub struct Membership {
    /// Member's DID (individual or organization)
    pub member_did: Did,

    /// Entity being joined
    pub entity_id: EntityId,

    /// Type of entity
    pub entity_type: EntityType,

    /// Membership class within this entity
    pub membership_class: MembershipClass,

    /// Current status
    pub status: MembershipStatus,

    /// Capabilities granted by this membership
    pub capabilities: Vec<MembershipCapability>,

    /// Timestamps
    pub applied_at: u64,
    pub approved_at: Option<u64>,
    pub suspended_at: Option<u64>,
    pub left_at: Option<u64>,

    /// Governance participation metrics
    pub participation: ParticipationMetrics,
}

pub enum EntityType {
    Cooperative,
    Community,
    Federation,
    WorkingGroup,
}

pub enum MembershipStatus {
    /// Application submitted, awaiting approval
    Pending,
    /// Approved by invitation, awaiting acceptance
    Invited,
    /// Full active member
    Active,
    /// Membership suspended (with reason)
    Suspended { reason: String, until: Option<u64> },
    /// Voluntarily departed
    Departed,
    /// Removed by governance action
    Removed { reason: String },
}
```

### 3.3 Joining a Cooperative

```
┌─────────────┐                    ┌─────────────┐                    ┌─────────────┐
│  Applicant  │                    │ Cooperative │                    │ Governance  │
│   (DID)     │                    │   System    │                    │   Process   │
└──────┬──────┘                    └──────┬──────┘                    └──────┬──────┘
       │                                   │                                   │
       │  1. Submit Application            │                                   │
       │──────────────────────────────────►│                                   │
       │                                   │                                   │
       │                                   │  2. Route based on policy         │
       │                                   │──────────────────────────────────►│
       │                                   │                                   │
       │                                   │  3a. Open: Auto-approve           │
       │                                   │◄──────────────────────────────────│
       │                                   │                                   │
       │                                   │  3b. Approval: Vote               │
       │                                   │◄──────────────────────────────────│
       │                                   │                                   │
       │                                   │  3c. Invitation: Check invite     │
       │                                   │◄──────────────────────────────────│
       │                                   │                                   │
       │  4. Status Update                 │                                   │
       │◄──────────────────────────────────│                                   │
       │                                   │                                   │
       │  5. (If approved) Complete        │                                   │
       │     onboarding actions            │                                   │
       │──────────────────────────────────►│                                   │
       │                                   │                                   │
```

#### Membership Policies

```rust
pub enum MembershipPolicy {
    /// Anyone can join
    Open,

    /// Requires invitation from existing member
    InvitationOnly {
        /// Minimum trust score of inviter
        inviter_min_trust: f64,
    },

    /// Requires governance approval vote
    ApprovalRequired {
        /// Quorum percentage
        quorum: f64,
        /// Approval threshold
        threshold: f64,
        /// Voting period in seconds
        voting_period: u64,
    },

    /// Combination: invitation + approval
    InvitationAndApproval {
        inviter_min_trust: f64,
        quorum: f64,
        threshold: f64,
        voting_period: u64,
    },

    /// Closed to new members
    Closed,
}
```

### 3.4 Joining a Community

Communities have more flexible membership, supporting both individuals and organizations:

```rust
pub struct CommunityMembershipRequest {
    /// Applicant (individual or coop)
    pub applicant: MemberIdentity,

    /// Community being joined
    pub community_id: CommunityId,

    /// Requested role
    pub requested_role: CommunityRole,

    /// Statement of purpose
    pub statement: String,

    /// Sponsor (if required by policy)
    pub sponsor: Option<Did>,
}

pub enum MemberIdentity {
    Individual(Did),
    Cooperative {
        coop_id: String,
        coop_did: Did,
        representative_did: Did,
    },
}

pub enum CommunityRole {
    /// Observer - can view, limited participation
    Observer,
    /// Participant - full participation rights
    Participant,
    /// Contributor - can create resources
    Contributor,
    /// Steward - governance responsibilities
    Steward,
}
```

### 3.5 Role-Based Permissions

```rust
/// Capability matrix for membership tiers
pub struct TierCapabilities {
    pub tier_name: String,
    pub capabilities: HashSet<MembershipCapability>,
}

pub enum MembershipCapability {
    // Governance
    Vote,
    Propose,
    Steward,

    // Resources
    AccessResources,
    AllocateResources,
    ManageResources,

    // Membership
    InviteMembers,
    ApproveMembership,
    SuspendMembers,

    // Financial
    Transact,
    ViewLedger,
    ManageTreasury,

    // Compute
    SubmitTasks,
    ProvideCompute,
    ManageCompute,

    // Identity
    AttestIdentity,
    RecoveryGuardian,
}
```

### 3.6 Departure and Removal

```rust
pub enum DepartureType {
    /// Voluntary resignation
    Voluntary {
        reason: Option<String>,
        effective_date: u64,
    },

    /// Governance-initiated removal
    Removal {
        reason: String,
        governance_ref: String,  // Reference to decision
        effective_date: u64,
    },

    /// Automatic (policy-based)
    Automatic {
        policy: String,
        triggered_at: u64,
    },
}

pub struct DepartureEffects {
    /// Capital to be returned (for coops)
    pub capital_return: Option<u64>,
    /// Grace period for data export
    pub data_export_deadline: u64,
    /// Outstanding obligations
    pub pending_obligations: Vec<Obligation>,
    /// Transferred responsibilities
    pub responsibility_transfers: Vec<Transfer>,
}
```

---

## 4. Federation Identity

### 4.1 Entity Representation in Federations

Federations in ICN connect cooperatives and communities, not individuals directly.

```rust
/// Entity that can participate in federation
pub struct FederatedEntity {
    /// Entity type
    pub entity_type: FederatedEntityType,

    /// Entity's institutional DID
    pub entity_did: Did,

    /// Human-readable name
    pub name: String,

    /// Federation metadata
    pub federation_metadata: FederationMetadata,

    /// Representatives authorized to act on behalf
    pub authorized_representatives: Vec<RepresentativeAuth>,
}

pub enum FederatedEntityType {
    /// Worker, consumer, producer cooperative
    Cooperative(CooperativeType),
    /// Geographic, interest, solidarity community
    Community(CommunityType),
    /// Federation of federations
    MetaFederation,
}

pub struct RepresentativeAuth {
    /// Representative's personal DID
    pub did: Did,
    /// Role within the entity
    pub role: String,
    /// Specific delegated capabilities
    pub delegated_capabilities: Vec<FederationCapability>,
    /// Validity period
    pub valid_from: u64,
    pub valid_until: Option<u64>,
    /// Signature by entity
    pub delegation_signature: Vec<u8>,
}
```

### 4.2 Cross-Entity Identity Recognition

When an individual belongs to multiple entities in a federation:

```rust
/// Cross-entity identity claim
pub struct CrossEntityClaim {
    /// Individual's DID
    pub individual_did: Did,

    /// Entity making the claim
    pub claiming_entity: EntityId,

    /// Claimed memberships in other entities
    pub claimed_memberships: Vec<MembershipClaim>,

    /// Timestamp
    pub claimed_at: u64,

    /// Signature by individual
    pub individual_signature: Vec<u8>,
}

pub struct MembershipClaim {
    /// Target entity
    pub entity_id: EntityId,
    /// Claimed role
    pub role: String,
    /// Proof (could be attestation, membership record hash)
    pub proof: MembershipProof,
}

pub enum MembershipProof {
    /// Attestation from the entity
    EntityAttestation {
        attestation: Vec<u8>,
        entity_signature: Vec<u8>,
    },
    /// Merkle proof against entity's membership tree
    MerkleProof {
        root: [u8; 32],
        proof: Vec<[u8; 32]>,
    },
    /// Third-party verification
    ThirdPartyAttestation {
        attestor_did: Did,
        attestation: Vec<u8>,
    },
}
```

### 4.3 Trust Propagation Through Federations

```rust
/// Trust propagation rules for federation
pub struct FederationTrustPolicy {
    /// Base trust for member entities
    pub member_base_trust: f64,

    /// Trust decay per hop (transitive trust)
    pub hop_decay: f64,

    /// Maximum hops for trust propagation
    pub max_hops: u8,

    /// Category-specific trust weights
    pub category_weights: HashMap<TrustCategory, f64>,
}

pub enum TrustCategory {
    /// Financial transactions
    Financial,
    /// Governance participation
    Governance,
    /// Identity attestation
    Identity,
    /// Compute resources
    Compute,
    /// General operations
    General,
}

/// Trust flow through federation
pub struct FederatedTrustPath {
    /// Source entity
    pub source: EntityId,
    /// Target entity
    pub target: EntityId,
    /// Intermediate entities
    pub path: Vec<EntityId>,
    /// Computed trust score
    pub trust_score: f64,
    /// Category
    pub category: TrustCategory,
}
```

---

## 5. Node Topology

### 5.1 Node Types

```
┌────────────────────────────────────────────────────────────────────────────────┐
│                             ICN Node Topology                                    │
├────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐              │
│  │   Full Node      │  │  Cooperative     │  │  Federation      │              │
│  │   (Personal)     │  │  Node (Shared)   │  │  Gateway         │              │
│  │                  │  │                  │  │                  │              │
│  │ - Single user    │  │ - Multi-tenant   │  │ - Inter-coop     │              │
│  │ - Local keystore │  │ - Member access  │  │ - API routing    │              │
│  │ - Full P2P       │  │ - Shared storage │  │ - Trust bridge   │              │
│  │ - Optional host  │  │ - Governance     │  │ - Clearing       │              │
│  └────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘              │
│           │                     │                     │                         │
│           └──────────────┬──────┴─────────────────────┘                         │
│                          │                                                       │
│                          ▼                                                       │
│  ┌────────────────────────────────────────────────────────────────────────────┐ │
│  │                        ICN P2P Network                                      │ │
│  │                 (QUIC/TLS + Gossip + Ledger Sync)                          │ │
│  └────────────────────────────────────────────────────────────────────────────┘ │
│                          ▲                                                       │
│                          │                                                       │
│           ┌──────────────┴────────────────────────────┐                         │
│           │                                           │                         │
│  ┌────────┴─────────┐                     ┌───────────┴────────┐               │
│  │  Light Client    │                     │  Web Application   │               │
│  │  (Mobile/Web)    │                     │  (Gateway-only)    │               │
│  │                  │                     │                    │               │
│  │ - Delegated auth │                     │ - No local keys    │               │
│  │ - Gateway relay  │                     │ - Session-based    │               │
│  │ - Local signing  │                     │ - Scoped access    │               │
│  └──────────────────┘                     └────────────────────┘               │
│                                                                                  │
└────────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Full Node (Personal)

Individual-owned node with complete functionality:

```rust
pub struct PersonalNode {
    /// Node's operational keypair (distinct from user DID)
    pub node_keypair: KeyPair,

    /// Owner's identity (may have multiple)
    pub owner_identities: Vec<Did>,

    /// Local keystore path
    pub keystore: AgeKeyStore,

    /// Node capabilities
    pub capabilities: NodeCapabilities,

    /// Hosting mode
    pub hosting: HostingMode,
}

pub enum HostingMode {
    /// Self-hosted on personal hardware
    SelfHosted,
    /// Hosted by a cooperative
    CoopHosted { coop_id: String },
    /// Cloud-hosted (user-controlled)
    CloudHosted { provider: String },
}

pub struct NodeCapabilities {
    pub gossip: bool,
    pub ledger_sync: bool,
    pub compute_provider: bool,
    pub gateway: bool,
    pub federation_relay: bool,
}
```

### 5.3 Shared/Hosted Nodes

Cooperative infrastructure supporting multiple members:

```rust
pub struct SharedNode {
    /// Node's institutional identity
    pub node_did: Did,

    /// Operating cooperative
    pub operator_coop: CooperativeId,

    /// Member access configuration
    pub member_access: MemberAccessConfig,

    /// Resource allocation
    pub resources: ResourceAllocation,

    /// Tenancy mode
    pub tenancy: TenancyMode,
}

pub struct MemberAccessConfig {
    /// Authentication methods accepted
    pub auth_methods: Vec<AuthMethod>,

    /// Per-member resource limits
    pub default_limits: ResourceLimits,

    /// Access control list
    pub acl: AccessControlList,
}

pub enum AuthMethod {
    /// DID-based signature challenge
    DidAuth,
    /// Delegated session token
    SessionToken,
    /// Capability-based access
    CapabilityToken { capabilities: Vec<String> },
}

pub enum TenancyMode {
    /// Shared resources, logical separation
    MultiTenant,
    /// Isolated containers per member
    Containerized,
    /// VMs per member
    VirtualMachine,
}
```

### 5.4 Light Clients

Mobile and web clients that don't run full nodes:

```rust
pub struct LightClient {
    /// Client identity
    pub did: Did,

    /// Local device keypair (for signing)
    pub device_keypair: KeyPair,

    /// Connected gateway(s)
    pub gateways: Vec<GatewayConnection>,

    /// Cached data
    pub local_cache: LightClientCache,

    /// Sync state
    pub sync_state: SyncState,
}

pub struct GatewayConnection {
    /// Gateway endpoint
    pub endpoint: String,

    /// Gateway DID (for verification)
    pub gateway_did: Did,

    /// Session token
    pub session: Option<SessionToken>,

    /// Delegated capabilities
    pub delegated_caps: Vec<Capability>,

    /// Connection status
    pub status: ConnectionStatus,
}

pub struct LightClientCache {
    /// Own DID document
    pub did_document: DidDocument,

    /// Known peer documents
    pub peer_documents: HashMap<Did, DidDocument>,

    /// Recent transactions
    pub recent_transactions: Vec<Transaction>,

    /// Governance proposals (if participating)
    pub active_proposals: Vec<ProposalSummary>,
}
```

### 5.5 Gateway Access Patterns

```rust
pub struct GatewayService {
    /// Gateway identity
    pub gateway_did: Did,

    /// Operating cooperative
    pub operator: CooperativeId,

    /// Supported protocols
    pub protocols: Vec<GatewayProtocol>,

    /// Rate limiting
    pub rate_limits: RateLimitConfig,

    /// Federation routing
    pub federation_routes: Vec<FederationRoute>,
}

pub enum GatewayProtocol {
    /// REST API
    Rest { base_url: String },
    /// WebSocket
    WebSocket { url: String },
    /// gRPC
    Grpc { endpoint: String },
    /// GraphQL
    GraphQL { endpoint: String },
}

/// Session for gateway-authenticated users
pub struct GatewaySession {
    /// Session ID
    pub session_id: String,

    /// User's DID
    pub user_did: Did,

    /// Device that created session
    pub device_id: String,

    /// Granted capabilities
    pub capabilities: Vec<Capability>,

    /// Expiration
    pub expires_at: u64,

    /// Refresh token
    pub refresh_token: Option<String>,
}
```

### 5.6 Node-Identity Separation

**Key Principle**: Node identity is operational; user identity is personal.

```rust
/// Mapping between nodes and identities
pub struct IdentityNodeMapping {
    /// User's DID
    pub user_did: Did,

    /// Nodes this user has access to
    pub accessible_nodes: Vec<NodeAccess>,

    /// Primary node (for routing)
    pub primary_node: Option<NodeId>,
}

pub struct NodeAccess {
    /// Node identifier
    pub node_id: NodeId,

    /// How user accesses this node
    pub access_type: NodeAccessType,

    /// Capabilities available on this node
    pub node_capabilities: Vec<Capability>,

    /// Trustworthiness of this access
    pub trust_level: TrustLevel,
}

pub enum NodeAccessType {
    /// User owns/controls the node
    Owner,
    /// User is a member with access
    Member,
    /// User accesses via gateway
    Gateway,
    /// User has delegated access
    Delegated { delegator: Did },
}
```

---

## 6. Implementation Phases

### Phase 1: Identity Foundation (4 weeks)

**Goal**: Establish individual identity independent of nodes

1. **Extend DID Document** (1 week)
   - Add `DeviceClass` to verification methods
   - Add `Delegate` and `Guardian` capabilities
   - Add organization-scoped capabilities

2. **Membership Registry** (1 week)
   - Create `MembershipRecord` structure
   - Implement storage in icn-store
   - Add gossip sync for membership changes

3. **Light Client Protocol** (2 weeks)
   - Define gateway authentication protocol
   - Implement session management
   - Create WebSocket subscription model

### Phase 2: Membership Lifecycle (3 weeks)

**Goal**: Unified membership model across entity types

1. **Unified Membership API** (1 week)
   - Refactor icn-cooperative and icn-community to use common types
   - Implement `MembershipPolicy` evaluation

2. **Membership Governance Integration** (1 week)
   - Connect membership decisions to icn-governance
   - Implement approval voting for membership

3. **Departure and Transfer** (1 week)
   - Implement graceful departure process
   - Build membership transfer between entities

### Phase 3: Federation Identity (3 weeks)

**Goal**: Cross-entity identity and trust

1. **Representative Delegation** (1 week)
   - Implement `RepresentativeAuth` structure
   - Add delegation verification to federation messages

2. **Cross-Entity Claims** (1 week)
   - Implement `CrossEntityClaim` protocol
   - Build membership proof verification

3. **Federated Trust Paths** (1 week)
   - Implement trust propagation algorithm
   - Add category-specific trust scoring

### Phase 4: Node Topology (4 weeks)

**Goal**: Support for diverse deployment patterns

1. **Shared Node Support** (1.5 weeks)
   - Implement multi-tenant architecture
   - Add member access control

2. **Gateway Services** (1.5 weeks)
   - Build gateway authentication service
   - Implement session management
   - Add federation routing

3. **Light Client SDK** (1 week)
   - Create TypeScript/JavaScript SDK
   - Implement React Native bindings
   - Add offline-first support

---

## 7. Security Considerations

### 7.1 Threat Model

| Threat | Mitigation |
|--------|------------|
| Key compromise | Multi-device, social recovery, key rotation |
| Sybil attacks | POP attestations, VUI commitments |
| Node compromise | Identity independent of nodes, delegation revocation |
| Gateway attacks | E2E encryption, signed envelopes, session limits |
| Federation attacks | Vouch-based admission, trust decay |
| Membership fraud | Governance approval, sponsor accountability |

### 7.2 Cryptographic Requirements

1. **Identity Keys**: Ed25519 (with ML-DSA hybrid option)
2. **Encryption**: X25519 for key agreement, AES-256-GCM for data
3. **Signatures**: Ed25519 for speed, ML-DSA for PQ resistance
4. **Key Derivation**: HKDF-SHA256
5. **Commitments**: SHA-256

### 7.3 Access Control

```rust
/// Access check for any operation
pub struct AccessCheck {
    /// Who is requesting
    pub requester: Did,
    /// What operation
    pub operation: Operation,
    /// Which resource
    pub resource: ResourceId,
    /// Context (e.g., which org, which node)
    pub context: AccessContext,
}

/// Result of access check
pub enum AccessDecision {
    /// Access granted
    Allowed { capabilities_used: Vec<Capability> },
    /// Access denied
    Denied { reason: String },
    /// Need additional auth
    NeedsElevation { required: Vec<Capability> },
}
```

---

## 8. Migration Strategy

### 8.1 From Node-Centric to Individual-Centric

Current state: Node keypair = User DID

Target state: User DID independent, Node has operational keypair

**Migration Steps**:

1. **Phase A**: Allow opt-in individual DID creation separate from node
2. **Phase B**: New users default to individual DID pattern
3. **Phase C**: Provide migration tool for existing users
4. **Phase D**: Deprecate node-as-identity pattern (soft deprecation)
5. **Phase E**: Full separation (breaking change, major version)

### 8.2 Backward Compatibility

```rust
/// Legacy compatibility wrapper
pub enum IdentityMode {
    /// New mode: individual DID separate from node
    IndividualIdentity {
        individual_did: Did,
        node_operational_did: Did,
    },

    /// Legacy mode: node DID is user DID
    NodeIdentity {
        node_did: Did,
    },
}

impl IdentityMode {
    /// Get the DID to use for signing user actions
    pub fn user_did(&self) -> &Did {
        match self {
            Self::IndividualIdentity { individual_did, .. } => individual_did,
            Self::NodeIdentity { node_did } => node_did,
        }
    }
}
```

### 8.3 Data Migration

1. **Membership records**: Add `member_identity_type` field
2. **Trust relationships**: Preserve for both DID types
3. **Ledger entries**: Signed by DID, no migration needed
4. **Governance votes**: Include identity type in vote metadata

---

## Appendix A: Cooperative Principles Alignment

| ICA Principle | Architecture Support |
|---------------|---------------------|
| 1. Voluntary and Open Membership | Open/InvitationOnly/Approval policies, easy departure |
| 2. Democratic Member Control | One-member-one-vote, tier-based participation rights |
| 3. Member Economic Participation | Capital contributions, profit sharing in membership |
| 4. Autonomy and Independence | Individual DIDs, cooperative institutional DIDs |
| 5. Education, Training, Information | Not directly addressed (application layer) |
| 6. Cooperation Among Cooperatives | Federation model with trust propagation |
| 7. Concern for Community | Community entity type, geographic communities |

---

## Appendix B: Related Documents

- `/home/matt/projects/icn/docs/ARCHITECTURE.md` - System architecture overview
- `/home/matt/projects/icn/docs/trust-multi-graph-migration.md` - Trust system design
- `/home/matt/projects/icn/docs/production-hardening.md` - Security measures
- `/home/matt/projects/icn/CLAUDE.md` - Development guidelines

---

## Appendix C: Open Questions

1. **Identity Portability**: How to handle identity when leaving all cooperatives? Should "homeless" identities be allowed?

2. **Organization DIDs**: Should cooperatives use anchor-based DIDs or remain with key-derived DIDs?

3. **Cross-Federation Identity**: If Alice is in Federation A and Federation B, how do we handle identity claims across federations that don't directly federate?

4. **Pseudonymity**: Should ICN support pseudonymous identities without full POP attestations? What are the implications for Sybil resistance?

5. **Identity Recovery Governance**: When social recovery is used, should the cooperative have a say if the identity has membership? Risk of collusion.

---

*Document Version: 1.0*
*Last Updated: 2025-12-25*
