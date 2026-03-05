//! Patronage tracking for cooperative internal capital accounts.
//!
//! Implements the NY Cooperative Corporations Law §93 concept of "internal capital
//! accounts" — per-member, attributable balances that the cooperative can use to
//! track surplus allocation, retain earnings, or record patronage refunds in an
//! auditable way.
//!
//! A **patronage account** is keyed by `(coop_id, member_did)`. Credits and debits
//! are recorded via [`PatronageEntry`] records with a caller-supplied `reference`
//! string that serves as the dedup key — the same reference cannot be applied twice
//! to the same account.
//!
//! # Example
//!
//! ```rust,ignore
//! use icn_ledger::patronage::PatronageTracker;
//! use icn_store::SledStore;
//! use std::sync::Arc;
//!
//! let store = Arc::new(SledStore::temporary()?);
//! let tracker = PatronageTracker::new(store);
//!
//! tracker.record_patronage("food-coop", &member_did, 200, "surplus:2025:q4")?;
//! tracker.record_patronage("food-coop", &member_did, -50, "adjustment:2025:q4")?;
//!
//! assert_eq!(tracker.get_balance("food-coop", &member_did)?, 150);
//! ```

use crate::error::{LedgerError, Result};
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Storage key prefixes
// ---------------------------------------------------------------------------

const ACCOUNT_PREFIX: &str = "patronage:account:";
const ENTRY_PREFIX: &str = "patronage:entry:";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A member's internal capital account within a cooperative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatronageAccount {
    /// The cooperative this account belongs to.
    pub coop_id: String,

    /// The member DID.
    pub member_did: Did,

    /// Current net balance (can be negative for adjustments/chargebacks).
    pub balance: i64,

    /// Unix timestamp of the last mutation.
    pub last_updated: u64,
}

/// A single debit or credit applied to a patronage account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatronageEntry {
    /// The cooperative this entry belongs to.
    pub coop_id: String,

    /// The member whose account is being updated.
    pub member_did: Did,

    /// Amount to apply (positive = credit, negative = debit).
    pub amount: i64,

    /// Caller-supplied reference used as dedup key (e.g., "surplus:2025:q4").
    pub reference: String,

    /// Unix timestamp when this entry was recorded.
    pub recorded_at: u64,
}

// ---------------------------------------------------------------------------
// PatronageTracker
// ---------------------------------------------------------------------------

/// Manages internal capital accounts across cooperative members.
///
/// Thread-safe via `Arc<dyn Store>`. All mutations are atomic at the KV level.
pub struct PatronageTracker {
    store: Arc<dyn Store>,
}

impl PatronageTracker {
    /// Create a new tracker backed by the given store.
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    // -----------------------------------------------------------------------
    // Write operations
    // -----------------------------------------------------------------------

