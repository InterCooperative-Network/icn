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
                // Apply trust penalty based on severity (max 25% per violation)
                let penalty = severity * 0.25;
                let own = self.own_did.clone();

                tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let current = {
                            let graph = self.graph.read().await;
                            match graph.compute_trust_score(&identity_did) {
                                Ok(score) => score,
                                Err(e) => {
                                    tracing::warn!(
                                        actor = %actor,
                                        "Failed to read trust score, skipping penalty: {e}"
                                    );
                                    return;
                                }
                            }
                        };
                        let new_score = (current - penalty).max(0.0);
                        let trust_score = icn_trust::TrustScore::unchecked(new_score);
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
                        let new_score = (current + weight * 0.25).min(1.0);
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
