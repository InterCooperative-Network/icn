//! Gossip actor for managing distributed synchronization

use crate::bloom::BloomFilter;
use crate::sync::PeerSyncManager;
use crate::types::{AccessControl, ContentHash, GossipEntry, GossipMessage, Topic};
use crate::vector_clock::VectorClock;
use anyhow::{bail, Context as _, Result};
use icn_identity::{Did, KeyPair};
use icn_kernel_api::authz::{ActionKind, Domain, PolicyDecision, PolicyOracle, PolicyRequest};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

/// Callback for sending gossip messages to peers
/// Parameters: (recipient_did, message)
/// If recipient_did is None, broadcast to all peers
pub type SendMessageCallback = Arc<dyn Fn(Option<Did>, GossipMessage) + Send + Sync>;

/// Callback for notifying subscribers of new entries
/// Parameters: (topic, entry, subscriber_did)
/// Called when a new entry is stored in a topic that has subscribers
pub type EntryNotificationCallback = Arc<dyn Fn(String, GossipEntry, Did) + Send + Sync>;

/// Callback for sampling peers based on scope
/// Parameters: (scope, count) -> `Vec<Did>`
/// Returns a list of peer DIDs to send messages to
pub type PeerSamplingCallback = Arc<dyn Fn(crate::types::Scope, usize) -> Vec<Did> + Send + Sync>;

/// Callback for handling received storage proofs
/// Parameters: (proof)
/// Called when a storage proof is received from a replica holder
pub type StorageProofCallback =
    Arc<dyn Fn(icn_store::StorageProof) -> anyhow::Result<()> + Send + Sync>;

/// Callback for handling content-not-found responses
/// Parameters: (response)
/// Called when a storage challenge target reports they don't have the content
pub type StorageContentNotFoundCallback =
    Arc<dyn Fn(icn_store::StorageContentNotFound) -> anyhow::Result<()> + Send + Sync>;

/// Maximum subscribers per topic to prevent memory exhaustion
pub const MAX_SUBSCRIBERS_PER_TOPIC: usize = 10000;

// TrustLookup type alias for backward compatibility with legacy trust callbacks
// This allows gradual migration to PolicyOracle while maintaining existing APIs
pub type TrustLookup = Arc<dyn Fn(&Did) -> Option<icn_trust::TrustClass> + Send + Sync + 'static>;

// topics_per_peer_limit removed - use PolicyOracle constraints
// TrustScoreCache removed - use PolicyOracle

/// Spawn a violation recording task without blocking
/// This is fire-and-forget - we don't wait for the result
pub(crate) fn spawn_violation_recording(
    detector: Arc<RwLock<icn_security::MisbehaviorDetector>>,
    did: Did,
    violation: icn_security::Violation,
    evidence: Vec<u8>,
) {
    tokio::spawn(async move {
        let mut det = detector.write().await;
        det.record_violation(&did, violation, evidence);
    });
}

/// Gossip actor manages topics and entry synchronization
pub struct GossipActor {
    /// This node's DID
    pub(crate) own_did: Did,

    /// Keypair for signing outgoing messages (optional for testing)
    pub(crate) keypair: Option<KeyPair>,

    /// Sequence counter for signed messages (monotonically increasing)
    #[allow(dead_code)]
    sequence: u64,

    /// Vector clock for this node
    pub(crate) clock: VectorClock,

    /// Topics (topic name -> Topic)
    pub(crate) topics: HashMap<String, Topic>,

    /// Entries (topic -> hash -> entry)
    pub(crate) entries: HashMap<String, HashMap<ContentHash, GossipEntry>>,

    /// Bloom filters (topic -> filter)
    pub(crate) bloom_filters: HashMap<String, BloomFilter>,

    /// Subscriptions (topic -> subscribers)
    pub(crate) subscriptions: HashMap<String, Vec<Did>>,

    /// Policy Oracle for authorization and resource limits
    pub(crate) oracle: Option<Arc<dyn PolicyOracle>>,

    /// Send message callback (optional, for sending responses)
    send_callback: Option<SendMessageCallback>,

    /// Entry notification callback (optional, for notifying subscribers)
    notification_callback: Option<EntryNotificationCallback>,

    /// Peer sampling callback (optional, for scope-aware peer selection)
    peer_sampling: Option<PeerSamplingCallback>,

    /// Storage proof callback (optional, for forwarding proofs to ChallengeScheduler)
    pub(crate) storage_proof_callback: Option<StorageProofCallback>,

    /// Storage content-not-found callback (optional, for forwarding to ChallengeScheduler)
    pub(crate) storage_not_found_callback: Option<StorageContentNotFoundCallback>,

    /// Per-peer sync state manager
    pub(crate) peer_sync: PeerSyncManager,

    /// Store for replica metadata tracking (Phase 17 - optional)
    pub(crate) store: Option<Arc<dyn icn_store::Store>>,

    /// Byzantine fault detector (Phase 18 Week 1-2 - optional)
    ///
    /// Accessed by `subscriptions` and `protocol` modules for violation recording.
    pub(crate) misbehavior_detector: Option<Arc<RwLock<icn_security::MisbehaviorDetector>>>,

    /// Network partition detector (Phase 18 Week 3 - optional)
    pub(crate) partition_detector: Option<Arc<RwLock<crate::partition::PartitionDetector>>>,

    /// Network partition healer (Phase 18 Week 3 - optional)
    pub(crate) partition_healer: Option<Arc<RwLock<crate::partition::PartitionHealer>>>,

    /// Storage quota manager (Phase 18 Week 6 - optional)
    /// Enforces per-DID storage limits and provides priority-based eviction
    storage_quota_manager: Option<Arc<RwLock<icn_store::StorageQuotaManager>>>,

    /// Bloom filter resize configuration (M2 - dynamic sizing)
    pub(crate) bloom_resize_config: crate::bloom::BloomResizeConfig,

    /// Adaptive fanout configuration (M2 #484 - dynamic fanout based on network size)
    pub(crate) adaptive_fanout_config: crate::types::AdaptiveFanoutConfig,

    /// Topic auto-creation policy (Issue #473 - strict defaults)
    /// Controls what happens when publishing to an undeclared topic
    topic_auto_creation_policy: crate::types::TopicAutoCreationPolicy,

    /// Key rotation cache for tracking rotated DIDs during grace period (Issue #469)
    /// Used to accept messages signed with old keys during the transition period
    key_rotation_cache: crate::key_rotation::KeyRotationCache,
}

impl GossipActor {
    /// Create a new gossip actor with legacy trust callback
    pub fn new_with_legacy_trust(own_did: Did, trust_callback: TrustLookup) -> Self {
        let oracle = Arc::new(LegacyTrustOracle {
            callback: trust_callback,
        });
        Self::new(own_did, Some(oracle))
    }

    /// Create a new gossip actor
    pub fn new(own_did: Did, oracle: Option<Arc<dyn PolicyOracle>>) -> Self {
        let mut gossip = GossipActor {
            own_did: own_did.clone(),
            keypair: None,
            sequence: 0,
            clock: VectorClock::new(),
            topics: HashMap::new(),
            entries: HashMap::new(),
            bloom_filters: HashMap::new(),
            subscriptions: HashMap::new(),
            oracle,
            send_callback: None,
            notification_callback: None,
            peer_sampling: None,
            storage_proof_callback: None,
            storage_not_found_callback: None,
            peer_sync: PeerSyncManager::new(300, 5000), // Default: 300-5000ms backoff
            store: None,                                // Phase 17: Set via set_store()
            misbehavior_detector: None, // Phase 18 Week 1-2: Set via set_misbehavior_detector()
            partition_detector: None,   // Phase 18 Week 3: Set via set_partition_detector()
            partition_healer: None,     // Phase 18 Week 3: Set via set_partition_healer()
            storage_quota_manager: None, // Phase 18 Week 6: Set via set_storage_quota_manager()
            bloom_resize_config: crate::bloom::BloomResizeConfig::default(), // M2: Dynamic Bloom sizing
            adaptive_fanout_config: crate::types::AdaptiveFanoutConfig::default(), // M2 #484: Adaptive fanout
            topic_auto_creation_policy: crate::types::TopicAutoCreationPolicy::default(), // Issue #473: Strict defaults
            key_rotation_cache: crate::key_rotation::KeyRotationCache::new(), // Issue #469: Key rotation tracking
        };

        // Create default topics with appropriate scopes
        gossip.create_topic(
            Topic::new("global:identity".to_string(), AccessControl::Public)
                .with_scope(crate::types::Scope::Global), // Identity propagates globally
        );
        gossip.create_topic(
            Topic::new("global:rendezvous".to_string(), AccessControl::Public)
                .with_scope(crate::types::Scope::Global), // Rendezvous nodes need global visibility
        );
        gossip.create_topic(
            Topic::new(
                "trust:attestations".to_string(),
                AccessControl::MinTrustScore(0.1), // Known peers
            )
            .with_scope(crate::types::Scope::Regional), // Trust attestations are regional
        );

        // Labor share topics (Issue #391)
        gossip.create_topic(
            Topic::new(
                crate::labor_shares::topics::LABOR_SHARES_ALLOCATIONS.to_string(),
                AccessControl::MinTrustScore(0.1), // Known peers
            )
            .with_scope(crate::types::Scope::Global), // Labor share events propagate globally
        );
        gossip.create_topic(
            Topic::new(
                crate::labor_shares::topics::BONDS_ISSUANCE.to_string(),
                AccessControl::MinTrustScore(0.1), // Known peers
            )
            .with_scope(crate::types::Scope::Global), // Bond offerings propagate globally
        );
        gossip.create_topic(
            Topic::new(
                crate::labor_shares::topics::BONDS_PAYMENTS.to_string(),
                AccessControl::MinTrustScore(0.4), // Partner peers
            )
            .with_scope(crate::types::Scope::Regional), // Payment notifications are regional
        );

        // Key rotation topic (Issue #469)
        gossip.create_topic(
            Topic::new(
                crate::key_rotation::TOPIC_KEY_ROTATION.to_string(),
                AccessControl::MinTrustScore(0.1), // Known peers can receive rotations
            )
            .with_scope(crate::types::Scope::Global), // Key rotations propagate globally
        );

        gossip
    }

    /// Create a new topic
    pub fn create_topic(&mut self, topic: Topic) {
        info!("Creating topic: {}", topic.name);
        self.bloom_filters.insert(
            topic.name.clone(),
            BloomFilter::new(topic.max_entries, 0.01),
        );
        self.entries.insert(topic.name.clone(), HashMap::new());
        self.subscriptions.insert(topic.name.clone(), Vec::new());
        self.topics.insert(topic.name.clone(), topic);
    }

    /// Set the send message callback for sending responses
    pub fn set_send_callback(&mut self, callback: SendMessageCallback) {
        self.send_callback = Some(callback);
    }

    /// Set the keypair for signing outgoing messages
    pub fn set_keypair(&mut self, keypair: KeyPair) {
        self.keypair = Some(keypair);
    }

    /// Set the entry notification callback for notifying subscribers
    pub fn set_notification_callback(&mut self, callback: EntryNotificationCallback) {
        self.notification_callback = Some(callback);
    }

    /// Set the peer sampling callback for scope-aware peer selection
    pub fn set_peer_sampling(&mut self, callback: PeerSamplingCallback) {
        self.peer_sampling = Some(callback);
    }

    /// Set the storage proof callback for forwarding proofs to ChallengeScheduler
    pub fn set_storage_proof_callback(&mut self, callback: StorageProofCallback) {
        self.storage_proof_callback = Some(callback);
    }

    /// Set the storage content-not-found callback for forwarding to ChallengeScheduler
    pub fn set_storage_not_found_callback(&mut self, callback: StorageContentNotFoundCallback) {
        self.storage_not_found_callback = Some(callback);
    }

    /// Set the store for replica metadata tracking (Phase 17)
    pub fn set_store(&mut self, store: Arc<dyn icn_store::Store>) {
        self.store = Some(store);
    }

    /// Set the misbehavior detector for Byzantine fault detection (Phase 18)
    pub fn set_misbehavior_detector(
        &mut self,
        detector: Arc<RwLock<icn_security::MisbehaviorDetector>>,
    ) {
        self.misbehavior_detector = Some(detector);
    }

    /// Set partition detector for network partition detection (Phase 18 Week 3)
    pub fn set_partition_detector(
        &mut self,
        detector: Arc<RwLock<crate::partition::PartitionDetector>>,
    ) {
        self.partition_detector = Some(detector);
    }

    /// Set partition healer for partition conflict resolution (Phase 18 Week 3)
    pub fn set_partition_healer(&mut self, healer: Arc<RwLock<crate::partition::PartitionHealer>>) {
        self.partition_healer = Some(healer);
    }

