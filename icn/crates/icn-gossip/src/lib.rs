//! ICN Gossip - Topic-based gossip protocol with ACLs
//!
//! # Safety
//! This crate denies panicking on unwrap/expect to prevent runtime crashes.
#![allow(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//!
//! This crate implements a distributed synchronization system using:
//!
//! - **Vector clocks** for causal ordering
//! - **Bloom filters** for efficient anti-entropy
//! - **Topic-based routing** with access control
//! - **Hybrid push/pull** sync protocol
//!
//! ## Example
//!
//! ```rust,ignore
//! use icn_gossip::{GossipActor, Topic, AccessControl};
//! use icn_identity::KeyPair;
//! use std::sync::Arc;
//!
//! let keypair = KeyPair::generate().unwrap();
//! let did = keypair.did().clone();
//!
//! // Create actor with no policy oracle (defaults to 0.0 trust)
//! let mut gossip = GossipActor::new(did, None);
//!
//! // Publish to a topic (async)
//! let data = b"Hello, distributed world!".to_vec();
//! let hash = gossip.publish("global:identity", data).await.unwrap();
//! ```

pub mod bloom;
#[allow(missing_docs)]
pub mod error;
pub mod gossip;
mod handlers;
pub mod key_rotation;
pub mod labor_shares;
#[allow(missing_docs)]
pub mod partition;
#[allow(missing_docs)]
pub mod scalability;
pub mod service_discovery;
pub mod sync;
#[allow(missing_docs)]
pub mod types;
pub mod vector_clock;

// Gossip protocol phase modules (extend gossip::GossipActor)
mod anti_entropy;
mod protocol;
mod subscriptions;

pub use bloom::{BloomFilter, BloomResizeConfig};
pub use error::{GossipError, Result};
pub use gossip::{
    start_digest_emitter, start_partition_checker, EntryNotificationCallback, GossipActor,
    GossipHandle, PeerSamplingCallback, SendMessageCallback, StorageContentNotFoundCallback,
    StorageProofCallback,
};
pub use partition::{
    Conflict, ConflictResolution, ConflictResolver, DataType, GapDirection, PartitionConfig,
    PartitionDetector, PartitionHealer, ResolutionOutcome, VectorClockMerger, VersionGap,
};
pub use scalability::{CompressedVectorClock, ShardStats, ShardedTopic, TopicShard, VarInt};
pub use sync::{Backoff, PeerSyncManager, PeerSyncState};
pub use types::{
    AccessControl, AdaptiveFanoutConfig, ContentHash, GossipEntry, GossipMessage, ResourceLimits,
    Scope, Subscription, SyncCursor, Topic, TopicAutoCreationPolicy,
};
pub use vector_clock::VectorClock;

// Labor share gossip messages (Issue #391)
pub use labor_shares::{
    topics as labor_share_topics, BondMessage, BondPaymentType, LaborShareMessage,
};

// Service discovery gossip messages (Epic 3, Issue #935)
pub use service_discovery::{
    sign_service_endpoint, topics as service_discovery_topics, verify_service_endpoint,
    ServiceDiscoveryMessage,
};

// Key rotation gossip messages (Issue #469)
pub use key_rotation::{
    KeyRotationCache, KeyRotationMessage, RotationReason, KEY_ROTATION_GRACE_PERIOD_SECS,
    TOPIC_KEY_ROTATION,
};
