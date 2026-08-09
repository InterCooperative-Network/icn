//! ICN Net - Network transport, discovery, and session management
//!
//! # Safety
//! This crate denies panicking on unwrap/expect to prevent runtime crashes.
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod actor;
pub mod blob_registry;
pub mod candidate;
pub mod candidate_cache;
pub mod discovery;
pub mod encryption;
pub mod envelope;
pub mod error;
pub mod global_rate_limit;
mod handlers;
pub mod nat;
pub mod preauth_admission;
pub mod protocol;
pub mod rate_limit;
pub mod relay_proxy;
pub mod replay_guard;
pub mod sequence_tracker;
pub mod session;
pub mod signing_sequence;
pub mod stun;
pub mod tls;
pub mod topology;
pub mod turn;
pub mod version;

pub use actor::{
    IncomingMessageHandler, NatStatus, NetworkActor, NetworkHandle, NetworkMsg, NetworkStats,
    TraversalMode,
};
pub use blob_registry::{
    BlobLocation, BlobLocationRegistry, BlobRegistryConfig, BlobRegistryError,
};
pub use candidate::{ConnectionCandidate, EndpointCandidate, EndpointKind};
pub use candidate_cache::CandidateCache;
pub use discovery::{Discovery, PeerInfo};
pub use encryption::{EncryptedEnvelope, EncryptionType};
pub use envelope::{PayloadType, SignedEnvelope};
pub use error::{NetError, Result};
pub use global_rate_limit::GlobalRateLimiter;
pub use nat::{NatConfig, NatTraversal, NatType, PublicAddress, TurnServerConfig};
pub use preauth_admission::{
    AdmissionGuard, AdmissionRefusal, PreAuthAdmission, MAX_PREAUTH_CONNECTIONS_PER_SOURCE,
    MAX_PREAUTH_CONNECTIONS_TOTAL, PREAUTH_AUTHENTICATION_DEADLINE,
    PREAUTH_AUTHENTICATION_TIMEOUT_CODE,
};
pub use protocol::{
    read_message, read_message_compressed, read_message_negotiated, write_message,
    write_message_compressed, write_message_negotiated, CompressionFormat, EncodingFormat,
    KnownPeer, MessagePayload, NetworkMessage, PeerExchangeMessage, COMPRESSION_THRESHOLD,
};
pub use rate_limit::{
    RateLimitConfig, RateLimiter, SourcePreAuthBudget, MAX_PREAUTH_BUDGET_SOURCES, NETWORK_DOMAIN,
    PREAUTH_SOURCE_BURST, PREAUTH_SOURCE_RENEWAL_WINDOW,
};
pub use relay_proxy::{ProxyHandle, TurnRelayProxy};
pub use replay_guard::ReplayGuard;
pub use sequence_tracker::OutgoingSequenceTracker;
pub use session::{DialOutcome, SessionManager};
pub use signing_sequence::SigningSequenceCounter;
pub use stun::StunClient;
pub use topology::{
    FanoutConfig, NeighborLimitsConfig, NeighborMetrics, NeighborSets, NetworkMetrics, NodeRole,
    PeerId, TopologyConfig, TopologyInfo,
};
pub use turn::{TurnAllocation, TurnClient, TurnConfig};
pub use version::{common_capabilities, negotiate_version, CapabilityFlags, VersionInfo};
