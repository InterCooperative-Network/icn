//! Ledger manager for cooperative namespaces

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use icn_identity::Did;
use icn_ledger::{
    entry::JournalEntryBuilder, CurrencyPair, FxConversionDetails, Ledger, SharedEventEmitter,
};
use icn_store::{SledStore, Store};

use crate::models::{CrossPaymentQuote, CrossPaymentResponse};

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
    /// - Uses efficient cursor-based pagination from ledger's timestamp index
    /// - When filtering by DID, loads all entries for filtering (still needed)
    /// - Returns up to `limit` entries starting from `offset`
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

        // If no DID filter, use efficient cursor-based pagination
        if filter_did.is_none() {
            let (entries, _total) = ledger
                .get_entries_paginated_asc(offset, limit)
                .map_err(GatewayError::SubstrateError)?;
            return Ok(entries);
        }

        // When filtering by DID, we need to load all entries for filtering
        // (the timestamp index doesn't support DID-based queries)
        let mut entries = ledger
            .get_all_entries()
            .map_err(GatewayError::SubstrateError)?;

        // Filter by DID
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

    /// Create a cross-currency payment
    ///
    /// Transfers funds between two currencies using the exchange rate oracle.
    /// The sender pays in `from_currency` and the recipient receives in `to_currency`.
    ///
    /// Returns the entry hash and conversion details for audit/display.
    ///
    /// # Note on locking
    /// This method holds a read lock across an await point during preparation.
    /// This is acceptable because read locks allow concurrent readers and don't block.
    #[allow(clippy::await_holding_lock)]
    pub async fn create_cross_payment(
        &self,
        coop_id: &CoopId,
        from: &Did,
        to: &Did,
        amount: i64,
        from_currency: &str,
        to_currency: &str,
        max_target_amount: Option<i64>,
        _memo: Option<String>,
    ) -> Result<CrossPaymentResponse> {
        // Enforce budget limits if store is available
        if let Some(store) = &self.budget_store {
            let from_account = from.to_string();
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

        // Prepare the transfer (fetches rate from oracle) - only needs read lock
        let prepared = {
            let ledger = ledger_arc
                .read()
                .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

            ledger
                .prepare_cross_currency_transfer(
                    from,
                    to,
                    amount,
                    from_currency,
                    to_currency,
                    max_target_amount,
                )
                .await
                .map_err(|e| GatewayError::InternalError(format!("FX error: {e}")))?
        };

        // Execute the transfer (writes to ledger) - needs write lock
        let (hash, details) = {
            let mut ledger = ledger_arc
                .write()
                .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

            let (hash, details) = ledger
                .execute_cross_currency_transfer(prepared)
                .map_err(|e| GatewayError::InternalError(format!("FX execution error: {e}")))?;

            (hash.to_hex(), details)
        };

        // Record spending in budget store
        if let Some(store) = &self.budget_store {
            let from_account = from.to_string();
            if let Err(e) = store.record_spending(&from_account, amount) {
                tracing::error!("Failed to record spending for {}: {}", from_account, e);
            }
        }

        // Broadcast event if broadcaster is available
        if let Some(broadcaster) = &self.event_broadcaster {
            let event = GatewayEvent::CrossPaymentCreated {
                coop_id: coop_id.clone(),
                hash: hash.clone(),
                from: from.to_string(),
                to: to.to_string(),
                source_amount: amount,
                from_currency: from_currency.to_string(),
                target_amount: details.net_target_amount,
                to_currency: to_currency.to_string(),
                rate: details.rate,
            };
            let broadcaster = broadcaster.clone();
            let coop_id = coop_id.clone();
            tokio::spawn(async move {
                broadcaster.broadcast(&coop_id, event).await;
            });
        }

        Ok(CrossPaymentResponse {
            hash,
            from: from.to_string(),
            to: to.to_string(),
            source_amount: amount,
            from_currency: from_currency.to_string(),
            gross_target_amount: details.gross_target_amount,
            fee_amount: details.fee_amount,
            net_target_amount: details.net_target_amount,
            to_currency: to_currency.to_string(),
            rate_used: details.rate,
            rate_timestamp: details.rate_timestamp,
            rate_sources: details.rate_sources,
        })
    }

    /// Get a quote for a cross-currency payment without executing it
    ///
    /// Returns the expected conversion details including rate, fees, and amounts.
    /// The quote is valid for a limited time based on oracle TTL.
    pub async fn get_cross_payment_quote(
        &self,
        coop_id: &CoopId,
        amount: i64,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<CrossPaymentQuote> {
        let ledger_arc = self.get_ledger(coop_id)?;

        // Get FX config and oracle under lock, then release before await
        let (fx_config, oracle) = {
            let ledger = ledger_arc
                .read()
                .map_err(|e| GatewayError::InternalError(format!("Lock poisoned: {e}")))?;

            let fx_config = ledger.fx_config().cloned().ok_or_else(|| {
                GatewayError::InternalError("Cross-currency payments not configured".to_string())
            })?;

            let oracle = ledger.oracle_manager().cloned().ok_or_else(|| {
                GatewayError::InternalError("Exchange rate oracle not configured".to_string())
            })?;

            (fx_config, oracle)
        };

        // Get current rate (after releasing lock)
        let pair = CurrencyPair::new(from_currency, to_currency);
        let rate = oracle
            .get_rate(&pair)
            .await
            .map_err(|e| GatewayError::InternalError(format!("Oracle error: {e}")))?;

        // Check staleness
        if rate.is_stale && !fx_config.allow_stale_rates {
            return Err(GatewayError::InternalError(
                "Exchange rate is stale and stale rates are not allowed".to_string(),
            ));
        }

        // Calculate conversion
        let gross_target_amount = rate
            .convert(amount)
            .ok_or_else(|| GatewayError::InternalError("Conversion would overflow".to_string()))?;

        // Calculate fee using same logic as actual execution for consistency
        let fee_amount = fx_config.calculate_fee(gross_target_amount);

        let net_target_amount = gross_target_amount - fee_amount;

        // Quote valid until rate expires
        let valid_until = rate.aggregated_at + rate.ttl_secs;

        Ok(CrossPaymentQuote {
            source_amount: amount,
            from_currency: from_currency.to_string(),
            gross_target_amount,
            fee_amount,
            net_target_amount,
            to_currency: to_currency.to_string(),
            rate: rate.rate,
            rate_timestamp: rate.aggregated_at,
            rate_sources: rate.sources(),
            valid_until,
            is_stale: rate.is_stale,
        })
    }

    /// Get conversion details for a cross-currency transfer
    ///
    /// This is a lower-level method that returns the FxConversionDetails directly.
    #[allow(dead_code)]
    pub fn get_fx_conversion_details(&self, details: &FxConversionDetails) -> CrossPaymentResponse {
        CrossPaymentResponse {
            hash: String::new(), // Not available without actual execution
            from: String::new(),
            to: String::new(),
            source_amount: details.source_amount,
            from_currency: details.from_currency.clone(),
            gross_target_amount: details.gross_target_amount,
            fee_amount: details.fee_amount,
            net_target_amount: details.net_target_amount,
            to_currency: details.to_currency.clone(),
            rate_used: details.rate,
            rate_timestamp: details.rate_timestamp,
            rate_sources: details.rate_sources.clone(),
        }
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
