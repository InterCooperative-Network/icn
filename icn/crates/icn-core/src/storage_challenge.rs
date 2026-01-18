//! Storage Challenge Scheduler
//!
//! Background task that periodically verifies replica holders actually store the data they claim.
//! Uses Merkle proofs and random byte challenges.
//!
//! # Protocol Flow
//!
//! 1. **Challenge Generation**: Challenger selects a random replica and generates a challenge
//!    containing a random byte offset and Merkle chunk index. The challenge is signed with
//!    the challenger's Ed25519 key.
//!
//! 2. **Challenge Delivery**: Challenge is sent via gossip to the target replica holder.
//!
//! 3. **Proof Generation**: Replica holder verifies challenge signature, rebuilds Merkle tree,
//!    extracts requested bytes, generates Merkle proof, signs the response.
//!
//! 4. **Proof Verification**: Challenger verifies proof signature, byte data hash, and Merkle
//!    path against expected values.
//!
//! 5. **Outcome**: On success, replica health is updated. On failure (after grace period),
//!    a Byzantine violation is recorded against the replica holder.
//!
//! # Security Model
//!
//! - **Merkle proofs** prevent returning precomputed data without storing the full content
//! - **Random byte challenges** force actual content access (not just metadata)
//! - **Ed25519 signatures** prevent forgery of challenges and proofs
//! - **Grace period** (3 failures) prevents false positives from network issues
//! - **Graduated severity** distinguishes network issues from Byzantine behavior

use anyhow::Result;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use icn_gossip::GossipActor;
use icn_identity::{Did, KeyPair};
use icn_security::{MisbehaviorDetector, StorageFailureReason, Violation};
use icn_store::{ChallengeConfig, ContentChunkTree, ContentHash, StorageChallenge, Store};
use icn_trust::TrustGraph;

/// Callback function type for triggering re-replication
///
/// Called when a replica fails too many storage challenges and should be replaced.
/// Arguments: (content_hash, unhealthy_peer_did, failure_count)
pub type ReReplicationCallback =
    Arc<dyn Fn(ContentHash, String, u32) -> anyhow::Result<()> + Send + Sync>;

/// Key prefix for persisted pending challenges
const PENDING_CHALLENGE_PREFIX: &[u8] = b"challenge:pending:";

/// Pending challenge tracking info (in-memory)
#[derive(Debug)]
struct PendingChallenge {
    /// The challenge that was sent
    challenge: StorageChallenge,

    /// When the challenge was sent
    sent_at: Instant,

    /// Expected Merkle root (for verification)
    expected_merkle_root: [u8; 32],

    /// Expected byte hash (for verification)
    expected_byte_hash: [u8; 32],

    /// Number of previous failures for this replica
    #[allow(dead_code)] // May be used for per-challenge tracking
    failure_count: u32,
}

/// Persisted pending challenge (serializable version)
///
/// Note: We use the challenge's built-in `expires_at` timestamp (absolute Unix time)
/// to determine expiration. The `sent_at` field is reconstructed on load using the
/// challenge's timestamps, which avoids clock-drift issues that would occur if we
/// tried to persist monotonic `Instant` values as wall-clock time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedPendingChallenge {
    /// The challenge that was sent (contains expires_at for deadline)
    challenge: StorageChallenge,

    /// Expected Merkle root (for verification)
    expected_merkle_root: [u8; 32],

    /// Expected byte hash (for verification)
    expected_byte_hash: [u8; 32],

    /// Number of previous failures for this replica
    failure_count: u32,
}

impl From<&PendingChallenge> for PersistedPendingChallenge {
    fn from(p: &PendingChallenge) -> Self {
        // The challenge itself contains created_at and expires_at as absolute Unix timestamps,
        // so we don't need to persist sent_at separately. On reload, we compute sent_at
        // from the challenge's timestamps to avoid clock-drift issues.
        Self {
            challenge: p.challenge.clone(),
            expected_merkle_root: p.expected_merkle_root,
            expected_byte_hash: p.expected_byte_hash,
            failure_count: p.failure_count,
        }
    }
}

