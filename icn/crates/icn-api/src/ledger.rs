//! Ledger service for shared read operations.

use std::sync::Arc;

use tokio::sync::RwLock;

use icn_identity::Did;
use icn_ledger::Ledger;

use crate::error::ApiError;

/// Canonical account balance response used by shared ledger service consumers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccountBalance {
    pub account_id: String,
    pub currency: String,
    pub amount: i64,
}

/// Shared ledger service used by both RPC and gateway layers.
pub struct LedgerService {
    ledger: Arc<RwLock<Ledger>>,
}

impl LedgerService {
    /// Create a ledger service backed by a ledger handle.
    pub fn new(ledger: Arc<RwLock<Ledger>>) -> Self {
        Self { ledger }
    }

    /// Get balances for an account.
    ///
    /// When `currency` is provided, returns one balance entry for that currency.
    /// Otherwise returns all known balances for the account.
    pub async fn get_balances(
        &self,
        account_id: &str,
        currency: Option<&str>,
    ) -> Result<Vec<AccountBalance>, ApiError> {
        let account_did: Did = account_id
            .parse()
            .map_err(|e| ApiError::InvalidParameter(format!("Invalid DID: {e}")))?;

        let ledger = self.ledger.read().await;

        if let Some(currency) = currency {
            let amount = ledger.get_balance(&account_did, currency);
            Ok(vec![AccountBalance {
                account_id: account_id.to_string(),
                currency: currency.to_string(),
                amount,
            }])
        } else {
            let account_balances = ledger.get_account_balances(&account_did);
            Ok(account_balances
                .balances
                .iter()
                .map(|(currency, amount)| AccountBalance {
                    account_id: account_id.to_string(),
                    currency: currency.clone(),
                    amount: *amount,
                })
                .collect())
        }
    }
}