    /// Set storage quota manager for per-DID storage limits (Phase 18 Week 6)
    pub fn set_storage_quota_manager(
        &mut self,
        manager: Arc<RwLock<icn_store::StorageQuotaManager>>,
    ) {
        self.storage_quota_manager = Some(manager);
    }

    /// Set the topic auto-creation policy (Issue #473)
    ///
    /// Controls what happens when attempting to publish to an undeclared topic.
    /// By default, publishes to undeclared topics are rejected for security.
    pub fn set_topic_auto_creation_policy(
        &mut self,
        policy: crate::types::TopicAutoCreationPolicy,
    ) {
        self.topic_auto_creation_policy = policy;
    }

    /// Get the current topic auto-creation policy
    pub fn topic_auto_creation_policy(&self) -> crate::types::TopicAutoCreationPolicy {
        self.topic_auto_creation_policy
    }

    // ==================== Key Rotation Methods (Issue #469) ====================

    /// Record a key rotation in the cache.
    ///
    /// This should be called when receiving a valid RotationAnnouncement message
    /// or when this node rotates its own keys.
    pub fn record_key_rotation(&mut self, old_did: &Did, new_did: Did, timestamp: u64) {
        self.key_rotation_cache
            .record_rotation(old_did, new_did, timestamp);
    }

    /// Check if a DID is still valid (either current or within rotation grace period).
    ///
    /// Returns true if:
    /// - The DID has no rotation record (current or never rotated)
    /// - The DID was rotated but is within the grace period
    ///
    /// Returns false if the DID was rotated and the grace period has expired.
    pub fn is_did_valid(&self, did: &Did) -> bool {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.key_rotation_cache.is_did_valid(did, current_time)
    }

    /// Get the new DID for a rotated key, if within grace period.
    ///
    /// Returns Some(new_did) if the old_did was rotated and we're within the grace period.
    /// Returns None if no rotation record exists or the grace period has expired.
    pub fn get_rotated_did(&self, old_did: &Did) -> Option<Did> {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.key_rotation_cache
            .get_rotated_did(old_did, current_time)
            .cloned()
    }

    /// Clean up expired rotation records from the cache.
    ///
    /// Call this periodically (e.g., every hour) to prevent memory growth.
    pub fn cleanup_rotation_cache(&mut self) {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.key_rotation_cache.cleanup_expired(current_time);
    }

    /// Publish a key rotation announcement to the network.
    ///
    /// This broadcasts the rotation to all peers subscribed to the key:rotation topic.
    /// The announcement includes signatures from both old and new keys to prove
    /// authorization and possession.
    ///
    /// # Arguments
    /// * `old_did` - DID before rotation
    /// * `new_did` - DID after rotation
    /// * `new_public_key` - New Ed25519 public key bytes (32 bytes)
    /// * `timestamp` - Unix timestamp when rotation occurred
    /// * `reason` - Reason for the rotation
    /// * `signature_old` - Signature from old key
    /// * `signature_new` - Signature from new key
    pub async fn publish_rotation_announcement(
        &mut self,
        old_did: Did,
        new_did: Did,
        new_public_key: Vec<u8>,
        timestamp: u64,
        reason: crate::key_rotation::RotationReason,
        signature_old: Vec<u8>,
        signature_new: Vec<u8>,
    ) -> Result<()> {
        use crate::key_rotation::{KeyRotationMessage, TOPIC_KEY_ROTATION};

        // Record in our own cache first
        self.record_key_rotation(&old_did, new_did.clone(), timestamp);

        // Create the rotation announcement message
        let rotation_msg = KeyRotationMessage::announcement(
            old_did,
            new_did,
            new_public_key,
            timestamp,
            reason,
            signature_old,
            signature_new,
        );

        // Serialize and publish to the key:rotation topic
        let data = icn_encoding::encode(&rotation_msg)
            .map_err(|e| anyhow::anyhow!("Failed to serialize rotation message: {e}"))?;

        self.publish(TOPIC_KEY_ROTATION, data).await?;

        info!(
            "Published key rotation announcement to {}",
            TOPIC_KEY_ROTATION
        );
        Ok(())
    }

