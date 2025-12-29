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
pub mod protocol;
pub mod rate_limit;
pub mod replay_guard;
pub mod sequence_tracker;
pub mod session;
pub mod stun;
pub mod tls;
pub mod topology;
pub mod turn;
pub mod version;

pub use actor::{IncomingMessageHandler, NetworkActor, NetworkHandle, NetworkMsg, NetworkStats};
pub use blob_registry::{BlobLocation, BlobLocationRegistry};
pub use candidate::ConnectionCandidate;
pub use candidate_cache::CandidateCache;
pub use discovery::{Discovery, PeerInfo};
pub use encryption::EncryptedEnvelope;
pub use envelope::{PayloadType, SignedEnvelope};
pub use error::{NetError, Result};
pub use global_rate_limit::GlobalRateLimiter;
pub use protocol::{
    read_message, read_message_compressed, write_message, write_message_compressed,
    CompressionFormat, KnownPeer, MessagePayload, NetworkMessage, PeerExchangeMessage,
    COMPRESSION_THRESHOLD,
};
pub use rate_limit::{RateLimitConfig, RateLimiter, TrustGatedRateLimitConfig};
pub use replay_guard::ReplayGuard;
pub use sequence_tracker::OutgoingSequenceTracker;
pub use session::SessionManager;
pub use stun::StunClient;
pub use topology::{
    FanoutConfig, NeighborLimitsConfig, NeighborMetrics, NeighborSets, NetworkMetrics, NodeRole,
    PeerId, TopologyConfig, TopologyInfo,
};
pub use turn::{TurnAllocation, TurnClient, TurnConfig};
pub use version::{common_capabilities, negotiate_version, CapabilityFlags, VersionInfo};
