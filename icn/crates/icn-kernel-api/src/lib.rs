//! # ICN Kernel API
//!
//! This crate defines the trait interfaces for all ICN kernel primitives.
//! Implementations live in other crates; this crate provides only contracts.
//!
//! ## Architecture
//!
//! The kernel provides **mechanisms**, not **semantics**. Domain logic
//! (membership, governance, ledger, trust) lives in apps that use these
//! primitives. The kernel treats all apps equally - "official" apps use
//! the same APIs as third-party apps.
//!
//! ## Primitives
//!
//! The kernel consists of eight primitives:
//!
//! 1. **Identity** - DID operations, signing, verification
//! 2. **Authorization** - Object-capabilities, policy oracles
//! 3. **State** - Logs, blobs, KV storage
//! 4. **Compute** - WASM execution, scheduling
//! 5. **Communication** - Pub/sub, request/response, streams
//! 6. **Time** - Logical clocks, scheduling, leases
//! 7. **Coordination** - Consensus groups, CRDTs
//! 8. **Naming** - Name resolution, service discovery
//!
//! ## The Meaning Firewall
//!
//! Before adding code to kernel crates, ask:
//! - Does this interpret domain semantics? → Must be an app
//! - Does this hardcode a schema? → Must be an app
//! - Does this privilege a specific application? → Must be an app
//!
//! The kernel is deliberately dumb. It provides pipes, not policies.

pub mod authz;
pub mod bootstrap;
pub mod budget;
pub mod comms;
pub mod compute;
pub mod container;
pub mod coord;
pub mod economics;
pub mod effects;
pub mod error;
pub mod escrow;
pub mod events;
pub mod execution;
pub mod governance;
pub mod identity;
pub mod invariants;
pub mod naming;
pub mod proofs;
pub mod protocol_params;
pub mod receipts;
pub mod resource;
pub mod scope;
pub mod services;
pub mod state;
pub mod storage;
pub mod time;
pub mod types;
pub mod version;