impl PersistedPendingChallenge {
    /// Convert back to in-memory PendingChallenge
    ///
    /// Computes a synthetic `sent_at` using the challenge's absolute timestamps,
    /// which is correct regardless of wall-clock changes between persist and load.
    fn into_pending(self) -> Option<PendingChallenge> {
        // Use the challenge's absolute timestamps to compute elapsed time
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // If challenge is already expired, return None
        if now_secs >= self.challenge.expires_at {
            return None;
        }

        // Compute original timeout from challenge timestamps
        let original_timeout_secs = self
            .challenge
            .expires_at
            .saturating_sub(self.challenge.created_at);

        // Compute how much time has passed since challenge was created
        let elapsed_secs = now_secs.saturating_sub(self.challenge.created_at);

        // Cap elapsed to original timeout (handles clock adjustments gracefully)
        let elapsed_secs = elapsed_secs.min(original_timeout_secs);

        // Create synthetic sent_at: Instant::now() - elapsed
        let sent_at = Instant::now()
            .checked_sub(Duration::from_secs(elapsed_secs))
            .unwrap_or_else(Instant::now);

        Some(PendingChallenge {
            challenge: self.challenge,
            sent_at,
            expected_merkle_root: self.expected_merkle_root,
            expected_byte_hash: self.expected_byte_hash,
            failure_count: self.failure_count,
        })
    }
}

/// Storage Challenge Scheduler
///
/// Periodically challenges replica holders to prove they store data.
pub struct ChallengeScheduler {
    /// Own DID
    own_did: Did,

    /// Own keypair for signing challenges
    keypair: Arc<KeyPair>,

    /// Challenge configuration
    config: ChallengeConfig,

    /// Store for accessing replica metadata and content
    store: Arc<dyn Store>,

    /// Trust graph for peer scoring (used for trust-gated challenge frequency)
    trust_graph: Arc<RwLock<TrustGraph>>,

    /// Gossip actor for sending challenges
    gossip: Arc<RwLock<GossipActor>>,

    /// Misbehavior detector for recording violations
    misbehavior: Arc<RwLock<MisbehaviorDetector>>,

    /// Pending challenges: challenge_id -> PendingChallenge
    pending: HashMap<[u8; 32], PendingChallenge>,

    /// Per-replica failure tracking: (content_hash, peer_did) -> (count, last_updated)
    failure_counts: HashMap<(ContentHash, String), (u32, Instant)>,

    /// Optional callback for triggering re-replication when a replica becomes unhealthy
    re_replication_callback: Option<ReReplicationCallback>,

    /// Cached trust scores: peer_did -> (score, last_updated)
    /// Avoids expensive trust graph computations on every challenge decision.
    trust_score_cache: HashMap<String, (f64, Instant)>,
}

/// Maximum age for failure count entries before cleanup (24 hours)
const FAILURE_COUNT_MAX_AGE_SECS: u64 = 86400;

/// Trust score cache TTL (5 minutes)
/// Prevents expensive trust graph recomputation on every challenge decision.
const TRUST_SCORE_CACHE_TTL_SECS: u64 = 300;

impl ChallengeScheduler {
    /// Create a new ChallengeScheduler
    pub fn new(
        own_did: Did,
        keypair: Arc<KeyPair>,
        config: ChallengeConfig,
        store: Arc<dyn Store>,
        trust_graph: Arc<RwLock<TrustGraph>>,
        gossip: Arc<RwLock<GossipActor>>,
        misbehavior: Arc<RwLock<MisbehaviorDetector>>,
    ) -> Self {
        Self {
            own_did,
            keypair,
            config,
            store,
            trust_graph,
            gossip,
            misbehavior,
            pending: HashMap::new(),
            failure_counts: HashMap::new(),
            re_replication_callback: None,
            trust_score_cache: HashMap::new(),
        }
    }

