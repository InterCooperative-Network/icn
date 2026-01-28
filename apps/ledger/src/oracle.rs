//! Ledger Policy Oracle
//!
//! Implements `PolicyOracle` for ledger-based authorization decisions.
//!
//! # The Meaning Firewall
//!
//! This oracle converts ledger state into kernel-enforceable constraints:
//! - Credit limits → transaction limits
//! - Account balances → resource quotas
//! - Credit policies → allow/deny decisions
//!
//! The kernel NEVER sees the ledger semantics behind these constraints.

use icn_kernel_api::authz::{
    ConstraintSet, Domain, PolicyContext, PolicyDecision, PolicyOracle, PolicyRequest,
};
use icn_ledger::Ledger;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Ledger policy oracle
///
/// Converts ledger state and credit policies into kernel-enforceable constraints.
pub struct LedgerPolicyOracle {
    #[allow(dead_code)] // Will be used for account-specific constraints
    ledger: Arc<RwLock<Ledger>>,
}

impl LedgerPolicyOracle {
    /// Create a new ledger policy oracle
    pub fn new(ledger: Arc<RwLock<Ledger>>) -> Self {
        Self { ledger }
    }

    /// Convert ledger state to constraints
    ///
    /// This is the MEANING FIREWALL BOUNDARY.
    /// Ledger semantics (credit limits, balances) are converted to generic constraints.
    fn ledger_to_constraints(&self, _context: &PolicyContext) -> ConstraintSet {
        // For now, return default constraints
        // TODO: Query ledger for account-specific limits when context provides account info
        ConstraintSet::default()
    }
}

impl PolicyOracle for LedgerPolicyOracle {
    fn domain(&self) -> Domain {
        Domain::new("ledger")
    }

    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // Only handle ledger domain requests
        if *request.domain() != self.domain() {
            debug!(
                domain = %request.domain(),
                "LedgerPolicyOracle abstaining from non-ledger domain"
            );
            return PolicyDecision::allow();
        }

        debug!(
            actor = %request.actor(),
            resource = ?request.context.resource,
            action = ?request.action(),
            "LedgerPolicyOracle evaluating request"
        );

        // Convert ledger state to constraints
        // This crosses the meaning firewall - ledger rules become generic limits
        let constraints = self.ledger_to_constraints(&request.context);

        PolicyDecision::Allow { constraints }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::authz::ActionKind;
    use icn_kernel_api::types::Did;

    fn create_test_ledger() -> Arc<RwLock<Ledger>> {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(icn_store::SledStore::open(temp_dir.path()).unwrap());
        let ledger = Ledger::new(store).unwrap();
        Arc::new(RwLock::new(ledger))
    }

    #[tokio::test]
    async fn test_oracle_domain() {
        let ledger = create_test_ledger();
        let oracle = LedgerPolicyOracle::new(ledger);
        assert_eq!(oracle.domain().as_str(), "ledger");
    }

    #[tokio::test]
    async fn test_oracle_allows_by_default() {
        let ledger = create_test_ledger();
        let oracle = LedgerPolicyOracle::new(ledger);

        let request = PolicyRequest::new(
            Did::from("did:icn:test123".to_string()),
            ActionKind::Write,
            Domain::new("ledger"),
        );

        let decision = oracle.evaluate(&request);
        assert!(matches!(decision, PolicyDecision::Allow { .. }));
    }

    #[tokio::test]
    async fn test_oracle_abstains_from_other_domains() {
        let ledger = create_test_ledger();
        let oracle = LedgerPolicyOracle::new(ledger);

        let request = PolicyRequest::new(
            Did::from("did:icn:test123".to_string()),
            ActionKind::Read,
            Domain::new("trust"),
        );

        let decision = oracle.evaluate(&request);
        // Should allow with empty constraints (abstain)
        if let PolicyDecision::Allow { constraints } = decision {
            assert!(constraints.rate_limit.is_none());
        } else {
            panic!("Expected Allow decision for non-ledger domain");
        }
    }
}
