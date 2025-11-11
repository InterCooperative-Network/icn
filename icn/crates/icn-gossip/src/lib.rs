//! ICN Gossip - Topic-based gossip protocol with ACLs
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
//! ```rust
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
//! // Publish to a topic
//! let data = b"Hello, distributed world!".to_vec();
//! let hash = gossip.publish("global:identity", data).unwrap();
//! ```

pub mod bloom;
pub mod gossip;
pub mod types;
pub mod vector_clock;

pub use bloom::BloomFilter;
pub use gossip::{GossipActor, GossipHandle, SendMessageCallback};
pub use types::{AccessControl, ContentHash, GossipEntry, GossipMessage, Subscription, Topic};
pub use vector_clock::VectorClock;
