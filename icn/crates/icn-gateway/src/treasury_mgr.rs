//! Treasury Manager for Gateway API
//!
//! Provides treasury operations for the gateway API.
//! This is a simplified interface that can be backed by:
//! 1. In-memory storage (for standalone gateway)
//! 2. TreasuryManager handle (when integrated with daemon)
//!
//! ## Actor-Backed Mode
//!
//! When created with `with_handle()`, the TreasuryManager delegates all operations
//! to the daemon's TreasuryManager, ensuring:
//! - Single source of truth for treasury data
//! - Persistence across restarts
//! - Proper governance integration for spending rules
//!
//! When created with `new()`, it uses in-memory storage (suitable for testing only).

use anyhow::Result;
use icn_entity::EntityId;
use icn_identity::Did;
use icn_ledger::{
    ApprovalType, Ledger, PaginatedAuditTrail, SpendingRule, Treasury, TreasuryAuditRecord,
    TreasuryBudget, TreasuryManager as LedgerTreasuryManager, TreasuryOperation,
};
use icn_obs::metrics::treasury as treasury_metrics;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Handle type for actor-backed treasury
///
/// This wraps the daemon's TreasuryManager for gateway integration.
pub type TreasuryHandle = Arc<RwLock<LedgerTreasuryManager>>;

/// Handle type for ledger balance queries
///
/// This wraps the daemon's Ledger for balance lookups.
pub type LedgerHandle = Arc<RwLock<Ledger>>;

/// Treasury manager for gateway API
///
/// Supports two modes:
/// - **Standalone mode** (`new()`): In-memory storage, for testing only
/// - **Actor-backed mode** (`with_handle()`): Delegates to daemon's TreasuryManager
///
/// When a `LedgerHandle` is set (via `set_ledger_handle`), balance queries
/// return actual ledger balances instead of placeholders.
pub struct GatewayTreasuryManager {
    /// In-memory TreasuryManager - used in standalone mode
    standalone: Option<LedgerTreasuryManager>,
    /// Optional handle to daemon's TreasuryManager (actor-backed mode)
    treasury_handle: Option<TreasuryHandle>,
    /// Optional handle to daemon's Ledger (for balance queries)
    ledger_handle: Option<LedgerHandle>,
    /// Optional ledger service handle for treasury nonce queries.
    ledger_service_handle: Option<Arc<dyn icn_kernel_api::services::LedgerService>>,
}

impl GatewayTreasuryManager {
    /// Create a new treasury manager with in-memory storage
    ///
    /// **Warning**: This mode is for testing only. State is lost on restart
    /// and not integrated with governance.
    pub fn new() -> Self {
        debug!("GatewayTreasuryManager created in standalone mode (in-memory only)");
        GatewayTreasuryManager {
            standalone: Some(LedgerTreasuryManager::new()),
            treasury_handle: None,
            ledger_handle: None,
            ledger_service_handle: None,
        }
    }

    /// Create a treasury manager backed by the daemon's TreasuryManager
    ///
    /// This is the recommended mode for production. All operations delegate
    /// to the daemon's TreasuryManager, ensuring:
    /// - Persistence across restarts
    /// - Governance integration for spending rules
    /// - Single source of truth
    pub fn with_handle(handle: TreasuryHandle) -> Self {
        debug!("GatewayTreasuryManager created with daemon TreasuryManager handle");
        GatewayTreasuryManager {
            standalone: None,
            treasury_handle: Some(handle),
            ledger_handle: None,
            ledger_service_handle: None,
        }
    }

    /// Set ledger handle for balance queries
    ///
    /// When set, balance queries return actual ledger balances.
    /// When not set, balance endpoints return None or 503.
    pub fn set_ledger_handle(&mut self, handle: LedgerHandle) {
        debug!("GatewayTreasuryManager wired to daemon Ledger for balance queries");
        self.ledger_handle = Some(handle);
    }

    /// Set ledger service handle for nonce queries.
    pub fn set_ledger_service_handle(
        &mut self,
        handle: Arc<dyn icn_kernel_api::services::LedgerService>,
    ) {
        debug!("GatewayTreasuryManager wired to daemon LedgerService for nonce queries");
        self.ledger_service_handle = Some(handle);
    }

