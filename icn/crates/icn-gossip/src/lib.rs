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
//! use icn_trust::TrustClass;
//! use std::sync::Arc;
//!
//! let keypair = KeyPair::generate().unwrap();
//! let did = keypair.did().clone();
//!
//! // Trust lookup (simplified)
//! let trust_lookup = Arc::new(|_did: &icn_identity::Did| Some(TrustClass::Partner));
//!
//! let mut gossip = GossipActor::new(did, trust_lookup);
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
pub mod labor_shares;
#[allow(missing_docs)]
pub mod partition;
#[allow(missing_docs)]
pub mod scalability;
pub mod sync;
#[allow(missing_docs)]
pub mod types;
pub mod vector_clock;

pub use bloom::BloomFilter;
pub use error::{GossipError, Result};
pub use gossip::{
    start_digest_emitter, start_partition_checker, EntryNotificationCallback, GossipActor,
    GossipHandle, PeerSamplingCallback, SendMessageCallback,
};
pub use partition::{
    Conflict, ConflictResolution, ConflictResolver, DataType, GapDirection, PartitionConfig,
    PartitionDetector, PartitionHealer, ResolutionOutcome, VectorClockMerger, VersionGap,
};
pub use scalability::{CompressedVectorClock, ShardStats, ShardedTopic, TopicShard, VarInt};
pub use sync::{Backoff, PeerSyncManager, PeerSyncState};
pub use types::{
    AccessControl, ContentHash, GossipEntry, GossipMessage, Scope, Subscription, SyncCursor, Topic,
    TrustResourceLimits,
};
pub use vector_clock::VectorClock;

// Labor share gossip messages (Issue #391)
pub use labor_shares::{
    topics as labor_share_topics, BondMessage, BondPaymentType, LaborShareMessage,
};
