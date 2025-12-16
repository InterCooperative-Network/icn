//! Ledger manager for cooperative namespaces

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use icn_identity::Did;
use icn_ledger::{entry::JournalEntryBuilder, Ledger, SharedEventEmitter};
use icn_store::{SledStore, Store};

use crate::coop::CoopId;
use crate::error::{GatewayError, Result};
use crate::events::{EventBroadcaster, GatewayEvent};

/// Ledger manager that maintains separate ledgers per cooperative
pub struct LedgerManager {
    ledgers: Arc<RwLock<HashMap<CoopId, Arc<RwLock<Ledger>>>>>,
    event_broadcaster: Option<Arc<EventBroadcaster>>,
    /// Shared event emitter for ledger events (used by notification triggers)
    ledger_emitter: SharedEventEmitter,
    data_dir: Option<PathBuf>,
    budget_store: Option<Arc<icn_store::budgets::BudgetStore>>,
}

impl Default for LedgerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LedgerManager {
    /// Create a new ledger manager with temporary storage (for testing)
    pub fn new() -> Self {
        Self {
            ledgers: Arc::new(RwLock::new(HashMap::new())),
            event_broadcaster: None,
            ledger_emitter: icn_ledger::create_shared_emitter(),
            data_dir: None,
            budget_store: None,
        }
    }

    /// Create a new ledger manager with persistent storage
    pub fn new_with_storage(data_dir: PathBuf) -> Self {
        Self {
            ledgers: Arc::new(RwLock::new(HashMap::new())),
            event_broadcaster: None,
            ledger_emitter: icn_ledger::create_shared_emitter(),
            data_dir: Some(data_dir),
            budget_store: None,
        }
    }

    /// Get the shared event emitter for ledger events
    ///
    /// Use this to subscribe to ledger events for notifications, logging, etc.
    pub fn get_event_emitter(&self) -> SharedEventEmitter {
        self.ledger_emitter.clone()
    }

    /// Set the event broadcaster for real-time updates
    pub fn set_event_broadcaster(&mut self, broadcaster: Arc<EventBroadcaster>) {
        self.event_broadcaster = Some(broadcaster);
    }

    /// Get or create a ledger for a cooperative
    pub fn get_ledger(&self, coop_id: &CoopId) -> Result<Arc<RwLock<Ledger>>> {
        // SECURITY: Validate coop_id BEFORE using in file path to prevent path traversal
        // This prevents attacks like coop_id = "../../etc/passwd" from accessing arbitrary files
        crate::validation::validate_coop_id(coop_id)?;

        let ledgers = self
            .ledgers
            .read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        if let Some(ledger) = ledgers.get(coop_id) {
            return Ok(ledger.clone());
        }

        drop(ledgers); // Release read lock

        // Create new ledger
        let mut ledgers = self
            .ledgers
            .write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        // Double-check pattern (another thread might have created it)
        if let Some(ledger) = ledgers.get(coop_id) {
            return Ok(ledger.clone());
        }

        // Create store for this coop's ledger
        let store: Arc<dyn Store> = if let Some(ref data_dir) = self.data_dir {
            // Use persistent storage with coop-specific subdirectory
            // coop_id has been validated above to only contain alphanumeric, hyphens, underscores
            let coop_ledger_path = data_dir.join("ledgers").join(coop_id);
            Arc::new(SledStore::open(&coop_ledger_path).map_err(GatewayError::SubstrateError)?)
        } else {
            // Use temporary storage (for testing)
            Arc::new(SledStore::temporary().map_err(GatewayError::SubstrateError)?)
        };

        let mut ledger = Ledger::new(store).map_err(GatewayError::SubstrateError)?;

        // Set event emitter for ledger events (notifications, real-time updates)
        ledger.set_event_emitter(self.ledger_emitter.clone());
        ledger.set_domain_id(coop_id.clone());

        let ledger_arc = Arc::new(RwLock::new(ledger));
        ledgers.insert(coop_id.clone(), ledger_arc.clone());

        Ok(ledger_arc)
    }

