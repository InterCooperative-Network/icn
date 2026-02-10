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
pub mod comms;
pub mod compute;
pub mod coord;
pub mod effects;
pub mod error;
pub mod events;
pub mod governance;
pub mod identity;
pub mod naming;
pub mod proofs;
pub mod protocol_params;
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
pub use comms::{PubSub, RequestResponse, Streams};
pub use compute::{ComputeEngine, DeterminismClass, Job, OperatorMode, PrivacyClass, Trigger};
pub use coord::Coordination;
pub use effects::{
    ControlEffect, DisputeEffect, EffectResult, FederationEffect, KernelEffect, MembershipEffect,
    ProtocolEffect, ResourceEffect, SdisEffect, TreasuryEffect,
};
pub use error::{ErrCode, IcnError};
pub use events::{EventCallback, EventEmitter, SystemEvent};
pub use governance::{
    DecisionReceiptId, DefaultEffectExecutor, EffectExecutor, ExecutionOutcome,
    FederationExecutor, FederationOperation, FederationOperationType, GovernanceExecutor,
    ProtocolChange, ProtocolExecutor, TreasuryExecutor, TreasuryOperation, TreasuryOperationType,
    federation_effect_to_operation,
};
pub use identity::{DidResolver, IdentityService, Keystore};
pub use naming::{
    Discovery, EndpointType, NamingService, ScopedDiscovery, ServiceEndpoint, ServiceEndpointId,
};
pub use proofs::ArtifactReceipt;
pub use scope::{CellId, MockCellService, ScopeLevel};
pub use services::{
    CellService, GovernanceEvent, GovernanceService, LedgerEvent, LedgerService, SecurityService,
    SecurityViolation, ServiceRegistry, TreasuryEntryRequest, TreasuryEntryResult,
    TreasuryOperationType as ServicesTreasuryOperationType, TrustClass, TrustEvent, TrustService,
    TRUST_THRESHOLD_FEDERATED, TRUST_THRESHOLD_KNOWN, TRUST_THRESHOLD_PARTNER,
};
pub use state::{
    BlobService, KvService, LogService, ObjectReplication, ReplicationPolicy, StateBackend,
    StateKey, StateOp, StateScope, StateValue,
};
pub use storage::{DataLocality, StorageClass, StorageValidationError};
pub use time::TimeService;
pub use version::Version;

// Re-export common types
pub use types::*;