    /// Record a patronage credit or debit for a member in a cooperative.
    ///
    /// `reference` is used as a dedup key: calling this method twice with the
    /// same `(coop_id, member_did, reference)` triple is rejected with
    /// [`LedgerError::DuplicateEntry`].
    ///
    /// `amount` may be positive (credit) or negative (debit).
    pub fn record_patronage(
        &self,
        coop_id: &str,
        member_did: &Did,
        amount: i64,
        reference: &str,
    ) -> Result<()> {
        // 1. Dedup check — reject if this reference was already applied.
        let entry_key = self.entry_key(coop_id, member_did, reference);
        if self
            .store
            .get(entry_key.as_bytes())
            .map_err(store_err)?
            .is_some()
        {
            return Err(LedgerError::DuplicateEntry(format!(
                "patronage reference already applied: {reference}"
            )));
        }

        let now = icn_time::current_timestamp_secs();

        // 2. Load or initialize the account.
        let mut account =
            self.get_account(coop_id, member_did)?
                .unwrap_or_else(|| PatronageAccount {
                    coop_id: coop_id.to_string(),
                    member_did: member_did.clone(),
                    balance: 0,
                    last_updated: now,
                });

        // 3. Apply amount (checked arithmetic — overflow is a hard error).
        account.balance = account.balance.checked_add(amount).ok_or_else(|| {
            LedgerError::ArithmeticOverflow(format!(
                "patronage balance overflow for {member_did} in {coop_id}: {} + {amount}",
                account.balance
            ))
        })?;
        account.last_updated = now;

        // 4. Persist the entry record (dedup key + metadata).
        let entry = PatronageEntry {
            coop_id: coop_id.to_string(),
            member_did: member_did.clone(),
            amount,
            reference: reference.to_string(),
            recorded_at: now,
        };
        let entry_bytes = serde_json::to_vec(&entry)
            .map_err(|e| LedgerError::Internal(format!("patronage entry serialization: {e}")))?;
        self.store
            .put(entry_key.as_bytes(), &entry_bytes)
            .map_err(store_err)?;

        // 5. Persist the updated account.
        let account_key = self.account_key(coop_id, member_did);
        let account_bytes = serde_json::to_vec(&account)
            .map_err(|e| LedgerError::Internal(format!("patronage account serialization: {e}")))?;
        self.store
            .put(account_key.as_bytes(), &account_bytes)
            .map_err(store_err)?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Read operations
    // -----------------------------------------------------------------------

    /// Return the current balance for a member in a cooperative.
    ///
    /// Returns `0` if no entries have been recorded yet.
    pub fn get_balance(&self, coop_id: &str, member_did: &Did) -> Result<i64> {
        Ok(self
            .get_account(coop_id, member_did)?
            .map(|a| a.balance)
            .unwrap_or(0))
    }

    /// Return the full account record for a member, or `None` if not found.
    pub fn get_account(&self, coop_id: &str, member_did: &Did) -> Result<Option<PatronageAccount>> {
        let key = self.account_key(coop_id, member_did);
        match self.store.get(key.as_bytes()).map_err(store_err)? {
            None => Ok(None),
            Some(bytes) => {
                let account = serde_json::from_slice(&bytes).map_err(|e| {
                    LedgerError::Internal(format!("patronage account deserialization: {e}"))
                })?;
                Ok(Some(account))
            }
        }
    }

    /// Return all accounts for a cooperative (for audit / statement generation).
    ///
    /// Results are not sorted; call sites may sort by `member_did` or `balance`
    /// as needed.
    pub fn snapshot(&self, coop_id: &str) -> Result<Vec<PatronageAccount>> {
        let prefix = format!("{ACCOUNT_PREFIX}{coop_id}:");
        let pairs = self.store.scan(prefix.as_bytes()).map_err(store_err)?;
        let mut accounts = Vec::with_capacity(pairs.len());
        for (_, value) in pairs {
            let account: PatronageAccount = serde_json::from_slice(&value).map_err(|e| {
                LedgerError::Internal(format!("patronage account deserialization: {e}"))
            })?;
            accounts.push(account);
        }
        Ok(accounts)
    }

    // -----------------------------------------------------------------------
    // Key helpers
    // -----------------------------------------------------------------------

    fn account_key(&self, coop_id: &str, member_did: &Did) -> String {
        format!("{ACCOUNT_PREFIX}{coop_id}:{}", member_did.as_str())
    }

    fn entry_key(&self, coop_id: &str, member_did: &Did, reference: &str) -> String {
        format!(
            "{ENTRY_PREFIX}{coop_id}:{}:{reference}",
            member_did.as_str()
        )
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn store_err(e: anyhow::Error) -> LedgerError {
    LedgerError::Storage(e.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    fn make_tracker() -> PatronageTracker {
        let store = Arc::new(SledStore::temporary().expect("temp store"));
        PatronageTracker::new(store)
    }

    fn did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    /// Multiple records for the same member accumulate correctly.
    #[test]
    fn test_accrual_accumulates() {
        let tracker = make_tracker();
        let member = did();

        tracker
            .record_patronage("coop-a", &member, 100, "surplus:2025:q1")
            .unwrap();
        tracker
            .record_patronage("coop-a", &member, 200, "surplus:2025:q2")
            .unwrap();
        tracker
            .record_patronage("coop-a", &member, -50, "adjustment:2025:q2")
            .unwrap();

        assert_eq!(tracker.get_balance("coop-a", &member).unwrap(), 250);
    }

    /// Same member DID in two cooperatives keeps completely separate balances.
    #[test]
    fn test_coop_isolation() {
        let tracker = make_tracker();
        let member = did();

        tracker
            .record_patronage("coop-alpha", &member, 400, "surplus:2025")
            .unwrap();
        tracker
            .record_patronage("coop-beta", &member, 100, "surplus:2025")
            .unwrap();

        assert_eq!(tracker.get_balance("coop-alpha", &member).unwrap(), 400);
        assert_eq!(tracker.get_balance("coop-beta", &member).unwrap(), 100);

        // snapshot scopes to one coop
        let snap = tracker.snapshot("coop-alpha").unwrap();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].balance, 400);
    }

    /// The same reference string cannot be applied twice to the same account.
    #[test]
    fn test_dedup_rejects_replay() {
        let tracker = make_tracker();
        let member = did();

        tracker
            .record_patronage("coop-a", &member, 300, "surplus:2025")
            .unwrap();

        let result = tracker.record_patronage("coop-a", &member, 300, "surplus:2025");
        assert!(
            result.is_err(),
            "replay of the same reference must be rejected"
        );
        // Balance must not have changed.
        assert_eq!(tracker.get_balance("coop-a", &member).unwrap(), 300);
    }

    /// Snapshot for an empty coop returns an empty vec, not an error.
    #[test]
    fn test_snapshot_empty_coop() {
        let tracker = make_tracker();
        let snap = tracker.snapshot("empty-coop").unwrap();
        assert!(snap.is_empty());
    }

    /// get_balance for a member with no entries returns 0.
    #[test]
    fn test_get_balance_unknown_member() {
        let tracker = make_tracker();
        let member = did();
        assert_eq!(tracker.get_balance("any-coop", &member).unwrap(), 0);
    }

    /// Negative amount (debit/adjustment) works correctly.
    #[test]
    fn test_negative_amount_debit() {
        let tracker = make_tracker();
        let member = did();

        tracker
            .record_patronage("coop-a", &member, 500, "initial")
            .unwrap();
        tracker
            .record_patronage("coop-a", &member, -200, "chargeback")
            .unwrap();

        assert_eq!(tracker.get_balance("coop-a", &member).unwrap(), 300);
    }

    /// Multiple members in the same coop each have independent balances.
    #[test]
    fn test_multiple_members_independent() {
        let tracker = make_tracker();
        let alice = did();
        let bob = did();

        tracker
            .record_patronage("coop-a", &alice, 100, "ref-alice")
            .unwrap();
        tracker
            .record_patronage("coop-a", &bob, 200, "ref-bob")
            .unwrap();

        assert_eq!(tracker.get_balance("coop-a", &alice).unwrap(), 100);
        assert_eq!(tracker.get_balance("coop-a", &bob).unwrap(), 200);

        let snap = tracker.snapshot("coop-a").unwrap();
        assert_eq!(snap.len(), 2);
    }
}