    /// Set the budget store for spending limits
    pub fn set_budget_store(&mut self, budget_store: Arc<icn_store::budgets::BudgetStore>) {
        self.budget_store = Some(budget_store);
    }

    /// Create a payment transaction
    pub fn create_payment(
        &self,
        coop_id: &CoopId,
        from: &Did,
        to: &Did,
        amount: i64,
        currency: String,
    ) -> Result<String> {
        // Enforce budget limits if store is available
        if let Some(store) = &self.budget_store {
            let from_account = from.to_string();
            // Check if spending is allowed
            // We only enforce budgets on the 'from' account (sender/payer)
            if !store
                .check_spending(&from_account, amount)
                .map_err(|e| GatewayError::InternalError(format!("Budget check failed: {e}")))?
            {
                return Err(GatewayError::BudgetExceeded(format!(
                    "Budget exceeded for account {from_account}"
                )));
            }
        }

        let ledger_arc = self.get_ledger(coop_id)?;

        // Build the journal entry
        let entry = JournalEntryBuilder::new(from.clone())
            .debit(from.clone(), currency.clone(), amount)
            .credit(to.clone(), currency.clone(), amount)
            .build()
            .map_err(GatewayError::SubstrateError)?;

        // Append to ledger
        let mut ledger = ledger_arc
            .write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        let hash = ledger
            .append_entry(entry)
            .map_err(GatewayError::SubstrateError)?;

        let hash_str = hash.to_hex();

        // Record spending in budget store
        if let Some(store) = &self.budget_store {
            let from_account = from.to_string();
            // We ignore errors here to not fail the transaction if it's already recorded in ledger
            // In a production system, we might want a robust retry mechanism or eventual consistency queue
            if let Err(e) = store.record_spending(&from_account, amount) {
                tracing::error!("Failed to record spending for {}: {}", from_account, e);
            }
        }

        // Broadcast event if broadcaster is available
        if let Some(broadcaster) = &self.event_broadcaster {
            let event = GatewayEvent::PaymentCreated {
                coop_id: coop_id.clone(),
                hash: hash_str.clone(),
                from: from.to_string(),
                to: to.to_string(),
                amount,
                currency,
            };
            let broadcaster = broadcaster.clone();
            let coop_id = coop_id.clone();
            tokio::spawn(async move {
                broadcaster.broadcast(&coop_id, event).await;
            });
        }

        Ok(hash_str)
    }

    /// Get balance for an account
    pub fn get_balance(&self, coop_id: &CoopId, did: &Did, currency: &str) -> Result<i64> {
        let ledger_arc = self.get_ledger(coop_id)?;
        let ledger = ledger_arc
            .read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        Ok(ledger.get_balance(did, currency))
    }

    /// Get all balances for an account
    pub fn get_all_balances(&self, coop_id: &CoopId, did: &Did) -> Result<HashMap<String, i64>> {
        let ledger_arc = self.get_ledger(coop_id)?;
        let ledger = ledger_arc
            .read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        let account_balances = ledger.get_account_balances(did);
        Ok(account_balances.balances)
    }