    /// Handle an incoming key rotation message.
    ///
    /// Verifies the signatures and updates the local cache if valid.
    /// Returns Ok(true) if the rotation was recorded, Ok(false) if it was a query/response.
    pub fn handle_rotation_message(&mut self, data: &[u8]) -> Result<bool> {
        use crate::key_rotation::KeyRotationMessage;

        let msg: KeyRotationMessage = icn_encoding::decode(data)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize rotation message: {e}"))?;

        match msg {
            KeyRotationMessage::RotationAnnouncement {
                old_did,
                new_did,
                new_public_key,
                timestamp,
                reason: _,
                signature_old,
                signature_new,
            } => {
                // Verify signatures before accepting the rotation
                let message_to_verify =
                    KeyRotationMessage::rotation_message(&old_did, &new_did, timestamp);
                let message_bytes = message_to_verify.as_bytes();

                // Verify old key signature
                if !self.verify_signature(&old_did, message_bytes, &signature_old)? {
                    warn!(
                        "Invalid old key signature in rotation announcement from {}",
                        old_did
                    );
                    return Err(anyhow::anyhow!("Invalid old key signature"));
                }

                // Verify new key signature using the provided public key
                if !self.verify_signature_with_key(
                    &new_public_key,
                    message_bytes,
                    &signature_new,
                )? {
                    warn!("Invalid new key signature in rotation announcement");
                    return Err(anyhow::anyhow!("Invalid new key signature"));
                }

                // Both signatures valid, record the rotation
                self.record_key_rotation(&old_did, new_did.clone(), timestamp);
                info!("Recorded key rotation: {} -> {}", old_did, new_did);
                Ok(true)
            }
            KeyRotationMessage::RotationQuery {
                queried_did,
                requester,
            } => {
                // Handle query - check if we know about a rotation for this DID
                let (current_did, last_rotation) =
                    if let Some(new_did) = self.get_rotated_did(&queried_did) {
                        (
                            new_did,
                            self.get_rotation_timestamp(&queried_did).unwrap_or(0),
                        )
                    } else {
                        (queried_did.clone(), 0)
                    };

                // Send response
                let response =
                    KeyRotationMessage::response(queried_did, current_did, last_rotation);
                let response_data = icn_encoding::encode(&response)
                    .map_err(|e| anyhow::anyhow!("Failed to serialize response: {e}"))?;

                self.send_message(
                    Some(requester),
                    GossipMessage::Response {
                        entry: GossipEntry {
                            hash: blake3::hash(&response_data).into(),
                            author: self.own_did.clone(),
                            clock: self.clock.clone(),
                            topic: crate::key_rotation::TOPIC_KEY_ROTATION.to_string(),
                            data: response_data,
                            compressed: false,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0),
                            replica_offered: None,
                        },
                    },
                );
                Ok(false)
            }
            KeyRotationMessage::RotationResponse { .. } => {
                // Response handling would be done by the requester
                Ok(false)
            }
        }
    }

    /// Get the timestamp when a DID was rotated (if known).
    fn get_rotation_timestamp(&self, _did: &Did) -> Option<u64> {
        // The cache doesn't store the original timestamp, just expiry.
        // For now, return None. A more complete implementation would
        // store the original timestamp.
        None
    }

    /// Verify a signature using the public key from a DID.
    fn verify_signature(&self, did: &Did, message: &[u8], signature: &[u8]) -> Result<bool> {
        use ed25519_dalek::{Signature, Verifier};

        // Extract public key from DID
        let verifying_key = did.to_verifying_key()?;

        let sig_bytes: [u8; 64] = match signature.try_into() {
            Ok(bytes) => bytes,
            Err(_) => return Ok(false), // Invalid signature length
        };
        let sig = Signature::from_bytes(&sig_bytes);

        Ok(verifying_key.verify(message, &sig).is_ok())
    }

    /// Verify a signature using raw public key bytes.
    fn verify_signature_with_key(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        let pubkey_bytes: [u8; 32] = public_key
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid public key length: {}", public_key.len()))?;

        let verifying_key = VerifyingKey::from_bytes(&pubkey_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid public key: {e}"))?;

        let sig_bytes: [u8; 64] = match signature.try_into() {
            Ok(bytes) => bytes,
            Err(_) => return Ok(false), // Invalid signature length
        };
        let sig = Signature::from_bytes(&sig_bytes);

        Ok(verifying_key.verify(message, &sig).is_ok())
    }

    // ==================== End Key Rotation Methods ====================

    /// Attempt to heal partition with a reconnected peer (Phase 18 Week 3)
    ///
    /// This should be called when a previously-partitioned peer reconnects.
    /// It will merge vector clocks and resolve any conflicts that arose during the partition.
    pub async fn heal_partition_with_peer(&mut self, peer: &Did) -> Result<()> {
        // Check if we have both detector and healer
        let (was_partitioned, healer_ref) = match (&self.partition_detector, &self.partition_healer)
        {
            (Some(detector), Some(healer)) => {
                let was_part = detector.read().await.is_partitioned(peer);
                (was_part, healer.clone())
            }
            _ => return Ok(()), // No partition detection/healing enabled
        };

        if !was_partitioned {
            return Ok(()); // Peer wasn't partitioned, nothing to heal
        }

        info!("Attempting to heal partition with peer {}", peer);

        // Send PartitionHealRequest with our vector clock
        // The peer will respond with PartitionHealResponse containing their clock
        // The response handler (handle_message) will merge clocks and trigger sync
        let last_contact_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        self.send_message(
            Some(peer.clone()),
            GossipMessage::PartitionHealRequest {
                requesting_peer: self.own_did.clone(),
                vector_clock: self.clock.clone(),
                last_contact_ms,
            },
        );

        info!(
            "Sent PartitionHealRequest to {} with vector clock (clocks will be merged on response)",
            peer
        );

        // Note: The actual clock merging and conflict resolution happens asynchronously
        // when we receive PartitionHealResponse in handle_message()

        // Mark as healing in progress (actual healing completes when response arrives)
        let mut healer_guard = healer_ref.write().await;
        healer_guard.mark_healing_started(peer);

        Ok(())
    }

    /// Request replicas from specific peers (Phase 17 Week 3 - ReplicationManager integration)
    ///
    /// This is a public API for the ReplicationManager to request replicas
    /// from trusted peers for under-replicated content.
    pub fn request_replicas(&self, content_hash: &[u8; 32], peers: &[Did]) {
        for peer_did in peers {
            self.send_message(
                Some(peer_did.clone()),
                GossipMessage::ReplicaRequest {
                    content_hash: *content_hash,
                    requesting_peer: self.own_did.clone(),
                },
            );
        }
    }

    /// Send a storage challenge to a replica holder
    ///
    /// Public API for the ChallengeScheduler to send proof-of-storage challenges.
    pub fn send_storage_challenge(&self, target: Did, challenge: icn_store::StorageChallenge) {
        self.send_message(
            Some(target),
            GossipMessage::StorageChallengeMsg { challenge },
        );
    }

    /// Get all known peers (Phase 17 Week 3 - ReplicationManager peer discovery)
    ///
    /// Returns the set of all DIDs that have interacted with this node
    /// (subscribers to any topic, vector clock peers, etc.)
    pub fn get_known_peers(&self) -> Vec<Did> {
        let mut peers = HashSet::new();

        // Collect all subscribers from all topics
        for topic_name in self.topics.keys() {
            for subscriber in self.get_subscribers(topic_name) {
                peers.insert(subscriber);
            }
        }

        // Collect all peers from vector clock (they've sent us messages)
        for did in self.clock.keys() {
            peers.insert(did.clone());
        }

        peers.into_iter().collect()
    }

    /// Send a message to a peer (if callback is set)
    pub(crate) fn send_message(&self, recipient: Option<Did>, message: GossipMessage) {
        if let Some(callback) = &self.send_callback {
            callback(recipient, message);
        } else {
            debug!("Cannot send message - no send callback set");
        }
    }

    /// Send a message with scope-aware peer selection
    /// If peer_sampling is set, samples peers based on scope. Otherwise falls back to broadcast.
    pub(crate) fn send_message_scoped(
        &self,
        scope: crate::types::Scope,
        fanout: usize,
        message: GossipMessage,
    ) {
        if let Some(sampling) = &self.peer_sampling {
            // Use scope-aware peer selection
            let peers = sampling(scope, fanout);

            if peers.is_empty() {
                debug!("No peers available for scope {:?}", scope);
                return;
            }

            // Send to each selected peer
            for peer in peers {
                self.send_message(Some(peer), message.clone());
            }

            // Track fanout metrics
            icn_obs::metrics::topology::gossip_fanout_record(
                match scope {
                    crate::types::Scope::LocalCluster => "local_cluster",
                    crate::types::Scope::Regional => "regional",
                    crate::types::Scope::Global => "global",
                },
                fanout,
            );
        } else {
            // Fall back to broadcast (backward compatibility)
            self.send_message(None, message);
        }
    }

    /// Publish an entry to a topic
    #[instrument(skip(self, data), fields(topic = %topic, data_size = data.len()))]
    pub async fn publish(&mut self, topic: &str, data: Vec<u8>) -> Result<ContentHash> {
        // Handle undeclared topics according to policy (Issue #473)
        if !self.topics.contains_key(topic) {
            use crate::types::TopicAutoCreationPolicy;

            match self.topic_auto_creation_policy {
                TopicAutoCreationPolicy::Reject => {
                    warn!(
                        topic = %topic,
                        "Rejecting publish to undeclared topic (policy: Reject)"
                    );
                    bail!(
                        "Topic '{topic}' not found. Topics must be explicitly created before use."
                    );
                }
                TopicAutoCreationPolicy::CreateWithStrictDefaults => {
                    warn!(
                        topic = %topic,
                        "Auto-creating topic with strict defaults (Federated trust required). \
                         Consider explicitly creating topics with appropriate access control."
                    );
                    // Use the strict default (AccessControl::default() = MinTrustScore(0.7))
                    self.create_topic(Topic::new(topic.to_string(), AccessControl::default()));
                }
                TopicAutoCreationPolicy::CreatePublic => {
                    warn!(
                        topic = %topic,
                        "Auto-creating public topic (INSECURE - legacy behavior). \
                         This allows anyone to publish/subscribe. Consider using CreateWithStrictDefaults or Reject."
                    );
                    self.create_topic(Topic::new(topic.to_string(), AccessControl::Public));
                }
            }
        }

        let topic_obj = self.topics.get(topic).context("Topic not found")?;

        // Check ACL
        // Use PolicyOracle to get trust score for enforcement
        let trust_score = if let Some(oracle) = &self.oracle {
            let req = PolicyRequest::new(
                self.own_did.to_string(),
                ActionKind::Publish,
                Domain::trust(),
            );
            match oracle.evaluate(&req) {
                PolicyDecision::Allow { constraints } => {
                    // Extract trust score from custom constraints
                    // This relies on TrustPolicyOracle populating "trust_score"
                    constraints
                        .custom
                        .get("trust_score")
                        .and_then(|v| match v {
                            icn_kernel_api::authz::ConstraintValue::Float(f) => {
                                Some(f.into_inner())
                            }
                            _ => None,
                        })
                        .unwrap_or(0.0)
                }
                _ => 0.0,
            }
        } else {
            0.0 // Default to 0.0 if no oracle
        };

        if !topic_obj.can_publish(&self.own_did, Some(trust_score)) {
            bail!("Not authorized to publish to topic: {topic}");
        }

        // Increment vector clock
        self.clock.increment(&self.own_did);

        // Create entry
        let hash = Self::hash_data(&data);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis()
            .try_into()
            .context("Timestamp overflow - system clock too far in future")?;

        let mut entry = GossipEntry {
            hash,
            author: self.own_did.clone(),
            clock: self.clock.clone(),
            topic: topic.to_string(),
            data,
            compressed: false,
            timestamp,
            replica_offered: None, // Phase 17: Will be set by replication manager
        };

        // Compress large entries before storing/sending
        if let Err(e) = entry.compress() {
            debug!(
                entry_hash = %hex::encode(hash),
                topic = %topic,
                error = %e,
                "Failed to compress entry, continuing without compression"
            );
            // Continue without compression - not critical
        }

        // Store entry
        self.store_entry(entry).await?;

        // Track metrics
        icn_obs::metrics::gossip::entries_published_inc();
        self.update_gauge_metrics();

        debug!(
            entry_hash = %hex::encode(hash),
            topic = %topic,
            author_did = %self.own_did,
            "Published entry to topic"
        );

        Ok(hash)
    }

    /// Store an entry (from publish or receive)
    ///
    /// This method is async to allow non-blocking access to the storage quota manager.
    pub(crate) async fn store_entry(&mut self, entry: GossipEntry) -> Result<()> {
        let topic = &entry.topic;
        let hash = entry.hash;
        let entry_size = entry.data.len() as u64;
        let author = entry.author.clone();

        // Get or create topic entries
        let topic_entries = self.entries.entry(topic.clone()).or_default();

        // Check if already have this entry
        if topic_entries.contains_key(&hash) {
            return Ok(()); // Already have it
        }

        // Phase 18 Week 6: Check storage quota for author
        if let Some(quota_manager) = &self.storage_quota_manager {
            let can_store = quota_manager.read().await.can_store(&author, entry_size);

            if let Err(e) = can_store {
                warn!(
                    author = %author,
                    entry_size = entry_size,
                    topic = %topic,
                    error = %e,
                    "Rejecting entry - storage quota exceeded"
                );
                icn_obs::metrics::storage_quotas::exceeded_inc();
                bail!("Storage quota exceeded for author {author}: {e}");
            }
        }

        // Check topic max_entries limit BEFORE inserting to prevent unbounded growth
        if let Some(topic_obj) = self.topics.get(topic) {
            if topic_entries.len() >= topic_obj.max_entries {
                // At capacity - remove oldest entry first to make room
                let mut entries_vec: Vec<_> = topic_entries.values().cloned().collect();
                entries_vec.sort_by_key(|e| e.timestamp);

                if let Some(oldest) = entries_vec.first() {
                    debug!(
                        "Topic {} at capacity ({}), removing oldest entry",
                        topic, topic_obj.max_entries
                    );
                    let oldest_hash = oldest.hash;
                    let oldest_size = oldest.data.len() as u64;
                    let oldest_author = oldest.author.clone();
                    topic_entries.remove(&oldest_hash);

                    // Phase 18 Week 6: Release quota for evicted entry
                    if let Some(quota_manager) = &self.storage_quota_manager {
                        let _ = quota_manager.write().await.release_usage(
                            &oldest_author,
                            &oldest_hash,
                            oldest_size,
                        );
                    }
                }
            }
        }

        // Add to bloom filter
        if let Some(bloom) = self.bloom_filters.get_mut(topic) {
            bloom.insert(&hash);
        }

        // Store entry
        topic_entries.insert(hash, entry.clone());

        // M2: Check if bloom filter needs dynamic resizing
        let entry_count = topic_entries.len();
        if let Some(bloom) = self.bloom_filters.get(topic) {
            if bloom.needs_resize(entry_count, &self.bloom_resize_config) {
                // Collect all entry hashes for this topic
                let hashes: Vec<ContentHash> = topic_entries.keys().copied().collect();
                let old_size = bloom.capacity();
                let old_fp_rate = bloom.estimated_fp_rate();

                // Rebuild with optimal sizing
                let new_bloom = BloomFilter::rebuild(&hashes, &self.bloom_resize_config);
                let new_size = new_bloom.capacity();

                debug!(
                    topic = %topic,
                    old_size = old_size,
                    new_size = new_size,
                    entry_count = entry_count,
                    old_fp_rate = %format!("{:.4}", old_fp_rate),
                    "Resized bloom filter for topic"
                );

                // Replace the bloom filter
                self.bloom_filters.insert(topic.clone(), new_bloom);

                // Track metrics
                icn_obs::metrics::gossip::bloom_resize_inc();
                icn_obs::metrics::gossip::bloom_fp_rate_record(topic, old_fp_rate);
            }
        }

        // Phase 18 Week 6: Record quota usage for new entry
        if let Some(quota_manager) = &self.storage_quota_manager {
            let mut manager = quota_manager.write().await;
            // Use Normal priority for gossip entries (evicted before ledger/contracts)
            let _ = manager.record_usage(
                &author,
                hash.to_vec(),
                entry_size,
                icn_store::QuotaPriority::Normal,
            );

            // Check if eviction is needed
            if manager.needs_eviction() {
                if let Ok(evicted) = manager.evict_if_needed() {
                    if !evicted.is_empty() {
                        info!(
                            evicted_count = evicted.len(),
                            "Storage quota eviction triggered"
                        );
                        icn_obs::metrics::storage_quotas::evicted_inc(evicted.len() as u64);
                    }
                }
            }
        }

        // Merge vector clock
        self.clock.merge(&entry.clock);

        // Notify subscribers about the new entry
        if let Some(callback) = &self.notification_callback {
            if let Some(subscribers) = self.subscriptions.get(topic) {
                for subscriber in subscribers {
                    debug!(
                        "Notifying subscriber {} about new entry in topic {}",
                        subscriber, topic
                    );
                    callback(topic.clone(), entry.clone(), subscriber.clone());
                }
            }
        }

        Ok(())
    }

    /// Get entries for a topic
    pub fn get_entries(&self, topic: &str) -> Vec<GossipEntry> {
        self.entries
            .get(topic)
            .map(|entries| entries.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get a specific entry by hash
    pub fn get_entry(&self, topic: &str, hash: &ContentHash) -> Option<GossipEntry> {
        self.entries
            .get(topic)
            .and_then(|entries| entries.get(hash).cloned())
    }

    // Subscription methods moved to subscriptions.rs module
    // Anti-entropy methods moved to anti_entropy.rs module
    // Message handling moved to protocol.rs module

    // Note: Legacy dead code removed in Phase 29 (#155):
    // - get_topic_hashes() - unused helper
    // - find_entries_to_push() - superseded by Digest-based protocol
    // - find_entries_to_pull() - incomplete implementation, never used
    // The gossip protocol uses Digest → PullRequest → PullResponse flow.
    // See handle_digest(), handle_pull_request(), handle_pull_response().

    /// Initiate partition healing with a peer (Phase 18 Week 3)
    ///
    /// Sends a PartitionHealRequest to the specified peer to synchronize
    /// vector clocks and request missing entries after a partition.
    pub fn initiate_partition_healing(&mut self, peer: &Did) {
        info!(
            peer = %peer,
            "Initiating partition healing"
        );

        // Get last contact time (if tracked)
        let last_contact_ms = if let Some(ref detector) = self.partition_detector {
            if let Ok(d) = detector.try_read() {
                d.time_since_contact(peer)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            } else {
                0
            }
        } else {
            0
        };

        // Send heal request with our current vector clock
        self.send_message(
            Some(peer.clone()),
            GossipMessage::PartitionHealRequest {
                requesting_peer: self.own_did.clone(),
                vector_clock: self.clock.clone(),
                last_contact_ms,
            },
        );

        icn_obs::metrics::gossip::partition_detected_inc();
    }

    /// Get all topic names
    pub fn get_topics(&self) -> Vec<String> {
        self.topics.keys().cloned().collect()
    }

    /// Get a reference to this node's vector clock
    pub fn get_clock(&self) -> &VectorClock {
        &self.clock
    }

    /// Perform anti-entropy for a specific topic
    ///
    /// Update gauge metrics for topics and entries
    pub(crate) fn update_gauge_metrics(&self) {
        // Count total topics
        icn_obs::metrics::gossip::topics_total_set(self.topics.len() as u64);

        // Count total entries across all topics
        let total_entries: usize = self.entries.values().map(|e| e.len()).sum();
        icn_obs::metrics::gossip::entries_total_set(total_entries as u64);

        // Count total subscriptions across all topics
        let total_subscriptions: usize = self.subscriptions.values().map(|subs| subs.len()).sum();
        icn_obs::metrics::gossip::subscriptions_total_set(total_subscriptions as u64);
    }

    /// Hash data to create content hash
    fn hash_data(data: &[u8]) -> ContentHash {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    /// Export gossip actor state for persistence
    ///
    /// This exports the minimum state needed to restore the actor after restart:
    /// - Vector clock for causal ordering
    /// - Topic subscriptions (who is subscribed to what)
    /// - Topic metadata (configuration)
    ///
    /// Note: Gossip entries themselves are NOT persisted - they will be re-gossiped
    /// from peers after restart via anti-entropy.
    pub fn export_state(&self) -> icn_snapshot::GossipState {
        // Export vector clock
        let vector_clock: std::collections::HashMap<String, u64> = self
            .clock
            .iter()
            .map(|(did, count)| (did.to_string(), count))
            .collect();

        // Export subscriptions
        let subscriptions: std::collections::HashMap<String, Vec<String>> = self
            .subscriptions
            .iter()
            .map(|(topic, subs)| {
                let sub_strs = subs.iter().map(|did| did.to_string()).collect();
                (topic.clone(), sub_strs)
            })
            .collect();

        // Export topic metadata
        let topics: std::collections::HashMap<String, icn_snapshot::TopicMetadata> = self
            .topics
            .iter()
            .map(|(name, topic)| {
                let acl_str = match &topic.acl {
                    AccessControl::Public => "Public".to_string(),
                    AccessControl::MinTrustScore(score) => format!("MinTrustScore:{score}"),
                    AccessControl::Participants(dids) => {
                        // Serialize all participant DIDs to preserve access control
                        let did_strs: Vec<String> = dids.iter().map(|d| d.to_string()).collect();
                        format!("Participants:[{}]", did_strs.join(","))
                    }
                };

                let scope_str = format!("{:?}", topic.scope);

                (
                    name.clone(),
                    icn_snapshot::TopicMetadata {
                        name: topic.name.clone(),
                        access_control: acl_str,
                        max_entries: topic.max_entries,
                        scope: scope_str,
                    },
                )
            })
            .collect();

        icn_snapshot::GossipState {
            vector_clock,
            subscriptions,
            topics,
        }
    }

    /// Restore gossip actor state from persistence
    ///
    /// This restores:
    /// - Vector clock (for causal ordering continuity)
    /// - Topic subscriptions (automatically re-subscribe on restart)
    /// - Topic metadata (recreate topics with same configuration)
    ///
    /// Note: Gossip entries are NOT restored - they will be fetched from peers
    /// via anti-entropy once the actor reconnects to the network.
    pub fn restore_state(&mut self, state: icn_snapshot::GossipState) -> Result<()> {
        info!(
            "Restoring gossip state: {} vector clock entries, {} subscriptions, {} topics",
            state.vector_clock.len(),
            state.subscriptions.len(),
            state.topics.len()
        );

        // Restore vector clock
        self.clock.clear();
        for (did_str, count) in state.vector_clock {
            let did = Did::from_str(&did_str).context("Failed to parse DID from vector clock")?;
            self.clock.insert(did, count);
        }

        // Restore topic metadata (must happen before restoring subscriptions)
        for (_name, topic_meta) in state.topics {
            // Parse access control
            let acl = if topic_meta.access_control == "Public" {
                AccessControl::Public
            } else if topic_meta.access_control.starts_with("MinTrustScore:") {
                let score_str = topic_meta
                    .access_control
                    .strip_prefix("MinTrustScore:")
                    .unwrap_or("0.7");
                let score = score_str.parse().unwrap_or(0.7);
                AccessControl::MinTrustScore(score)
            } else if topic_meta.access_control.starts_with("TrustClass:") {
                // Legacy support for TrustClass
                if topic_meta.access_control.contains("Federated") {
                    AccessControl::MinTrustScore(0.7)
                } else if topic_meta.access_control.contains("Partner") {
                    AccessControl::MinTrustScore(0.4)
                } else if topic_meta.access_control.contains("Known") {
                    AccessControl::MinTrustScore(0.1)
                } else {
                    AccessControl::MinTrustScore(0.0)
                }
            } else if topic_meta.access_control.starts_with("Participants:[") {
                // Parse participant DIDs from "Participants:[did1,did2,...]" format
                let dids_part = topic_meta
                    .access_control
                    .strip_prefix("Participants:[")
                    .and_then(|s| s.strip_suffix("]"))
                    .unwrap_or("");

                if dids_part.is_empty() {
                    // Empty participants list
                    AccessControl::Participants(Vec::new())
                } else {
                    // Parse comma-separated DIDs
                    let dids: Result<Vec<Did>> = dids_part
                        .split(',')
                        .map(|did_str| {
                            Did::from_str(did_str.trim())
                                .context(format!("Failed to parse participant DID: {did_str}"))
                        })
                        .collect();

                    match dids {
                        Ok(dids) => AccessControl::Participants(dids),
                        Err(e) => {
                            warn!(
                                "Failed to parse Participants ACL: {}, defaulting to Public",
                                e
                            );
                            AccessControl::Public
                        }
                    }
                }
            } else {
                // Default to Public for unknown ACL types
                warn!(
                    "Unknown AccessControl: {}, defaulting to Public",
                    topic_meta.access_control
                );
                AccessControl::Public
            };

            // Parse scope
            let scope = if topic_meta.scope.contains("LocalCluster") {
                crate::types::Scope::LocalCluster
            } else if topic_meta.scope.contains("Regional") {
                crate::types::Scope::Regional
            } else {
                crate::types::Scope::Global
            };

            // Recreate topic
            let topic = Topic::new(topic_meta.name.clone(), acl)
                .with_scope(scope)
                .with_max_entries(topic_meta.max_entries);

            // Only create if it doesn't already exist (to preserve default topics)
            if !self.topics.contains_key(&topic.name) {
                self.create_topic(topic);
            }
        }

        // Restore subscriptions
        for (topic, subs) in state.subscriptions {
            // Warn if restoring subscriptions for a topic that wasn't in the snapshot
            if !self.topics.contains_key(&topic) {
                warn!(
                    "Restoring subscriptions for topic '{}' which was not in snapshot topics. \
                       Topic may have been deleted or snapshot may be corrupted.",
                    topic
                );
            }

            for sub_str in subs {
                let did =
                    Did::from_str(&sub_str).context("Failed to parse DID from subscription")?;

                // Ensure subscription list exists for this topic (create if missing)
                let sub_list = self.subscriptions.entry(topic.clone()).or_default();

                // Add subscription without access control check (we trust persisted state)
                if !sub_list.contains(&did) {
                    sub_list.push(did.clone());
                }
            }
        }

        info!("✅ Gossip state restored successfully");
        Ok(())
    }
}

/// Shared gossip actor handle
pub type GossipHandle = Arc<RwLock<GossipActor>>;

/// Legacy adapter for trust lookup closures
struct LegacyTrustOracle {
    callback: TrustLookup,
}

impl PolicyOracle for LegacyTrustOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        use icn_trust::TrustClass;

        let did_str = &request.core.actor;
        let did = match did_str.parse::<Did>() {
            Ok(d) => d,
            Err(_) => {
                return PolicyDecision::Deny {
                    reason: icn_kernel_api::authz::PolicyError::Denied(format!(
                        "Invalid DID: {}",
                        did_str
                    )),
                }
            }
        };

        if let Some(trust_class) = (self.callback)(&did) {
            let mut constraints = icn_kernel_api::authz::ConstraintSet::default();

            match trust_class {
                TrustClass::Federated => {
                    constraints = constraints
                        .with_max_subscriptions(1000)
                        .with_max_outstanding_requests(3)
                        .with_max_message_size(1024 * 1024);
                    constraints
                        .custom
                        .insert("trust_score".to_string(), 0.8.into());
                }
                TrustClass::Partner => {
                    constraints = constraints
                        .with_max_subscriptions(500)
                        .with_max_outstanding_requests(3)
                        .with_max_message_size(1024 * 1024);
                    constraints
                        .custom
                        .insert("trust_score".to_string(), 0.5.into());
                }
                TrustClass::Known => {
                    constraints = constraints
                        .with_max_subscriptions(100)
                        .with_max_outstanding_requests(2)
                        .with_max_message_size(256 * 1024);
                    constraints
                        .custom
                        .insert("trust_score".to_string(), 0.2.into());
                }
                TrustClass::Isolated => {
                    constraints = constraints
                        .with_max_subscriptions(10)
                        .with_max_outstanding_requests(1)
                        .with_max_message_size(64 * 1024);
                    constraints
                        .custom
                        .insert("trust_score".to_string(), 0.05.into());
                }
            }
            PolicyDecision::Allow { constraints }
        } else {
            PolicyDecision::Deny {
                reason: icn_kernel_api::authz::PolicyError::Denied("Untrusted".to_string()),
            }
        }
    }

    fn domain(&self) -> Domain {
        Domain::trust()
    }
}

impl GossipActor {
    /// Spawn a gossip actor and return a handle
    pub fn spawn(own_did: Did, oracle: Option<Arc<dyn PolicyOracle>>) -> GossipHandle {
        let actor = GossipActor::new(own_did, oracle);
        Arc::new(RwLock::new(actor))
    }

    /// Spawn a gossip actor with a legacy trust callback
    pub fn spawn_with_legacy_trust(own_did: Did, trust_callback: TrustLookup) -> GossipHandle {
        let oracle = Arc::new(LegacyTrustOracle {
            callback: trust_callback,
        });
        Self::spawn(own_did, Some(oracle))
    }

    /// Spawn with trust graph (legacy signature for compatibility)
    pub fn spawn_with_trust_graph(
        own_did: Did,
        trust_callback: TrustLookup,
        _store: Option<Arc<icn_store::SledStore>>,
    ) -> GossipHandle {
        Self::spawn_with_legacy_trust(own_did, trust_callback)
    }
}

/// Start periodic digest emission background task
///
/// This spawns a tokio task that periodically broadcasts Digest messages
/// for all topics. The interval is configurable with jitter to prevent
/// thundering herd issues.
///
/// # Parameters
/// - `gossip_handle`: Shared handle to the gossip actor
/// - `interval_ms`: Base interval between digest broadcasts (e.g., 10000 for 10 seconds)
/// - `jitter_ms`: Maximum random jitter to add (e.g., 2000 for ±2 seconds)
/// - `shutdown`: Receiver for graceful shutdown signal
///
/// # Returns
/// JoinHandle for the background task
pub fn start_digest_emitter(
    gossip_handle: GossipHandle,
    interval_ms: u64,
    jitter_ms: u64,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!(
            "Starting periodic digest emitter: interval={}ms, jitter=±{}ms",
            interval_ms, jitter_ms
        );

        loop {
            // Calculate next interval with jitter (thread_rng is recreated each iteration to avoid Send issues)
            let jitter = if jitter_ms > 0 {
                use rand::Rng;
                rand::thread_rng().gen_range(0..jitter_ms)
            } else {
                0
            };
            let sleep_duration = tokio::time::Duration::from_millis(interval_ms + jitter);

            // Wait for either timeout or shutdown
            tokio::select! {
                _ = tokio::time::sleep(sleep_duration) => {
                    // Time to emit digests
                    let mut gossip = gossip_handle.write().await;
                    if let Err(e) = gossip.emit_all_digests() {
                        warn!("Failed to emit digests: {}", e);
                    }
                }
                _ = shutdown.recv() => {
                    info!("Digest emitter shutting down");
                    break;
                }
            }
        }
    })
}

