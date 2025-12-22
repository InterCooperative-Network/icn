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

/// Cursor expiry in milliseconds (5 minutes)
const CURSOR_EXPIRY_MS: u64 = 5 * 60 * 1000;

/// Opaque cursor for pagination in pull protocol (Issue #123)
///
/// Enables resumable sync by tracking position within a paginated response.
/// Cursors are stateless on the server side - all state is encoded in the cursor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncCursor {
    /// Index of the last sent entry (for ordered iteration)
    pub last_index: u64,

    /// Hash of the last sent entry (for verification)
    pub last_hash: ContentHash,

    /// Topic being synced
    pub topic: String,

    /// Timestamp of cursor creation (Unix ms, for expiry)
    pub created_at: u64,
}

impl SyncCursor {
    /// Create a new sync cursor
    pub fn new(last_index: u64, last_hash: ContentHash, topic: String) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            last_index,
            last_hash,
            topic,
            created_at,
        }
    }

    /// Check if cursor is expired (default: 5 minutes)
    pub fn is_expired(&self) -> bool {
        self.is_expired_with_ttl(CURSOR_EXPIRY_MS)
    }

    /// Check if cursor is expired with custom TTL
    pub fn is_expired_with_ttl(&self, max_age_ms: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        now.saturating_sub(self.created_at) > max_age_ms
    }

    /// Validate cursor against expected topic
    pub fn is_valid_for_topic(&self, topic: &str) -> bool {
        self.topic == topic && !self.is_expired()
    }
}

/// Gossip scope for targeted message propagation
///
/// Determines how far gossip messages should propagate based on
/// geographic/organizational proximity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Scope {
    /// Local cluster only (same region + cluster)
    LocalCluster,
    /// Regional scope (same region, may span clusters)
    Regional,
    /// Global scope (all neighbors, cross-region)
    #[default]
    Global,
}

/// Replica health status (Phase 17)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReplicaHealth {
    /// Replica verified recently (healthy)
    Healthy,
    /// Replica not verified recently (stale)
    Stale,
    /// Peer reported as offline/unreachable
    Unreachable,
}

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

    /// Optional: Replica metadata for data durability (Phase 17)
    /// If present, indicates this peer is willing to serve as a replica
    /// Note: Don't use skip_serializing_if with bincode - it doesn't support self-describing format
    #[serde(default)]
    pub replica_offered: Option<bool>,
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

    /// Digest announcing what a peer has (enhanced anti-entropy)
    Digest {
        topic: String,
        vector: VectorClock,
        bloom: BloomFilterData,
        hint_count: u32,
        nonce: u64,
    },

    /// Targeted pull request for missing entries
    PullRequest {
        topic: String,
        want_ids: Vec<ContentHash>,
        max_bytes: u32,
        nonce: u64,
        /// Optional cursor for resuming from previous response (Issue #123)
        #[serde(default)]
        cursor: Option<SyncCursor>,
    },

    /// Response with multiple entries (may be truncated)
    PullResponse {
        topic: String,
        entries: Vec<GossipEntry>,
        truncated: bool,
        nonce: u64,
        /// Cursor for fetching next page, None if complete (Issue #123)
        #[serde(default)]
        next_cursor: Option<SyncCursor>,
    },

    /// Announce blob availability (Phase 16C - data locality)
    BlobAnnounce {
        /// Blob hash (same as ContentHash)
        blob_hash: ContentHash,
        /// Peer that has the blob
        peer_did: Did,
        /// Blob size in bytes
        size_bytes: u64,
    },

    /// Request replica for a content hash (Phase 17 - data durability)
    ReplicaRequest {
        /// Content hash to replicate
        content_hash: ContentHash,
        /// DID of peer requesting replication
        requesting_peer: Did,
    },

    /// Offer to serve as replica for a content hash (Phase 17)
    ReplicaOffer {
        /// Content hash being offered for replication
        content_hash: ContentHash,
        /// DID of peer offering to replicate
        offering_peer: Did,
        /// Health status of this replica
        health: ReplicaHealth,
    },

    /// Status update about replicas for a content hash (Phase 17)
    ReplicaStatus {
        /// Content hash being reported
        content_hash: ContentHash,
        /// Known replicas and their health
        replicas: Vec<(Did, ReplicaHealth)>,
    },

    /// Request partition healing with peer (Phase 18 Week 3)
    /// Sent when a node detects it's reconnecting after a partition
    PartitionHealRequest {
        /// Requesting peer's DID
        requesting_peer: Did,
        /// Requesting peer's current vector clock
        vector_clock: VectorClock,
        /// Time of last known contact (Unix timestamp ms)
        last_contact_ms: u64,
    },

    /// Response to partition heal request (Phase 18 Week 3)
    PartitionHealResponse {
        /// Responding peer's DID
        responding_peer: Did,
        /// Responding peer's current vector clock
        vector_clock: VectorClock,
        /// Topics that may have conflicts
        diverged_topics: Vec<String>,
        /// Number of entries that need sync
        entries_behind: u64,
    },
}

