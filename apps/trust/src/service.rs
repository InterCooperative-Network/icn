//! Trust Service Implementation
//!
//! Implements the `TrustService` trait from icn-kernel-api, providing
//! trust functionality to the kernel without exposing domain types.

use icn_kernel_api::authz::PolicyOracle;
use icn_kernel_api::services::{TrustEvent, TrustService};
use icn_trust::TrustGraph;
use parking_lot::RwLock;
use std::sync::Arc;

use crate::oracle::TrustPolicyOracle;

/// Trust service implementation
///
/// This wraps TrustGraph and TrustPolicyOracle, exposing them through
/// the abstract TrustService interface.
pub struct TrustServiceImpl {
    graph: Arc<RwLock<TrustGraph>>,
    oracle: Arc<TrustPolicyOracle>,
    own_did: icn_identity::Did,
}

impl TrustServiceImpl {
    /// Create a new trust service with the given TrustGraph
    pub fn new(graph: Arc<RwLock<TrustGraph>>) -> Self {
        let own_did = graph.read().own_did().clone();
        let oracle = Arc::new(TrustPolicyOracle::new(graph.clone()));
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

impl TrustService for TrustServiceImpl {
    fn oracle(&self) -> Arc<dyn PolicyOracle> {
        self.oracle.clone()
    }

    fn trust_score(&self, actor: &icn_kernel_api::types::Did) -> f64 {
        let graph = self.graph.read();
        // Convert from kernel Did to identity Did using FromStr
        if let Ok(identity_did) = actor.to_string().parse::<icn_identity::Did>() {
            graph.compute_trust_score(&identity_did).unwrap_or(0.0)
        } else {
            0.0
        }
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
                let penalty = severity * 0.25;
                let current = {
                    let graph = self.graph.read();
                    match graph.compute_trust_score(&identity_did) {
                        Ok(score) => score,
                        Err(_) => {
                            // Unknown actors start at 0.0 trust — penalty still
                            // creates a trust edge recording the violation.
                            0.0
                        }
                    }
                };
                let new_score = (current - penalty).max(0.0);
                let trust_score = icn_trust::TrustScore::unchecked(new_score);
                let edge =
                    icn_trust::TrustEdge::new(self.own_did.clone(), identity_did, trust_score);
                let mut graph = self.graph.write();
                if let Err(e) = graph.add_edge(edge) {
                    tracing::warn!(actor = %actor, "Failed to persist trust penalty: {}", e);
                } else {
                    tracing::debug!(
                        actor = %actor,
                        current = current,
                        penalty = penalty,
                        new_score = new_score,
                        "Trust penalty persisted via TrustEdge"
                    );
                }
            }
            TrustEvent::PositiveInteraction { weight } => {
                tracing::debug!(
                    actor = %actor,
                    weight = weight,
                    "Trust event: positive interaction"
                );
                let current = {
                    let graph = self.graph.read();
                    graph.compute_trust_score(&identity_did).unwrap_or(0.0)
                };
                let new_score = (current + weight * 0.25).min(1.0);
                let trust_score = icn_trust::TrustScore::unchecked(new_score);
                let edge =
                    icn_trust::TrustEdge::new(self.own_did.clone(), identity_did, trust_score);
                let mut graph = self.graph.write();
                if let Err(e) = graph.add_edge(edge) {
                    tracing::warn!(actor = %actor, "Failed to persist trust boost: {}", e);
                } else {
                    tracing::debug!(
                        actor = %actor,
                        new_score = new_score,
                        "Trust boost persisted via TrustEdge"
                    );
                }
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

    #[test]
    fn test_trust_service_creation() {
        // Use temporary store from icn_store
        let store = icn_store::SledStore::temporary().unwrap();
        let store: Arc<dyn icn_store::Store> = Arc::new(store);

        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create trust graph
        let graph = TrustGraph::new(store, did.clone());
        let graph = Arc::new(RwLock::new(graph));

        let service = TrustServiceImpl::new(graph);

        // Should have zero trust for unknown actors
        let unknown_keypair = icn_identity::KeyPair::generate().unwrap();
        let unknown_did = icn_kernel_api::types::Did::from(unknown_keypair.did().to_string());
        let score = service.trust_score(&unknown_did);
        assert!(score >= 0.0 && score <= 1.0);

        // Should return an oracle
        let _oracle = service.oracle();
    }
}
