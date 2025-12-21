//! ICN Federation Layer
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//!
//! This crate implements the federation protocol for inter-cooperative coordination.
//! It enables multiple ICN cooperatives to:
//!
//! - **Discover** each other via gossip-based registry announcements
//! - **Bridge trust** through federated attestations
//! - **Settle credits** across cooperative boundaries
//! - **Route gossip** with cooperative-scoped visibility
//! - **Resolve DIDs** across federation boundaries
//!
//! # Architecture
//!
//! The federation layer integrates with existing ICN components:
//!
//! - `icn-gossip`: Topic-based message distribution for registry and attestations
//! - `icn-trust`: Extended with federated trust score computation
//! - `icn-ledger`: Cross-cooperative transfer settlement
//! - `icn-identity`: DID format with cooperative prefix routing
//!
//! # DID Format
//!
//! Federation uses prefix-format DIDs for routing:
//! ```text
//! did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK
//! ```
//!
//! Where `food-coop` is the cooperative ID and the rest is the base58-encoded public key.

pub mod error;
pub mod gossip;
pub mod metrics;
pub mod registry;
pub mod types;

// Phase F2: Trust Bridging
pub mod attestation;
pub mod attestation_store;

// Phase F3: Credit Settlement
pub mod clearing;
pub mod clearing_manager;

// Phase F4: Scoped Gossip
pub mod channel;
pub mod router;

// Phase F5: DID Resolution
pub mod resolver;

// Re-exports
pub use attestation::{EvidenceSummary, FederatedTrustAttestation, TrustContext};
pub use attestation_store::AttestationStore;
pub use channel::FederationChannel;
pub use clearing::{
    BilateralClearingAgreement, ClearingPosition, CrossCoopTransfer, SettlementInterval,
    TransferStatus,
};
pub use clearing_manager::{ClearingManager, SettlementReport};
pub use error::{FederationError, Result};
pub use gossip::FederationGossipHandler;
pub use registry::CooperativeRegistry;
pub use resolver::{CachedDidResolution, FederatedDidResolver};
pub use router::FederatedGossipRouter;
pub use router::GossipScope;
pub use types::{
    CooperativeInfo, CurrencyInfo, FederationMessage, FederationPolicy, PolicyResult, Vouch,
};

/// Topic constants for federation gossip (re-exported at crate level)
pub const TOPIC_FEDERATION_REGISTRY: &str = "federation:registry";
pub const TOPIC_FEDERATION_TRUST: &str = "federation:trust";
pub const TOPIC_FEDERATION_CLEARING: &str = "federation:clearing";

/// Federation gossip topics
pub mod topics {
    /// Topic for cooperative registry announcements
    pub const FEDERATION_REGISTRY: &str = "federation:registry";

    /// Topic for trust attestation broadcasts
    pub const FEDERATION_ATTESTATIONS: &str = "federation:attestations";

    /// Topic for DID resolution queries
    pub const FEDERATION_DID_RESOLUTION: &str = "federation:did-resolution";

    /// Topic prefix for bilateral clearing (format: federation:clearing:{coop_a}:{coop_b})
    pub const FEDERATION_CLEARING_PREFIX: &str = "federation:clearing";
}

/// Default configuration values
pub mod defaults {
    use std::time::Duration;

    /// How often to announce own cooperative to the network
    pub const ANNOUNCEMENT_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes

    /// Default trust decay factor for federated attestations
    pub const TRUST_DECAY_FACTOR: f64 = 0.5;

    /// Default attestation expiry (30 days)
    pub const ATTESTATION_EXPIRY_SECS: u64 = 30 * 24 * 60 * 60;

    /// Maximum attestations per minute per cooperative (rate limit)
    pub const MAX_ATTESTATIONS_PER_MINUTE: u32 = 10;

    /// Maximum federation channels
    pub const MAX_CHANNELS: usize = 50;

    /// Channel timeout
    pub const CHANNEL_TIMEOUT: Duration = Duration::from_secs(300);

    /// DID resolution cache TTL
    pub const DID_RESOLUTION_CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour

    /// DID resolution cache size
    pub const DID_RESOLUTION_CACHE_SIZE: usize = 1000;
}