impl GossipMessage {
    /// Get the variant name for logging and tracing
    pub fn variant_name(&self) -> &'static str {
        match self {
            GossipMessage::Announce { .. } => "Announce",
            GossipMessage::Request { .. } => "Request",
            GossipMessage::Response { .. } => "Response",
            GossipMessage::RequestBloomFilter { .. } => "RequestBloomFilter",
            GossipMessage::SendBloomFilter { .. } => "SendBloomFilter",
            GossipMessage::RequestMissing { .. } => "RequestMissing",
            GossipMessage::Digest { .. } => "Digest",
            GossipMessage::PullRequest { .. } => "PullRequest",
            GossipMessage::PullResponse { .. } => "PullResponse",
            GossipMessage::BlobAnnounce { .. } => "BlobAnnounce",
            GossipMessage::ReplicaRequest { .. } => "ReplicaRequest",
            GossipMessage::ReplicaOffer { .. } => "ReplicaOffer",
            GossipMessage::ReplicaStatus { .. } => "ReplicaStatus",
            GossipMessage::PartitionHealRequest { .. } => "PartitionHealRequest",
            GossipMessage::PartitionHealResponse { .. } => "PartitionHealResponse",
        }
    }
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

    /// Gossip scope for this topic (determines propagation distance)
    #[allow(dead_code)] // Will be used in Phase 1C gossip fanout
    pub scope: Scope,

    /// Minimum trust score required for subscription (0.0 - 1.0)
    /// When set, overrides coarse-grained AccessControl with fine-grained trust score check
    /// Requires GossipActor to have trust_graph configured
    /// Examples: 0.1 (Known+), 0.4 (Partner+), 0.7 (Federated)
    pub min_trust_threshold: Option<f64>,

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
            scope: Scope::Global,      // Default to global scope
            min_trust_threshold: None, // No fine-grained threshold by default
            retention: Duration::from_secs(86400 * 30), // 30 days default
            max_entries: 10000,        // Default limit
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

    /// Set gossip scope for this topic
    pub fn with_scope(mut self, scope: Scope) -> Self {
        self.scope = scope;
        self
    }

    /// Set minimum trust score threshold for subscription
    /// This enables fine-grained trust-based access control
    /// Requires GossipActor to be configured with trust_graph
    pub fn with_min_trust_threshold(mut self, threshold: f64) -> Self {
        self.min_trust_threshold = Some(threshold);
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

/// Resource limits for a specific trust class
#[derive(Debug, Clone)]
pub struct TrustResourceLimits {
    /// Maximum bytes per pull request
    pub max_pull_bytes: u32,

    /// Maximum bytes per push
    pub max_push_bytes: u32,

    /// Maximum outstanding pull requests
    pub max_outstanding_reqs: u32,

    /// Minimum retry backoff in milliseconds
    pub retry_min_ms: u64,

    /// Maximum retry backoff in milliseconds
    pub retry_max_ms: u64,
}

impl TrustResourceLimits {
    /// Get resource limits for a specific trust class
    pub fn for_trust_class(trust_class: TrustClass) -> Self {
        match trust_class {
            TrustClass::Isolated => Self {
                max_pull_bytes: 64 * 1024, // 64 KB
                max_push_bytes: 64 * 1024,
                max_outstanding_reqs: 1,
                retry_min_ms: 1500,
                retry_max_ms: 5000,
            },
            TrustClass::Known => Self {
                max_pull_bytes: 256 * 1024, // 256 KB
                max_push_bytes: 256 * 1024,
                max_outstanding_reqs: 2,
                retry_min_ms: 800,
                retry_max_ms: 2500,
            },
            TrustClass::Partner => Self {
                max_pull_bytes: 1024 * 1024, // 1 MB
                max_push_bytes: 1024 * 1024,
                max_outstanding_reqs: 3,
                retry_min_ms: 300,
                retry_max_ms: 1200,
            },
            TrustClass::Federated => Self {
                max_pull_bytes: 1024 * 1024, // 1 MB (same as Partner)
                max_push_bytes: 1024 * 1024,
                max_outstanding_reqs: 3,
                retry_min_ms: 300,
                retry_max_ms: 1200,
            },
        }
    }
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
            replica_offered: None,
        };

        // Should not compress (too small)
        entry.compress().unwrap();
        assert!(
            !entry.compressed,
            "Should not compress data below threshold"
        );
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
            replica_offered: None,
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
            replica_offered: None,
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
            replica_offered: None,
        };

        // Compress twice - should be idempotent
        entry.compress().unwrap();
        let compressed_once = entry.data.clone();

        entry.compress().unwrap();
        assert_eq!(
            entry.data, compressed_once,
            "Double compression should be no-op"
        );
    }

    // Issue #123: SyncCursor tests

    #[test]
    fn test_sync_cursor_creation() {
        let hash: ContentHash = [1u8; 32];
        let cursor = SyncCursor::new(42, hash, "test-topic".to_string());

        assert_eq!(cursor.last_index, 42);
        assert_eq!(cursor.last_hash, hash);
        assert_eq!(cursor.topic, "test-topic");
        assert!(cursor.created_at > 0);
    }

    #[test]
    fn test_sync_cursor_not_expired() {
        let cursor = SyncCursor::new(0, [0u8; 32], "topic".to_string());

        // Should not be expired immediately after creation
        assert!(!cursor.is_expired());
        assert!(!cursor.is_expired_with_ttl(60_000)); // 1 minute
    }

    #[test]
    fn test_sync_cursor_expired_with_zero_ttl() {
        let mut cursor = SyncCursor::new(0, [0u8; 32], "topic".to_string());

        // Simulate old cursor
        cursor.created_at = 0;

        assert!(cursor.is_expired_with_ttl(1)); // 1ms TTL
    }

    #[test]
    fn test_sync_cursor_valid_for_topic() {
        let cursor = SyncCursor::new(0, [0u8; 32], "my-topic".to_string());

        assert!(cursor.is_valid_for_topic("my-topic"));
        assert!(!cursor.is_valid_for_topic("other-topic"));
    }

    #[test]
    fn test_sync_cursor_invalid_for_topic_when_expired() {
        let mut cursor = SyncCursor::new(0, [0u8; 32], "my-topic".to_string());
        cursor.created_at = 0; // Make it expired

        // Even with matching topic, expired cursor is invalid
        assert!(!cursor.is_valid_for_topic("my-topic"));
    }
}
