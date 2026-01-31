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
//!
//! # Service Interface
//!
//! For kernel integration, use `TrustServiceImpl` which implements the
//! `TrustService` trait from `icn-kernel-api`. This provides a clean
//! abstraction that the kernel can use without domain knowledge.

pub mod oracle;
pub mod oracle_tokio;
pub mod reducer;
pub mod service;
pub mod service_tokio;

use std::sync::Arc;

pub use oracle::TrustPolicyOracle;
pub use oracle_tokio::TrustPolicyOracleTokio;
pub use service::TrustServiceImpl;
pub use service_tokio::TrustServiceImplTokio;

/// Create a TrustPolicyOracle instance.
///
/// This is the main entry point for kernel integration.
/// The kernel registers this oracle with the OracleRegistry.
///
/// # Arguments
/// * `trust_graph` - The trust graph to use for trust score computation.
///   The trust graph remains owned by the caller; the oracle holds a reference.
pub fn create_oracle(
    trust_graph: Arc<parking_lot::RwLock<icn_trust::TrustGraph>>,
) -> Arc<dyn icn_kernel_api::authz::PolicyOracle> {
    Arc::new(TrustPolicyOracle::new(trust_graph))
}

/// Create a TrustPolicyOracle instance from a tokio RwLock.
///
/// Use this when integrating with icn-core which uses tokio locks.
/// The oracle uses `tokio::task::block_in_place` internally.
///
/// # Arguments
/// * `trust_graph` - The trust graph with tokio RwLock wrapper.
///
/// # Panics
/// If called outside of a tokio multi-threaded runtime context.
pub fn create_oracle_tokio(
    trust_graph: Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>,
) -> Arc<dyn icn_kernel_api::authz::PolicyOracle> {
    Arc::new(TrustPolicyOracleTokio::new(trust_graph))
}

/// Create a TrustService instance.
///
/// This is the preferred entry point for kernel integration as it provides
/// the full TrustService interface, not just PolicyOracle.
///
/// # Arguments
/// * `trust_graph` - The trust graph to use for trust operations.
pub fn create_service(
    trust_graph: Arc<parking_lot::RwLock<icn_trust::TrustGraph>>,
) -> Arc<dyn icn_kernel_api::services::TrustService> {
    Arc::new(TrustServiceImpl::new(trust_graph))
}

/// Create a TrustService instance from a tokio RwLock.
///
/// Use this when integrating with icn-core which uses tokio locks.
/// The service uses `tokio::task::block_in_place` internally.
///
/// # Arguments
/// * `trust_graph` - The trust graph with tokio RwLock wrapper.
/// * `keypair` - The node's keypair for signing outgoing attestations.
///
/// # Panics
/// If called outside of a tokio multi-threaded runtime context.
pub fn create_service_tokio(
    trust_graph: Arc<tokio::sync::RwLock<icn_trust::TrustGraph>>,
    keypair: icn_identity::KeyPair,
) -> Arc<dyn icn_kernel_api::services::TrustService> {
    Arc::new(TrustServiceImplTokio::new(trust_graph, keypair))
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