// Re-export primary traits for convenience
pub use authz::{
    ActionKind, AllowAllOracle, CapabilityEngine, ConstraintSet, ConstraintValue, DenyAllOracle,
    Domain, PolicyContext, PolicyDecision, PolicyError, PolicyOracle, PolicyRequest,
    PolicyRequestCore, RateLimit,
};
pub use bootstrap::{
    BootstrapPhase, CacheStats, CapabilityRequest, CapabilitySet, DecisionCache,
    GenesisCapabilities, OracleRegistry,
};
pub use budget::{
    BeginSpendOutcome, BudgetRecord, BudgetSpendError, BudgetStatus, BudgetStore, PendingSpend,
};
pub use comms::{PubSub, RequestResponse, Streams};
pub use compute::{ComputeEngine, DeterminismClass, Job, OperatorMode, PrivacyClass, Trigger};
pub use container::{
    ContainerError, ContainerResult, ContainerRuntime, ContainerSpec, ResourceLimits, ResourceUsage,
};
pub use coord::Coordination;
pub use economics::{AssetType, DepreciationSchedule, SettlementIntent};
pub use effects::{
    kernel_effect_subsystem, ControlEffect, DispatchEvidenceSink, DisputeEffect, EffectOutcome,
    EffectResult, FederationEffect, KernelEffect, MembershipEffect, ProtocolEffect, ResourceEffect,
    SdisEffect, TreasuryEffect,
};
pub use error::{ErrCode, IcnError};
pub use escrow::{
    BeginReleaseOutcome, EscrowRecord, EscrowReleaseError, EscrowStatus, EscrowStore,
};
pub use events::{EventCallback, EventEmitter, SystemEvent};
pub use execution::{ExecutionRecord, ExecutionStatus, ExecutionStore};
pub use governance::{
    federation_effect_to_operation, treasury_effect_to_operation, DecisionReceiptId,
    DefaultEffectExecutor, EffectExecutor, ExecutionOutcome, FederationExecutor,
    FederationOperation, FederationOperationType, GovernanceExecutor, ProtocolChange,
    ProtocolExecutor, TreasuryExecutor, TreasuryOperation, TreasuryOperationType,
};
pub use identity::{DidResolver, IdentityService, Keystore};
pub use naming::{
    Discovery, EndpointType, NamingService, ScopedDiscovery, ServiceEndpoint, ServiceEndpointId,
};
pub use proofs::{
    AntiEntropyProbe, ArtifactDigest, ArtifactReceipt, AuthorityBasis, BloomProjection,
    BoundaryRuleRef, BoundaryRuleSet, DegradationLevel, DigestMismatch, DivergenceClass,
    DivergenceEvidence, ExpectedRepairReceiptClass, FederationSyncWindow,
    FederationSyncWindowError, MerkleRootProjection, PeerSet, PeerSyncOutcome, PeerSyncReport,
    PeerSyncReportError, PolicyClauseRef, ProbeScope, QuorumSyncCheck, QuorumSyncCheckError,
    ReceiptDigest, RedundancyOutcome, RedundancyProof, RedundancyProofError, RepairAction,
    RepairFailureReason, RepairPlan, RepairReceipt, RepairReceiptClass, RepairReceiptError,
    RequestedResponseClass, RoutedMessageKind, RoutingProof, RoutingProofError, ShortDigestList,
    StateClass, StateDigest, SyncDegradedStatus, SyncDegradedStatusError, TriggerSource,
    UnknownOutOfScopeReason, VectorClockProjection, ANTI_ENTROPY_PROBE_SCHEMA_VERSION,
    DIVERGENCE_EVIDENCE_SCHEMA_VERSION, FEDERATION_SYNC_WINDOW_SCHEMA_VERSION,
    PEER_SYNC_REPORT_SCHEMA_VERSION, QUORUM_SYNC_CHECK_SCHEMA_VERSION,
    REDUNDANCY_PROOF_SCHEMA_VERSION, REPAIR_PLAN_SCHEMA_VERSION, REPAIR_RECEIPT_SCHEMA_VERSION,
    ROUTING_PROOF_SCHEMA_VERSION, SYNC_DEGRADED_STATUS_SCHEMA_VERSION,
};
pub use receipts::{
    compute_canonical_hash, AllocationReceipt, CanonicalReceipt, Hash, ProvenanceAnchors, ReceiptId,
};
pub use resource::{ResourceAccessRecord, ResourceAccessStore};
pub use scope::{CellId, MockCellService, ScopeLevel};
pub use services::{
    AddMemberRequest, AddMemberResult, AppointStewardRequest, AppointStewardResult, CellService,
    ClearingAgreementView, ClearingPositionView, ControlService, CooperativeView,
    FederationClearingRequest, FederationClearingResult, FederationClearingSettleRequest,
    FederationClearingSettleResult, FederationJoinRequest, FederationJoinResult,
    FederationLeaveRequest, FederationLeaveResult, FederationRevokeVouchRequest,
    FederationRevokeVouchResult, FederationService, FederationTerminateClearingRequest,
    FederationTerminateClearingResult, FederationVouchRequest, FederationVouchResult,
    ForceCloseProposalRequest, ForceCloseProposalResult, FreezeMemberRequest, FreezeMemberResult,
    GovernanceEvent, GovernanceService, LedgerEvent, LedgerService, MembershipService,
    NoOpSettlementQueryService, ReconfirmStewardRequest, ReconfirmStewardResult,
    ReinstateStewardRequest, ReinstateStewardResult, RemoveMemberRequest, RemoveMemberResult,
    RevokeStewardRequest, RevokeStewardResult, SanctionStewardRequest, SanctionStewardResult,
    SdisService, SecurityService, SecurityViolation, ServiceRegistry, SettlementQueryResult,
    SettlementQueryService, SettlementReceiptResult, SuspendStewardRequest, SuspendStewardResult,
    TreasuryEntryRequest, TreasuryEntryResult,
    TreasuryOperationType as ServicesTreasuryOperationType, TrustClass, TrustEvent, TrustService,
    UnfreezeMemberRequest, UnfreezeMemberResult, UpdateMemberRequest, UpdateMemberResult,
    VetoProposalRequest, VetoProposalResult, TRUST_THRESHOLD_FEDERATED, TRUST_THRESHOLD_KNOWN,
    TRUST_THRESHOLD_PARTNER,
};
pub use state::{
    BlobService, KvService, LogService, ObjectReplication, ReplicationPolicy, StateBackend,
    StateKey, StateOp, StateScope, StateValue,
};
pub use storage::{DataLocality, StorageClass, StorageValidationError};
pub use time::TimeService;
pub use version::Version;

pub use invariants::{
    BlockHeight, InvariantDomain, InvariantId, InvariantReport, InvariantViolation,
};

// Re-export common types
pub use types::*;