    /// Get transaction history for a cooperative with pagination
    ///
    /// Security: This method enforces pagination to prevent OOM attacks.
    /// - Loads ALL entries into memory (limitation of current ledger API)
    /// - Applies filtering and pagination AFTER loading
    /// - Returns up to `limit` entries starting from `offset`
    ///
    /// TODO: Update icn-ledger to support cursor-based pagination for efficiency
    pub fn get_history(
        &self,
        coop_id: &CoopId,
        filter_did: Option<&Did>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<icn_ledger::JournalEntry>> {
        let ledger_arc = self.get_ledger(coop_id)?;
        let ledger = ledger_arc
            .read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        // SECURITY: We still load all entries here because the underlying ledger
        // API doesn't support pagination. This is a known limitation.
        // The pagination happens AFTER loading to at least limit what's returned.
        // A full fix would require updating icn-ledger to support cursor-based queries.
        let mut entries = ledger
            .get_all_entries()
            .map_err(GatewayError::SubstrateError)?;

        // Filter by DID if requested
        if let Some(did) = filter_did {
            entries.retain(|entry| entry.accounts.iter().any(|delta| &delta.account_id == did));
        }

        // Apply pagination
        let total = entries.len();
        if offset >= total {
            return Ok(Vec::new());
        }

        // SECURITY: Use saturating_add to prevent integer overflow
        // Even with validation, defense in depth requires safe arithmetic
        let end = offset.saturating_add(limit).min(total);
        Ok(entries[offset..end].to_vec())
    }

    /// Get aggregate transaction statistics for a cooperative
    ///
    /// Returns (transaction_count, total_hours_exchanged) for public stats display.
    /// This endpoint is safe to expose publicly as it only returns aggregate data.
    pub fn get_transaction_stats(&self, coop_id: &CoopId) -> Result<(usize, f64)> {
        let ledger_arc = self.get_ledger(coop_id)?;
        let ledger = ledger_arc
            .read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        // Get all entries
        let entries = ledger
            .get_all_entries()
            .map_err(GatewayError::SubstrateError)?;

        let transaction_count = entries.len();

        // Sum up all transaction amounts (absolute values)
        let mut total_hours_exchanged = 0.0;
        for entry in &entries {
            for delta in &entry.accounts {
                // Use credit amounts (positive side of double-entry)
                if let Some(credit) = delta.credit {
                    total_hours_exchanged += credit as f64;
                }
            }
        }

        Ok((transaction_count, total_hours_exchanged))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::IdentityBundle;

    #[test]
    fn test_create_payment() {
        let mgr = LedgerManager::new();
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        let hash = mgr
            .create_payment(
                &"test-coop".to_string(),
                alice.did(),
                bob.did(),
                10,
                "hours".to_string(),
            )
            .unwrap();

        assert!(!hash.is_empty());

        // Check balances
        let alice_balance = mgr
            .get_balance(&"test-coop".to_string(), alice.did(), "hours")
            .unwrap();
        let bob_balance = mgr
            .get_balance(&"test-coop".to_string(), bob.did(), "hours")
            .unwrap();

        assert_eq!(alice_balance, 10); // Alice is owed 10 hours
        assert_eq!(bob_balance, -10); // Bob owes 10 hours
    }

    #[test]
    fn test_get_all_balances() {
        let mgr = LedgerManager::new();
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        mgr.create_payment(
            &"test-coop".to_string(),
            alice.did(),
            bob.did(),
            10,
            "hours".to_string(),
        )
        .unwrap();

        let balances = mgr
            .get_all_balances(&"test-coop".to_string(), alice.did())
            .unwrap();
        assert_eq!(balances.get("hours"), Some(&10));
    }

    #[test]
    fn test_get_history() {
        let mgr = LedgerManager::new();
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        mgr.create_payment(
            &"test-coop".to_string(),
            alice.did(),
            bob.did(),
            10,
            "hours".to_string(),
        )
        .unwrap();

        // Get all history with pagination
        let history = mgr
            .get_history(&"test-coop".to_string(), None, 0, 100)
            .unwrap();
        assert_eq!(history.len(), 1);

        // Filter by Alice
        let alice_history = mgr
            .get_history(&"test-coop".to_string(), Some(alice.did()), 0, 100)
            .unwrap();
        assert_eq!(alice_history.len(), 1);

        // Filter by random DID (should be empty)
        let other = IdentityBundle::generate().unwrap();
        let other_history = mgr
            .get_history(&"test-coop".to_string(), Some(other.did()), 0, 100)
            .unwrap();
        assert_eq!(other_history.len(), 0);

        // Test pagination
        let empty_page = mgr
            .get_history(&"test-coop".to_string(), None, 10, 100)
            .unwrap();
        assert_eq!(empty_page.len(), 0); // Offset beyond available entries
    }
}