    /// Check if running in actor-backed mode
    pub fn is_actor_backed(&self) -> bool {
        self.treasury_handle.is_some()
    }

    /// Check if ledger is wired for balance queries
    pub fn is_ledger_wired(&self) -> bool {
        self.ledger_handle.is_some()
    }

    // ============================================================================
    // Treasury Operations
    // ============================================================================

    /// Get treasury by cooperative ID
    pub async fn get_treasury_by_coop(&self, coop_id: &str) -> Result<Option<Treasury>> {
        if let Some(ref handle) = self.treasury_handle {
            let mgr = handle.read().await;
            return Ok(mgr.get_treasury_by_coop(coop_id).cloned());
        }

        // Standalone mode
        if let Some(ref standalone) = self.standalone {
            return Ok(standalone.get_treasury_by_coop(coop_id).cloned());
        }

        Ok(None)
    }

    /// Get treasury by DID
    pub async fn get_treasury(&self, treasury_did: &Did) -> Result<Option<Treasury>> {
        if let Some(ref handle) = self.treasury_handle {
            let mgr = handle.read().await;
            return Ok(mgr.get_treasury(treasury_did).cloned());
        }

        if let Some(ref standalone) = self.standalone {
            return Ok(standalone.get_treasury(treasury_did).cloned());
        }

        Ok(None)
    }

    /// Register a new treasury for a cooperative (legacy API)
    ///
    /// **Deprecated**: Use `register_treasury_with_entity()` for type-safe entity references.
    pub async fn register_treasury(
        &self,
        treasury_did: Did,
        coop_id: String,
        currency: String,
        created_by: Did,
        description: Option<String>,
    ) -> Result<Treasury> {
        if let Some(ref handle) = self.treasury_handle {
            let mut mgr = handle.write().await;
            return mgr.register_treasury(treasury_did, coop_id, currency, created_by, description);
        }

        anyhow::bail!("Treasury registration not supported in standalone mode")
    }

    /// Register a new treasury for an entity (preferred API)
    ///
    /// Uses type-safe `EntityId` instead of string `coop_id`.
    /// The `coop_id` is automatically derived from `entity_id.identifier()`.
    pub async fn register_treasury_with_entity(
        &self,
        treasury_did: Did,
        entity_id: EntityId,
        currency: String,
        created_by: Did,
        description: Option<String>,
    ) -> Result<Treasury> {
        if let Some(ref handle) = self.treasury_handle {
            let mut mgr = handle.write().await;
            return mgr.register_treasury_with_entity(
                treasury_did,
                entity_id,
                currency,
                created_by,
                description,
            );
        }

        anyhow::bail!("Treasury registration not supported in standalone mode")
    }

    /// Get treasury by entity ID
    ///
    /// Looks up a treasury using its associated entity ID.
    pub async fn get_treasury_by_entity(&self, entity_id: &EntityId) -> Result<Option<Treasury>> {
        // Entity ID's identifier matches the coop_id
        self.get_treasury_by_coop(entity_id.identifier()).await
    }

    // ============================================================================
    // Budget Operations
    // ============================================================================

    /// List budgets for a treasury
    pub async fn list_budgets(&self, treasury_did: &Did) -> Result<Vec<TreasuryBudget>> {
        if let Some(ref handle) = self.treasury_handle {
            let mgr = handle.read().await;
            return Ok(mgr
                .list_budgets(treasury_did)
                .into_iter()
                .cloned()
                .collect());
        }

        if let Some(ref standalone) = self.standalone {
            return Ok(standalone
                .list_budgets(treasury_did)
                .into_iter()
                .cloned()
                .collect());
        }

        Ok(Vec::new())
    }

    /// Get a specific budget
    pub async fn get_budget(&self, budget_id: &str) -> Result<Option<TreasuryBudget>> {
        if let Some(ref handle) = self.treasury_handle {
            let mgr = handle.read().await;
            return Ok(mgr.get_budget(budget_id).cloned());
        }

        if let Some(ref standalone) = self.standalone {
            return Ok(standalone.get_budget(budget_id).cloned());
        }

        Ok(None)
    }

