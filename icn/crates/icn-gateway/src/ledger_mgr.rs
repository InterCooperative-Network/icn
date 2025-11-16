//! Ledger manager for cooperative namespaces

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use icn_identity::Did;
use icn_ledger::{entry::JournalEntryBuilder, Ledger};
use icn_store::{SledStore, Store};

use crate::coop::CoopId;
use crate::error::{GatewayError, Result};
use crate::events::{EventBroadcaster, GatewayEvent};

/// Ledger manager that maintains separate ledgers per cooperative
pub struct LedgerManager {
    ledgers: Arc<RwLock<HashMap<CoopId, Arc<RwLock<Ledger>>>>>,
    event_broadcaster: Option<Arc<EventBroadcaster>>,
}

impl Default for LedgerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LedgerManager {
    /// Create a new ledger manager
    pub fn new() -> Self {
        Self {
            ledgers: Arc::new(RwLock::new(HashMap::new())),
            event_broadcaster: None,
        }
    }

    /// Set the event broadcaster for real-time updates
    pub fn set_event_broadcaster(&mut self, broadcaster: Arc<EventBroadcaster>) {
        self.event_broadcaster = Some(broadcaster);
    }

    /// Get or create a ledger for a cooperative
    pub fn get_ledger(&self, coop_id: &CoopId) -> Result<Arc<RwLock<Ledger>>> {
        let ledgers = self.ledgers.read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        if let Some(ledger) = ledgers.get(coop_id) {
            return Ok(ledger.clone());
        }

        drop(ledgers); // Release read lock

        // Create new ledger
        let mut ledgers = self.ledgers.write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        // Double-check pattern (another thread might have created it)
        if let Some(ledger) = ledgers.get(coop_id) {
            return Ok(ledger.clone());
        }

        // Create temporary store for this coop's ledger
        // TODO: Use persistent storage with proper paths in production
        let store: Arc<dyn Store> = Arc::new(SledStore::temporary()
            .map_err(GatewayError::SubstrateError)?);
        let ledger = Ledger::new(store)
            .map_err(GatewayError::SubstrateError)?;

        let ledger_arc = Arc::new(RwLock::new(ledger));
        ledgers.insert(coop_id.clone(), ledger_arc.clone());

        Ok(ledger_arc)
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
        let ledger_arc = self.get_ledger(coop_id)?;

        // Build the journal entry
        let entry = JournalEntryBuilder::new(from.clone())
            .debit(from.clone(), currency.clone(), amount)
            .credit(to.clone(), currency.clone(), amount)
            .build()
            .map_err(GatewayError::SubstrateError)?;

        // Append to ledger
        let mut ledger = ledger_arc.write()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        let hash = ledger.append_entry(entry)
            .map_err(GatewayError::SubstrateError)?;

        let hash_str = hash.to_hex();

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
        let ledger = ledger_arc.read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        Ok(ledger.get_balance(did, currency))
    }

    /// Get all balances for an account
    pub fn get_all_balances(&self, coop_id: &CoopId, did: &Did) -> Result<HashMap<String, i64>> {
        let ledger_arc = self.get_ledger(coop_id)?;
        let ledger = ledger_arc.read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        let account_balances = ledger.get_account_balances(did);
        Ok(account_balances.balances)
    }

    /// Get transaction history for a cooperative
    pub fn get_history(&self, coop_id: &CoopId, filter_did: Option<&Did>) -> Result<Vec<icn_ledger::JournalEntry>> {
        let ledger_arc = self.get_ledger(coop_id)?;
        let ledger = ledger_arc.read()
            .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

        let mut entries = ledger.get_all_entries()
            .map_err(GatewayError::SubstrateError)?;

        // Filter by DID if requested
        if let Some(did) = filter_did {
            entries.retain(|entry| {
                entry.accounts.iter().any(|delta| &delta.account_id == did)
            });
        }

        Ok(entries)
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

        let hash = mgr.create_payment(
            &"test-coop".to_string(),
            alice.did(),
            bob.did(),
            10,
            "hours".to_string(),
        ).unwrap();

        assert!(!hash.is_empty());

        // Check balances
        let alice_balance = mgr.get_balance(&"test-coop".to_string(), alice.did(), "hours").unwrap();
        let bob_balance = mgr.get_balance(&"test-coop".to_string(), bob.did(), "hours").unwrap();

        assert_eq!(alice_balance, 10);  // Alice is owed 10 hours
        assert_eq!(bob_balance, -10);   // Bob owes 10 hours
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
        ).unwrap();

        let balances = mgr.get_all_balances(&"test-coop".to_string(), alice.did()).unwrap();
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
        ).unwrap();

        let history = mgr.get_history(&"test-coop".to_string(), None).unwrap();
        assert_eq!(history.len(), 1);

        // Filter by Alice
        let alice_history = mgr.get_history(&"test-coop".to_string(), Some(alice.did())).unwrap();
        assert_eq!(alice_history.len(), 1);

        // Filter by random DID (should be empty)
        let other = IdentityBundle::generate().unwrap();
        let other_history = mgr.get_history(&"test-coop".to_string(), Some(other.did())).unwrap();
        assert_eq!(other_history.len(), 0);
    }
}