    /// Set the re-replication callback
    ///
    /// This callback is invoked when a replica fails too many storage challenges
    /// and should be replaced with a new replica to maintain data availability.
    pub fn set_re_replication_callback(&mut self, callback: ReReplicationCallback) {
        self.re_replication_callback = Some(callback);
    }

    /// Persist a pending challenge to the store
    fn persist_pending_challenge(&self, challenge_id: &[u8; 32], pending: &PendingChallenge) {
        let key = [PENDING_CHALLENGE_PREFIX, challenge_id.as_slice()].concat();
        let persisted = PersistedPendingChallenge::from(pending);

        match serde_json::to_vec(&persisted) {
            Ok(value) => {
                if let Err(e) = self.store.put(&key, &value) {
                    warn!(
                        "Failed to persist pending challenge {}: {}",
                        hex::encode(&challenge_id[..8]),
                        e
                    );
                    icn_obs::metrics::storage_challenge::persist_failures_inc("write");
                }
            }
            Err(e) => {
                warn!(
                    "Failed to serialize pending challenge {}: {}",
                    hex::encode(&challenge_id[..8]),
                    e
                );
                icn_obs::metrics::storage_challenge::persist_failures_inc("serialize");
            }
        }
    }

    /// Remove a persisted pending challenge from the store
    fn remove_persisted_challenge(&self, challenge_id: &[u8; 32]) {
        let key = [PENDING_CHALLENGE_PREFIX, challenge_id.as_slice()].concat();
        if let Err(e) = self.store.delete(&key) {
            debug!(
                "Failed to remove persisted challenge {}: {}",
                hex::encode(&challenge_id[..8]),
                e
            );
        }
    }

    /// Calculate trust-adjusted challenge probability for a peer
    ///
    /// Lower trust peers are challenged more frequently. The formula scales
    /// the base probability by the inverse of the trust score:
    /// - Trust 1.0 (fully trusted): base_probability (no increase)
    /// - Trust 0.5: base_probability * 1.5
    /// - Trust 0.0 (no trust): base_probability * 2.0
    ///
    /// Returns the adjusted probability, capped at 1.0.
    async fn trust_adjusted_probability(&mut self, peer_did: &str) -> f64 {
        let base = self.config.challenge_probability;

        // Check cache first (TTL-based caching to avoid expensive trust computations)
        let now = Instant::now();
        let cache_ttl = Duration::from_secs(TRUST_SCORE_CACHE_TTL_SECS);

        let trust_score =
            if let Some((cached_score, cached_at)) = self.trust_score_cache.get(peer_did) {
                if now.duration_since(*cached_at) < cache_ttl {
                    // Cache hit - use cached score
                    *cached_score
                } else {
                    // Cache expired - recompute
                    let score = self.compute_trust_score(peer_did).await;
                    self.trust_score_cache
                        .insert(peer_did.to_string(), (score, now));
                    score
                }
            } else {
                // Cache miss - compute and cache
                let score = self.compute_trust_score(peer_did).await;
                self.trust_score_cache
                    .insert(peer_did.to_string(), (score, now));
                score
            };

        // Scale probability: lower trust = higher probability
        // Formula: base * (1 + (1 - trust_score))
        // - Trust 1.0 -> multiplier 1.0
        // - Trust 0.5 -> multiplier 1.5
        // - Trust 0.0 -> multiplier 2.0
        let multiplier = 1.0 + (1.0 - trust_score.clamp(0.0, 1.0));
        let adjusted = base * multiplier;

        // Cap at 1.0 to ensure valid probability
        adjusted.min(1.0)
    }

    /// Compute trust score for a peer (expensive operation - use caching)
    async fn compute_trust_score(&self, peer_did: &str) -> f64 {
        if let Ok(did) = Did::from_str(peer_did) {
            let trust_graph = self.trust_graph.read().await;
            trust_graph.compute_trust_score(&did).unwrap_or(0.0)
        } else {
            0.0 // Unknown peer, treat as untrusted
        }
    }