    /// Create a new budget
    pub async fn create_budget(
        &self,
        treasury_did: Did,
        purpose: String,
        amount: i64,
        currency: String,
        period_end: Option<u64>,
        created_by: Did,
        proposal_id: Option<String>,
    ) -> Result<TreasuryBudget> {
        if let Some(ref handle) = self.treasury_handle {
            let mut mgr = handle.write().await;
            return mgr.create_budget(
                treasury_did,
                purpose,
                amount,
                currency,
                period_end,
                created_by,
                proposal_id,
            );
        }

        anyhow::bail!("Budget creation not supported in standalone mode")
    }

    // ============================================================================
    // Spending Rules
    // ============================================================================

    /// List spending rules for a treasury
    pub async fn list_spending_rules(&self, treasury_did: &Did) -> Result<Vec<SpendingRule>> {
        if let Some(ref handle) = self.treasury_handle {
            let mgr = handle.read().await;
            return Ok(mgr
                .list_spending_rules(treasury_did)
                .into_iter()
                .cloned()
                .collect());
        }

        if let Some(ref standalone) = self.standalone {
            return Ok(standalone
                .list_spending_rules(treasury_did)
                .into_iter()
                .cloned()
                .collect());
        }

        Ok(Vec::new())
    }

    /// Check if an amount requires governance approval
    pub async fn requires_approval(
        &self,
        treasury_did: &Did,
        amount: i64,
        currency: &str,
    ) -> Result<Option<ApprovalType>> {
        if let Some(ref handle) = self.treasury_handle {
            let mgr = handle.read().await;
            return Ok(mgr.requires_approval(treasury_did, amount, currency));
        }

        if let Some(ref standalone) = self.standalone {
            return Ok(standalone.requires_approval(treasury_did, amount, currency));
        }

        Ok(None)
    }

    // ============================================================================
    // Audit Trail
    // ============================================================================

    /// Get audit trail for a treasury
    pub async fn get_audit_trail(
        &self,
        treasury_did: &Did,
        limit: usize,
        offset: usize,
    ) -> Result<PaginatedAuditTrail> {
        if let Some(ref handle) = self.treasury_handle {
            let mgr = handle.read().await;
            return mgr.get_audit_trail(treasury_did, limit, offset);
        }

        if let Some(ref standalone) = self.standalone {
            return standalone.get_audit_trail(treasury_did, limit, offset);
        }

        Ok(PaginatedAuditTrail {
            records: Vec::new(),
            total: 0,
            offset,
            limit,
        })
    }

    /// Record an audit entry
    pub async fn record_audit(
        &self,
        treasury_did: &Did,
        operation: TreasuryOperation,
        performed_by: Did,
        balance_after: i64,
        proposal_id: Option<String>,
        ledger_entry_hash: Option<icn_ledger::ContentHash>,
    ) -> Result<TreasuryAuditRecord> {
        if let Some(ref handle) = self.treasury_handle {
            let mut mgr = handle.write().await;
            return mgr.record_audit(
                treasury_did,
                operation,
                performed_by,
                balance_after,
                proposal_id,
                ledger_entry_hash,
            );
        }

        anyhow::bail!("Audit recording not supported in standalone mode")
    }

    // ============================================================================
    // Ledger Balance Queries
    // ============================================================================

    /// Get balance for a treasury account from ledger
    ///
    /// Returns the balance for a specific currency, or None if ledger not wired.
    pub async fn get_treasury_balance(
        &self,
        treasury_did: &Did,
        currency: &str,
    ) -> Result<Option<i64>> {
        if let Some(ref ledger_handle) = self.ledger_handle {
            let ledger = ledger_handle.read().await;
            let balance = ledger.get_balance(treasury_did, currency);
            return Ok(Some(balance));
        }
        Ok(None) // Ledger not wired
    }

