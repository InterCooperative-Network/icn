//! Governance App
//!
//! Provides governance-based policy decisions for the ICN kernel.
//!
//! # The Meaning Firewall
//!
//! This app wraps the internal governance implementation and exposes
//! it via the `PolicyOracle` and `GovernanceService` interfaces. The kernel
//! never sees proposals, votes, or governance rules directly.
//! It only sees the constraints derived from governance decisions.
//!
//! # Architecture
//!
//! ```text
//! Kernel                    Governance App                   Internal
//! ------                    --------------                   --------
//! PolicyRequest  ────────>  GovernancePolicyOracle  ──────>  ParameterStore
//!                                   │
//!                                   │ param_to_constraints()
//!                                   │ (MEANING FIREWALL BOUNDARY)
//!                                   v
//! ConstraintSet  <────────  PolicyDecision
//! ```
//!
//! The kernel calls `PolicyOracle::evaluate()` and receives a `PolicyDecision`
//! with `ConstraintSet`. It enforces these constraints without knowing they
//! came from governance rules.
//!
//! # Service Interface
//!
//! For kernel integration, use `GovernanceServiceImpl` which implements the
//! `GovernanceService` trait from `icn-kernel-api`. This provides a clean
//! abstraction that the kernel can use without domain knowledge.

pub mod oracle;
pub mod service;

use std::sync::Arc;

pub use oracle::GovernancePolicyOracle;
pub use service::GovernanceServiceImpl;

/// Create a GovernancePolicyOracle instance.
///
/// This is the main entry point for kernel integration.
/// The kernel registers this oracle with the OracleRegistry.
///
/// # Arguments
/// * `parameter_store` - The protocol parameter store for governance parameters.
pub fn create_oracle(
    parameter_store: Arc<dyn icn_governance::ProtocolParameterStore>,
) -> Arc<dyn icn_kernel_api::authz::PolicyOracle> {
    Arc::new(GovernancePolicyOracle::new(parameter_store))
}

/// Create a GovernanceService instance.
///
/// This is the preferred entry point for kernel integration as it provides
/// the full GovernanceService interface, not just PolicyOracle.
///
/// # Arguments
/// * `parameter_store` - The protocol parameter store for governance parameters.
pub fn create_service(
    parameter_store: Arc<dyn icn_governance::ProtocolParameterStore>,
) -> Arc<dyn icn_kernel_api::services::GovernanceService> {
    Arc::new(GovernanceServiceImpl::new(parameter_store))
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_governance::SledParameterStore;

    #[test]
    fn test_create_oracle() {
        // Create a minimal parameter store for testing
        let temp_dir = tempfile::tempdir().unwrap();
        let db = sled::open(temp_dir.path()).unwrap();
        let store: Arc<dyn icn_governance::ProtocolParameterStore> =
            Arc::new(SledParameterStore::new(Arc::new(db)).unwrap());

        let oracle = create_oracle(store);

        // Verify oracle domain
        assert_eq!(oracle.domain().as_str(), "governance");
    }

    #[test]
    fn test_create_service() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = sled::open(temp_dir.path()).unwrap();
        let store: Arc<dyn icn_governance::ProtocolParameterStore> =
            Arc::new(SledParameterStore::new(Arc::new(db)).unwrap());

        let service = create_service(store);

        // Verify oracle domain via service
        assert_eq!(service.oracle().domain().as_str(), "governance");
    }
}
