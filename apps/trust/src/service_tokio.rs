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
use icn_kernel_api::services::{TrustEvent, TrustService};
use icn_trust::TrustGraph;

/// Maximum reputation score delta per single event (25%).
/// Applied as a penalty for ProtocolViolation and as a boost for PositiveInteraction.
const EVENT_SCORE_DELTA: f64 = 0.25;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::oracle_tokio::TrustPolicyOracleTokio;

/// Trust service implementation for tokio locks.
///
/// This wraps TrustGraph with `tokio::sync::RwLock` for compatibility
/// with icn-core's async lock usage.
pub struct TrustServiceImplTokio {
    graph: Arc<RwLock<TrustGraph>>,
    oracle: Arc<TrustPolicyOracleTokio>,
    own_did: icn_identity::Did,
}

impl TrustServiceImplTokio {
    /// Create a new trust service with the given TrustGraph
    pub fn new(graph: Arc<RwLock<TrustGraph>>) -> Self {
        let own_did = {
            let rt = tokio::runtime::Handle::current();
            tokio::task::block_in_place(|| {
                rt.block_on(async { graph.read().await.own_did().clone() })
            })
        };
        let oracle = Arc::new(TrustPolicyOracleTokio::new(graph.clone()));
        Self {
            graph,
            oracle,
            own_did,
        }
    }

    /// Get direct access to the TrustGraph
    ///
    /// This is for use by other domain apps that need TrustGraph access.
    /// Kernel code should NOT use this - use the TrustService trait instead.
    pub fn graph(&self) -> &Arc<RwLock<TrustGraph>> {
        &self.graph
    }
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
                // Convert from kernel Did to identity Did using FromStr
                if let Ok(identity_did) = actor.to_string().parse::<icn_identity::Did>() {
                    graph.compute_trust_score(&identity_did).unwrap_or(0.0)
                } else {
                    0.0
                }
            })
        })
    }

    fn record_event(&self, actor: &icn_kernel_api::types::Did, event: TrustEvent) {
        let identity_did = match actor.to_string().parse::<icn_identity::Did>() {
            Ok(did) => did,
            Err(_) => {
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
                            match graph.compute_trust_score(&identity_did) {
                                Ok(score) => score,
                                Err(_) => {
                                    // Unknown actors start at 0.0 trust — penalty
                                    // still creates a trust edge recording the violation.
                                    0.0
                                }
                            }
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

        // Verify signature
        if let Err(e) = attestation.verify() {
            tracing::warn!(
                source = %source,
                "Rejecting trust attestation with invalid signature: {} -> {} (error: {})",
                attestation.issuer, attestation.subject, e
            );
            return Ok(()); // Silently reject invalid signatures
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
                // Return serialized attestation for gossip broadcast
                let attestation = icn_trust::TrustAttestation::from_trust_edge(&edge);
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
                // Return empty bytes (no gossip message for revocation currently)
                Ok(Vec::new())
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
                    Ok(edges) => edges
                        .into_iter()
                        .filter_map(|e| serde_json::to_value(&e).ok())
                        .collect(),
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
                                for edge in edges {
                                    if let Ok(val) = serde_json::to_value(&edge) {
                                        all_edges.push(val);
                                    }
                                }
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
    use icn_kernel_api::services::TrustService as _;

    fn create_test_graph() -> (Arc<RwLock<TrustGraph>>, icn_identity::Did) {
        let store = icn_store::SledStore::temporary().unwrap();
        let store: Arc<dyn icn_store::Store> = Arc::new(store);

        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        (
            Arc::new(RwLock::new(TrustGraph::new(store, did.clone()))),
            did,
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_service_tokio_creation() {
        let (graph, _did) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph);

        // Should have zero trust for unknown actors
        let unknown_keypair = icn_identity::KeyPair::generate().unwrap();
        let unknown_did = icn_kernel_api::types::Did::from(unknown_keypair.did().to_string());
        let score = service.trust_score(&unknown_did);
        assert!(score >= 0.0 && score <= 1.0);

        // Should return an oracle
        let _oracle = service.oracle();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_trust_score_unknown_actor() {
        let (graph, _did) = create_test_graph();
        let service = TrustServiceImplTokio::new(graph);

        let unknown_keypair = icn_identity::KeyPair::generate().unwrap();
        let unknown_did = icn_kernel_api::types::Did::from(unknown_keypair.did().to_string());

        // Unknown actors should get 0.0 trust
        let score = service.trust_score(&unknown_did);
        assert_eq!(score, 0.0);
    }
}