    /// Load all persisted pending challenges from the store
    ///
    /// Called on scheduler startup to restore in-flight challenges.
    pub fn load_pending_challenges(&mut self) -> Result<usize> {
        let entries = self.store.scan(PENDING_CHALLENGE_PREFIX)?;
        let mut loaded = 0;

        for (key, value) in entries {
            // Extract challenge ID from key
            if key.len() != PENDING_CHALLENGE_PREFIX.len() + 32 {
                debug!("Skipping malformed pending challenge key");
                continue;
            }

            let challenge_id: [u8; 32] = key[PENDING_CHALLENGE_PREFIX.len()..]
                .try_into()
                .unwrap_or([0u8; 32]);

            match serde_json::from_slice::<PersistedPendingChallenge>(&value) {
                Ok(persisted) => {
                    // Convert to in-memory format (returns None if expired)
                    match persisted.into_pending() {
                        Some(pending) => {
                            self.pending.insert(challenge_id, pending);
                            loaded += 1;
                        }
                        None => {
                            // Challenge expired during downtime
                            debug!(
                                "Removing expired persisted challenge {}",
                                hex::encode(&challenge_id[..8])
                            );
                            self.remove_persisted_challenge(&challenge_id);
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to deserialize pending challenge {}: {}",
                        hex::encode(&challenge_id[..8]),
                        e
                    );
                    // Remove corrupted entry
                    self.remove_persisted_challenge(&challenge_id);
                }
            }
        }

        if loaded > 0 {
            info!("Loaded {} pending challenges from storage", loaded);
        }

        Ok(loaded)
    }

    /// Spawn the scheduler as a background task
    pub fn spawn(
        own_did: Did,
        keypair: Arc<KeyPair>,
        config: ChallengeConfig,
        store: Arc<dyn Store>,
        trust_graph: Arc<RwLock<TrustGraph>>,
        gossip: Arc<RwLock<GossipActor>>,
        misbehavior: Arc<RwLock<MisbehaviorDetector>>,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> ChallengeSchedulerHandle {
        let mut scheduler = Self::new(
            own_did,
            keypair,
            config.clone(),
            store,
            trust_graph,
            gossip,
            misbehavior,
        );

        // Load any persisted pending challenges from previous run
        if let Err(e) = scheduler.load_pending_challenges() {
            warn!("Failed to load pending challenges: {}", e);
        }

        let handle = Arc::new(RwLock::new(scheduler));

        let handle_clone = handle.clone();
        let base_interval = Duration::from_secs(config.base_interval_secs);
        let jitter_max = config.jitter_secs;

        tokio::spawn(async move {
            info!(
                "Storage challenge scheduler starting (interval: {:?})",
                base_interval
            );

            loop {
                // Add jitter to prevent thundering herd
                let jitter = if jitter_max > 0 {
                    Duration::from_secs(rand::random::<u64>() % jitter_max)
                } else {
                    Duration::ZERO
                };
                let wait_duration = base_interval + jitter;

                tokio::select! {
                    _ = tokio::time::sleep(wait_duration) => {
                        let mut scheduler = handle_clone.write().await;

                        // First check for expired challenges
                        if let Err(e) = scheduler.process_expired_challenges().await {
                            warn!("Error processing expired challenges: {}", e);
                        }

                        // Then run a challenge round
                        if let Err(e) = scheduler.run_challenge_round().await {
                            warn!("Challenge round error: {}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Storage challenge scheduler received shutdown signal");
                        break;
                    }
                }
            }

            info!("Storage challenge scheduler stopped");
        });

        ChallengeSchedulerHandle { inner: handle }
    }

    /// Run a single challenge round
    async fn run_challenge_round(&mut self) -> Result<()> {
        debug!("Running storage challenge round");
        let start = Instant::now();

        // Get all content with replicas
        let content_hashes = self.store.list_replica_hashes()?;

        if content_hashes.is_empty() {
            debug!("No replicated content to challenge");
            return Ok(());
        }

        let mut challenges_sent = 0;
        let mut replicas_checked = 0;

        for content_hash in &content_hashes {
            // Get replica metadata
            let metadata = match self.store.get_replica_metadata(content_hash)? {
                Some(m) => m,
                None => continue,
            };

            // Challenge each replica based on probability
            for replica in &metadata.replicas {
                replicas_checked += 1;

                // Skip if we have too many pending challenges globally
                if self.pending.len() >= self.config.max_pending_total {
                    debug!(
                        "Max total pending challenges reached ({})",
                        self.config.max_pending_total
                    );
                    break; // Stop this round entirely
                }

                // Skip if we have too many pending challenges for this node
                let pending_for_node = self
                    .pending
                    .values()
                    .filter(|p| p.challenge.target_peer == replica.peer_did)
                    .count();

                if pending_for_node >= self.config.max_pending_per_node {
                    debug!("Max pending challenges reached for {}", replica.peer_did);
                    continue;
                }

                // Probabilistic challenge with trust-adjusted frequency
                // Lower trust peers are challenged more frequently
                let adjusted_probability = self.trust_adjusted_probability(&replica.peer_did).await;
                let roll: f64 = rand::random();
                if roll > adjusted_probability {
                    continue;
                }

                // Don't challenge ourselves
                if replica.peer_did == self.own_did.to_string() {
                    continue;
                }

                // Generate and send challenge
                if let Err(e) = self.send_challenge(content_hash, &replica.peer_did).await {
                    warn!(
                        "Failed to send challenge for {} to {}: {}",
                        hex::encode(content_hash),
                        replica.peer_did,
                        e
                    );
                } else {
                    challenges_sent += 1;
                }
            }
        }

        // Emit metrics
        icn_obs::metrics::storage_challenge::replicas_checked_set(replicas_checked);
        icn_obs::metrics::storage_challenge::pending_set(self.pending.len());
        icn_obs::metrics::storage_challenge::round_duration_record(start.elapsed().as_secs_f64());

        info!(
            "Challenge round complete: {} challenges sent, {} replicas checked (duration: {:?})",
            challenges_sent,
            replicas_checked,
            start.elapsed()
        );

        Ok(())
    }

    /// Send a storage challenge to a replica holder
    async fn send_challenge(
        &mut self,
        content_hash: &ContentHash,
        target_peer: &str,
    ) -> Result<()> {
        // Get the content to compute expected values
        let content = self.store.get(content_hash)?;
        let content = match content {
            Some(c) => c,
            None => {
                debug!(
                    "Cannot challenge {} - content not found locally",
                    hex::encode(content_hash)
                );
                return Ok(());
            }
        };

        let content_size = content.len() as u64;
        if content_size == 0 {
            return Ok(());
        }

        // Build Merkle tree to get expected root
        let tree = ContentChunkTree::new(content.clone(), self.config.chunk_size);
        let expected_merkle_root = tree.root();

        // Random byte offset (using gen_range to avoid modulo bias)
        let byte_offset = rand::thread_rng().gen_range(0..content_size);
        // Compute byte length: min of configured sample size and remaining content
        let byte_length = self
            .config
            .byte_sample_size
            .min((content_size - byte_offset) as u32);

        // Generate random chunk indices (v2 multi-block challenges)
        // Uses rand::seq::index::sample for O(k) time complexity where k = blocks_to_challenge
        let num_chunks = tree.num_chunks();
        if num_chunks == 0 {
            // Edge case: empty or invalid content tree - cannot challenge
            debug!(
                "Cannot challenge {} - content has no chunks",
                hex::encode(content_hash)
            );
            return Ok(());
        }

        let blocks_to_challenge = self.config.blocks_per_challenge.min(num_chunks);
        let chunk_indices: Vec<u32> = {
            use rand::seq::index::sample;
            let mut rng = rand::thread_rng();
            sample(&mut rng, num_chunks as usize, blocks_to_challenge as usize)
                .into_iter()
                .map(|i| i as u32)
                .collect()
        };

        // Compute expected byte hash
        let expected_bytes = tree.get_bytes(byte_offset, byte_length).unwrap_or_default();
        let expected_byte_hash: [u8; 32] = Sha256::digest(&expected_bytes).into();

        // Get failure count for this replica
        let failure_count = self
            .failure_counts
            .get(&(*content_hash, target_peer.to_string()))
            .map(|(count, _)| *count)
            .unwrap_or(0);

        // Create and sign the challenge (v2 multi-block)
        let mut challenge = StorageChallenge::new_multi_block(
            *content_hash,
            target_peer.to_string(),
            byte_offset,
            byte_length,
            chunk_indices,
            self.own_did.to_string(),
            self.config.timeout_secs,
        );

        // Sign the challenge with our keypair (required for verification by recipient)
        challenge.sign(&self.keypair)?;

        // Track pending challenge
        let pending = PendingChallenge {
            challenge: challenge.clone(),
            sent_at: Instant::now(),
            expected_merkle_root,
            expected_byte_hash,
            failure_count,
        };

        let challenge_id = challenge.id;
        self.pending.insert(challenge_id, pending);

        // Persist challenge for crash recovery
        if let Some(p) = self.pending.get(&challenge_id) {
            self.persist_pending_challenge(&challenge_id, p);
        }

        // Send via gossip
        let target_did = Did::from_str(target_peer)?;
        {
            let gossip = self.gossip.read().await;
            gossip.send_storage_challenge(target_did, challenge.clone());
        }

        // Emit metric
        icn_obs::metrics::storage_challenge::sent_inc();

        debug!(
            "Sent storage challenge {} to {} for content {}",
            hex::encode(&challenge.id[..8]),
            target_peer,
            hex::encode(content_hash)
        );

        Ok(())
    }

    /// Process expired challenges and record violations
    async fn process_expired_challenges(&mut self) -> Result<()> {
        let timeout = Duration::from_secs(self.config.timeout_secs);
        let now = Instant::now();

        // Find expired challenges
        let expired: Vec<[u8; 32]> = self
            .pending
            .iter()
            .filter(|(_, p)| now.duration_since(p.sent_at) > timeout)
            .map(|(id, _)| *id)
            .collect();

        for challenge_id in expired {
            if let Some(pending) = self.pending.remove(&challenge_id) {
                // Remove persisted challenge
                self.remove_persisted_challenge(&challenge_id);

                let key = (
                    pending.challenge.content_hash,
                    pending.challenge.target_peer.clone(),
                );

                // Increment failure count with timestamp
                let entry = self.failure_counts.entry(key.clone()).or_insert((0, now));
                entry.0 += 1;
                entry.1 = now;
                let count = entry.0;

                // Emit timeout metric
                icn_obs::metrics::storage_challenge::timeouts_inc();

                warn!(
                    "Storage challenge {} timed out for {} (failure #{}/{})",
                    hex::encode(&challenge_id[..8]),
                    pending.challenge.target_peer,
                    count,
                    self.config.grace_attempts
                );

                // Record violation if past grace period
                if count > self.config.grace_attempts {
                    let violation = Violation::FailedStorageChallenge {
                        content_hash: pending.challenge.content_hash,
                        challenge_id,
                        reason: StorageFailureReason::NoResponse,
                    };

                    if let Ok(peer_did) = Did::from_str(&pending.challenge.target_peer) {
                        let mut detector = self.misbehavior.write().await;
                        // Evidence: serialized challenge ID
                        let evidence = challenge_id.to_vec();
                        detector.record_violation(&peer_did, violation, evidence);

                        // Emit violation metric
                        icn_obs::metrics::storage_challenge::violations_inc("no_response");

                        info!(
                            "Recorded NoResponse violation for {} (content: {})",
                            pending.challenge.target_peer,
                            hex::encode(pending.challenge.content_hash)
                        );
                    }
                }
            }
        }

        // Clean up stale failure count entries
        let max_age = Duration::from_secs(FAILURE_COUNT_MAX_AGE_SECS);
        self.failure_counts
            .retain(|_, (_, last_updated)| now.duration_since(*last_updated) < max_age);

        Ok(())
    }

    /// Handle a received storage proof (called from gossip handler)
    pub async fn handle_proof(&mut self, proof: icn_store::StorageProof) -> Result<()> {
        // Validate proof protocol version (reject unsupported versions early)
        if let Err(e) = proof.validate_version() {
            warn!(
                "Rejecting proof with unsupported version {}: {}",
                proof.version, e
            );
            return Err(e);
        }

        // Find the pending challenge
        let pending = match self.pending.remove(&proof.challenge_id) {
            Some(p) => {
                // Remove persisted challenge
                self.remove_persisted_challenge(&proof.challenge_id);
                p
            }
            None => {
                debug!(
                    "Received proof for unknown challenge {}",
                    hex::encode(&proof.challenge_id[..8])
                );
                return Ok(());
            }
        };

        // Check if proof was generated before challenge expired (prevents race condition
        // where proof arrives after timeout but was generated in time)
        if proof.generated_at > pending.challenge.expires_at {
            warn!(
                "Proof generated after challenge expired: {} > {}",
                proof.generated_at, pending.challenge.expires_at
            );
            return self
                .record_failure(&pending, StorageFailureReason::Expired)
                .await;
        }

        // Verify prover matches target
        if proof.prover != pending.challenge.target_peer {
            warn!(
                "Proof prover {} doesn't match challenge target {}",
                proof.prover, pending.challenge.target_peer
            );
            return self
                .record_failure(&pending, StorageFailureReason::ChallengeMismatch)
                .await;
        }

        // Verify proof signature (required - prevents impersonation)
        if let Err(e) = proof.verify_signature() {
            warn!(
                "Proof signature verification failed for challenge {}: {}",
                hex::encode(&proof.challenge_id[..8]),
                e
            );
            return self
                .record_failure(&pending, StorageFailureReason::InvalidSignature)
                .await;
        }

        // Verify byte data hash
        let byte_hash: [u8; 32] = sha2::Sha256::digest(&proof.byte_data).into();
        if byte_hash != pending.expected_byte_hash {
            warn!(
                "Proof byte hash mismatch for challenge {}",
                hex::encode(&proof.challenge_id[..8])
            );
            return self
                .record_failure(&pending, StorageFailureReason::DataMismatch)
                .await;
        }

        // Verify all Merkle proofs (v2: one per challenged chunk)
        // All challenged chunk indices must have corresponding valid proofs
        for expected_idx in &pending.challenge.chunk_indices {
            // Find the proof for this chunk index
            let proof_data = proof
                .merkle_proofs
                .iter()
                .find(|p| p.chunk_index == *expected_idx);

            match proof_data {
                Some(p) => {
                    if !p.verify_against_root(&pending.expected_merkle_root) {
                        warn!(
                            "Invalid Merkle proof for chunk {} in challenge {}",
                            expected_idx,
                            hex::encode(&proof.challenge_id[..8])
                        );
                        return self
                            .record_failure(&pending, StorageFailureReason::InvalidMerkleProof)
                            .await;
                    }
                }
                None => {
                    warn!(
                        "Missing Merkle proof for chunk {} in challenge {}",
                        expected_idx,
                        hex::encode(&proof.challenge_id[..8])
                    );
                    return self
                        .record_failure(&pending, StorageFailureReason::InvalidMerkleProof)
                        .await;
                }
            }
        }

        // Success! Reset failure count
        let key = (
            pending.challenge.content_hash,
            pending.challenge.target_peer.clone(),
        );
        self.failure_counts.remove(&key);

        // Update replica metadata with verification timestamp
        if let Ok(Some(mut metadata)) = self
            .store
            .get_replica_metadata(&pending.challenge.content_hash)
        {
            metadata.record_verification(&pending.challenge.target_peer);
            let _ = self.store.put_replica_metadata(&metadata);
        }

        // Emit success metric
        icn_obs::metrics::storage_challenge::proofs_verified_inc();

        info!(
            "Verified storage proof from {} for content {}",
            pending.challenge.target_peer,
            hex::encode(pending.challenge.content_hash)
        );

        Ok(())
    }

    /// Record a challenge failure
    async fn record_failure(
        &mut self,
        pending: &PendingChallenge,
        reason: StorageFailureReason,
    ) -> Result<()> {
        let key = (
            pending.challenge.content_hash,
            pending.challenge.target_peer.clone(),
        );

        // Increment failure count with timestamp
        let now = Instant::now();
        let entry = self.failure_counts.entry(key).or_insert((0, now));
        entry.0 += 1;
        entry.1 = now;
        let count = entry.0;

        // Emit failure metric
        let reason_str = match reason {
            StorageFailureReason::NoResponse => "no_response",
            StorageFailureReason::DataMismatch => "data_mismatch",
            StorageFailureReason::InvalidMerkleProof => "invalid_merkle_proof",
            StorageFailureReason::ContentNotFound => "content_not_found",
            StorageFailureReason::InvalidSignature => "invalid_signature",
            StorageFailureReason::ChallengeMismatch => "challenge_mismatch",
            StorageFailureReason::Expired => "expired",
        };
        icn_obs::metrics::storage_challenge::failures_inc(reason_str);

        // Record violation if past grace period
        if count > self.config.grace_attempts {
            let violation = Violation::FailedStorageChallenge {
                content_hash: pending.challenge.content_hash,
                challenge_id: pending.challenge.id,
                reason: reason.clone(),
            };

            if let Ok(peer_did) = Did::from_str(&pending.challenge.target_peer) {
                let mut detector = self.misbehavior.write().await;
                // Evidence: serialized challenge ID
                let evidence = pending.challenge.id.to_vec();
                detector.record_violation(&peer_did, violation, evidence);

                // Emit violation metric
                icn_obs::metrics::storage_challenge::violations_inc(reason_str);

                info!(
                    "Recorded {:?} violation for {} (content: {})",
                    reason,
                    pending.challenge.target_peer,
                    hex::encode(pending.challenge.content_hash)
                );
            }

            // Mark replica as unhealthy in metadata
            if let Ok(Some(mut metadata)) = self
                .store
                .get_replica_metadata(&pending.challenge.content_hash)
            {
                metadata.mark_replica_unhealthy(&pending.challenge.target_peer, count);
                let _ = self.store.put_replica_metadata(&metadata);

                // Trigger re-replication callback if configured
                if let Some(ref callback) = self.re_replication_callback {
                    if let Err(e) = callback(
                        pending.challenge.content_hash,
                        pending.challenge.target_peer.clone(),
                        count,
                    ) {
                        warn!(
                            "Re-replication callback failed for content {}: {}",
                            hex::encode(pending.challenge.content_hash),
                            e
                        );
                    } else {
                        info!(
                            "Triggered re-replication for content {} (unhealthy replica: {})",
                            hex::encode(pending.challenge.content_hash),
                            pending.challenge.target_peer
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Get pending challenge count
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Get configuration
    pub fn config(&self) -> &ChallengeConfig {
        &self.config
    }
}

/// Handle for interacting with the challenge scheduler
#[derive(Clone)]
pub struct ChallengeSchedulerHandle {
    inner: Arc<RwLock<ChallengeScheduler>>,
}

impl ChallengeSchedulerHandle {
    /// Handle a received storage proof
    pub async fn handle_proof(&self, proof: icn_store::StorageProof) -> Result<()> {
        self.inner.write().await.handle_proof(proof).await
    }

    /// Get pending challenge count
    pub async fn pending_count(&self) -> usize {
        self.inner.read().await.pending_count()
    }

    /// Get current configuration
    pub async fn config(&self) -> ChallengeConfig {
        self.inner.read().await.config().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_challenge_config_defaults() {
        let config = ChallengeConfig::default();
        assert_eq!(config.base_interval_secs, 3600);
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.grace_attempts, 3);
        assert!(config.challenge_probability > 0.0);
    }
}
