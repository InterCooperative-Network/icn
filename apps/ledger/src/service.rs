//! Ledger Service Implementation
//!
//! Implements `LedgerService` trait from `icn-kernel-api`.
//! This provides the full ledger interface for kernel integration.

use icn_kernel_api::authz::PolicyOracle;
use icn_kernel_api::services::{LedgerEvent, LedgerService};
use icn_kernel_api::types::Did;
use icn_ledger::Ledger;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::LedgerPolicyOracle;

/// Implementation of `LedgerService` for the ICN ledger subsystem
///
/// This service wraps the ledger and exposes ledger functionality
/// through the kernel-api interface.
pub struct LedgerServiceImpl {
    ledger: Arc<RwLock<Ledger>>,
    oracle: Arc<LedgerPolicyOracle>,
}

impl LedgerServiceImpl {
    /// Create a new ledger service
    pub fn new(ledger: Arc<RwLock<Ledger>>) -> Self {
        let oracle = Arc::new(LedgerPolicyOracle::new(ledger.clone()));
        Self { ledger, oracle }
    }

    /// Get access to the underlying ledger
    ///
    /// This is for internal use only - the kernel should use the trait methods.
    pub fn ledger(&self) -> &Arc<RwLock<Ledger>> {
        &self.ledger
    }

    /// Convert a kernel Did to an icn_identity::Did
    fn to_identity_did(did: &Did) -> Result<icn_identity::Did, anyhow::Error> {
        did.to_string()
            .parse::<icn_identity::Did>()
            .map_err(|e| anyhow::anyhow!("Invalid DID format: {}", e))
    }
}

impl LedgerService for LedgerServiceImpl {
    fn oracle(&self) -> Arc<dyn PolicyOracle> {
        self.oracle.clone()
    }

    fn balance(&self, account: &Did, currency: &str) -> i64 {
        // Convert kernel DID to identity DID
        let identity_did = match Self::to_identity_did(account) {
            Ok(did) => did,
            Err(e) => {
                warn!(error = %e, "Failed to parse DID for balance query");
                return 0;
            }
        };

        // Query ledger for balance
        // SAFETY: Using block_in_place since LedgerService is sync but ledger is async
        tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let ledger = self.ledger.read().await;
                let balance = ledger.get_balance(&identity_did, currency);
                debug!(
                    account = %account,
                    currency = %currency,
                    balance = balance,
                    "Balance query successful"
                );
                balance
            })
        })
    }

    fn credit_limit(&self, _account: &Did, _currency: &str) -> i64 {
        // Credit limits are calculated dynamically by CreditPolicyManager based on:
        // - Member tenure (how long they've been a member)
        // - Contribution history
        // - Trust score
        // For now, return 0 as credit limit lookup requires additional context
        // TODO: Integrate with CreditPolicyManager when available through Ledger
        debug!(
            account = %_account,
            currency = %_currency,
            "Credit limit query - returning 0 (dynamic calculation not yet integrated)"
        );
        0
    }

    fn record_event(&self, event: LedgerEvent) {
        // Log events for observability
        match &event {
            LedgerEvent::TransactionSubmitted {
                from,
                to,
                amount,
                currency,
            } => {
                debug!(
                    from = %from,
                    to = %to,
                    amount = amount,
                    currency = %currency,
                    "Transaction submitted"
                );
            }
            LedgerEvent::TransactionCompleted { transaction_id } => {
                debug!(
                    transaction_id = %transaction_id,
                    "Transaction completed"
                );
            }
            LedgerEvent::TransactionFailed {
                transaction_id,
                reason,
            } => {
                debug!(
                    transaction_id = %transaction_id,
                    reason = %reason,
                    "Transaction failed"
                );
            }
            LedgerEvent::BalanceQueried { account, querier } => {
                debug!(
                    account = %account,
                    querier = %querier,
                    "Balance queried"
                );
            }
        }

        // TODO: Emit metrics
        // icn_obs::metrics::ledger::event_recorded_inc(&event_type);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_ledger() -> Arc<RwLock<Ledger>> {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = Arc::new(icn_store::SledStore::open(temp_dir.path()).unwrap());
        let ledger = Ledger::new(store).unwrap();
        Arc::new(RwLock::new(ledger))
    }

    #[tokio::test]
    async fn test_service_oracle() {
        let ledger = create_test_ledger();
        let service = LedgerServiceImpl::new(ledger);

        let oracle = service.oracle();
        assert_eq!(oracle.domain().as_str(), "ledger");
    }

    #[tokio::test]
    async fn test_service_balance_unknown_account() {
        let ledger = create_test_ledger();
        let service = LedgerServiceImpl::new(ledger);

        // Unknown account should return 0
        let balance = service.balance(&Did::from("did:icn:test123".to_string()), "hours");
        assert_eq!(balance, 0);
    }

    #[tokio::test]
    async fn test_service_credit_limit_unknown_account() {
        let ledger = create_test_ledger();
        let service = LedgerServiceImpl::new(ledger);

        // Unknown account should return 0
        let limit = service.credit_limit(&Did::from("did:icn:test123".to_string()), "hours");
        assert_eq!(limit, 0);
    }

    #[tokio::test]
    async fn test_service_record_event() {
        let ledger = create_test_ledger();
        let service = LedgerServiceImpl::new(ledger);

        // This should not panic
        service.record_event(LedgerEvent::TransactionSubmitted {
            from: Did::from("did:icn:alice".to_string()),
            to: Did::from("did:icn:bob".to_string()),
            amount: 100,
            currency: "hours".to_string(),
        });

        service.record_event(LedgerEvent::TransactionCompleted {
            transaction_id: "tx123".to_string(),
        });

        service.record_event(LedgerEvent::TransactionFailed {
            transaction_id: "tx456".to_string(),
            reason: "insufficient balance".to_string(),
        });

        service.record_event(LedgerEvent::BalanceQueried {
            account: Did::from("did:icn:alice".to_string()),
            querier: Did::from("did:icn:bob".to_string()),
        });
    }
}
