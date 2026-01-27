//! Gossip protocol types

use crate::vector_clock::VectorClock;
use icn_identity::Did;
use icn_store::{StorageChallenge, StorageContentNotFound, StorageProof};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Content hash for gossip entries
pub type ContentHash = [u8; 32];

/// Minimum size (in bytes) for compression to be worthwhile
const COMPRESSION_THRESHOLD: usize = 1024; // 1 KB

/// Maximum decompressed size for gossip entry data (10 MB)
/// This prevents memory exhaustion from malicious/corrupted compressed data
const MAX_DECOMPRESSED_SIZE: usize = 10 * 1024 * 1024;

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

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::LocalCluster => write!(f, "local_cluster"),
            Scope::Regional => write!(f, "regional"),
            Scope::Global => write!(f, "global"),
        }
    }
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
    ///
    /// Uses bounded decompression to prevent memory exhaustion from
    /// malicious or corrupted compressed data.
    pub fn decompress(&mut self) -> anyhow::Result<()> {
        if !self.compressed {
            return Ok(()); // Not compressed
        }

        let decompressed_data = decompress_bounded(&self.data, MAX_DECOMPRESSED_SIZE)?;
        self.data = decompressed_data;
        self.compressed = false;

        Ok(())
    }

    /// Get decompressed data without modifying the entry
    ///
    /// Uses bounded decompression to prevent memory exhaustion from
    /// malicious or corrupted compressed data.
    pub fn get_data(&self) -> anyhow::Result<Vec<u8>> {
        if self.compressed {
            decompress_bounded(&self.data, MAX_DECOMPRESSED_SIZE)
        } else {
            Ok(self.data.clone())
        }
    }
}

/// Decompress zstd data with a bounded output size to prevent memory exhaustion.
///
/// This prevents malicious or corrupted compressed data with an absurd size header
/// from causing allocation failures.
fn decompress_bounded(compressed: &[u8], max_size: usize) -> anyhow::Result<Vec<u8>> {
    use anyhow::Context;
    use std::io::Read;

    let mut decoder =
        zstd::stream::read::Decoder::new(compressed).context("Failed to create zstd decoder")?;

    // Allocate buffer with reasonable initial capacity
    let initial_capacity = std::cmp::min(compressed.len().saturating_mul(4), max_size);
    let mut decompressed = Vec::with_capacity(initial_capacity);

    // Read up to max_size + 1 bytes to detect if we'd exceed the limit
    let mut limited_reader = (&mut decoder).take((max_size + 1) as u64);
    limited_reader
        .read_to_end(&mut decompressed)
        .context("Failed to decompress data")?;

    if decompressed.len() > max_size {
        anyhow::bail!(
            "Decompressed data too large: {} bytes (max {})",
            decompressed.len(),
            max_size
        );
    }

    Ok(decompressed)
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

    /// Storage challenge from verifier (Proof-of-Storage)
    StorageChallengeMsg {
        /// The storage challenge
        challenge: StorageChallenge,
    },

    /// Storage proof response from replica holder (Proof-of-Storage)
    StorageProofMsg {
        /// The storage proof
        proof: StorageProof,
    },

    /// Content not found response from replica holder (Proof-of-Storage)
    ///
    /// Sent when a node receives a storage challenge but doesn't have the content.
    /// This allows the challenger to distinguish between nodes that don't have the
    /// content (honest) versus nodes that refuse to prove (potential misbehavior).
    StorageContentNotFoundMsg {
        /// The content-not-found response
        response: StorageContentNotFound,
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
            GossipMessage::StorageChallengeMsg { .. } => "StorageChallengeMsg",
            GossipMessage::StorageProofMsg { .. } => "StorageProofMsg",
            GossipMessage::StorageContentNotFoundMsg { .. } => "StorageContentNotFoundMsg",
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
    pub fn can_publish(&self, did: &Did, trust_score: Option<f64>) -> bool {
        match &self.acl {
            AccessControl::Public => true,
            AccessControl::MinTrustScore(min_score) => {
                if let Some(actual_score) = trust_score {
                    actual_score >= *min_score
                } else {
                    false
                }
            }
            AccessControl::Participants(allowed) => allowed.contains(did),
        }
    }

    /// Check if a DID can subscribe to this topic
    pub fn can_subscribe(&self, did: &Did, trust_score: Option<f64>) -> bool {
        // For now, same rules as publish
        // Could be more permissive in the future
        self.can_publish(did, trust_score)
    }
}

/// Access control for topic
#[derive(Debug, Clone, PartialEq)]
pub enum AccessControl {
    /// Public access
    Public,
    /// Minimum trust score required (0.0 - 1.0)
    MinTrustScore(f64),
    /// Allow list of DIDs
    Participants(Vec<Did>),
}

impl Default for AccessControl {
    fn default() -> Self {
        // Default to high trust requirement (Federated level)
        AccessControl::MinTrustScore(0.7)
    }
}

/// Policy for handling undeclared topics during publish operations
///
/// Controls what happens when a peer attempts to publish to a topic
/// that hasn't been explicitly registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TopicAutoCreationPolicy {
    /// Reject publishes to undeclared topics (most secure)
    #[default]
    Reject,

    /// Auto-create with strict default access control (MinTrustScore(0.7))
    /// Logs a warning when this happens
    CreateWithStrictDefaults,

    /// Auto-create with public access (legacy behavior, not recommended)
    /// Logs a warning when this happens
    CreatePublic,
}

