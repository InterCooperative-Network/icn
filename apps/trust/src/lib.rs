//! Trust App
//!
//! Provides trust-based policy decisions for the ICN kernel.
//!
//! # The Meaning Firewall
//!
//! This app wraps the internal `TrustGraph` implementation and exposes
//! it via the `PolicyOracle` interface. The kernel never sees trust
//! scores, trust classes, or any semantic trust concepts directly.
//! It only sees the constraints derived from trust decisions.
//!
//! # Architecture
//!
//! ```text
//! Kernel                    Trust App                    Internal
//! ------                    ---------                    --------
//! PolicyRequest  ────────>  TrustPolicyOracle  ───────>  TrustGraph
//!                                   │
//!                                   │ score_to_constraints()
//!                                   │ (MEANING FIREWALL BOUNDARY)
//!                                   v
//! ConstraintSet  <────────  PolicyDecision
//! ```
//!
//! The kernel calls `PolicyOracle::evaluate()` and receives a `PolicyDecision`
//! with `ConstraintSet`. It enforces these constraints without knowing they
//! came from trust scores.

pub mod oracle;

use icn_core::apps::dispatcher::{BoxedReducer, BoxedService};
use icn_core::apps::runtime::AppRuntime;
use std::sync::Arc;

pub use oracle::TrustPolicyOracle;

/// Register trust app handlers with the runtime.
///
/// Called by the supervisor during app startup.
pub fn register_handlers(
    _runtime: &AppRuntime,
) -> (
    Vec<(&'static str, BoxedReducer)>,
    Vec<(&'static str, BoxedService)>,
) {
    // TODO: Implement attestation reducer and score query service
    // For now, return empty - the main value is the PolicyOracle
    (vec![], vec![])
}

/// Create a TrustPolicyOracle instance.
///
/// This is the main entry point for kernel integration.
/// The kernel registers this oracle with the OracleRegistry.
pub fn create_oracle(
    trust_graph: Arc<parking_lot::RwLock<icn_trust::TrustGraph>>,
) -> Arc<dyn icn_kernel_api::authz::PolicyOracle> {
    Arc::new(TrustPolicyOracle::new(trust_graph))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_oracle() {
        // Create a minimal trust graph for testing
        let temp_dir = tempfile::tempdir().unwrap();
        let store = icn_store::SledStore::open(temp_dir.path()).unwrap();

        // Create a valid test DID using ed25519 keygen
        let keypair = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let own_did = icn_identity::Did::from_public_key(&keypair.verifying_key());

        let graph = icn_trust::TrustGraph::new(std::sync::Arc::new(store), own_did);

        let oracle = create_oracle(Arc::new(parking_lot::RwLock::new(graph)));

        // Verify oracle domain
        assert_eq!(oracle.domain().as_str(), "trust");
    }
}
