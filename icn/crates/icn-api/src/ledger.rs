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

/// Canonical account delta for ledger history/read endpoints.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LedgerAccountDeltaView {
    pub account_id: String,
    pub currency: String,
    pub debit: Option<i64>,
    pub credit: Option<i64>,
}

/// Canonical ledger entry view used by shared ledger service consumers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LedgerEntryView {
    pub id: String,
    pub timestamp: u64,
    pub author: String,
    pub accounts: Vec<LedgerAccountDeltaView>,
    pub decision_receipt_id: Option<String>,
    pub decision_hash: Option<String>,
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

    /// Get ledger entries authorized by a decision hash.
    ///
    /// This is intentionally scoped for current gateway boundary work:
    /// it provides a stable DTO surface while keeping storage/query details internal.
    pub async fn get_entries_by_decision(
        &self,
        decision_hash: &str,
        limit: usize,
    ) -> Result<Vec<LedgerEntryView>, ApiError> {
        let ledger = self.ledger.read().await;
        let (entries, _total) = ledger
            .get_entries_paginated_asc(0, 1000)
            .map_err(|e| ApiError::LedgerError(e.to_string()))?;

        Ok(entries
            .into_iter()
            .filter(|entry| entry.decision_hash.as_deref() == Some(decision_hash))
            .take(limit)
            .map(|entry| LedgerEntryView {
                id: entry.id.map(|h| h.to_hex()).unwrap_or_default(),
                timestamp: entry.timestamp,
                author: entry.author.to_string(),
                accounts: entry
                    .accounts
                    .iter()
                    .map(|delta| LedgerAccountDeltaView {
                        account_id: delta.account_id.to_string(),
                        currency: delta.currency.clone(),
                        debit: delta.debit,
                        credit: delta.credit,
                    })
                    .collect(),
                decision_receipt_id: entry.decision_receipt_id.clone(),
                decision_hash: entry.decision_hash.clone(),
            })
            .collect())
    }
}
