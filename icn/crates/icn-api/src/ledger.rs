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
    /// Index-backed: resolves the governance `decision_hash` through the ledger's
    /// `decision_hash -> entry` secondary index, so the lookup costs O(matches)
    /// rather than scanning the whole journal (the scale follow-up to #1988,
    /// which fixed correctness by paging the full journal). The index is
    /// maintained on the ledger write path and re-verified against the journal on
    /// read, so removed/archived entries never surface and the result matches the
    /// old full-scan semantics exactly.
    ///
    /// The DTO surface stays stable while storage/query details remain internal.
    /// Returns up to `limit` matching entries in deterministic chronological
    /// order, with `has_more` set when further matches exist.
    pub async fn get_entries_by_decision(
        &self,
        decision_hash: &str,
        limit: usize,
    ) -> Result<DecisionEntriesPage, ApiError> {
        // A zero-sized page has nothing to return; short-circuit without touching
        // the index. Gateway callers already clamp `limit` to >= 1; this guards
        // the public method against a degenerate request.
        if limit == 0 {
            return Ok(DecisionEntriesPage {
                entries: Vec::new(),
                has_more: false,
            });
        }

        let ledger = self.ledger.read().await;

        // `total` counts all live matches; the returned page holds the first
        // `limit` of them, so `has_more` is exact.
        let (entries, total) = ledger
            .get_entries_by_decision_hash(decision_hash, 0, limit)
            .map_err(|e| ApiError::LedgerError(e.to_string()))?;

        let page_entries = entries
            .into_iter()
            .map(|entry| {
                let (view_receipt_id, view_decision_hash) = match &entry.provenance {
                    ProvenanceRef::Governance {
                        receipt_id,
                        decision_hash: dh,
                    } => (Some(receipt_id.clone()), Some(dh.clone())),
                    _ => (None, None),
                };

                LedgerEntryView {
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
                }
            })
            .collect();

        Ok(DecisionEntriesPage {
            entries: page_entries,
            has_more: total > limit,
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
        // unrelated, self-balancing system-provenance entries. A unique amount per
        // entry guarantees distinct content hashes (a collision would overwrite an
        // existing entry and corrupt the count).
        const FILLER_ENTRIES: usize = 1005;
        for i in 0..FILLER_ENTRIES {
            let amount = (i as i64) + 1;
            let entry = JournalEntryBuilder::new(alice.clone())
                .debit(alice.clone(), "hours".to_string(), amount)
                .credit(bob.clone(), "hours".to_string(), amount)
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
        // Distinct from every filler amount above, keeping this entry's content
        // hash unique.
        let gov_amount = (FILLER_ENTRIES as i64) + 1;
        let gov_entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), gov_amount)
            .credit(bob.clone(), "hours".to_string(), gov_amount)
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

    /// Helper: append a governance-authorized, self-balancing entry. The unique
    /// `amount` keeps each entry's content hash distinct.
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn append_gov(ledger: &mut Ledger, a: &Did, b: &Did, amount: i64, decision_hash: &str) {
        let entry = JournalEntryBuilder::new(a.clone())
            .debit(a.clone(), "hours".to_string(), amount)
            .credit(b.clone(), "hours".to_string(), amount)
            .with_governance_provenance(format!("receipt-{amount}"), decision_hash)
            .build()
            .unwrap();
        ledger.append_entry(entry).await.unwrap();
    }

    /// Test B (DTO level): the page is bounded to `limit` and `has_more` is exact.
    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn decision_lookup_bounds_page_and_reports_has_more() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let mut ledger = Ledger::new(store).unwrap();
        let a = KeyPair::generate().unwrap().did().clone();
        let b = KeyPair::generate().unwrap().did().clone();
        let dh = "dh-page";

        for i in 0..5 {
            append_gov(&mut ledger, &a, &b, i + 1, dh).await;
        }
        let service = LedgerService::new(Arc::new(RwLock::new(ledger)));

        let page = service.get_entries_by_decision(dh, 2).await.unwrap();
        assert_eq!(page.entries.len(), 2, "page is bounded to limit");
        assert!(page.has_more, "5 matches exceed limit 2");

        let full = service.get_entries_by_decision(dh, 10).await.unwrap();
        assert_eq!(full.entries.len(), 5);
        assert!(!full.has_more, "limit covers all matches");
        assert!(full
            .entries
            .iter()
            .all(|e| e.decision_hash.as_deref() == Some(dh)));
    }

    /// Test C: `limit == 0` returns an empty page and never reports more.
    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn decision_lookup_limit_zero_returns_empty_page() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let mut ledger = Ledger::new(store).unwrap();
        let a = KeyPair::generate().unwrap().did().clone();
        let b = KeyPair::generate().unwrap().did().clone();
        append_gov(&mut ledger, &a, &b, 1, "dh-zero").await;
        let service = LedgerService::new(Arc::new(RwLock::new(ledger)));

        let page = service.get_entries_by_decision("dh-zero", 0).await.unwrap();
        assert!(page.entries.is_empty());
        assert!(!page.has_more);
    }

    /// Test D: non-governance entries and non-matching hashes never appear.
    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn decision_lookup_excludes_non_matching_and_non_governance() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
        let mut ledger = Ledger::new(store).unwrap();
        let a = KeyPair::generate().unwrap().did().clone();
        let b = KeyPair::generate().unwrap().did().clone();

        // A system (non-governance) entry, plus a governance entry under dh-A.
        let sys = JournalEntryBuilder::new(a.clone())
            .debit(a.clone(), "hours".to_string(), 1)
            .credit(b.clone(), "hours".to_string(), 1)
            .with_system_provenance("sys")
            .build()
            .unwrap();
        ledger.append_entry(sys).await.unwrap();
        append_gov(&mut ledger, &a, &b, 2, "dh-A").await;
        let service = LedgerService::new(Arc::new(RwLock::new(ledger)));

        // A different decision hash matches nothing (the system entry never leaks).
        let other = service.get_entries_by_decision("dh-B", 10).await.unwrap();
        assert!(other.entries.is_empty());
        assert!(!other.has_more);

        // dh-A returns exactly its one governance entry.
        let a_page = service.get_entries_by_decision("dh-A", 10).await.unwrap();
        assert_eq!(a_page.entries.len(), 1);
        assert_eq!(a_page.entries[0].decision_hash.as_deref(), Some("dh-A"));
    }
}
