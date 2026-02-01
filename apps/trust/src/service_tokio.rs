//! Tokio-compatible Trust Service Implementation
//!
//! Implements the `TrustService` trait from icn-kernel-api using
//! `tokio::sync::RwLock` for compatibility with icn-core.
//!
//! # Note
//!
//! Since `TrustService::trust_score()` is synchronous, this implementation
//! uses `tokio::task::block_in_place` to safely access the tokio lock
//! from a sync context. This should be called from a multi-threaded
//! tokio runtime.

use icn_kernel_api::authz::PolicyOracle;
use icn_kernel_api::services::{TrustEvent, TrustScoreResult, TrustService};
use icn_trust::{TrustAttestation, TrustGraph, TrustRevocation};

/// Maximum reputation score delta per single event (25%).
/// Applied as a penalty for ProtocolViolation and as a boost for PositiveInteraction.
const EVENT_SCORE_DELTA: f64 = 0.25;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::oracle_tokio::TrustPolicyOracleTokio;
use crate::reducer;
use crate::sequence::SequenceTracker;

/// Trust service implementation for tokio locks.
///
/// This wraps TrustGraph with `tokio::sync::RwLock` for compatibility
/// with icn-core's async lock usage.
pub struct TrustServiceImplTokio {
    graph: Arc<RwLock<TrustGraph>>,
    oracle: Arc<TrustPolicyOracleTokio>,
    own_did: icn_identity::Did,
    /// Keypair for signing outgoing attestations.
    keypair: icn_identity::KeyPair,
    /// Monotonic epoch counter — incremented on each state mutation
    /// (new edge, removed edge, trust event). Used by `TrustScoreResult`
    /// so caches can detect when scores may have changed.
    epoch: Arc<AtomicU64>,
    /// Sequence tracker for replay protection
    sequence_tracker: Arc<RwLock<SequenceTracker>>,
}

impl TrustServiceImplTokio {
    /// Create a new trust service with the given TrustGraph and keypair.
    ///
    /// The keypair is used to sign outgoing attestations before gossip broadcast.
    /// The store is used for sequence number tracking.
    pub fn new(
        graph: Arc<RwLock<TrustGraph>>,
        keypair: icn_identity::KeyPair,
        store: Arc<dyn icn_store::Store>,
    ) -> Self {
        let own_did = keypair.did().clone();
        let oracle = Arc::new(TrustPolicyOracleTokio::new(graph.clone()));
        let sequence_tracker = Arc::new(RwLock::new(SequenceTracker::new(store, own_did.clone())));
        Self {
            graph,
            oracle,
            own_did,
            keypair,
            epoch: Arc::new(AtomicU64::new(0)),
            sequence_tracker,
        }
    }

    /// Get the current epoch (for testing/diagnostics)
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Relaxed)
    }

    /// Increment the epoch counter (called on state mutations)
    fn bump_epoch(&self) {
        self.epoch.fetch_add(1, Ordering::Relaxed);
    }

    /// Return a zero-valued TrustScoreResult (for error/unknown cases).
    ///
    /// Falls back to `computed_at = 0` if the system clock is before Unix epoch,
    /// which would indicate a misconfigured system clock.
    fn empty_score_result(&self) -> TrustScoreResult {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0); // Pre-epoch clock → 0 (misconfigured system)
        TrustScoreResult {
            score: 0.0,
            epoch: self.epoch.load(Ordering::Relaxed),
            computed_at: now,
            input_count: 0,
            inputs_hash: [0u8; 32],
            reducer_version: reducer::REDUCER_VERSION.to_string(),
        }
    }

    /// Parse a kernel-api DID into an identity DID.
    fn parse_kernel_did(did: &icn_kernel_api::types::Did) -> Option<icn_identity::Did> {
        did.to_string().parse().ok()
    }

    /// Get direct access to the TrustGraph
    ///
    /// This is for use by other domain apps that need TrustGraph access.
    /// Kernel code should NOT use this - use the TrustService trait instead.
    pub fn graph(&self) -> &Arc<RwLock<TrustGraph>> {
        &self.graph
    }
}

