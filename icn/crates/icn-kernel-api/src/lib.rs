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

pub mod types;
pub mod identity;
pub mod authz;
pub mod state;
pub mod compute;
pub mod comms;
pub mod time;
pub mod coord;
pub mod naming;

// Re-export primary traits for convenience
pub use identity::{IdentityService, DidResolver, Keystore};
pub use authz::{CapabilityEngine, PolicyOracle, PolicyDecision};
pub use state::{LogService, BlobService, KvService};
pub use compute::{ComputeEngine, Job, Trigger};
pub use comms::{PubSub, RequestResponse, Streams};
pub use time::TimeService;
pub use coord::Coordination;
pub use naming::{NamingService, Discovery};

// Re-export common types
pub use types::*;
