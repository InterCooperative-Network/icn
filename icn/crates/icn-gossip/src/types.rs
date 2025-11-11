//! Gossip protocol types

use crate::vector_clock::VectorClock;
use icn_identity::Did;
use icn_trust::TrustClass;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Content hash for gossip entries
pub type ContentHash = [u8; 32];

/// Minimum size (in bytes) for compression to be worthwhile
const COMPRESSION_THRESHOLD: usize = 1024; // 1 KB

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

    /// Actual entry data (may be compressed)
    pub data: Vec<u8>,

    /// Whether data is zstd-compressed
    #[serde(default)]
    pub compressed: bool,

    /// Timestamp (local, not for ordering)
    pub timestamp: u64,
}

impl GossipEntry {
    /// Compress entry data if it exceeds the compression threshold
    ///
    /// Uses zstd compression with level 3 (fast compression, good ratio).
    /// Only compresses if data is >= 1KB and compression reduces size.
    pub fn compress(&mut self) -> anyhow::Result<()> {
        if self.compressed || self.data.len() < COMPRESSION_THRESHOLD {
            return Ok(()); // Already compressed or too small
        }

        let original_size = self.data.len();
        let compressed_data = zstd::encode_all(self.data.as_slice(), 3)?;

        // Only use compression if it actually reduces size
        if compressed_data.len() < original_size {
            tracing::debug!(
                "Compressed gossip entry: {} -> {} bytes",
                original_size,
                compressed_data.len()
            );
            self.data = compressed_data;
            self.compressed = true;
        }

        Ok(())
    }

    /// Decompress entry data if it's compressed
    pub fn decompress(&mut self) -> anyhow::Result<()> {
        if !self.compressed {
            return Ok(()); // Not compressed
        }

        let decompressed_data = zstd::decode_all(self.data.as_slice())?;
        self.data = decompressed_data;
        self.compressed = false;

        Ok(())
    }

    /// Get decompressed data without modifying the entry
    pub fn get_data(&self) -> anyhow::Result<Vec<u8>> {
        if self.compressed {
            Ok(zstd::decode_all(self.data.as_slice())?)
        } else {
            Ok(self.data.clone())
        }
    }
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

    #[test]
    fn test_compression_small_data() {
        let kp = KeyPair::generate().unwrap();
        let data = vec![0u8; 512]; // 512 bytes - below threshold

        let mut entry = GossipEntry {
            hash: [0u8; 32],
            author: kp.did().clone(),
            clock: VectorClock::new(),
            topic: "test".to_string(),
            data: data.clone(),
            compressed: false,
            timestamp: 0,
        };

        // Should not compress (too small)
        entry.compress().unwrap();
        assert!(!entry.compressed, "Should not compress data below threshold");
        assert_eq!(entry.data, data);
    }

    #[test]
    fn test_compression_large_data() {
        let kp = KeyPair::generate().unwrap();
        // Create compressible data (repetitive)
        let data = vec![42u8; 2048]; // 2KB of same byte

        let mut entry = GossipEntry {
            hash: [0u8; 32],
            author: kp.did().clone(),
            clock: VectorClock::new(),
            topic: "test".to_string(),
            data: data.clone(),
            compressed: false,
            timestamp: 0,
        };

        let original_size = entry.data.len();

        // Should compress
        entry.compress().unwrap();
        assert!(entry.compressed, "Should compress data above threshold");
        assert!(
            entry.data.len() < original_size,
            "Compressed size should be smaller"
        );

        // Should decompress back to original
        entry.decompress().unwrap();
        assert!(!entry.compressed);
        assert_eq!(entry.data, data);
    }

    #[test]
    fn test_get_data_compressed() {
        let kp = KeyPair::generate().unwrap();
        let data = vec![99u8; 2048];

        let mut entry = GossipEntry {
            hash: [0u8; 32],
            author: kp.did().clone(),
            clock: VectorClock::new(),
            topic: "test".to_string(),
            data: data.clone(),
            compressed: false,
            timestamp: 0,
        };

        // Compress
        entry.compress().unwrap();
        assert!(entry.compressed);

        // get_data should return decompressed without modifying entry
        let retrieved = entry.get_data().unwrap();
        assert_eq!(retrieved, data);
        assert!(entry.compressed, "Entry should still be compressed");
    }

    #[test]
    fn test_compression_idempotent() {
        let kp = KeyPair::generate().unwrap();
        let data = vec![1u8; 2048];

        let mut entry = GossipEntry {
            hash: [0u8; 32],
            author: kp.did().clone(),
            clock: VectorClock::new(),
            topic: "test".to_string(),
            data: data.clone(),
            compressed: false,
            timestamp: 0,
        };

        // Compress twice - should be idempotent
        entry.compress().unwrap();
        let compressed_once = entry.data.clone();

        entry.compress().unwrap();
        assert_eq!(entry.data, compressed_once, "Double compression should be no-op");
    }
}
