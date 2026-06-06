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
    /// Walks the entire journal in bounded offset pages (chronological order),
    /// filtering by governance `decision_hash`, until `limit` matching entries
    /// are collected (plus one extra to determine `has_more`) or the journal is
    /// exhausted. Paging over the whole ledger — rather than scanning only a
    /// fixed prefix from offset 0 — keeps the receipt-chain lookup correct for
    /// ledgers holding more than one page of entries, where a recent decision's
    /// entries would otherwise sit beyond the scan window and be missed.
    ///
    /// The DTO surface stays stable while storage/query details remain internal.
    pub async fn get_entries_by_decision(
        &self,
        decision_hash: &str,
        limit: usize,
    ) -> Result<DecisionEntriesPage, ApiError> {
        // Number of journal entries fetched per page. Memory stays bounded to one
        // page regardless of total ledger size.
        const PAGE_SIZE: usize = 1000;

        let ledger = self.ledger.read().await;

        let mut offset = 0usize;
        let mut matched_count = 0usize;
        let mut page_entries = Vec::new();

        loop {
            let (entries, total) = ledger
                .get_entries_paginated_asc(offset, PAGE_SIZE)
                .map_err(|e| ApiError::LedgerError(e.to_string()))?;

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

            offset = offset.saturating_add(PAGE_SIZE);

            // Stop once the journal is exhausted, or once enough matches exist to
            // both fill the page and prove `has_more` (one past `limit`).
            if offset >= total || matched_count > limit {
                break;
            }
        }

        Ok(DecisionEntriesPage {
            entries: page_entries,
            has_more: matched_count > limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use tokio::sync::RwLock;

    use icn_identity::KeyPair;
    use icn_ledger::entry::JournalEntryBuilder;
    use icn_ledger::Ledger;
    use icn_store::SledStore;
    use tempfile::TempDir;

    /// Regression test for the production-scale receipt-chain journal lookup.
    ///
    /// Before bounded-offset paging, `get_entries_by_decision` only scanned the
    /// first ~1000 entries from offset 0, so a governance entry recorded after
    /// the ledger already held 1000+ entries was never found and the
    /// receipt-chain endpoints reported "0 journal entries" for a real decision.
    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn finds_decision_entry_beyond_first_1000_entries() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let mut ledger = Ledger::new(store).unwrap();

        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Fill the ledger past the old fixed 1000-entry scan window with
        // unrelated, self-balancing system-provenance entries. A unique nonce per
        // entry guarantees distinct content hashes (a collision would overwrite an
        // existing entry and corrupt the count).
        const FILLER_ENTRIES: usize = 1005;
        for i in 0..FILLER_ENTRIES {
            let mut nonce = [0u8; 32];
            nonce[..8].copy_from_slice(&(i as u64).to_le_bytes());
            let entry = JournalEntryBuilder::new(alice.clone())
                .debit(alice.clone(), "hours".to_string(), 1)
                .credit(bob.clone(), "hours".to_string(), 1)
                .nonce(nonce)
                .with_system_provenance("filler")
                .build()
                .unwrap();
            ledger.append_entry(entry).await.unwrap();
        }

        // Entries are ordered by timestamp in the journal index, with ties broken
        // by hash. Sleeping here guarantees the governance entry's millisecond
        // timestamp is strictly greater than every filler entry, so it sorts last
        // and is unambiguously beyond the first 1000 entries.
        std::thread::sleep(std::time::Duration::from_millis(5));

        let decision_hash = "decision-hash-beyond-scan-window";
        let mut gov_nonce = [0u8; 32];
        gov_nonce[..8].copy_from_slice(&(FILLER_ENTRIES as u64).to_le_bytes());
        let gov_entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 1)
            .credit(bob.clone(), "hours".to_string(), 1)
            .nonce(gov_nonce)
            .with_governance_provenance("receipt-beyond-window", decision_hash)
            .build()
            .unwrap();
        ledger.append_entry(gov_entry).await.unwrap();

        let service = LedgerService::new(Arc::new(RwLock::new(ledger)));

        // Limit 100 makes the old code's scan window exactly the 1000-entry cap,
        // so the governance entry at index 1005 was previously unreachable.
        let page = service
            .get_entries_by_decision(decision_hash, 100)
            .await
            .expect("decision lookup should succeed");

        assert_eq!(
            page.entries.len(),
            1,
            "the governance entry recorded beyond the first 1000 entries must be found"
        );
        assert_eq!(
            page.entries[0].decision_hash.as_deref(),
            Some(decision_hash),
            "the matched entry must carry the requested decision hash"
        );
        assert!(
            !page.has_more,
            "exactly one matching entry exists, so there is no further page"
        );
    }
}