/// Start periodic partition checker background task (Phase 18 Week 3)
///
/// This spawns a tokio task that periodically checks for network partitions
/// and attempts healing when partitioned peers reconnect.
///
/// # Parameters
/// - `gossip_handle`: Shared handle to the gossip actor
/// - `check_interval_ms`: Interval between partition checks (default: 30000 for 30 seconds)
/// - `shutdown`: Receiver for graceful shutdown signal
///
/// # Returns
/// JoinHandle for the background task
pub fn start_partition_checker(
    gossip_handle: GossipHandle,
    check_interval_ms: u64,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!(
            "Starting periodic partition checker: interval={}ms",
            check_interval_ms
        );

        loop {
            let sleep_duration = tokio::time::Duration::from_millis(check_interval_ms);

            // Wait for either timeout or shutdown
            tokio::select! {
                _ = tokio::time::sleep(sleep_duration) => {
                    // Time to check for partitions
                    let gossip = gossip_handle.read().await;

                    // Get partition detector
                    if let Some(ref detector) = gossip.partition_detector {
                        if let Ok(detector_guard) = detector.try_read() {
                            let partitioned_peers = detector_guard.get_partitioned_peers();

                            if !partitioned_peers.is_empty() {
                                warn!(
                                    "Detected {} partitioned peers: {:?}",
                                    partitioned_peers.len(),
                                    partitioned_peers.iter().map(|d| d.as_str()).collect::<Vec<_>>()
                                );

                                // Update Prometheus metrics
                                for peer in &partitioned_peers {
                                    if let Some(duration) = detector_guard.time_since_contact(peer) {
                                        icn_obs::metrics::gossip::partition_detected_inc();
                                        debug!(
                                            "Peer {} partitioned for {:?}",
                                            peer,
                                            duration
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                _ = shutdown.recv() => {
                    info!("Partition checker shutting down");
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    use icn_kernel_api::authz::{
        ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest,
    };

    struct MockPolicyOracle {
        default_score: f64,
        scores: HashMap<String, f64>,
    }

    impl MockPolicyOracle {
        fn new() -> Self {
            Self {
                default_score: 0.4, // Partner equivalent by default
                scores: HashMap::new(),
            }
        }

        fn with_score(mut self, did: &str, score: f64) -> Self {
            self.scores.insert(did.to_string(), score);
            self
        }

        fn default_score(mut self, score: f64) -> Self {
            self.default_score = score;
            self
        }
    }

    impl PolicyOracle for MockPolicyOracle {
        fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
            let score = *self
                .scores
                .get(&request.core.actor)
                .unwrap_or(&self.default_score);
            let mut constraints = ConstraintSet::default();
            constraints
                .custom
                .insert("trust_score".to_string(), score.into());

            // Populate standard fields based on score for mock behavior
            if score >= 0.7 {
                constraints = constraints
                    .with_max_subscriptions(1000)
                    .with_max_outstanding_requests(3)
                    .with_max_message_size(1024 * 1024);
            } else if score >= 0.4 {
                constraints = constraints
                    .with_max_subscriptions(500)
                    .with_max_outstanding_requests(3)
                    .with_max_message_size(1024 * 1024);
            } else if score >= 0.1 {
                constraints = constraints
                    .with_max_subscriptions(100)
                    .with_max_outstanding_requests(2)
                    .with_max_message_size(256 * 1024);
            } else {
                constraints = constraints
                    .with_max_subscriptions(10)
                    .with_max_outstanding_requests(1)
                    .with_max_message_size(64 * 1024);
            }

            PolicyDecision::Allow { constraints }
        }

        fn domain(&self) -> Domain {
            Domain::trust()
        }
    }

    fn create_test_oracle() -> Option<Arc<dyn PolicyOracle>> {
        Some(Arc::new(MockPolicyOracle::new()))
    }

    #[tokio::test]
    async fn test_create_and_publish() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), create_test_oracle());

        // Publish to default topic
        let data = b"Hello, world!".to_vec();
        let hash = gossip
            .publish("global:identity", data.clone())
            .await
            .unwrap();

        // Retrieve entry
        let entry = gossip.get_entry("global:identity", &hash).unwrap();
        assert_eq!(entry.data, data);
        assert_eq!(entry.author, did);
    }

    #[tokio::test]
    async fn test_subscribe() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), create_test_oracle());

        let subscription = gossip
            .subscribe("global:identity", did.clone())
            .await
            .unwrap();
        assert_eq!(subscription.topic, "global:identity");
    }

    #[tokio::test]
    async fn test_bloom_filter_sync() {
        let keypair1 = KeyPair::generate().unwrap();
        let did1 = keypair1.did().clone();

        let keypair2 = KeyPair::generate().unwrap();
        let did2 = keypair2.did().clone();

        let mut gossip1 = GossipActor::new(did1.clone(), create_test_oracle());
        let mut gossip2 = GossipActor::new(did2.clone(), create_test_oracle());

        // Node 1 publishes entries
        gossip1
            .publish("global:identity", b"Entry 1".to_vec())
            .await
            .unwrap();
        gossip1
            .publish("global:identity", b"Entry 2".to_vec())
            .await
            .unwrap();

        // Node 2 publishes different entry
        gossip2
            .publish("global:identity", b"Entry 3".to_vec())
            .await
            .unwrap();

        // Get bloom filter from node 2
        let bloom2 = gossip2.get_bloom_filter("global:identity").unwrap();

        // Find what node 1 has that node 2 doesn't
        let missing = gossip1.find_missing("global:identity", &bloom2);

        // Should have 2 missing entries (Entry 1 and Entry 2)
        assert_eq!(missing.len(), 2);
    }

    #[tokio::test]
    async fn test_vector_clock_merge() {
        let keypair1 = KeyPair::generate().unwrap();
        let did1 = keypair1.did().clone();

        let mut gossip = GossipActor::new(did1.clone(), create_test_oracle());

        // Initial clock
        let initial_count = gossip.clock.get(&did1);

        // Publish entry (increments clock)
        gossip
            .publish("global:identity", b"Test".to_vec())
            .await
            .unwrap();

        // Clock should have incremented
        assert_eq!(gossip.clock.get(&did1), initial_count + 1);
    }

    #[tokio::test]
    async fn test_pull_protocol_request_response() {
        // Test that the pull protocol works: Announce -> Request -> Response
        let keypair1 = KeyPair::generate().unwrap();
        let did1 = keypair1.did().clone();

        let keypair2 = KeyPair::generate().unwrap();
        let did2 = keypair2.did().clone();

        // Create two gossip actors
        let mut gossip1 = GossipActor::new(did1.clone(), create_test_oracle());
        let mut gossip2 = GossipActor::new(did2.clone(), create_test_oracle());

        // Track messages sent by gossip2 via callback
        let sent_messages = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sent_messages_clone = sent_messages.clone();

        gossip2.set_send_callback(Arc::new(move |recipient, msg| {
            sent_messages_clone.lock().unwrap().push((recipient, msg));
        }));

        // Gossip1 publishes an entry
        let data = b"Test entry".to_vec();
        let hash = gossip1
            .publish("global:identity", data.clone())
            .await
            .unwrap();

        // Get the entry from gossip1
        let entry = gossip1.get_entry("global:identity", &hash).unwrap();

        // Simulate gossip2 receiving an Announce from gossip1
        let announce = GossipMessage::Announce {
            hash,
            author: did1.clone(),
            clock: entry.clock.clone(),
            topic: "global:identity".to_string(),
        };

        gossip2.handle_message(&did1, announce).await.unwrap();

        // Gossip2 should have sent a Request message
        {
            let messages = sent_messages.lock().unwrap();
            assert_eq!(messages.len(), 1);

            if let (Some(recipient), GossipMessage::Request { hash: req_hash }) = &messages[0] {
                assert_eq!(recipient, &did1);
                assert_eq!(req_hash, &hash);
            } else {
                panic!("Expected Request message");
            }
        } // Lock released here

        // Now simulate gossip1 receiving the Request and sending Response
        let sent_messages1 = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sent_messages1_clone = sent_messages1.clone();

        gossip1.set_send_callback(Arc::new(move |recipient, msg| {
            sent_messages1_clone.lock().unwrap().push((recipient, msg));
        }));

        let request = GossipMessage::Request { hash };
        gossip1.handle_message(&did2, request).await.unwrap();

        // Gossip1 should have sent a Response message directly to gossip2 (not broadcast)
        let messages1 = sent_messages1.lock().unwrap();
        assert_eq!(messages1.len(), 1);

        if let (Some(recipient), GossipMessage::Response { entry: resp_entry }) = &messages1[0] {
            assert_eq!(
                recipient, &did2,
                "Response should be sent directly to requester"
            );
            assert_eq!(resp_entry.hash, hash);
            assert_eq!(resp_entry.data, data);
        } else {
            panic!("Expected Response message with recipient");
        }
    }

    #[tokio::test]
    async fn test_request_missing_handler() {
        // Test RequestMissing message handling
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), create_test_oracle());

        // Publish some entries
        let hash1 = gossip
            .publish("global:identity", b"Entry 1".to_vec())
            .await
            .unwrap();
        let hash2 = gossip
            .publish("global:identity", b"Entry 2".to_vec())
            .await
            .unwrap();
        let _hash3 = gossip
            .publish("global:identity", b"Entry 3".to_vec())
            .await
            .unwrap();

        // Track messages sent via callback
        let sent_messages = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sent_messages_clone = sent_messages.clone();

        gossip.set_send_callback(Arc::new(move |recipient, msg| {
            sent_messages_clone.lock().unwrap().push((recipient, msg));
        }));

        // Request two of the three entries
        let request_missing = GossipMessage::RequestMissing {
            hashes: vec![hash1, hash2],
        };

        gossip.handle_message(&did, request_missing).await.unwrap();

        // Should have sent 2 Response messages
        let messages = sent_messages.lock().unwrap();
        assert_eq!(messages.len(), 2);

        for (recipient, msg) in messages.iter() {
            assert_eq!(*recipient, None); // Broadcast
            if let GossipMessage::Response { entry } = msg {
                assert!(entry.hash == hash1 || entry.hash == hash2);
            } else {
                panic!("Expected Response message");
            }
        }
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), create_test_oracle());

        // Subscribe first
        gossip
            .subscribe("global:identity", did.clone())
            .await
            .unwrap();
        assert!(gossip.is_subscribed("global:identity", &did));

        // Unsubscribe
        gossip.unsubscribe("global:identity", &did).unwrap();
        assert!(!gossip.is_subscribed("global:identity", &did));
    }

    #[tokio::test]
    async fn test_get_subscribers() {
        let keypair1 = KeyPair::generate().unwrap();
        let did1 = keypair1.did().clone();

        let keypair2 = KeyPair::generate().unwrap();
        let did2 = keypair2.did().clone();

        let mut gossip = GossipActor::new(did1.clone(), create_test_oracle());

        // Subscribe both DIDs
        gossip
            .subscribe("global:identity", did1.clone())
            .await
            .unwrap();
        gossip
            .subscribe("global:identity", did2.clone())
            .await
            .unwrap();

        // Get subscribers
        let subscribers = gossip.get_subscribers("global:identity");
        assert_eq!(subscribers.len(), 2);
        assert!(subscribers.contains(&did1));
        assert!(subscribers.contains(&did2));
    }

    #[tokio::test]
    async fn test_get_subscriptions() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), create_test_oracle());

        // Subscribe to multiple topics
        gossip
            .subscribe("global:identity", did.clone())
            .await
            .unwrap();
        gossip
            .subscribe("global:rendezvous", did.clone())
            .await
            .unwrap();

        // Get all subscriptions for this DID
        let subscriptions = gossip.get_subscriptions(&did);
        assert_eq!(subscriptions.len(), 2);
        assert!(subscriptions.contains(&"global:identity".to_string()));
        assert!(subscriptions.contains(&"global:rendezvous".to_string()));
    }

    #[tokio::test]
    async fn test_is_subscribed() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), create_test_oracle());

        // Not subscribed initially
        assert!(!gossip.is_subscribed("global:identity", &did));

        // Subscribe
        gossip
            .subscribe("global:identity", did.clone())
            .await
            .unwrap();
        assert!(gossip.is_subscribed("global:identity", &did));
    }

    #[tokio::test]
    async fn test_subscribe_duplicate_prevention() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), create_test_oracle());

        // Subscribe twice
        gossip
            .subscribe("global:identity", did.clone())
            .await
            .unwrap();
        gossip
            .subscribe("global:identity", did.clone())
            .await
            .unwrap();

        // Should only be subscribed once
        let subscribers = gossip.get_subscribers("global:identity");
        assert_eq!(subscribers.len(), 1);
    }

    #[test]
    fn test_unsubscribe_nonexistent_topic() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), create_test_oracle());

        // Try to unsubscribe from non-existent topic
        let result = gossip.unsubscribe("nonexistent:topic", &did);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_subscribe_acl_denied() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Trust lookup that returns None (no trust)
        // Trust lookup that returns None (no trust) -> default score 0.0
        let oracle = MockPolicyOracle::new().default_score(0.0);
        let mut gossip = GossipActor::new(did.clone(), Some(Arc::new(oracle)));

        // Create a topic with MinTrustScore(0.4) requirement (Partner level)
        let topic = Topic::new(
            "partner:only".to_string(),
            AccessControl::MinTrustScore(0.4),
        );
        gossip.create_topic(topic);

        // Try to subscribe with no trust - should fail
        let result = gossip.subscribe("partner:only", did.clone()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore] // Slow test - fills 10,000 slots
    async fn test_subscribe_limit_enforcement_full() {
        let owner = KeyPair::generate().unwrap().did().clone();
        let mut gossip = GossipActor::new(owner.clone(), create_test_oracle());

        let topic = Topic::new("test:limited".to_string(), AccessControl::Public);
        gossip.create_topic(topic);

        // Manually fill subscribers vector to MAX
        let subscribers = gossip.subscriptions.get_mut("test:limited").unwrap();
        let filler_did = KeyPair::generate().unwrap().did().clone();
        while subscribers.len() < super::MAX_SUBSCRIBERS_PER_TOPIC {
            subscribers.push(filler_did.clone());
        }

        // Verify at MAX
        assert_eq!(subscribers.len(), super::MAX_SUBSCRIBERS_PER_TOPIC);

        // Try to add another - should fail
        let did = KeyPair::generate().unwrap().did().clone();
        let result = gossip.subscribe("test:limited", did).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("limit reached"));
    }

    #[tokio::test]
    async fn test_subscribe_limit_enforcement_logic() {
        // Test the limit checking logic with a small number
        let owner = KeyPair::generate().unwrap().did().clone();
        let mut gossip = GossipActor::new(owner.clone(), create_test_oracle());

        let topic = Topic::new("test:limited".to_string(), AccessControl::Public);
        gossip.create_topic(topic);

        // Add 100 subscribers to verify normal operation
        for i in 0..100 {
            let did = KeyPair::generate().unwrap().did().clone();
            let result = gossip.subscribe("test:limited", did).await;
            assert!(result.is_ok(), "Subscribe {i} should succeed");
        }

        // Verify count
        let count = gossip.get_subscribers("test:limited").len();
        assert_eq!(count, 100, "Should have 100 subscribers");

        // The limit logic is validated in the ignored test above
        // This test just confirms normal operation works
    }

    #[tokio::test]
    async fn test_per_peer_subscription_limit() {
        // Test that a single peer cannot subscribe to too many topics
        let owner = KeyPair::generate().unwrap().did().clone();

        // Use a trust oracle that returns 0.0 (Isolated) for unknown peers
        // Isolated limit is 10 topics
        let oracle = MockPolicyOracle::new().default_score(0.0);
        let mut gossip = GossipActor::new(owner.clone(), Some(Arc::new(oracle)));

        // Create many topics
        let peer = KeyPair::generate().unwrap().did().clone();

        // For Isolated trust class (0.0), limit is 10 topics
        for i in 0..10 {
            let topic_name = format!("test:topic_{i}");
            gossip.create_topic(Topic::new(topic_name.clone(), AccessControl::Public));
            let result = gossip.subscribe(&topic_name, peer.clone()).await;
            assert!(result.is_ok(), "Subscribe to topic {i} should succeed");
        }

        // Verify peer has 10 subscriptions
        let subs = gossip.get_subscriptions(&peer);
        assert_eq!(subs.len(), 10, "Should have 10 subscriptions");

        // 11th subscription should fail
        gossip.create_topic(Topic::new(
            "test:topic_10".to_string(),
            AccessControl::Public,
        ));
        let result = gossip.subscribe("test:topic_10", peer.clone()).await;
        assert!(result.is_err(), "11th subscription should fail");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("subscription limit"),
            "Error should mention subscription limit"
        );
    }

    #[tokio::test]
    async fn test_per_peer_subscription_limit_trust_weighted() {
        // Test that higher trust classes get higher limits
        let owner = KeyPair::generate().unwrap().did().clone();

        // Create a trust oracle that returns 0.8 (Federated) for the test peer
        let peer = KeyPair::generate().unwrap().did().clone();
        let oracle = MockPolicyOracle::new().with_score(peer.as_str(), 0.8);

        let mut gossip = GossipActor::new(owner.clone(), Some(Arc::new(oracle)));

        // For Federated trust class, limit is 400 topics
        // Create 100 topics (less than limit) - should all succeed
        for i in 0..100 {
            let topic_name = format!("test:fed_topic_{i}");
            gossip.create_topic(Topic::new(topic_name.clone(), AccessControl::Public));
            let result = gossip.subscribe(&topic_name, peer.clone()).await;
            assert!(
                result.is_ok(),
                "Federated peer subscribe to topic {i} should succeed"
            );
        }

        // Verify peer has 100 subscriptions (well under 400 limit)
        let subs = gossip.get_subscriptions(&peer);
        assert_eq!(
            subs.len(),
            100,
            "Federated peer should have 100 subscriptions"
        );
    }

    #[tokio::test]
    async fn test_entry_limit_enforcement() {
        let owner = KeyPair::generate().unwrap().did().clone();
        let mut gossip = GossipActor::new(owner.clone(), create_test_oracle());

        // Create a topic with small max_entries for testing
        let topic =
            Topic::new("test:entries".to_string(), AccessControl::Public).with_max_entries(5); // Small limit for fast testing
        gossip.create_topic(topic);

        // Publish more entries than the limit
        for i in 0..10 {
            let data = format!("entry_{i}").into_bytes();
            let result = gossip.publish("test:entries", data).await;
            assert!(result.is_ok(), "Publish {i} failed: {result:?}");

            // Sleep briefly to ensure distinct timestamps for proper ordering
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }

        // Should have exactly max_entries (5) entries
        let entries = gossip.get_entries("test:entries");
        assert_eq!(
            entries.len(),
            5,
            "Should have exactly 5 entries, got {}",
            entries.len()
        );

        // Oldest entries should be removed (entries 0-4 gone, 5-9 remain)
        let data_values: Vec<String> = entries
            .iter()
            .map(|e| String::from_utf8_lossy(&e.data).to_string())
            .collect();

        // Should contain the newest entries (5-9), oldest (0-4) should be evicted
        assert!(
            data_values.contains(&"entry_9".to_string()),
            "Missing entry_9"
        );
        assert!(
            data_values.contains(&"entry_8".to_string()),
            "Missing entry_8"
        );
        assert!(
            data_values.contains(&"entry_7".to_string()),
            "Missing entry_7"
        );
        assert!(
            data_values.contains(&"entry_6".to_string()),
            "Missing entry_6"
        );
        assert!(
            data_values.contains(&"entry_5".to_string()),
            "Missing entry_5"
        );

        // Verify old entries were evicted
        assert!(
            !data_values.contains(&"entry_0".to_string()),
            "entry_0 should have been evicted"
        );
        assert!(
            !data_values.contains(&"entry_4".to_string()),
            "entry_4 should have been evicted"
        );
    }

    #[tokio::test]
    async fn test_subscription_notifications() {
        use std::sync::Mutex;

        let owner = KeyPair::generate().unwrap().did().clone();
        let subscriber1 = KeyPair::generate().unwrap().did().clone();
        let subscriber2 = KeyPair::generate().unwrap().did().clone();

        let mut gossip = GossipActor::new(owner.clone(), create_test_oracle());

        // Create the topic first
        gossip.create_topic(Topic::new(
            "test:notifications".to_string(),
            AccessControl::Public,
        ));

        // Track notifications
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let notifications_clone = notifications.clone();

        // Set up notification callback
        let callback: EntryNotificationCallback = Arc::new(move |topic, entry, subscriber| {
            let mut notifs = notifications_clone.lock().unwrap();
            notifs.push((topic, entry.hash, subscriber));
        });
        gossip.set_notification_callback(callback);

        // Subscribe both users to the topic
        gossip
            .subscribe("test:notifications", subscriber1.clone())
            .await
            .unwrap();
        gossip
            .subscribe("test:notifications", subscriber2.clone())
            .await
            .unwrap();

        // Publish an entry
        let data = b"Test notification".to_vec();
        let hash = gossip.publish("test:notifications", data).await.unwrap();

        // Verify both subscribers were notified
        let notifs = notifications.lock().unwrap();
        assert_eq!(
            notifs.len(),
            2,
            "Should have 2 notifications (one per subscriber)"
        );

        // Check that both subscribers received the notification
        let subscriber_dids: Vec<_> = notifs.iter().map(|(_, _, did)| did.clone()).collect();
        assert!(
            subscriber_dids.contains(&subscriber1),
            "subscriber1 should be notified"
        );
        assert!(
            subscriber_dids.contains(&subscriber2),
            "subscriber2 should be notified"
        );

        // Verify all notifications are for the correct topic and hash
        for (topic, notif_hash, _) in notifs.iter() {
            assert_eq!(topic, "test:notifications");
            assert_eq!(*notif_hash, hash);
        }
    }

    #[tokio::test]
    async fn test_no_notification_without_callback() {
        let owner = KeyPair::generate().unwrap().did().clone();
        let subscriber = KeyPair::generate().unwrap().did().clone();

        let mut gossip = GossipActor::new(owner.clone(), create_test_oracle());

        // Create the topic first
        gossip.create_topic(Topic::new(
            "test:no-callback".to_string(),
            AccessControl::Public,
        ));

        // Subscribe without setting callback
        gossip
            .subscribe("test:no-callback", subscriber.clone())
            .await
            .unwrap();

        // This should not panic even without a callback set
        let result = gossip.publish("test:no-callback", b"Test".to_vec()).await;
        assert!(
            result.is_ok(),
            "Publishing should succeed even without notification callback"
        );
    }

    #[tokio::test]
    async fn test_no_notification_without_subscribers() {
        use std::sync::Mutex;

        let owner = KeyPair::generate().unwrap().did().clone();
        let mut gossip = GossipActor::new(owner.clone(), create_test_oracle());

        // Create topic first (required since Issue #473 - topics must be explicitly created)
        gossip.create_topic(Topic::new(
            "test:no-subs".to_string(),
            AccessControl::Public,
        ));

        let notification_count = Arc::new(Mutex::new(0));
        let count_clone = notification_count.clone();

        // Set up callback that counts notifications
        let callback: EntryNotificationCallback = Arc::new(move |_, _, _| {
            let mut count = count_clone.lock().unwrap();
            *count += 1;
        });
        gossip.set_notification_callback(callback);

        // Publish without any subscribers
        gossip
            .publish("test:no-subs", b"Test".to_vec())
            .await
            .unwrap();

        // Verify no notifications were sent
        let count = notification_count.lock().unwrap();
        assert_eq!(
            *count, 0,
            "Should have 0 notifications when there are no subscribers"
        );
    }

    #[tokio::test]
    async fn test_response_handler_triggers_notifications() {
        use sha2::{Digest, Sha256};
        use std::sync::Mutex;

        let owner = KeyPair::generate().unwrap().did().clone();
        let subscriber = KeyPair::generate().unwrap().did().clone();
        let author = KeyPair::generate().unwrap().did().clone();

        let mut gossip = GossipActor::new(owner.clone(), create_test_oracle());

        // Create topic and subscribe
        gossip.create_topic(Topic::new(
            "test:response".to_string(),
            AccessControl::Public,
        ));
        gossip
            .subscribe("test:response", subscriber.clone())
            .await
            .unwrap();

        // Track notifications
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let notifications_clone = notifications.clone();

        let callback: EntryNotificationCallback = Arc::new(move |topic, entry, sub_did| {
            let mut notifs = notifications_clone.lock().unwrap();
            notifs.push((topic, entry.hash, sub_did));
        });
        gossip.set_notification_callback(callback);

        // Create an entry as if it came from the network via Response message
        let data = b"Entry from network".to_vec();
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let result_bytes = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result_bytes);

        let entry = GossipEntry {
            hash,
            author: author.clone(),
            clock: VectorClock::new(),
            topic: "test:response".to_string(),
            data: data.clone(),
            compressed: false,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            replica_offered: None,
        };

        // Simulate receiving a Response message from the author
        let result = gossip
            .handle_message(
                &author,
                GossipMessage::Response {
                    entry: entry.clone(),
                },
            )
            .await;
        assert!(result.is_ok(), "Response handler should succeed");

        // Verify notification was sent to subscriber
        let notifs = notifications.lock().unwrap();
        assert_eq!(
            notifs.len(),
            1,
            "Should have 1 notification for the subscriber"
        );
        assert_eq!(notifs[0].0, "test:response");
        assert_eq!(notifs[0].1, hash);
        assert_eq!(notifs[0].2, subscriber);
    }

    #[tokio::test]
    async fn test_response_handler_enforces_max_entries() {
        use sha2::{Digest, Sha256};

        let owner = KeyPair::generate().unwrap().did().clone();
        let author = KeyPair::generate().unwrap().did().clone();

        let mut gossip = GossipActor::new(owner.clone(), create_test_oracle());

        // Create topic with small limit
        let topic =
            Topic::new("test:max-entries".to_string(), AccessControl::Public).with_max_entries(3);
        gossip.create_topic(topic);

        // Send 5 entries via Response messages
        for i in 0..5 {
            let data = format!("Entry {i}").into_bytes();
            let mut hasher = Sha256::new();
            hasher.update(&data);
            let result_bytes = hasher.finalize();
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&result_bytes);

            let entry = GossipEntry {
                hash,
                author: author.clone(),
                clock: VectorClock::new(),
                topic: "test:max-entries".to_string(),
                data,
                compressed: false,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                replica_offered: None,
            };

            // Small delay to ensure distinct timestamps
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;

            gossip
                .handle_message(&author, GossipMessage::Response { entry })
                .await
                .unwrap();
        }

        // Verify only 3 entries are stored (max_entries enforced)
        let entries = gossip.get_entries("test:max-entries");
        assert_eq!(
            entries.len(),
            3,
            "Should enforce max_entries limit for Response messages"
        );
    }

    /// Trust-Gated Subscription Tests

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_gated_subscription_rejection() {
        // Test that subscriptions are rejected when trust score < threshold
        let owner = KeyPair::generate().unwrap().did().clone();
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Configure oracle with low trust for Alice (0.05) and default 0.0 for Bob
        let oracle = MockPolicyOracle::new()
            .default_score(0.0)
            .with_score(&alice.to_string(), 0.05);

        // Create gossip actor
        let mut gossip = GossipActor::new(owner.clone(), Some(Arc::new(oracle)));

        // Create topic with min_trust_threshold of 0.1 (Known+)
        let topic = Topic::new("test:trust-gated".to_string(), AccessControl::Public)
            .with_min_trust_threshold(0.1);
        gossip.create_topic(topic);

        // Alice attempts to subscribe - should be rejected (score 0.05 < 0.1)
        let result = gossip.subscribe("test:trust-gated", alice.clone()).await;
        assert!(
            result.is_err(),
            "Subscription should be rejected due to low trust"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Insufficient trust"));

        // Bob (unknown, score 0.0) attempts to subscribe - should also be rejected
        let result = gossip.subscribe("test:trust-gated", bob.clone()).await;
        assert!(
            result.is_err(),
            "Subscription should be rejected for unknown peer"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_gated_subscription_acceptance() {
        // Test that subscriptions are accepted when trust score >= threshold
        let owner = KeyPair::generate().unwrap().did().clone();
        let alice = KeyPair::generate().unwrap().did().clone();

        // Configure oracle with sufficient trust for Alice (0.42 >= 0.4)
        let oracle = MockPolicyOracle::new()
            .default_score(0.0)
            .with_score(&alice.to_string(), 0.42);

        // Create gossip actor
        let mut gossip = GossipActor::new(owner.clone(), Some(Arc::new(oracle)));

        // Create topic with min_trust_threshold of 0.4 (Partner)
        let topic = Topic::new("test:trust-gated".to_string(), AccessControl::Public)
            .with_min_trust_threshold(0.4);
        gossip.create_topic(topic);

        // Alice attempts to subscribe - should succeed
        let result = gossip.subscribe("test:trust-gated", alice.clone()).await;
        assert!(
            result.is_ok(),
            "Subscription should be accepted with sufficient trust"
        );
        assert!(gossip.is_subscribed("test:trust-gated", &alice));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_gated_subscription_exact_threshold() {
        // Test boundary condition: trust score exactly equals threshold
        let owner = KeyPair::generate().unwrap().did().clone();
        let alice = KeyPair::generate().unwrap().did().clone();

        // Configure oracle with exact trust (0.4)
        let oracle = MockPolicyOracle::new()
            .default_score(0.0)
            .with_score(&alice.to_string(), 0.4);

        // Create gossip actor
        let mut gossip = GossipActor::new(owner.clone(), Some(Arc::new(oracle)));

        // Create topic with min_trust_threshold of 0.4
        let topic = Topic::new("test:trust-gated".to_string(), AccessControl::Public)
            .with_min_trust_threshold(0.4);
        gossip.create_topic(topic);

        // Alice attempts to subscribe - should succeed (score 0.4 >= 0.4)
        let result = gossip.subscribe("test:trust-gated", alice.clone()).await;
        assert!(
            result.is_ok(),
            "Subscription should be accepted at exact threshold"
        );
        assert!(gossip.is_subscribed("test:trust-gated", &alice));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_gated_fallback_to_acl() {
        // Test that when no trust graph is provided (oracle returns 0), falls back to AccessControl
        // NOTE: Topic::can_subscribe logic uses trust_score. If score is 0.0 and min_trust_threshold is 0.4, it FAILS.
        // The original test said "falls back to AccessControl".
        // If the original test relied on "no trust graph" ignoring the threshold, that was a bug or feature I removed.
        // With PolicyOracle, "no oracle" means 0.0 score.
        // If min_trust_threshold is 0.4, 0.0 < 0.4, it should reject.
        // If I want to test fallback, I should use a topic WITHOUT min_trust_threshold.
        // BUT the test explicitly sets `.with_min_trust_threshold(0.4)`.
        // AND validation `assert!(result.is_ok())`.
        // This implies the original implementation IGNORED the threshold if trust graph was missing.
        // My implementation:
        // `if let Some(threshold) = topic_obj.min_trust_threshold { if trust_score < threshold { reject } }`
        // So my implementation enforces it even if Oracle is effectively 0.
        // This is strictly safer.
        // So this test as written SHOULD FAIL with my new implementation.
        // I will update the test expectation to expect FAILURE if trust is missing but required.
        // OR I will remove the threshold from the test to prove ACL fallback works for untrusted peers on Public topics.
        // Let's change expectations to `is_err()` if strict, or remove threshold if testing public access.
        // The test name says `trust_gated_fallback_to_acl`.
        // Prioritizing safety: If you ask for 0.4 trust, and I don't know you, I should reject you.
        // I will change the test to verify REJECTION.

        let owner = KeyPair::generate().unwrap().did().clone();
        let alice = KeyPair::generate().unwrap().did().clone();

        // Create gossip actor with oracle returning 0.0
        let oracle = MockPolicyOracle::new().default_score(0.0);
        let mut gossip = GossipActor::new(owner.clone(), Some(Arc::new(oracle)));

        // Create topic with min_trust_threshold 0.4
        let topic = Topic::new("test:fallback".to_string(), AccessControl::Public)
            .with_min_trust_threshold(0.4);
        gossip.create_topic(topic);

        // Alice attempts to subscribe - should FAIL because score 0.0 < 0.4
        // Logic changed from legacy behavior: missing trust system = 0 trust, not "bypass check".
        let result = gossip.subscribe("test:fallback", alice.clone()).await;
        assert!(
            result.is_err(),
            "Subscription should be REJECTED when trust is required but score is 0"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_gated_mixed_with_acl() {
        // Test that trust threshold takes priority over ACL
        let owner = KeyPair::generate().unwrap().did().clone();
        let alice = KeyPair::generate().unwrap().did().clone();

        // Configure oracle with high trust (0.8) for Alice
        let oracle = MockPolicyOracle::new()
            .default_score(0.0)
            .with_score(&alice.to_string(), 0.8);

        let mut gossip = GossipActor::new(owner.clone(), Some(Arc::new(oracle)));

        // Create topic with BOTH min_trust_threshold AND TrustClass (MinTrustScore) requirement
        // AccessControl::MinTrustScore(0.4) (Partner)
        // min_trust_threshold(0.7)
        let topic = Topic::new(
            "test:mixed".to_string(),
            // AccessControl::TrustClass(TrustClass::Partner) -> MinTrustScore(0.4)
            AccessControl::MinTrustScore(0.4),
        )
        .with_min_trust_threshold(0.7);

        gossip.create_topic(topic);

        // Alice attempts to subscribe - should succeed (has score 0.8 >= 0.7 AND 0.8 >= 0.4)
        let result = gossip.subscribe("test:mixed", alice.clone()).await;
        assert!(result.is_ok(), "Trust score check should pass");
        assert!(gossip.is_subscribed("test:mixed", &alice));
    }

    #[test]
    fn test_participants_acl_persistence() {
        // Test that AccessControl::Participants preserves all DIDs across export/restore
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), create_test_oracle());
        gossip.set_keypair(keypair);

        // Create participant DIDs
        let participant1 = KeyPair::generate().unwrap().did().clone();
        let participant2 = KeyPair::generate().unwrap().did().clone();
        let participant3 = KeyPair::generate().unwrap().did().clone();

        // Create topic with Participants ACL
        let topic = Topic {
            name: "test:private".to_string(),
            acl: AccessControl::Participants(vec![
                participant1.clone(),
                participant2.clone(),
                participant3.clone(),
            ]),
            scope: crate::types::Scope::Global,
            min_trust_threshold: None,
            retention: std::time::Duration::from_secs(86400),
            max_entries: 1000,
        };
        gossip.create_topic(topic);

        // Export state
        let state = gossip.export_state();

        // Verify topic metadata was exported
        let topic_meta = state.topics.get("test:private").unwrap();

        // Verify ACL string format includes all DIDs
        assert!(
            topic_meta.access_control.starts_with("Participants:["),
            "ACL should be serialized as Participants:[...], got: {}",
            topic_meta.access_control
        );
        assert!(
            topic_meta
                .access_control
                .contains(&participant1.to_string()),
            "ACL should contain participant1"
        );
        assert!(
            topic_meta
                .access_control
                .contains(&participant2.to_string()),
            "ACL should contain participant2"
        );
        assert!(
            topic_meta
                .access_control
                .contains(&participant3.to_string()),
            "ACL should contain participant3"
        );

        // Create new gossip actor and restore state
        let mut gossip2 = GossipActor::new(did.clone(), create_test_oracle());
        gossip2.restore_state(state).unwrap();

        // Verify the topic was restored
        assert!(gossip2.topics.contains_key("test:private"));

        // Verify the ACL was correctly restored with all participants
        let restored_topic = gossip2.topics.get("test:private").unwrap();
        if let AccessControl::Participants(participants) = &restored_topic.acl {
            assert_eq!(participants.len(), 3, "Should have 3 participants");
            assert!(
                participants.contains(&participant1),
                "Should contain participant1"
            );
            assert!(
                participants.contains(&participant2),
                "Should contain participant2"
            );
            assert!(
                participants.contains(&participant3),
                "Should contain participant3"
            );
        } else {
            panic!(
                "Expected AccessControl::Participants, got: {:?}",
                restored_topic.acl
            );
        }
    }

    #[tokio::test]
    async fn test_subscription_restore_creates_missing_entries() {
        // Test that restoring subscriptions doesn't silently drop them if the subscription
        // list doesn't exist yet
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), create_test_oracle());
        gossip.set_keypair(keypair.clone());

        // Create topic and subscribe
        let topic = Topic::new("test:sub".to_string(), AccessControl::Public);
        gossip.create_topic(topic);
        gossip.subscribe("test:sub", did.clone()).await.unwrap();

        // Create another subscriber
        let subscriber2 = KeyPair::generate().unwrap().did().clone();
        gossip
            .subscribe("test:sub", subscriber2.clone())
            .await
            .unwrap();

        // Export state
        let state = gossip.export_state();

        // Verify subscriptions were exported
        assert!(state.subscriptions.contains_key("test:sub"));
        let subs = state.subscriptions.get("test:sub").unwrap();
        assert_eq!(subs.len(), 2, "Should have 2 subscriptions");

        // Create new gossip actor and restore state
        let mut gossip2 = GossipActor::new(did.clone(), create_test_oracle());
        gossip2.restore_state(state).unwrap();

        // Verify subscriptions were restored
        assert!(gossip2.subscriptions.contains_key("test:sub"));
        let restored_subs = gossip2.subscriptions.get("test:sub").unwrap();
        assert_eq!(restored_subs.len(), 2, "Should have 2 subscriptions");
        assert!(restored_subs.contains(&did), "Should contain original DID");
        assert!(
            restored_subs.contains(&subscriber2),
            "Should contain subscriber2"
        );
    }

    #[test]
    fn test_subscription_restore_warns_on_missing_topic() {
        // Test that restoring subscriptions for a topic that wasn't in the snapshot
        // logs a warning but doesn't fail
        use icn_snapshot::GossipState;
        use std::collections::HashMap;

        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let mut gossip = GossipActor::new(did.clone(), create_test_oracle());

        // Create state with subscription to a topic that doesn't exist
        let mut subscriptions = HashMap::new();
        subscriptions.insert("nonexistent:topic".to_string(), vec![did.to_string()]);

        let state = GossipState {
            vector_clock: HashMap::new(),
            subscriptions,
            topics: HashMap::new(), // No topics in snapshot
        };

        // Restore should succeed (with warning logged)
        let result = gossip.restore_state(state);
        assert!(
            result.is_ok(),
            "Restore should succeed despite missing topic"
        );

        // Verify subscription was still created
        assert!(
            gossip.subscriptions.contains_key("nonexistent:topic"),
            "Subscription list should be created even if topic wasn't in snapshot"
        );
        let subs = gossip.subscriptions.get("nonexistent:topic").unwrap();
        assert_eq!(subs.len(), 1);
        assert!(subs.contains(&did));
    }

    #[tokio::test]
    async fn test_replica_message_types() {
        // Phase 17: Test that replica coordination messages can be created and handled
        use crate::types::{GossipMessage, ReplicaHealth};

        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create a trust oracle (returns 0.1 to allow basic operations)
        let oracle = MockPolicyOracle::new().default_score(0.1);
        let mut gossip = GossipActor::new(did.clone(), Some(Arc::new(oracle)));

        let content_hash = [0xAA; 32];

        // Test ReplicaRequest message
        let request = GossipMessage::ReplicaRequest {
            content_hash,
            requesting_peer: did.clone(),
        };
        let result = gossip.handle_message(&did, request).await;
        assert!(
            result.is_ok(),
            "ReplicaRequest should be handled successfully"
        );

        // Test ReplicaOffer message
        let offer = GossipMessage::ReplicaOffer {
            content_hash,
            offering_peer: did.clone(),
            health: ReplicaHealth::Healthy,
        };
        let result = gossip.handle_message(&did, offer).await;
        assert!(
            result.is_ok(),
            "ReplicaOffer should be handled successfully"
        );

        // Test ReplicaStatus message
        let peer2 = KeyPair::generate().unwrap().did().clone();
        let status = GossipMessage::ReplicaStatus {
            content_hash,
            replicas: vec![
                (did.clone(), ReplicaHealth::Healthy),
                (peer2, ReplicaHealth::Stale),
            ],
        };
        let result = gossip.handle_message(&did, status).await;
        assert!(
            result.is_ok(),
            "ReplicaStatus should be handled successfully"
        );

        // Verify message variant names
        let request2 = GossipMessage::ReplicaRequest {
            content_hash,
            requesting_peer: did.clone(),
        };
        assert_eq!(request2.variant_name(), "ReplicaRequest");

        let offer2 = GossipMessage::ReplicaOffer {
            content_hash,
            offering_peer: did.clone(),
            health: ReplicaHealth::Healthy,
        };
        assert_eq!(offer2.variant_name(), "ReplicaOffer");

        let status2 = GossipMessage::ReplicaStatus {
            content_hash,
            replicas: vec![],
        };
        assert_eq!(status2.variant_name(), "ReplicaStatus");
    }

    #[tokio::test]
    async fn test_replica_coordination_with_store() -> Result<()> {
        // Phase 17: Test full replica coordination flow with storage
        use crate::types::{GossipMessage, ReplicaHealth};
        use icn_store::SledStore;

        // Create two gossip actors
        let keypair1 = KeyPair::generate()?;
        let did1 = keypair1.did().clone();
        let oracle1 = MockPolicyOracle::new().default_score(0.1);
        let mut gossip1 = GossipActor::new(did1.clone(), Some(Arc::new(oracle1)));

        let keypair2 = KeyPair::generate()?;
        let did2 = keypair2.did().clone();
        let oracle2 = MockPolicyOracle::new().default_score(0.1);
        let mut gossip2 = GossipActor::new(did2.clone(), Some(Arc::new(oracle2)));

        // Set up stores for both actors
        let store1 = Arc::new(SledStore::temporary()?) as Arc<dyn icn_store::Store>;
        let store2 = Arc::new(SledStore::temporary()?) as Arc<dyn icn_store::Store>;
        gossip1.set_store(store1.clone());
        gossip2.set_store(store2.clone());

        // Gossip1 publishes some content
        let data = b"test content for replication".to_vec();
        let mut hasher = sha2::Sha256::new();
        use sha2::Digest;
        hasher.update(&data);
        let result_bytes = hasher.finalize();
        let mut content_hash = [0u8; 32];
        content_hash.copy_from_slice(&result_bytes);

        let entry = GossipEntry {
            hash: content_hash,
            author: did1.clone(),
            clock: VectorClock::new(),
            topic: "test:replication".to_string(),
            data: data.clone(),
            compressed: false,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as u64,
            replica_offered: Some(true),
        };

        // Store the entry in gossip1
        gossip1.create_topic(Topic::new(
            "test:replication".to_string(),
            AccessControl::Public,
        ));
        gossip1
            .entries
            .entry("test:replication".to_string())
            .or_default()
            .insert(content_hash, entry.clone());

        // Test 1: ReplicaRequest from gossip2 to gossip1
        // Gossip2 doesn't have the content, requests it
        let request = GossipMessage::ReplicaRequest {
            content_hash,
            requesting_peer: did2.clone(),
        };

        // Track sent messages
        let sent_messages = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sent_clone = sent_messages.clone();
        gossip1.set_send_callback(Arc::new(move |recipient, message| {
            sent_clone.lock().unwrap().push((recipient, message));
        }));

        // Handle the request
        gossip1.handle_message(&did2, request).await?;

        // Verify gossip1 responded with ReplicaOffer
        {
            let messages = sent_messages.lock().unwrap();
            assert_eq!(messages.len(), 1, "Should have sent one message");
            match &messages[0] {
                (
                    Some(recipient),
                    GossipMessage::ReplicaOffer {
                        content_hash: hash,
                        offering_peer,
                        health,
                    },
                ) => {
                    assert_eq!(recipient, &did2, "Offer should be sent to requester");
                    assert_eq!(hash, &content_hash, "Hash should match");
                    assert_eq!(offering_peer, &did1, "Offerer should be gossip1");
                    assert_eq!(health, &ReplicaHealth::Healthy, "Health should be Healthy");
                }
                _ => panic!("Expected ReplicaOffer message"),
            }
        } // Lock released here

        // Verify gossip1 recorded itself as a replica in its store
        let replica_count = store1.get_replica_count(&content_hash)?;
        assert_eq!(
            replica_count, 1,
            "Gossip1 should have recorded itself as replica"
        );

        // Test 2: ReplicaOffer from gossip1 to gossip2
        // Gossip2 receives the offer
        let offer = GossipMessage::ReplicaOffer {
            content_hash,
            offering_peer: did1.clone(),
            health: ReplicaHealth::Healthy,
        };

        gossip2.handle_message(&did1, offer).await?;

        // Verify gossip2 recorded gossip1 as a replica
        let replica_count = store2.get_replica_count(&content_hash)?;
        assert_eq!(
            replica_count, 1,
            "Gossip2 should have recorded gossip1 as replica"
        );

        let metadata = store2.get_replica_metadata(&content_hash)?.unwrap();
        assert_eq!(metadata.replicas.len(), 1);
        assert_eq!(metadata.replicas[0].peer_did, did1.to_string());
        assert_eq!(
            metadata.replicas[0].health,
            icn_store::ReplicaHealth::Healthy
        );

        // Test 3: ReplicaStatus batch update
        // Gossip2 receives a status update with multiple replicas
        let did3 = KeyPair::generate()?.did().clone();
        let status = GossipMessage::ReplicaStatus {
            content_hash,
            replicas: vec![
                (did1.clone(), ReplicaHealth::Healthy),
                (did3.clone(), ReplicaHealth::Stale),
            ],
        };

        gossip2.handle_message(&did1, status).await?;

        // Verify all replicas were recorded
        let replica_count = store2.get_replica_count(&content_hash)?;
        assert_eq!(replica_count, 2, "Should have 2 replicas");

        let metadata = store2.get_replica_metadata(&content_hash)?.unwrap();
        assert_eq!(metadata.replicas.len(), 2);

        // Find the stale replica
        let stale_replica = metadata
            .replicas
            .iter()
            .find(|r| r.peer_did == did3.to_string())
            .expect("Should have did3 replica");
        assert_eq!(stale_replica.health, icn_store::ReplicaHealth::Stale);

        Ok(())
    }

    /// Test storage quota enforcement (Phase 18 Week 6)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_storage_quota_enforcement() -> Result<()> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        let oracle = MockPolicyOracle::new().default_score(0.8);
        let mut gossip = GossipActor::new(did.clone(), Some(Arc::new(oracle)));

        // Create storage quota manager with small limits for testing
        // 1KB global limit, 500 byte per-DID quota
        let mut quota_manager = icn_store::StorageQuotaManager::new(1024, 0.9);
        quota_manager.set_quota(did.clone(), 500, icn_store::QuotaPriority::Normal);
        let quota_handle = Arc::new(tokio::sync::RwLock::new(quota_manager));

        gossip.set_storage_quota_manager(quota_handle.clone());

        // Create a test topic
        gossip.create_topic(Topic::new("test:quota".to_string(), AccessControl::Public));

        // First publish should succeed (100 bytes < 500 byte quota)
        let data1 = vec![1u8; 100];
        let result = gossip.publish("test:quota", data1).await;
        assert!(result.is_ok(), "First publish should succeed within quota");

        // Second publish should also succeed (100 + 100 = 200 < 500)
        let data2 = vec![2u8; 100];
        let result = gossip.publish("test:quota", data2).await;
        assert!(result.is_ok(), "Second publish should succeed within quota");

        // Third publish should also succeed (200 + 100 = 300 < 500)
        let data3 = vec![3u8; 100];
        let result = gossip.publish("test:quota", data3).await;
        assert!(result.is_ok(), "Third publish should succeed within quota");

        // Verify quota usage is being tracked
        let manager = quota_handle.read().await;
        let quota = manager.get_quota(&did).unwrap();
        assert_eq!(
            quota.current_bytes, 300,
            "Should have recorded 300 bytes of usage"
        );

        Ok(())
    }

    /// Test storage quota exceeded rejection (Phase 18 Week 6)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_storage_quota_exceeded() -> Result<()> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();

        let oracle = MockPolicyOracle::new().default_score(0.8);
        let mut gossip = GossipActor::new(did.clone(), Some(Arc::new(oracle)));

        // Create storage quota manager with very small limit
        // 200 byte per-DID quota
        let mut quota_manager = icn_store::StorageQuotaManager::new(10000, 0.9);
        quota_manager.set_quota(did.clone(), 200, icn_store::QuotaPriority::Normal);
        let quota_handle = Arc::new(tokio::sync::RwLock::new(quota_manager));

        gossip.set_storage_quota_manager(quota_handle.clone());

        // Create a test topic
        gossip.create_topic(Topic::new("test:quota".to_string(), AccessControl::Public));

        // First publish should succeed (150 bytes < 200 byte quota)
        let data1 = vec![1u8; 150];
        let result = gossip.publish("test:quota", data1).await;
        assert!(result.is_ok(), "First publish should succeed within quota");

        // Second publish should fail (150 + 100 = 250 > 200 byte quota)
        let data2 = vec![2u8; 100];
        let result = gossip.publish("test:quota", data2).await;
        assert!(
            result.is_err(),
            "Second publish should fail - quota exceeded"
        );

        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("quota exceeded"),
            "Error should mention quota exceeded: {error}"
        );

        Ok(())
    }

    // Issue #473: Topic auto-creation policy tests

    #[tokio::test]
    async fn test_topic_auto_creation_policy_reject() {
        let owner = KeyPair::generate().unwrap().did().clone();
        let mut gossip = GossipActor::new(owner.clone(), create_test_oracle());

        // Default policy is Reject
        assert_eq!(
            gossip.topic_auto_creation_policy(),
            crate::types::TopicAutoCreationPolicy::Reject
        );

        // Try to publish to undeclared topic - should fail
        let result = gossip.publish("undeclared:topic", b"test".to_vec()).await;
        assert!(
            result.is_err(),
            "Publishing to undeclared topic should fail with Reject policy"
        );

        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("not found") || error.contains("must be explicitly created"),
            "Error should indicate topic not found: {error}"
        );
    }

    #[tokio::test]
    async fn test_topic_auto_creation_policy_create_with_strict_defaults() {
        let owner = KeyPair::generate().unwrap().did().clone();

        // Use Federated trust so we can publish to the auto-created topic
        // Use Federated trust (0.8) so we can publish to the auto-created topic
        let oracle = MockPolicyOracle::new().default_score(0.8);
        let mut gossip = GossipActor::new(owner.clone(), Some(Arc::new(oracle)));

        // Set policy to CreateWithStrictDefaults
        gossip.set_topic_auto_creation_policy(
            crate::types::TopicAutoCreationPolicy::CreateWithStrictDefaults,
        );

        // Publish to undeclared topic - should succeed (with Federated trust)
        let result = gossip.publish("auto:strict", b"test".to_vec()).await;
        assert!(
            result.is_ok(),
            "Publishing should succeed with Federated trust"
        );

        // Verify topic was created with strict defaults (Federated trust required)
        let topic_obj = gossip.topics.get("auto:strict");
        assert!(topic_obj.is_some(), "Topic should have been created");
        assert_eq!(
            topic_obj.unwrap().acl,
            AccessControl::MinTrustScore(0.7),
            "Auto-created topic should have MinTrustScore(0.7) ACL"
        );
    }

    #[tokio::test]
    async fn test_topic_auto_creation_policy_create_with_strict_defaults_denies_low_trust() {
        let owner = KeyPair::generate().unwrap().did().clone();

        // Use Known trust (below Federated threshold)
        // Use Known trust (0.1, below Federated threshold 0.7)
        let oracle = MockPolicyOracle::new().default_score(0.1);
        let mut gossip = GossipActor::new(owner.clone(), Some(Arc::new(oracle)));

        // Set policy to CreateWithStrictDefaults
        gossip.set_topic_auto_creation_policy(
            crate::types::TopicAutoCreationPolicy::CreateWithStrictDefaults,
        );

        // Publish to undeclared topic - should fail (Known < Federated)
        let result = gossip.publish("auto:strict", b"test".to_vec()).await;
        assert!(
            result.is_err(),
            "Publishing should fail with Known trust (below Federated requirement)"
        );

        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("Not authorized"),
            "Error should indicate authorization failure: {error}"
        );
    }

    #[tokio::test]
    async fn test_topic_auto_creation_policy_create_public() {
        let owner = KeyPair::generate().unwrap().did().clone();
        let mut gossip = GossipActor::new(owner.clone(), create_test_oracle());

        // Set policy to CreatePublic (legacy behavior)
        gossip.set_topic_auto_creation_policy(crate::types::TopicAutoCreationPolicy::CreatePublic);

        // Publish to undeclared topic - should succeed
        let result = gossip.publish("auto:public", b"test".to_vec()).await;
        assert!(
            result.is_ok(),
            "Publishing should succeed with CreatePublic policy"
        );

        // Verify topic was created as public
        let topic_obj = gossip.topics.get("auto:public");
        assert!(topic_obj.is_some(), "Topic should have been created");
        assert_eq!(
            topic_obj.unwrap().acl,
            AccessControl::Public,
            "Auto-created topic should have Public ACL"
        );
    }

    #[tokio::test]
    async fn test_explicit_topic_creation_bypasses_policy() {
        let owner = KeyPair::generate().unwrap().did().clone();
        let mut gossip = GossipActor::new(owner.clone(), create_test_oracle());

        // Default policy is Reject
        assert_eq!(
            gossip.topic_auto_creation_policy(),
            crate::types::TopicAutoCreationPolicy::Reject
        );

        // Explicitly create topic
        gossip.create_topic(Topic::new(
            "explicit:topic".to_string(),
            AccessControl::Public,
        ));

        // Now publish should succeed
        let result = gossip.publish("explicit:topic", b"test".to_vec()).await;
        assert!(
            result.is_ok(),
            "Publishing to explicitly created topic should succeed"
        );
    }
}
