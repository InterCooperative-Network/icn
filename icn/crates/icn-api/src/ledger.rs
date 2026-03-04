//! Ledger service for shared read operations.

use std::sync::Arc;

use tokio::sync::RwLock;

use icn_identity::Did;
use icn_ledger::{types::ProvenanceRef, Ledger};

use crate::error::ApiError;

/// Canonical account position response used by shared ledger service consumers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccountBalance {
    pub account_id: String,
    pub unit: String,
    pub amount: i64,
}

/// Canonical account delta for ledger history/read endpoints.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LedgerAccountDeltaView {
    pub account_id: String,
    pub unit: String,
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

/// Bounded decision-read page returned by the shared ledger service.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DecisionEntriesPage {
    pub entries: Vec<LedgerEntryView>,
    pub has_more: bool,
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

    /// Get positions for an account.
    ///
    /// When `unit` is provided, returns one position entry for that unit.
    /// Otherwise returns all known positions for the account.
    pub async fn get_positions(
        &self,
        account_id: &str,
        unit: Option<&str>,
    ) -> Result<Vec<AccountBalance>, ApiError> {
        let account_did: Did = account_id
            .parse()
            .map_err(|e| ApiError::InvalidParameter(format!("Invalid DID: {e}")))?;

        let ledger = self.ledger.read().await;

        if let Some(unit) = unit {
            let amount = ledger.get_balance(&account_did, unit);
            Ok(vec![AccountBalance {
                account_id: account_id.to_string(),
                unit: unit.to_string(),
                amount,
            }])
        } else {
            let account_balances = ledger.get_account_balances(&account_did);
            Ok(account_balances
                .balances
                .iter()
                .map(|(currency, amount)| AccountBalance {
                    account_id: account_id.to_string(),
                    unit: currency.clone(),
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
    ) -> Result<DecisionEntriesPage, ApiError> {
        const PILOT_MAX_SCAN_SIZE: usize = 1000;
        let scan_size = limit
            .saturating_mul(10)
            .clamp(limit.max(1), PILOT_MAX_SCAN_SIZE);

        let ledger = self.ledger.read().await;
        let (entries, _total) = ledger
            .get_entries_paginated_asc(0, scan_size)
            .map_err(|e| ApiError::LedgerError(e.to_string()))?;

        let mut matched_count = 0usize;
        let mut page_entries = Vec::new();

        for entry in entries {
            // Extract governance provenance fields for filtering and view projection
            let (view_receipt_id, view_decision_hash) = match &entry.provenance {
                ProvenanceRef::Governance {
                    receipt_id,
                    decision_hash: dh,
                } => (Some(receipt_id.clone()), Some(dh.clone())),
                _ => (None, None),
            };

            if view_decision_hash.as_deref() != Some(decision_hash) {
                continue;
            }

            matched_count += 1;
            if page_entries.len() >= limit {
                continue;
            }

            page_entries.push(LedgerEntryView {
                id: entry.id.map(|h| h.to_hex()).unwrap_or_default(),
                timestamp: entry.timestamp,
                author: entry.author.to_string(),
                accounts: entry
                    .accounts
                    .iter()
                    .map(|delta| LedgerAccountDeltaView {
                        account_id: delta.account_id.to_string(),
                        unit: delta.currency.clone(),
                        debit: delta.debit,
                        credit: delta.credit,
                    })
                    .collect(),
                decision_receipt_id: view_receipt_id,
                decision_hash: view_decision_hash,
            });
        }

        Ok(DecisionEntriesPage {
            entries: page_entries,
            has_more: matched_count > limit,
        })
    }
}
