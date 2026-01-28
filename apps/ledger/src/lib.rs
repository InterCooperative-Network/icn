//! Ledger App
//!
//! Provides ledger-based policy decisions for the ICN kernel.
//!
//! # The Meaning Firewall
//!
//! This app wraps the internal ledger implementation and exposes
//! it via the `PolicyOracle` and `LedgerService` interfaces. The kernel
//! never sees account structures, credit policies, or ledger rules directly.
//! It only sees the constraints derived from ledger decisions.
//!
//! # Architecture
//!
//! ```text
//! Kernel                    Ledger App                     Internal
//! ------                    ----------                     --------
//! PolicyRequest  ────────>  LedgerPolicyOracle  ─────────>  Ledger
//!                                   │
//!                                   │ credit_to_constraints()
//!                                   │ (MEANING FIREWALL BOUNDARY)
//!                                   v
//! ConstraintSet  <────────  PolicyDecision
//! ```
//!
//! The kernel calls `PolicyOracle::evaluate()` and receives a `PolicyDecision`
//! with `ConstraintSet`. It enforces these constraints without knowing they
//! came from credit policies.
//!
//! # Service Interface
//!
//! For kernel integration, use `LedgerServiceImpl` which implements the
//! `LedgerService` trait from `icn-kernel-api`. This provides a clean
//! abstraction that the kernel can use without domain knowledge.

pub mod oracle;
pub mod service;

use std::sync::Arc;
use tokio::sync::RwLock;

pub use oracle::LedgerPolicyOracle;
pub use service::LedgerServiceImpl;

/// Create a LedgerPolicyOracle instance.
///
/// This is the main entry point for kernel integration.
/// The kernel registers this oracle with the OracleRegistry.
///
/// # Arguments
/// * `ledger` - The ledger to use for credit policy decisions.
pub fn create_oracle(
    ledger: Arc<RwLock<icn_ledger::Ledger>>,
) -> Arc<dyn icn_kernel_api::authz::PolicyOracle> {
    Arc::new(LedgerPolicyOracle::new(ledger))
}

/// Create a LedgerService instance.
///
/// This is the preferred entry point for kernel integration as it provides
/// the full LedgerService interface, not just PolicyOracle.
///
/// # Arguments
/// * `ledger` - The ledger to use for ledger operations.
pub fn create_service(
    ledger: Arc<RwLock<icn_ledger::Ledger>>,
) -> Arc<dyn icn_kernel_api::services::LedgerService> {
    Arc::new(LedgerServiceImpl::new(ledger))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_oracle() {
        // Create a minimal ledger for testing
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(icn_store::SledStore::open(temp_dir.path()).unwrap());
        let ledger = icn_ledger::Ledger::new(store).unwrap();
        let ledger = Arc::new(RwLock::new(ledger));

        let oracle = create_oracle(ledger);

        // Verify oracle domain
        assert_eq!(oracle.domain().as_str(), "ledger");
    }

    #[tokio::test]
    async fn test_create_service() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(icn_store::SledStore::open(temp_dir.path()).unwrap());
        let ledger = icn_ledger::Ledger::new(store).unwrap();
        let ledger = Arc::new(RwLock::new(ledger));

        let service = create_service(ledger);

        // Verify oracle domain via service
        assert_eq!(service.oracle().domain().as_str(), "ledger");
    }
}
