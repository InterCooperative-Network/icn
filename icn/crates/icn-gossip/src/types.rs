//! Gossip protocol types

use crate::vector_clock::VectorClock;
use icn_identity::Did;
use icn_trust::TrustClass;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Content hash for gossip entries
pub type ContentHash = [u8; 32];

/// Gossip entry metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipEntry {
    /// Unique hash of the entry content
    pub hash: ContentHash,

    /// Author of the entry
    pub author: Did,

    /// Vector clock for causal ordering
    pub clock: VectorClock,

    /// Topic this entry belongs to
    pub topic: String,

    /// Actual entry data (serialized)
    pub data: Vec<u8>,

    /// Timestamp (local, not for ordering)
    pub timestamp: u64,
}

/// Gossip message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    /// Announce a new entry (push)
    Announce {
        hash: ContentHash,
        author: Did,
        clock: VectorClock,
        topic: String,
    },

    /// Request full entry (pull)
    Request { hash: ContentHash },

    /// Response with full entry
    Response { entry: GossipEntry },

    /// Request bloom filter for anti-entropy
    RequestBloomFilter { topic: String },

    /// Send bloom filter
    SendBloomFilter {
        topic: String,
        filter: BloomFilterData,
    },

    /// Request missing entries based on bloom filter
    RequestMissing { hashes: Vec<ContentHash> },
}

/// Serialized bloom filter data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloomFilterData {
    /// Bloom filter bits
    pub bits: Vec<u8>,

    /// Number of hash functions
    pub num_hashes: u32,

    /// Size in bits
    pub size: u64,
}

/// Topic configuration
#[derive(Debug, Clone)]
pub struct Topic {
    /// Topic name (e.g., "global:identity", "contract:abc123")
    pub name: String,

    /// Access control for this topic
    pub acl: AccessControl,

    /// How long to retain entries
    pub retention: Duration,

    /// Maximum entries to store
    pub max_entries: usize,
}

impl Topic {
    /// Create a new topic
    pub fn new(name: String, acl: AccessControl) -> Self {
        Topic {
            name,
            acl,
            retention: Duration::from_secs(86400 * 30), // 30 days default
            max_entries: 10000,                         // Default limit
        }
    }

    /// Set retention period
    pub fn with_retention(mut self, retention: Duration) -> Self {
        self.retention = retention;
        self
    }

    /// Set max entries
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// Check if a DID can publish to this topic
    pub fn can_publish(&self, did: &Did, trust_class: Option<TrustClass>) -> bool {
        match &self.acl {
            AccessControl::Public => true,
            AccessControl::TrustClass(required_class) => {
                if let Some(actual_class) = trust_class {
                    actual_class >= *required_class
                } else {
                    false
                }
            }
            AccessControl::Participants(allowed) => allowed.contains(did),
        }
    }

    /// Check if a DID can subscribe to this topic
    pub fn can_subscribe(&self, did: &Did, trust_class: Option<TrustClass>) -> bool {
        // For now, same rules as publish
        // Could be more permissive in the future
        self.can_publish(did, trust_class)
    }
}

/// Access control for topics
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessControl {
    /// Anyone can publish/subscribe
    Public,

    /// Requires minimum trust class
    TrustClass(TrustClass),

    /// Only specific participants (e.g., contract members)
    Participants(Vec<Did>),
}

/// Subscription handle
#[derive(Debug, Clone)]
pub struct Subscription {
    /// Topic name
    pub topic: String,

    /// Subscriber DID
    pub subscriber: Did,
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_topic_public_access() {
        let topic = Topic::new("test".to_string(), AccessControl::Public);
        let did = KeyPair::generate().unwrap().did().clone();

        assert!(topic.can_publish(&did, None));
        assert!(topic.can_subscribe(&did, None));
    }

    #[test]
    fn test_topic_trust_class_access() {
        let topic = Topic::new(
            "test".to_string(),
            AccessControl::TrustClass(TrustClass::Partner),
        );
        let did = KeyPair::generate().unwrap().did().clone();

        // No trust class - denied
        assert!(!topic.can_publish(&did, None));

        // Known - denied (lower than Partner)
        assert!(!topic.can_publish(&did, Some(TrustClass::Known)));

        // Partner - allowed
        assert!(topic.can_publish(&did, Some(TrustClass::Partner)));

        // Federated - allowed (higher than Partner)
        assert!(topic.can_publish(&did, Some(TrustClass::Federated)));
    }

    #[test]
    fn test_topic_participants_access() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let charlie = KeyPair::generate().unwrap().did().clone();

        let topic = Topic::new(
            "test".to_string(),
            AccessControl::Participants(vec![alice.clone(), bob.clone()]),
        );

        assert!(topic.can_publish(&alice, None));
        assert!(topic.can_publish(&bob, None));
        assert!(!topic.can_publish(&charlie, None));
    }
}