/// Convert a TrustEdge to the RPC-facing { target_did, score, labels } shape.
fn edge_to_rpc_value(edge: icn_trust::TrustEdge) -> serde_json::Value {
    serde_json::json!({
        "target_did": edge.target.to_string(),
        "score": edge.score.value(),
        "labels": edge.labels,
    })
}

impl TrustService for TrustServiceImplTokio {
    fn oracle(&self) -> Arc<dyn PolicyOracle> {
        self.oracle.clone()
    }

    fn trust_score(&self, actor: &icn_kernel_api::types::Did) -> f64 {
        // Use block_in_place to safely access tokio lock from sync context.
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let graph = self.graph.read().await;
                if let Some(identity_did) = Self::parse_kernel_did(actor) {
                    graph.compute_trust_score(&identity_did).unwrap_or(0.0) // Unknown actors start at zero trust
                } else {
                    0.0
                }
            })
        })
    }

    // TODO(#1000): Refactor to use `AttestationReducer` once the compute handler
    // pipeline converts TrustEdges to TrustAttestations. Currently the reducer is
    // used by the compute handler path; this method reimplements hash logic directly
    // from the graph's edge storage for the service query path.
    fn trust_score_detailed(&self, actor: &icn_kernel_api::types::Did) -> TrustScoreResult {
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let graph = self.graph.read().await;
                let identity_did = match Self::parse_kernel_did(actor) {
                    Some(d) => d,
                    None => return self.empty_score_result(),
                };

                let score = graph.compute_trust_score(&identity_did).unwrap_or(0.0); // Unknown actors start at zero trust

                // Collect edges pointing to this actor to derive input count + hash
                let input_edges = graph
                    .get_all_known_dids()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|did| {
                        graph
                            .get_outgoing_edges(&did)
                            .ok()
                            .and_then(|edges| edges.into_iter().find(|e| e.target == identity_did))
                    })
                    .collect::<Vec<_>>();

                let input_count = input_edges.len() as u32;

                // Compute deterministic hash over the input edges.
                //
                // NOTE: This hashes TrustEdges (storage format) while AttestationReducer
                // hashes TrustAttestations (wire format). These two hash methods serve
                // different purposes and should not be compared:
                // - This method: live score provenance from the graph's current state
                // - AttestationReducer: pure scoring from a set of attestations
                // When the reducer is integrated into this method (see TODO below),
                // both paths will use the same attestation-based hashing.
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(reducer::REDUCER_VERSION.as_bytes());
                // Sort by source DID for determinism
                let mut sorted = input_edges;
                sorted.sort_by(|a, b| a.source.to_string().cmp(&b.source.to_string()));
                for edge in &sorted {
                    hasher.update(edge.source.as_str().as_bytes());
                    hasher.update(edge.target.as_str().as_bytes());
                    hasher.update(edge.score.value().to_le_bytes());
                    hasher.update(edge.created_at.to_le_bytes());
                    hasher.update(edge.graph_type.as_str().as_bytes());
                }
                let inputs_hash: [u8; 32] = hasher.finalize().into();

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                TrustScoreResult {
                    score,
                    epoch: self.epoch.load(Ordering::Relaxed),
                    computed_at: now,
                    input_count,
                    inputs_hash,
                    reducer_version: reducer::REDUCER_VERSION.to_string(),
                }
            })
        })
    }

    fn record_event(&self, actor: &icn_kernel_api::types::Did, event: TrustEvent) {
        let identity_did = match Self::parse_kernel_did(actor) {
            Some(did) => did,
            None => {
                tracing::warn!(actor = %actor, "Invalid DID format, ignoring trust event");
                return;
            }
        };

        match event {
            TrustEvent::ProtocolViolation { severity, category } => {
                tracing::warn!(
                    actor = %actor,
                    severity = severity,
                    category = %category,
                    "Trust event: protocol violation"
                );
                let penalty = severity * EVENT_SCORE_DELTA;
                let own = self.own_did.clone();

                tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let current = {
                            let graph = self.graph.read().await;
                            // Unknown actors start at 0.0 trust — penalty
                            // still creates a trust edge recording the violation.
                            graph.compute_trust_score(&identity_did).unwrap_or(0.0)
                        };
                        let new_score = (current - penalty).max(0.0);
                        debug_assert!(
                            (0.0..=1.0).contains(&new_score),
                            "Trust score out of bounds: {new_score}"
                        );
                        let trust_score = icn_trust::TrustScore::unchecked(new_score);
                        // Uses default Social graph type — misbehavior events affect social
                        // trust rather than TechnicalReliability, which tracks uptime/latency.
                        let edge =
                            icn_trust::TrustEdge::new(own, identity_did.clone(), trust_score);
                        let mut graph = self.graph.write().await;
                        if let Err(e) = graph.add_edge(edge) {
                            tracing::warn!(
                                actor = %actor,
                                "Failed to persist trust penalty: {}",
                                e
                            );
                        } else {
                            self.bump_epoch();
                            tracing::debug!(
                                actor = %actor,
                                current = current,
                                penalty = penalty,
                                new_score = new_score,
                                "Trust penalty persisted via TrustEdge"
                            );
                        }
                    })
                });
            }
            TrustEvent::PositiveInteraction { weight } => {
                tracing::debug!(
                    actor = %actor,
                    weight = weight,
                    "Trust event: positive interaction"
                );
                // Boost trust by adding an edge with improved score
                let own = self.own_did.clone();

                tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let current = {
                            let graph = self.graph.read().await;
                            graph.compute_trust_score(&identity_did).unwrap_or(0.0)
                        };
                        let new_score = (current + weight * EVENT_SCORE_DELTA).min(1.0);
                        debug_assert!(
                            (0.0..=1.0).contains(&new_score),
                            "Trust score out of bounds: {new_score}"
                        );
                        let trust_score = icn_trust::TrustScore::unchecked(new_score);
                        let edge =
                            icn_trust::TrustEdge::new(own, identity_did.clone(), trust_score);
                        let mut graph = self.graph.write().await;
                        if let Err(e) = graph.add_edge(edge) {
                            tracing::warn!(
                                actor = %actor,
                                "Failed to persist trust boost: {}",
                                e
                            );
                        } else {
                            self.bump_epoch();
                            tracing::debug!(
                                actor = %actor,
                                new_score = new_score,
                                "Trust boost persisted via TrustEdge"
                            );
                        }
                    })
                });
            }
            TrustEvent::QuarantineRequested { duration_secs } => {
                tracing::warn!(
                    actor = %actor,
                    duration_secs = duration_secs,
                    "Trust event: quarantine requested"
                );
            }
        }
    }

    fn ingest_attestation(
        &self,
        bytes: &[u8],
        source: &icn_kernel_api::types::Did,
    ) -> Result<(), String> {
        use icn_trust::TrustAttestation;

        // Deserialize
        let attestation: TrustAttestation =
            serde_json::from_slice(bytes).map_err(|e| format!("Invalid attestation: {e}"))?;

        // Verify signature — reject unsigned or invalid attestations
        if let Err(e) = attestation.verify() {
            tracing::warn!(
                source = %source,
                "Rejecting trust attestation with invalid signature: {} -> {} (error: {})",
                attestation.issuer, attestation.subject, e
            );
            return Err(format!(
                "Invalid attestation signature from {} -> {} (envelope source: {}): {e}",
                attestation.issuer, attestation.subject, source
            ));
        }

        // Check if expired
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if attestation.is_expired(now) {
            tracing::warn!(
                source = %source,
                "Received expired trust attestation: {} -> {}",
                attestation.issuer, attestation.subject,
            );
            return Ok(()); // Silently reject expired attestations
        }

        // Validate sequence number for replay protection
        // Attestations with sequence > 0 must have a monotonically increasing sequence
        if attestation.sequence > 0 {
            let seq_valid = tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let tracker = self.sequence_tracker.read().await;
                    tracker
                        .validate_sequence(&attestation.issuer, attestation.sequence)
                        .await
                })
            });
            if let Err(e) = seq_valid {
                tracing::warn!(
                    source = %source,
                    "Rejecting replayed trust attestation: {} -> {} ({})",
                    attestation.issuer, attestation.subject, e
                );
                return Err(format!(
                    "Replay detected for attestation {} -> {}: {e}",
                    attestation.issuer, attestation.subject
                ));
            }
        }

        // Save sequence info before converting (attestation is consumed)
        let att_issuer = attestation.issuer.clone();
        let att_sequence = attestation.sequence;

        // Convert to trust edge
        let edge = attestation.to_trust_edge();

        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut graph = self.graph.write().await;

                // Check if we already have this edge — supersede check
                match graph.get_edge(&edge.source, &edge.target) {
                    Ok(Some(existing)) => {
                        if !attestation
                            .should_supersede(existing.created_at, existing.score.value())
                        {
                            tracing::debug!(
                                "Rejecting outdated trust attestation: {} -> {}",
                                edge.source,
                                edge.target,
                            );
                            return Ok(());
                        }
                    }
                    Ok(None) => { /* new edge */ }
                    Err(e) => {
                        tracing::warn!("Edge lookup error during attestation: {e}");
                    }
                }

                graph.add_edge(edge.clone()).map_err(|e| format!("{e}"))?;
                self.bump_epoch();

                // Update sequence tracker after successful write
                if att_sequence > 0 {
                    let tracker = self.sequence_tracker.read().await;
                    if let Err(e) = tracker.update_last_seen(&att_issuer, att_sequence).await {
                        tracing::warn!(
                            "Failed to update sequence tracker for {}: {}",
                            att_issuer,
                            e
                        );
                    }
                }

                tracing::info!(
                    "Applied remote trust attestation: {} -> {} (score: {})",
                    edge.source,
                    edge.target,
                    edge.score,
                );

                // If this attestation is about us, log it specially
                if edge.target == self.own_did {
                    tracing::info!("Received trust from {}: score {}", edge.source, edge.score,);
                }

                Ok(())
            })
        })
    }

    fn recover_identity(
        &self,
        old_did: &icn_kernel_api::types::Did,
        new_did: &icn_kernel_api::types::Did,
    ) -> Result<usize, String> {
        let old: icn_identity::Did = old_did
            .parse()
            .map_err(|e| format!("Invalid old DID: {e}"))?;
        let new: icn_identity::Did = new_did
            .parse()
            .map_err(|e| format!("Invalid new DID: {e}"))?;

        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut graph = self.graph.write().await;
                graph
                    .map_did_recovery(&old, &new)
                    .map_err(|e| format!("{e}"))
            })
        })
    }

    /// Submit a trust attestation and return Ed25519-signed bytes for gossip broadcast.
    ///
    /// The attestation is signed with the node's keypair before serialization.
    /// Receiving nodes will verify the signature via `ingest_attestation()` before
    /// applying to their trust graph — unsigned or tampered attestations are rejected.
    ///
    /// This implementation automatically assigns a sequence number for replay protection.
    fn submit_attestation(
        &self,
        target: &icn_kernel_api::types::Did,
        score: f64,
        labels: Vec<String>,
    ) -> Result<Vec<u8>, String> {
        let target_did: icn_identity::Did = target
            .parse()
            .map_err(|e| format!("Invalid target DID: {e}"))?;

        let trust_score =
            icn_trust::TrustScore::new(score).map_err(|e| format!("Invalid trust score: {e}"))?;
        let mut edge = icn_trust::TrustEdge::new(self.own_did.clone(), target_did, trust_score);
        for label in labels {
            edge = edge.with_label(label);
        }

        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut graph = self.graph.write().await;
                graph.add_edge(edge.clone()).map_err(|e| format!("{e}"))?;
                self.bump_epoch();

                // Get next sequence number
                let sequence = self
                    .sequence_tracker
                    .write()
                    .await
                    .next_issuer_sequence()
                    .await
                    .map_err(|e| format!("Failed to get sequence: {e}"))?;

                // Create attestation with sequence
                let mut attestation =
                    TrustAttestation::from_trust_edge(&edge).with_sequence(sequence);

                // Sign attestation before gossip broadcast
                attestation
                    .sign(&self.keypair)
                    .map_err(|e| format!("Failed to sign attestation: {e}"))?;

                serde_json::to_vec(&attestation).map_err(|e| format!("{e}"))
            })
        })
    }

    fn revoke_trust(&self, target: &icn_kernel_api::types::Did) -> Result<Vec<u8>, String> {
        let target_did: icn_identity::Did = target
            .parse()
            .map_err(|e| format!("Invalid target DID: {e}"))?;

        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut graph = self.graph.write().await;
                graph
                    .remove_edge(&self.own_did, &target_did)
                    .map_err(|e| format!("{e}"))?;
                self.bump_epoch();

                // Create and sign revocation message
                let mut revocation = TrustRevocation::new(self.own_did.clone(), target_did);
                revocation
                    .sign(&self.keypair)
                    .map_err(|e| format!("Failed to sign revocation: {e}"))?;

                // Return serialized revocation for gossip broadcast
                serde_json::to_vec(&revocation).map_err(|e| format!("{e}"))
            })
        })
    }

    fn get_edges(&self, actor: &icn_kernel_api::types::Did) -> Vec<serde_json::Value> {
        let did: icn_identity::Did = match actor.parse() {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };

        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let graph = self.graph.read().await;
                match graph.get_outgoing_edges(&did) {
                    Ok(edges) => edges.into_iter().map(edge_to_rpc_value).collect(),
                    Err(_) => Vec::new(),
                }
            })
        })
    }

    fn get_all_edges(&self) -> Vec<serde_json::Value> {
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let graph = self.graph.read().await;
                // Get all known DIDs, then collect outgoing edges from each
                match graph.get_all_known_dids() {
                    Ok(dids) => {
                        let mut all_edges = Vec::new();
                        for did in dids {
                            if let Ok(edges) = graph.get_outgoing_edges(&did) {
                                all_edges.extend(edges.into_iter().map(edge_to_rpc_value));
                            }
                        }
                        all_edges
                    }
                    Err(_) => Vec::new(),
                }
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_graph() -> (
        Arc<RwLock<TrustGraph>>,
        icn_identity::KeyPair,
        Arc<dyn icn_store::Store>,
    ) {
        let store = icn_store::SledStore::temporary().unwrap();
        let store: Arc<dyn icn_store::Store> = Arc::new(store);

        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        (
            Arc::new(RwLock::new(TrustGraph::new(store.clone(), did.clone()))),
            keypair,
            store,
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_service_tokio_creation() {
        let (graph, keypair, store) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph, keypair, store);

        // Should have zero trust for unknown actors
        let unknown_keypair = icn_identity::KeyPair::generate().unwrap();
        let unknown_did = icn_kernel_api::types::Did::from(unknown_keypair.did().to_string());
        let score = service.trust_score(&unknown_did);
        assert!((0.0..=1.0).contains(&score));

        // Should return an oracle
        let _oracle = service.oracle();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_score_unknown_actor() {
        let (graph, keypair, store) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph, keypair, store);

        let unknown_keypair = icn_identity::KeyPair::generate().unwrap();
        let unknown_did = icn_kernel_api::types::Did::from(unknown_keypair.did().to_string());

        // Unknown actors should get 0.0 trust
        let score = service.trust_score(&unknown_did);
        assert_eq!(score, 0.0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_epoch_starts_at_zero() {
        let (graph, keypair, store) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph, keypair, store);
        assert_eq!(service.epoch(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_epoch_increments_on_mutations() {
        let (graph, keypair, store) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph, keypair, store);
        assert_eq!(service.epoch(), 0);

        // submit_attestation should bump epoch
        let target = icn_identity::KeyPair::generate().unwrap();
        let target_did = icn_kernel_api::types::Did::from(target.did().to_string());
        service
            .submit_attestation(&target_did, 0.5, vec![])
            .unwrap();
        assert_eq!(service.epoch(), 1);

        // record_event (ProtocolViolation) should bump epoch
        service.record_event(
            &target_did,
            TrustEvent::ProtocolViolation {
                severity: 0.5,
                category: "test".to_string(),
            },
        );
        assert_eq!(service.epoch(), 2);

        // revoke_trust should bump epoch
        service.revoke_trust(&target_did).unwrap();
        assert_eq!(service.epoch(), 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_score_detailed_returns_enriched_result() {
        let (graph, keypair, store) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph, keypair, store);

        let target = icn_identity::KeyPair::generate().unwrap();
        let target_did = icn_kernel_api::types::Did::from(target.did().to_string());

        // Unknown actor returns zero score with epoch
        let result = service.trust_score_detailed(&target_did);
        assert_eq!(result.score, 0.0);
        assert_eq!(result.epoch, 0);
        assert_eq!(result.input_count, 0);
        assert_eq!(result.reducer_version, reducer::REDUCER_VERSION);

        // After adding a trust edge, detailed result should reflect it
        service
            .submit_attestation(&target_did, 0.8, vec![])
            .unwrap();
        let result = service.trust_score_detailed(&target_did);
        assert!(result.score > 0.0, "score should be non-zero after edge");
        assert_eq!(result.epoch, 1);
        assert_eq!(result.input_count, 1);
        assert_ne!(result.inputs_hash, [0u8; 32], "hash should be non-zero");
        assert!(result.computed_at > 0, "timestamp should be set");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_score_detailed_invalid_did() {
        let (graph, keypair, store) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph, keypair, store);

        let bad_did = icn_kernel_api::types::Did::from("not-a-valid-did".to_string());
        let result = service.trust_score_detailed(&bad_did);
        assert_eq!(result.score, 0.0);
        assert_eq!(result.input_count, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_submit_attestation_is_signed() {
        let (graph, keypair, store) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph, keypair, store);

        let target = icn_identity::KeyPair::generate().unwrap();
        let target_did = icn_kernel_api::types::Did::from(target.did().to_string());

        // Submit attestation — should be signed
        let bytes = service
            .submit_attestation(&target_did, 0.7, vec!["partner".into()])
            .expect("submit_attestation should succeed");

        // Deserialize and verify signature
        let attestation: icn_trust::TrustAttestation =
            serde_json::from_slice(&bytes).expect("should deserialize");
        assert!(
            !attestation.signature.is_empty(),
            "Attestation must be signed before gossip broadcast"
        );
        attestation
            .verify()
            .expect("Attestation signature must be valid");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ingest_rejects_unsigned_attestation() {
        let (graph, keypair, store) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph, keypair, store);

        let issuer = icn_identity::KeyPair::generate().unwrap();
        let target = icn_identity::KeyPair::generate().unwrap();

        // Create an unsigned attestation
        let attestation =
            icn_trust::TrustAttestation::new(issuer.did().clone(), target.did().clone(), 0.5);
        let bytes = serde_json::to_vec(&attestation).unwrap();

        let source = icn_kernel_api::types::Did::from(issuer.did().to_string());
        let result = service.ingest_attestation(&bytes, &source);
        assert!(
            result.is_err(),
            "Unsigned attestations must be rejected, got: {:?}",
            result
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_submit_then_ingest_roundtrip() {
        // Node A submits attestation, Node B ingests it
        let (graph_a, keypair_a, store_a) = create_test_graph();
        let service_a = TrustServiceImplTokio::new(graph_a, keypair_a, store_a);

        let (graph_b, keypair_b, store_b) = create_test_graph();
        let service_b = TrustServiceImplTokio::new(graph_b, keypair_b, store_b);

        let target = icn_identity::KeyPair::generate().unwrap();
        let target_did = icn_kernel_api::types::Did::from(target.did().to_string());

        // Node A submits signed attestation
        let bytes = service_a
            .submit_attestation(&target_did, 0.8, vec!["validator".into()])
            .expect("submit should succeed");

        // Node B ingests it — should succeed because it's signed
        let source_a = icn_kernel_api::types::Did::from(service_a.own_did.to_string());
        service_b
            .ingest_attestation(&bytes, &source_a)
            .expect("ingest of signed attestation should succeed");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ingest_rejects_tampered_attestation() {
        let (graph, keypair, store) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph, keypair, store);

        let issuer = icn_identity::KeyPair::generate().unwrap();
        let target = icn_identity::KeyPair::generate().unwrap();

        // Create a properly signed attestation, then tamper with the score
        let mut attestation =
            icn_trust::TrustAttestation::new(issuer.did().clone(), target.did().clone(), 0.5);
        attestation.sign(&issuer).unwrap();
        attestation.score = 0.99; // tamper after signing

        let bytes = serde_json::to_vec(&attestation).unwrap();
        let source = icn_kernel_api::types::Did::from(issuer.did().to_string());
        let result = service.ingest_attestation(&bytes, &source);
        assert!(
            result.is_err(),
            "Tampered attestation must be rejected, got: {:?}",
            result
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ingest_rejects_expired_signed_attestation() {
        let (graph, keypair, store) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph, keypair, store);

        let issuer = icn_identity::KeyPair::generate().unwrap();
        let target = icn_identity::KeyPair::generate().unwrap();

        // Create a signed attestation that is already expired
        // Default TTL is 30 days (2,592,000 seconds)
        let mut attestation =
            icn_trust::TrustAttestation::new(issuer.did().clone(), target.did().clone(), 0.5);
        attestation.created_at = 1000; // ancient timestamp
        attestation.sign(&issuer).unwrap();

        let bytes = serde_json::to_vec(&attestation).unwrap();
        let source = icn_kernel_api::types::Did::from(issuer.did().to_string());
        // Expired attestations return Ok(()) — silently dropped, not an error
        let result = service.ingest_attestation(&bytes, &source);
        assert!(
            result.is_ok(),
            "Expired attestations are silently dropped, not errors: {:?}",
            result
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ingest_rejects_spoofed_issuer() {
        let (graph, keypair, store) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph, keypair, store);

        let alice = icn_identity::KeyPair::generate().unwrap();
        let bob = icn_identity::KeyPair::generate().unwrap();
        let target = icn_identity::KeyPair::generate().unwrap();

        // Alice signs a valid attestation, then changes issuer to Bob's DID
        // (simulating an attacker trying to impersonate Bob)
        let mut attestation =
            icn_trust::TrustAttestation::new(alice.did().clone(), target.did().clone(), 0.5);
        attestation.sign(&alice).unwrap();
        attestation.issuer = bob.did().clone(); // spoof issuer after signing

        let bytes = serde_json::to_vec(&attestation).unwrap();
        let source = icn_kernel_api::types::Did::from(alice.did().to_string());
        let result = service.ingest_attestation(&bytes, &source);
        assert!(
            result.is_err(),
            "Attestation with spoofed issuer must be rejected, got: {:?}",
            result
        );
    }
}