    /// Get all balances for a treasury account
    ///
    /// Returns a map of currency -> balance, or empty if ledger not wired.
    pub async fn get_all_treasury_balances(
        &self,
        treasury_did: &Did,
    ) -> Result<HashMap<String, i64>> {
        if let Some(ref ledger_handle) = self.ledger_handle {
            let ledger = ledger_handle.read().await;
            let account_balances = ledger.get_account_balances(treasury_did);
            return Ok(account_balances.balances);
        }
        Ok(HashMap::new()) // Ledger not wired
    }

    /// Get current nonce for a treasury identity from the ledger service.
    ///
    /// Returns `None` when no ledger service is wired.
    pub async fn get_treasury_nonce(&self, treasury_id: &str) -> Result<Option<u64>> {
        if let Some(ref service) = self.ledger_service_handle {
            let nonce = service
                .get_treasury_nonce(treasury_id)
                .map_err(anyhow::Error::msg)?;
            return Ok(Some(nonce));
        }
        Ok(None)
    }

    // ============================================================================
    // Ledger Write Operations
    // ============================================================================

    /// Create a deposit entry in the ledger
    ///
    /// Creates a journal entry that transfers value from the depositor to the treasury.
    /// In ICN's mutual credit accounting:
    /// - The depositor is credited (balance decreases = giving funds)
    /// - The treasury is debited (balance increases = receiving funds)
    ///
    /// The balance is captured atomically with the ledger update to ensure
    /// the audit trail reflects the correct post-deposit balance.
    ///
    /// Returns the entry hash on success.
    ///
    /// Requires ledger_handle to be wired. Treasury handle is optional but
    /// required for audit trail recording.
    pub async fn create_deposit(
        &self,
        treasury_did: &Did,
        from_did: &Did,
        amount: i64,
        currency: String,
        memo: Option<String>,
    ) -> Result<icn_ledger::ContentHash> {
        let ledger_handle = self
            .ledger_handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Ledger not wired - deposits require daemon mode"))?;

        // Build the journal entry: credit depositor (giving funds), debit treasury (receiving funds)
        // In ICN's mutual credit accounting:
        // - Credit = balance decreases (giving value)
        // - Debit = balance increases (receiving value)
        let mut entry = icn_ledger::entry::JournalEntryBuilder::new(from_did.clone())
            .credit(from_did.clone(), currency.clone(), amount) // Depositor gives funds
            .debit(treasury_did.clone(), currency.clone(), amount) // Treasury receives funds
            .build()?;

        // Compute the hash
        let hash = entry.compute_hash()?;

        // Append to ledger and get new balance atomically to avoid race conditions.
        // We must capture the balance while holding the lock to ensure consistency
        // between the ledger state and the audit trail.
        let new_balance = {
            let mut ledger = ledger_handle.write().await;
            ledger.append_entry(entry).await?;
            ledger.get_balance(treasury_did, &currency)
        };

        // Capture currency for metrics before moving into operation
        let currency_for_metrics = currency.clone();

        // Record audit trail if treasury handle is available
        if self.treasury_handle.is_some() {
            let operation = TreasuryOperation::Deposit {
                from: from_did.clone(),
                amount,
                currency,
                memo,
            };

            self.record_audit(
                treasury_did,
                operation,
                from_did.clone(),
                new_balance,
                None, // No proposal for deposits
                Some(hash.clone()),
            )
            .await?;
        }

        // Emit metrics
        treasury_metrics::deposit_processed_inc();
        treasury_metrics::deposit_by_currency_inc(&currency_for_metrics);
        treasury_metrics::balance_set(
            &treasury_did.to_string(),
            &currency_for_metrics,
            new_balance,
        );

        debug!(
            treasury = %treasury_did,
            from = %from_did,
            amount = amount,
            hash = %hash,
            "Created treasury deposit entry"
        );

        Ok(hash)
    }
}

impl Default for GatewayTreasuryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_standalone_mode() {
        let mgr = GatewayTreasuryManager::new();
        assert!(!mgr.is_actor_backed());

        // Treasury should not exist
        let result = mgr.get_treasury_by_coop("test-coop").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_actor_backed_mode() {
        let treasury_mgr = Arc::new(RwLock::new(LedgerTreasuryManager::new()));
        let mgr = GatewayTreasuryManager::with_handle(treasury_mgr);
        assert!(mgr.is_actor_backed());
    }
}
