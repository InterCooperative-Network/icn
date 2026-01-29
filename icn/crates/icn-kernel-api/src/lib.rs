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
pub mod identity;
pub mod naming;
pub mod scope;
pub mod services;
pub mod state;
pub mod time;
pub mod types;

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
pub use compute::{ComputeEngine, Job, Trigger};
pub use coord::Coordination;
pub use identity::{DidResolver, IdentityService, Keystore};
pub use naming::{Discovery, NamingService};
pub use scope::{CellId, MockCellService, ScopeLevel};
pub use services::{
    CellService, GovernanceEvent, GovernanceService, LedgerEvent, LedgerService, SecurityService,
    SecurityViolation, ServiceRegistry, TrustClass, TrustEvent, TrustService,
    TRUST_THRESHOLD_FEDERATED, TRUST_THRESHOLD_KNOWN, TRUST_THRESHOLD_PARTNER,
};
pub use state::{BlobService, KvService, LogService};
pub use time::TimeService;

// Re-export common types
pub use types::*;