/// Resource limits for a specific trust score
#[derive(Debug, Clone)]
pub struct ResourceLimits {
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

    /// Maximum subscriptions per peer
    pub max_subscriptions: u32,
}

impl ResourceLimits {
    /// Get resource limits from policy constraints.
    ///
    /// This follows the "Meaning Firewall" principle: the kernel blindly enforces
    /// limits provided by the PolicyOracle without understanding trust semantics.
    pub fn from_constraints(constraints: &icn_kernel_api::authz::ConstraintSet) -> Self {
        Self {
            // Using max_message_size for both pull and push for now, casting u64 to u32
            max_pull_bytes: constraints
                .max_message_size
                .map(|s| s as u32)
                .unwrap_or(64 * 1024_u32),
            max_push_bytes: constraints
                .max_message_size
                .map(|s| s as u32)
                .unwrap_or(64 * 1024_u32),
            max_outstanding_reqs: constraints.max_outstanding_requests.unwrap_or(1),
            // Time-based limits (using defaults if not in ConstraintSet)
            retry_min_ms: 1500,
            retry_max_ms: 5000,
            max_subscriptions: constraints.max_subscriptions.unwrap_or(10),
        }
    }

    /// [DEPRECATED] Get resource limits for a specific trust score.
    /// Use `from_constraints` instead to avoid hardcoding trust thresholds in the kernel.
    pub fn for_trust_score(score: f64) -> Self {
        if score >= 0.7 {
            // Federated
            Self {
                max_pull_bytes: 1024 * 1024, // 1 MB
                max_push_bytes: 1024 * 1024,
                max_outstanding_reqs: 3,
                retry_min_ms: 300,
                retry_max_ms: 1200,
                max_subscriptions: 1000_u32,
            }
        } else if score >= 0.4 {
            // Partner
            Self {
                max_pull_bytes: 1024 * 1024, // 1 MB
                max_push_bytes: 1024 * 1024,
                max_outstanding_reqs: 3,
                retry_min_ms: 300,
                retry_max_ms: 1200,
                max_subscriptions: 500_u32,
            }
        } else if score >= 0.1 {
            // Known
            Self {
                max_pull_bytes: 256 * 1024, // 256 KB
                max_push_bytes: 256 * 1024,
                max_outstanding_reqs: 2,
                retry_min_ms: 800,
                retry_max_ms: 2500,
                max_subscriptions: 100_u32,
            }
        } else {
            // Isolated
            Self {
                max_pull_bytes: 64 * 1024, // 64 KB
                max_push_bytes: 64 * 1024,
                max_outstanding_reqs: 1,
                retry_min_ms: 1500,
                retry_max_ms: 5000,
                max_subscriptions: 10_u32,
            }
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        // Default to "Isolated" class limits
        Self {
            max_pull_bytes: 64 * 1024,
            max_push_bytes: 64 * 1024,
            max_outstanding_reqs: 1,
            retry_min_ms: 1500,
            retry_max_ms: 5000,
            max_subscriptions: 10_u32,
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

/// Configuration for adaptive gossip fanout (#484)
///
/// Adjusts gossip fanout dynamically based on network size using the formula:
/// `fanout = clamp(ceil(log2(network_size) + 1), min, max)`
///
/// This prevents:
/// - Over-messaging in small networks (wastes bandwidth)
/// - Under-propagation in large networks (slow convergence)
#[derive(Debug, Clone)]
pub struct AdaptiveFanoutConfig {
    /// Minimum fanout (for small networks or floor)
    pub min_fanout: usize,

    /// Maximum fanout (prevents flooding)
    pub max_fanout: usize,

    /// Multiplier per scope (Local=1.5, Regional=1.0, Global=0.75)
    /// Higher multiplier for local scope where latency matters more
    pub local_multiplier: f64,
    pub regional_multiplier: f64,
    pub global_multiplier: f64,

    /// Minimum peers before adaptive fanout kicks in
    /// Below this threshold, use min_fanout
    pub min_peers_for_adaptive: usize,
}

impl Default for AdaptiveFanoutConfig {
    fn default() -> Self {
        Self {
            min_fanout: 3,
            max_fanout: 10,
            local_multiplier: 1.5,
            regional_multiplier: 1.0,
            global_multiplier: 0.75,
            min_peers_for_adaptive: 5,
        }
    }
}

impl AdaptiveFanoutConfig {
    /// Calculate adaptive fanout based on network size and scope
    ///
    /// Formula: fanout = clamp(ceil(log2(network_size) + 1) * scope_multiplier, min, max)
    pub fn calculate_fanout(&self, network_size: usize, scope: &Scope) -> usize {
        // Below threshold, use minimum
        if network_size < self.min_peers_for_adaptive {
            return self.min_fanout;
        }

        // Base fanout: ceil(log2(n) + 1)
        // This gives: 4->3, 8->4, 16->5, 32->6, 64->7, 128->8, etc.
        let log2_size = (network_size as f64).log2();
        let base_fanout = (log2_size + 1.0).ceil();

        // Apply scope multiplier
        let multiplier = match scope {
            Scope::LocalCluster => self.local_multiplier,
            Scope::Regional => self.regional_multiplier,
            Scope::Global => self.global_multiplier,
        };

        let adjusted = (base_fanout * multiplier).ceil() as usize;

        // Clamp to bounds
        adjusted.clamp(self.min_fanout, self.max_fanout)
    }
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
    fn test_topic_trust_score_access() {
        let topic = Topic::new(
            "test".to_string(),
            AccessControl::MinTrustScore(0.4), // Partner level
        );
        let did = KeyPair::generate().unwrap().did().clone();

        // No trust score - denied
        assert!(!topic.can_publish(&did, None));

        // Low score - denied (0.1 < 0.4)
        assert!(!topic.can_publish(&did, Some(0.1)));

        // Exact score - allowed
        assert!(topic.can_publish(&did, Some(0.4)));

        // High score - allowed (0.8 > 0.4)
        assert!(topic.can_publish(&did, Some(0.8)));
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

    // Issue #473: Test strict access control defaults

    #[test]
    fn test_access_control_default_is_strict() {
        // Verify default is MinTrustScore(0.7) - Federated level
        let default_acl = AccessControl::default();
        assert_eq!(
            default_acl,
            AccessControl::MinTrustScore(0.7),
            "Default AccessControl should be MinTrustScore(0.7) for security"
        );
    }

    #[test]
    fn test_access_control_default_requires_high_trust() {
        let topic = Topic::new("test".to_string(), AccessControl::default());
        let did = KeyPair::generate().unwrap().did().clone();

        // No trust - denied
        assert!(
            !topic.can_publish(&did, None),
            "Default ACL should deny peers with no trust"
        );

        // Low trust - denied
        assert!(
            !topic.can_publish(&did, Some(0.0)),
            "Default ACL should deny low trust"
        );

        // Known - denied (0.1 < 0.7)
        assert!(
            !topic.can_publish(&did, Some(0.1)),
            "Default ACL should deny Known trust (0.1)"
        );

        // Partner - denied (0.4 < 0.7)
        assert!(
            !topic.can_publish(&did, Some(0.4)),
            "Default ACL should deny Partner trust (0.4)"
        );

        // Federated - allowed (0.7 >= 0.7)
        assert!(
            topic.can_publish(&did, Some(0.7)),
            "Default ACL should allow Federated trust (0.7)"
        );

        // Higher - allowed
        assert!(
            topic.can_publish(&did, Some(0.9)),
            "Default ACL should allow higher trust"
        );
    }

    #[test]
    fn test_topic_auto_creation_policy_default() {
        // Default policy should be Reject (most secure)
        let policy = TopicAutoCreationPolicy::default();
        assert_eq!(
            policy,
            TopicAutoCreationPolicy::Reject,
            "Default policy should be Reject for security"
        );
    }

    #[test]
    fn test_topic_auto_creation_policy_variants() {
        // Verify all variants exist and are distinguishable
        let reject = TopicAutoCreationPolicy::Reject;
        let strict = TopicAutoCreationPolicy::CreateWithStrictDefaults;
        let public = TopicAutoCreationPolicy::CreatePublic;

        assert_ne!(reject, strict);
        assert_ne!(strict, public);
        assert_ne!(reject, public);
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

    // Adaptive fanout tests (#484)

    #[test]
    fn test_adaptive_fanout_default_config() {
        let config = AdaptiveFanoutConfig::default();
        assert_eq!(config.min_fanout, 3);
        assert_eq!(config.max_fanout, 10);
        assert_eq!(config.min_peers_for_adaptive, 5);
    }

    #[test]
    fn test_adaptive_fanout_small_network() {
        let config = AdaptiveFanoutConfig::default();

        // Below threshold, use min_fanout
        assert_eq!(config.calculate_fanout(1, &Scope::Global), 3);
        assert_eq!(config.calculate_fanout(4, &Scope::Global), 3);
    }

    #[test]
    fn test_adaptive_fanout_scales_with_network_size() {
        let config = AdaptiveFanoutConfig::default();

        // Using Regional (multiplier=1.0) for cleaner math
        // log2(8) + 1 = 4
        assert_eq!(config.calculate_fanout(8, &Scope::Regional), 4);

        // log2(16) + 1 = 5
        assert_eq!(config.calculate_fanout(16, &Scope::Regional), 5);

        // log2(32) + 1 = 6
        assert_eq!(config.calculate_fanout(32, &Scope::Regional), 6);

        // log2(64) + 1 = 7
        assert_eq!(config.calculate_fanout(64, &Scope::Regional), 7);

        // log2(128) + 1 = 8
        assert_eq!(config.calculate_fanout(128, &Scope::Regional), 8);
    }

    #[test]
    fn test_adaptive_fanout_scope_multipliers() {
        let config = AdaptiveFanoutConfig::default();

        // 32 peers: base = log2(32) + 1 = 6
        // Local: 6 * 1.5 = 9
        // Regional: 6 * 1.0 = 6
        // Global: 6 * 0.75 = 4.5 -> 5
        assert_eq!(config.calculate_fanout(32, &Scope::LocalCluster), 9);
        assert_eq!(config.calculate_fanout(32, &Scope::Regional), 6);
        assert_eq!(config.calculate_fanout(32, &Scope::Global), 5);
    }

    #[test]
    fn test_adaptive_fanout_respects_max() {
        let config = AdaptiveFanoutConfig::default();

        // Very large network: log2(10000) + 1 = 14.3
        // With local multiplier 1.5 = 21, but capped at max_fanout=10
        assert_eq!(config.calculate_fanout(10000, &Scope::LocalCluster), 10);
    }

    #[test]
    fn test_adaptive_fanout_custom_config() {
        let config = AdaptiveFanoutConfig {
            min_fanout: 2,
            max_fanout: 5,
            local_multiplier: 1.0,
            regional_multiplier: 1.0,
            global_multiplier: 1.0,
            min_peers_for_adaptive: 3,
        };

        // Below threshold
        assert_eq!(config.calculate_fanout(2, &Scope::Global), 2);

        // At threshold: log2(3) + 1 ≈ 2.58 -> 3
        assert_eq!(config.calculate_fanout(3, &Scope::Global), 3);

        // Large network capped at 5
        assert_eq!(config.calculate_fanout(1000, &Scope::Global), 5);
    }
}
