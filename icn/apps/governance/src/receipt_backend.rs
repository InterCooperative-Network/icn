//! Abstraction over gateway's ReceiptStore so GovernanceManager can live
//! in this crate without a circular dependency on icn-gateway.

use icn_governance::GovernanceDecisionReceipt;
use icn_kernel_api::{AllocationReceipt, Hash};

/// Minimal receipt-storage interface required by [`GovernanceManager`].
///
/// Gateway implements this for its [`ReceiptStore`]; tests can provide a
/// simple in-memory stand-in.
///
/// [`GovernanceManager`]: crate::manager::GovernanceManager
/// [`ReceiptStore`]: icn_gateway::receipt_store::ReceiptStore
pub trait GovernanceReceiptBackend: Send + Sync {
    /// Persist a governance decision receipt.
    fn put_governance(&self, receipt: &GovernanceDecisionReceipt) -> Result<(), String>;

    /// Retrieve a governance decision receipt by proposal ID.
    fn get_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String>;

    /// Persist an allocation receipt linked to a governance decision.
    ///
    /// Called by the governance manager when a budget/treasury proposal
    /// is accepted, creating the governance→economics binding.
    fn put_allocation(&self, receipt: &AllocationReceipt) -> Result<Hash, String>;

    /// Retrieve a governance decision receipt by decision hash (cross-node canonical).
    fn get_governance_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String>;

    /// Retrieve all allocation receipts linked to a governance decision.
    fn list_allocations_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<AllocationReceipt>, String>;
}
