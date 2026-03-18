//! Cooperative Treasury System
//!
//! This module provides treasury management for cooperative-owned accounts with
//! governance-controlled spending. Treasuries enable cooperatives to:
//! - Pool resources for collective operations
//! - Allocate budgets to projects or working groups
//! - Enforce spending rules requiring governance approval
//! - Maintain complete audit trails of all treasury operations
//!
//! ## Core Concepts
//!
//! - **Treasury**: A cooperative-owned account identified by a DID
//! - **Budget**: An earmarked allocation from treasury for a specific purpose
//! - **Spending Rule**: Thresholds that trigger governance approval requirements
//! - **Audit Trail**: Complete history of all treasury operations
//!
//! ## Module Organization
//!
//! This module is organized into focused submodules by operation type:
//!
//! - `budgets`: Budget lifecycle management (create, spend, status updates)
//! - `approvals`: Governance rules, velocity limits, and approval logic
//! - `audit`: Audit trail recording and paginated retrieval
//!
//! All types are re-exported from this module for backward compatibility.
//!
//! ## Example
//!
//! ```rust,no_run
//! use icn_ledger::treasury::{TreasuryManager, Treasury, SpendingRule, ApprovalType};
//! use icn_identity::KeyPair;
//! use icn_store::SledStore;
//! use std::sync::Arc;
//!
//! # fn main() -> anyhow::Result<()> {
//! let store = Arc::new(SledStore::open("./data")?);
//! let mut treasury_mgr = TreasuryManager::with_store(store)?;
//!
//! let treasury_keypair = KeyPair::generate()?;
//! let treasury_did = treasury_keypair.did().clone();
//! let admin = KeyPair::generate()?.did().clone();
//!
//! // Register a treasury for a cooperative
//! treasury_mgr.register_treasury(
//!     treasury_did.clone(),
//!     "food-coop".to_string(),
//!     "hours".to_string(),
//!     admin.clone(),
//!     None,
//! )?;
//!
//! // Add spending rule requiring governance for large withdrawals
//! let rule = SpendingRule::new(
//!     treasury_did.clone(),
//!     "Large withdrawal approval".to_string(),
//!     1000, // Amounts > 1000 need approval
//!     "hours".to_string(),
//!     ApprovalType::SimpleMajority,
//! );
//! treasury_mgr.add_spending_rule(rule)?;
//!
//! // Check if withdrawal requires approval
//! assert!(treasury_mgr.requires_approval(&treasury_did, 1500, "hours").is_some());
//! assert!(treasury_mgr.requires_approval(&treasury_did, 500, "hours").is_none());
//! # Ok(())
//! # }
//! ```

use crate::labor_shares::{
    BondId, BondPaymentType, BondStatus, CooperativeBond, LaborShare, ScheduledPayout, ShareId,
    SurplusAllocation,
};
use crate::types::JournalEntry;
use anyhow::{bail, Result};
use icn_entity::EntityId;
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

// Submodules (private - types are re-exported below)
mod approvals;
mod audit;
mod budgets;

// Re-export types from submodules
pub use approvals::{ApprovalType, SpendingRule, VelocityLimit, VelocityWindow};
pub use audit::{PaginatedAuditTrail, TreasuryAuditRecord, TreasuryOperation};
pub use budgets::{BudgetStatus, TreasuryBudget};

// Storage key prefixes
const TREASURY_PREFIX: &str = "ledger:treasury:";
const BUDGET_PREFIX: &str = "ledger:treasury:budget:";
const SPENDING_RULE_PREFIX: &str = "ledger:treasury:rule:";
const TREASURY_AUDIT_PREFIX: &str = "ledger:treasury:audit:";
const TREASURY_IDX_COOP_PREFIX: &str = "ledger:treasury:idx:coop:";
const TREASURY_IDX_BUDGETS_PREFIX: &str = "ledger:treasury:idx:budgets:";

// Labor share storage prefixes
const LABOR_SHARE_PREFIX: &str = "ledger:labor_share:";
const BOND_PREFIX: &str = "ledger:bond:";
const SURPLUS_ALLOCATION_PREFIX: &str = "ledger:surplus_allocation:";
// Index prefixes for future direct lookups (indices currently rebuilt on load)
#[allow(dead_code)]
const LABOR_SHARE_IDX_HOLDER_PREFIX: &str = "ledger:labor_share:idx:holder:";
#[allow(dead_code)]
const LABOR_SHARE_IDX_COOP_PREFIX: &str = "ledger:labor_share:idx:coop:";
#[allow(dead_code)]
const BOND_IDX_ISSUER_PREFIX: &str = "ledger:bond:idx:issuer:";

/// Treasury account configuration for a cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Treasury {
    /// The treasury DID (cooperative-owned account)
    pub treasury_did: Did,

    /// Entity that owns this treasury (cooperative or federation)
    ///
    /// This provides type-safe organizational identity. When set, `coop_id`
    /// is derived from `entity_id.identifier()` for backwards compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<EntityId>,

    /// Cooperative/domain this treasury belongs to
    ///
    /// **Deprecated**: Use `entity_id` instead. Retained for backwards compatibility.
    /// When `entity_id` is set, this field is automatically derived from it.
    pub coop_id: String,

    /// Currency managed by this treasury
    pub currency: String,

    /// When the treasury was created (Unix timestamp)
    pub created_at: u64,

    /// Who created/authorized the treasury
    pub created_by: Did,

    /// Optional description/purpose
    pub description: Option<String>,

    /// Whether the treasury is active
    pub is_active: bool,
}

impl Treasury {
    /// Create a new treasury with entity_id
    ///
    /// The `entity_id` provides type-safe organizational identity.
    /// The `coop_id` field is derived from `entity_id.identifier()` for
    /// backwards compatibility with existing code.
    pub fn new_with_entity(
        treasury_did: Did,
        entity_id: EntityId,
        currency: String,
        created_by: Did,
        description: Option<String>,
    ) -> Self {
        let coop_id = entity_id.identifier().to_string();
        Self {
            treasury_did,
            entity_id: Some(entity_id),
            coop_id,
            currency,
            created_at: icn_time::current_timestamp_secs(),
            created_by,
            description,
            is_active: true,
        }
    }

    /// Create a new treasury (legacy API)
    ///
    /// **Deprecated**: Use `new_with_entity` instead for type-safe entity references.
    pub fn new(
        treasury_did: Did,
        coop_id: String,
        currency: String,
        created_by: Did,
        description: Option<String>,
    ) -> Self {
        Self {
            treasury_did,
            entity_id: None,
            coop_id,
            currency,
            created_at: icn_time::current_timestamp_secs(),
            created_by,
            description,
            is_active: true,
        }
    }

    /// Get the owning entity ID, if set
    pub fn entity_id(&self) -> Option<&EntityId> {
        self.entity_id.as_ref()
    }

    /// Get the cooperative/domain identifier
    ///
    /// This returns the coop_id string regardless of whether entity_id is set.
    pub fn coop_id(&self) -> &str {
        &self.coop_id
    }
}

/// Manager for treasury accounts with persistent storage
pub struct TreasuryManager {
    /// Storage backend
    store: Option<Arc<dyn Store>>,

    /// In-memory cache of treasuries by DID
    treasuries: HashMap<Did, Treasury>,

    /// In-memory cache of budgets by ID
    budgets: HashMap<String, TreasuryBudget>,

    /// Index: treasury DID -> budget IDs
    treasury_budgets: HashMap<Did, Vec<String>>,

    /// In-memory cache of spending rules by ID
    spending_rules: HashMap<String, SpendingRule>,

    /// Index: treasury DID -> rule IDs
    treasury_rules: HashMap<Did, Vec<String>>,

    /// Index: coop_id -> treasury DID
    coop_treasuries: HashMap<String, Did>,

    /// Velocity limits by ID
    velocity_limits: HashMap<String, VelocityLimit>,

    /// Index: treasury DID -> velocity limit IDs
    treasury_velocity_limits: HashMap<Did, Vec<String>>,

    /// Velocity tracking windows: (treasury_did, currency) -> window state
    velocity_windows: HashMap<(Did, String), VelocityWindow>,

    // === Labor Share State (Razeto Integration) ===
    /// Labor shares by ID
    labor_shares: HashMap<ShareId, LaborShare>,

    /// Index: holder DID -> share IDs
    holder_shares: HashMap<Did, Vec<ShareId>>,

    /// Index: cooperative ID -> share IDs
    coop_shares: HashMap<String, Vec<ShareId>>,

    /// Cooperative bonds by ID
    bonds: HashMap<BondId, CooperativeBond>,

    /// Index: issuer coop ID -> bond IDs
    issuer_bonds: HashMap<String, Vec<BondId>>,

    /// Surplus allocations by ID
    surplus_allocations: HashMap<String, SurplusAllocation>,
}

impl TreasuryManager {
    /// Create a new treasury manager (in-memory only)
    pub fn new() -> Self {
        Self {
            store: None,
            treasuries: HashMap::new(),
            budgets: HashMap::new(),
            treasury_budgets: HashMap::new(),
            spending_rules: HashMap::new(),
            treasury_rules: HashMap::new(),
            coop_treasuries: HashMap::new(),
            velocity_limits: HashMap::new(),
            treasury_velocity_limits: HashMap::new(),
            velocity_windows: HashMap::new(),
            // Labor share state
            labor_shares: HashMap::new(),
            holder_shares: HashMap::new(),
            coop_shares: HashMap::new(),
            bonds: HashMap::new(),
            issuer_bonds: HashMap::new(),
            surplus_allocations: HashMap::new(),
        }
    }

    /// Create a treasury manager with persistent storage
    pub fn with_store(store: Arc<dyn Store>) -> Result<Self> {
        let mut manager = Self {
            store: Some(store),
            treasuries: HashMap::new(),
            budgets: HashMap::new(),
            treasury_budgets: HashMap::new(),
            spending_rules: HashMap::new(),
            treasury_rules: HashMap::new(),
            coop_treasuries: HashMap::new(),
            velocity_limits: HashMap::new(),
            treasury_velocity_limits: HashMap::new(),
            velocity_windows: HashMap::new(),
            // Labor share state
            labor_shares: HashMap::new(),
            holder_shares: HashMap::new(),
            coop_shares: HashMap::new(),
            bonds: HashMap::new(),
            issuer_bonds: HashMap::new(),
            surplus_allocations: HashMap::new(),
        };

        manager.load_from_store()?;
        Ok(manager)
    }

    // === Treasury Operations ===

    /// Register a treasury for a cooperative
    pub fn register_treasury(
        &mut self,
        treasury_did: Did,
        coop_id: String,
        currency: String,
        created_by: Did,
        description: Option<String>,
    ) -> Result<Treasury> {
        if self.treasuries.contains_key(&treasury_did) {
            bail!("Treasury already exists for DID: {treasury_did}");
        }

        if self.coop_treasuries.contains_key(&coop_id) {
            bail!("Treasury already exists for cooperative: {coop_id}");
        }

        let treasury = Treasury::new(
            treasury_did.clone(),
            coop_id.clone(),
            currency,
            created_by,
            description,
        );

        info!(
            treasury_did = %treasury_did,
            coop_id = %coop_id,
            "Registering new treasury"
        );

        self.treasuries
            .insert(treasury_did.clone(), treasury.clone());
        self.coop_treasuries
            .insert(coop_id.clone(), treasury_did.clone());
        self.treasury_budgets
            .insert(treasury_did.clone(), Vec::new());
        self.treasury_rules.insert(treasury_did.clone(), Vec::new());

        // Persist
        if let Some(ref store) = self.store {
            self.persist_treasury(&treasury, store)?;
            self.persist_coop_index(&coop_id, &treasury_did, store)?;
        }

        Ok(treasury)
    }

    /// Register a treasury for an entity (cooperative or federation)
    ///
    /// This is the preferred method for creating treasuries as it uses
    /// type-safe EntityId for organizational identity.
    pub fn register_treasury_with_entity(
        &mut self,
        treasury_did: Did,
        entity_id: EntityId,
        currency: String,
        created_by: Did,
        description: Option<String>,
    ) -> Result<Treasury> {
        let coop_id = entity_id.identifier().to_string();

        if self.treasuries.contains_key(&treasury_did) {
            bail!("Treasury already exists for DID: {treasury_did}");
        }

        if self.coop_treasuries.contains_key(&coop_id) {
            bail!("Treasury already exists for entity: {entity_id}");
        }

        let treasury = Treasury::new_with_entity(
            treasury_did.clone(),
            entity_id.clone(),
            currency,
            created_by,
            description,
        );

        info!(
            treasury_did = %treasury_did,
            entity_id = %entity_id,
            "Registering new treasury for entity"
        );

        self.treasuries
            .insert(treasury_did.clone(), treasury.clone());
        self.coop_treasuries
            .insert(coop_id.clone(), treasury_did.clone());
        self.treasury_budgets
            .insert(treasury_did.clone(), Vec::new());
        self.treasury_rules.insert(treasury_did.clone(), Vec::new());

        // Persist
        if let Some(ref store) = self.store {
            self.persist_treasury(&treasury, store)?;
            self.persist_coop_index(&coop_id, &treasury_did, store)?;
        }

        Ok(treasury)
    }

    /// Get treasury by DID
    pub fn get_treasury(&self, treasury_did: &Did) -> Option<&Treasury> {
        self.treasuries.get(treasury_did)
    }

    /// Get treasury by cooperative ID
    pub fn get_treasury_by_coop(&self, coop_id: &str) -> Option<&Treasury> {
        self.coop_treasuries
            .get(coop_id)
            .and_then(|did| self.treasuries.get(did))
    }

    /// Check if a DID is a treasury account
    pub fn is_treasury_account(&self, did: &Did) -> bool {
        self.treasuries.contains_key(did)
    }

    /// List all treasuries
    pub fn list_treasuries(&self) -> Vec<&Treasury> {
        self.treasuries.values().collect()
    }

    // === Labor Share Operations (Razeto Integration) ===

    /// Create a new labor share for a cooperative member
    ///
    /// This should be called when a member joins the cooperative.
    /// The share starts with 0 labor_days and 0 accumulated_surplus.
    ///
    /// # Important
    /// **Callers MUST validate that `holder` is an active member of `cooperative_id`
    /// before calling this function.** This validation should be performed against
    /// `icn-entity` membership records. Creating shares for non-members violates
    /// the cooperative ownership model.
    pub fn create_labor_share(
        &mut self,
        holder: Did,
        cooperative_id: String,
        currency: String,
    ) -> Result<LaborShare> {
        let now = icn_time::current_timestamp_secs();
        let share_id = ShareId::new(format!("share-{}-{}", now, uuid_simple()));

        let share = LaborShare::new(
            share_id.clone(),
            holder.clone(),
            cooperative_id.clone(),
            currency,
            now,
        );

        info!(
            share_id = %share_id,
            holder = %holder,
            cooperative_id = %cooperative_id,
            "Creating labor share"
        );

        // Add to indexes
        self.labor_shares.insert(share_id.clone(), share.clone());
        self.holder_shares
            .entry(holder.clone())
            .or_default()
            .push(share_id.clone());
        self.coop_shares
            .entry(cooperative_id.clone())
            .or_default()
            .push(share_id.clone());

        // Persist
        if let Some(ref store) = self.store {
            self.persist_labor_share(&share, store)?;
        }

        Ok(share)
    }

    /// Get a labor share by ID
    pub fn get_labor_share(&self, share_id: &ShareId) -> Option<&LaborShare> {
        self.labor_shares.get(share_id)
    }

    /// Get mutable labor share by ID
    pub fn get_labor_share_mut(&mut self, share_id: &ShareId) -> Option<&mut LaborShare> {
        self.labor_shares.get_mut(share_id)
    }

    /// List all labor shares for a holder
    pub fn list_holder_shares(&self, holder: &Did) -> Vec<&LaborShare> {
        self.holder_shares
            .get(holder)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.labor_shares.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// List all labor shares for a cooperative
    pub fn list_coop_shares(&self, cooperative_id: &str) -> Vec<&LaborShare> {
        self.coop_shares
            .get(cooperative_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.labor_shares.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Record labor contribution to a share
    ///
    /// Updates the labor_days and adds a provenance event.
    pub fn record_labor_contribution(
        &mut self,
        share_id: &ShareId,
        labor_days: u64,
        description: Option<String>,
    ) -> Result<()> {
        let now = icn_time::current_timestamp_secs();

        let share = self
            .labor_shares
            .get_mut(share_id)
            .ok_or_else(|| anyhow::anyhow!("Labor share not found: {share_id}"))?;

        if !share.is_active() {
            bail!("Cannot record labor on inactive share: {share_id}");
        }

        info!(
            share_id = %share_id,
            labor_days = labor_days,
            "Recording labor contribution"
        );

        share
            .record_labor(labor_days, now, description)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let share_clone = share.clone();
        if let Some(ref store) = self.store {
            self.persist_labor_share(&share_clone, store)?;
        }

        Ok(())
    }

    /// Execute a surplus allocation to all active shareholders
    ///
    /// This distributes surplus proportionally based on labor days.
    /// Requires governance approval (allocation should contain proposal_id).
    pub fn execute_surplus_allocation(&mut self, allocation: SurplusAllocation) -> Result<()> {
        info!(
            allocation_id = %allocation.id,
            cooperative_id = %allocation.cooperative_id,
            total_surplus = allocation.total_surplus,
            period = %allocation.period,
            "Executing surplus allocation"
        );

        // Validate all shares belong to the specified cooperative
        for (share_id, _) in &allocation.allocations {
            if let Some(share) = self.labor_shares.get(share_id) {
                if share.cooperative_id != allocation.cooperative_id {
                    bail!(
                        "Share {} belongs to cooperative '{}', not '{}'",
                        share_id,
                        share.cooperative_id,
                        allocation.cooperative_id
                    );
                }
            } else {
                bail!("Share not found: {share_id}");
            }
        }

        let now = icn_time::current_timestamp_secs();

        // Apply allocations to each share
        for (share_id, amount) in &allocation.allocations {
            if let Some(share) = self.labor_shares.get_mut(share_id) {
                share
                    .allocate_surplus(*amount, allocation.period.clone(), now)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
        }

        // Store the allocation record
        self.surplus_allocations
            .insert(allocation.id.clone(), allocation.clone());

        // Persist all updated shares
        if let Some(ref store) = self.store {
            for (share_id, _) in &allocation.allocations {
                if let Some(share) = self.labor_shares.get(share_id) {
                    self.persist_labor_share(share, store)?;
                }
            }
            self.persist_surplus_allocation(&allocation, store)?;
        }

        Ok(())
    }

    /// Start share redemption process
    ///
    /// Transitions the share to Redeeming status with a payout schedule.
    /// Requires governance approval.
    pub fn start_share_redemption(
        &mut self,
        share_id: &ShareId,
        payout_schedule: Vec<ScheduledPayout>,
        proposal_id: String,
    ) -> Result<()> {
        let now = icn_time::current_timestamp_secs();

        let share = self
            .labor_shares
            .get_mut(share_id)
            .ok_or_else(|| anyhow::anyhow!("Labor share not found: {share_id}"))?;

        if !share.is_active() {
            bail!("Cannot redeem inactive share: {share_id}");
        }

        info!(
            share_id = %share_id,
            payout_count = payout_schedule.len(),
            "Starting share redemption"
        );

        share.start_redemption(payout_schedule, proposal_id, now);

        let share_clone = share.clone();
        if let Some(ref store) = self.store {
            self.persist_labor_share(&share_clone, store)?;
        }

        Ok(())
    }

    /// Record a redemption payout
    ///
    /// Records that a scheduled payout has been completed.
    pub fn record_redemption_payout(&mut self, share_id: &ShareId, amount: i64) -> Result<()> {
        let now = icn_time::current_timestamp_secs();

        let share = self
            .labor_shares
            .get_mut(share_id)
            .ok_or_else(|| anyhow::anyhow!("Labor share not found: {share_id}"))?;

        info!(
            share_id = %share_id,
            amount = amount,
            "Recording redemption payout"
        );

        share.record_payout(amount, now);

        let share_clone = share.clone();
        if let Some(ref store) = self.store {
            self.persist_labor_share(&share_clone, store)?;
        }

        Ok(())
    }

    // === Bond Operations ===

    /// Create a new cooperative bond
    ///
    /// Requires governance approval (bond should contain approval_proposal).
    pub fn create_bond(&mut self, bond: CooperativeBond) -> Result<()> {
        if self.bonds.contains_key(&bond.id) {
            bail!("Bond already exists: {}", bond.id);
        }

        // Validate bond parameters
        if bond.principal <= 0 {
            bail!("Bond principal must be positive: {}", bond.principal);
        }

        info!(
            bond_id = %bond.id,
            issuer_id = %bond.issuer_id,
            principal = bond.principal,
            "Creating cooperative bond"
        );

        self.issuer_bonds
            .entry(bond.issuer_id.clone())
            .or_default()
            .push(bond.id.clone());
        self.bonds.insert(bond.id.clone(), bond.clone());

        if let Some(ref store) = self.store {
            self.persist_bond(&bond, store)?;
        }

        Ok(())
    }

    /// Get a bond by ID
    pub fn get_bond(&self, bond_id: &BondId) -> Option<&CooperativeBond> {
        self.bonds.get(bond_id)
    }

    /// Get mutable bond by ID
    pub fn get_bond_mut(&mut self, bond_id: &BondId) -> Option<&mut CooperativeBond> {
        self.bonds.get_mut(bond_id)
    }

    /// List all bonds issued by a cooperative
    pub fn list_issuer_bonds(&self, issuer_id: &str) -> Vec<&CooperativeBond> {
        self.issuer_bonds
            .get(issuer_id)
            .map(|ids| ids.iter().filter_map(|id| self.bonds.get(id)).collect())
            .unwrap_or_default()
    }

    /// Record a bond payment
    ///
    /// Only accepts payments on Active bonds. Bonds in Offering, Matured,
    /// or Defaulted status cannot receive payments.
    pub fn record_bond_payment(
        &mut self,
        bond_id: &BondId,
        payment_type: BondPaymentType,
        amount: i64,
    ) -> Result<()> {
        let now = icn_time::current_timestamp_secs();

        let bond = self
            .bonds
            .get_mut(bond_id)
            .ok_or_else(|| anyhow::anyhow!("Bond not found: {bond_id}"))?;

        // Validate bond is in Active status
        if !matches!(bond.status, BondStatus::Active) {
            bail!(
                "Cannot record payment on bond {}: status is {:?}, expected Active",
                bond_id,
                bond.status
            );
        }

        // Validate payment amount is positive
        if amount <= 0 {
            bail!("Payment amount must be positive: {amount}");
        }

        // Validate payment doesn't exceed outstanding balance
        let outstanding = bond.total_owed().saturating_sub(bond.total_paid());
        if amount > outstanding {
            bail!(
                "Payment amount {amount} exceeds outstanding balance {outstanding} for bond {bond_id}"
            );
        }

        info!(
            bond_id = %bond_id,
            payment_type = ?payment_type,
            amount = amount,
            "Recording bond payment"
        );

        bond.record_payment(payment_type, amount, now);

        // Check if bond is fully repaid (principal + interest)
        if bond.total_paid() >= bond.total_owed() {
            bond.mark_matured(now);
        }

        let bond_clone = bond.clone();
        if let Some(ref store) = self.store {
            self.persist_bond(&bond_clone, store)?;
        }

        Ok(())
    }

    /// Activate a bond (move from Offering to Active)
    pub fn activate_bond(&mut self, bond_id: &BondId) -> Result<()> {
        let bond = self
            .bonds
            .get_mut(bond_id)
            .ok_or_else(|| anyhow::anyhow!("Bond not found: {bond_id}"))?;

        info!(bond_id = %bond_id, "Activating bond");

        bond.activate().map_err(|e| anyhow::anyhow!("{e}"))?;

        let bond_clone = bond.clone();
        if let Some(ref store) = self.store {
            self.persist_bond(&bond_clone, store)?;
        }

        Ok(())
    }

    // === Labor Share Persistence ===

    fn persist_labor_share(&self, share: &LaborShare, store: &Arc<dyn Store>) -> Result<()> {
        let key = format!("{}{}", LABOR_SHARE_PREFIX, share.id);
        let value = serde_json::to_vec(share)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }

    fn persist_bond(&self, bond: &CooperativeBond, store: &Arc<dyn Store>) -> Result<()> {
        let key = format!("{}{}", BOND_PREFIX, bond.id);
        let value = serde_json::to_vec(bond)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }

    fn persist_surplus_allocation(
        &self,
        allocation: &SurplusAllocation,
        store: &Arc<dyn Store>,
    ) -> Result<()> {
        let key = format!("{}{}", SURPLUS_ALLOCATION_PREFIX, allocation.id);
        let value = serde_json::to_vec(allocation)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }

    // === Validation ===

    /// Validate a journal entry against treasury spending rules
    ///
    /// Returns Ok(()) if the entry is valid, or Err if it violates treasury rules.
    /// This method is designed to be used as a ledger validation hook.
    ///
    /// # Authorization Model
    ///
    /// Treasury withdrawals (debits from treasury accounts) that exceed spending
    /// rule thresholds require governance approval. The governance system indicates
    /// approval by setting a `contract_ref` on the journal entry when executing
    /// an approved treasury proposal.
    ///
    /// ## Security Note
    ///
    /// Currently, we check for the presence of ANY contract_ref to indicate
    /// governance authorization. This is secure because:
    /// 1. Contract execution is controlled by the governance system
    /// 2. Only approved proposals can execute contracts that create ledger entries
    /// 3. The contract_ref provides an audit trail back to the proposal
    ///
    /// For enhanced security, a future improvement could verify that the
    /// contract_ref specifically references an approved treasury proposal by
    /// checking a "treasury:" prefix or looking up the proposal details.
    pub fn validate_entry(&self, entry: &JournalEntry) -> Result<()> {
        for delta in &entry.accounts {
            // Only check treasury accounts
            if !self.is_treasury_account(&delta.account_id) {
                continue;
            }

            // Only check debits (withdrawals from treasury)
            let Some(debit_amount) = delta.debit else {
                continue;
            };

            // Check if this withdrawal requires approval
            if let Some(approval_type) =
                self.requires_approval(&delta.account_id, debit_amount, &delta.currency)
            {
                // Treasury withdrawals requiring approval must have a governance authorization.
                // The governance system sets contract_ref when executing approved proposals.
                //
                // SECURITY: The presence of contract_ref indicates this entry was created
                // by contract execution, which requires governance approval for treasury ops.
                // Direct ledger entries (without contract_ref) are blocked.
                if entry.contract_ref.is_none() {
                    bail!(
                        "Treasury withdrawal of {debit_amount} {} from {} requires {approval_type:?} approval. \
                         Submit a treasury proposal for governance approval.",
                        delta.currency,
                        delta.account_id,
                    );
                }
            }
        }

        Ok(())
    }

    // === Persistence Methods ===

    fn load_from_store(&mut self) -> Result<()> {
        let Some(ref store) = self.store else {
            return Ok(());
        };

        // Load treasuries
        let treasury_pairs = store.scan(TREASURY_PREFIX.as_bytes())?;
        for (key, value) in treasury_pairs {
            let key_str = String::from_utf8_lossy(&key);
            // Skip index entries
            if key_str.contains(":idx:")
                || key_str.contains(":budget:")
                || key_str.contains(":rule:")
                || key_str.contains(":audit:")
            {
                continue;
            }
            if let Ok(treasury) = serde_json::from_slice::<Treasury>(&value) {
                self.coop_treasuries
                    .insert(treasury.coop_id.clone(), treasury.treasury_did.clone());
                self.treasury_budgets
                    .insert(treasury.treasury_did.clone(), Vec::new());
                self.treasury_rules
                    .insert(treasury.treasury_did.clone(), Vec::new());
                self.treasuries
                    .insert(treasury.treasury_did.clone(), treasury);
            }
        }

        // Load budgets
        let budget_pairs = store.scan(BUDGET_PREFIX.as_bytes())?;
        for (_, value) in budget_pairs {
            if let Ok(budget) = serde_json::from_slice::<TreasuryBudget>(&value) {
                self.treasury_budgets
                    .entry(budget.treasury_did.clone())
                    .or_default()
                    .push(budget.id.clone());
                self.budgets.insert(budget.id.clone(), budget);
            }
        }

        // Load spending rules
        let rule_pairs = store.scan(SPENDING_RULE_PREFIX.as_bytes())?;
        for (_, value) in rule_pairs {
            if let Ok(rule) = serde_json::from_slice::<SpendingRule>(&value) {
                self.treasury_rules
                    .entry(rule.treasury_did.clone())
                    .or_default()
                    .push(rule.id.clone());
                self.spending_rules.insert(rule.id.clone(), rule);
            }
        }

        // Load labor shares
        let share_pairs = store.scan(LABOR_SHARE_PREFIX.as_bytes())?;
        for (key, value) in share_pairs {
            let key_str = String::from_utf8_lossy(&key);
            // Skip index entries
            if key_str.contains(":idx:") {
                continue;
            }
            if let Ok(share) = serde_json::from_slice::<LaborShare>(&value) {
                // Build indices
                self.holder_shares
                    .entry(share.holder.clone())
                    .or_default()
                    .push(share.id.clone());
                self.coop_shares
                    .entry(share.cooperative_id.clone())
                    .or_default()
                    .push(share.id.clone());
                self.labor_shares.insert(share.id.clone(), share);
            }
        }

        // Load bonds
        let bond_pairs = store.scan(BOND_PREFIX.as_bytes())?;
        for (key, value) in bond_pairs {
            let key_str = String::from_utf8_lossy(&key);
            // Skip index entries
            if key_str.contains(":idx:") {
                continue;
            }
            if let Ok(bond) = serde_json::from_slice::<CooperativeBond>(&value) {
                // Build indices
                self.issuer_bonds
                    .entry(bond.issuer_id.clone())
                    .or_default()
                    .push(bond.id.clone());
                self.bonds.insert(bond.id.clone(), bond);
            }
        }

        // Load surplus allocations
        let allocation_pairs = store.scan(SURPLUS_ALLOCATION_PREFIX.as_bytes())?;
        for (_, value) in allocation_pairs {
            if let Ok(allocation) = serde_json::from_slice::<SurplusAllocation>(&value) {
                self.surplus_allocations
                    .insert(allocation.id.clone(), allocation);
            }
        }

        info!(
            treasuries = self.treasuries.len(),
            budgets = self.budgets.len(),
            rules = self.spending_rules.len(),
            labor_shares = self.labor_shares.len(),
            bonds = self.bonds.len(),
            surplus_allocations = self.surplus_allocations.len(),
            "Loaded treasury data from store"
        );

        Ok(())
    }

    fn persist_treasury(&self, treasury: &Treasury, store: &Arc<dyn Store>) -> Result<()> {
        let key = format!("{}{}", TREASURY_PREFIX, treasury.treasury_did);
        let value = serde_json::to_vec(treasury)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }

    fn persist_coop_index(
        &self,
        coop_id: &str,
        treasury_did: &Did,
        store: &Arc<dyn Store>,
    ) -> Result<()> {
        let key = format!("{TREASURY_IDX_COOP_PREFIX}{coop_id}");
        let value = treasury_did.to_string();
        store.put(key.as_bytes(), value.as_bytes())?;
        Ok(())
    }
}

impl Default for TreasuryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a simple unique ID suffix
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{nanos:08x}")
}

#[cfg(test)]
mod tests {
    use super::approvals::approval_type_priority;
    use super::*;
    use crate::types::ContentHash;
    use icn_identity::KeyPair;

    fn test_did(name: &str) -> Did {
        let _ = name;
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_register_treasury() {
        let mut manager = TreasuryManager::new();
        let treasury_did = test_did("treasury");
        let admin = test_did("admin");

        let result = manager.register_treasury(
            treasury_did.clone(),
            "test-coop".to_string(),
            "hours".to_string(),
            admin,
            Some("Test treasury".to_string()),
        );

        assert!(result.is_ok());
        let treasury = result.unwrap();
        assert_eq!(treasury.coop_id, "test-coop");
        assert!(treasury.is_active);

        // Should be retrievable
        assert!(manager.is_treasury_account(&treasury_did));
        assert!(manager.get_treasury(&treasury_did).is_some());
        assert!(manager.get_treasury_by_coop("test-coop").is_some());
    }

    #[test]
    fn test_register_treasury_with_entity() {
        let mut manager = TreasuryManager::new();
        let treasury_did = test_did("treasury");
        let admin = test_did("admin");
        let entity_id = EntityId::cooperative("food-coop").unwrap();

        let result = manager.register_treasury_with_entity(
            treasury_did.clone(),
            entity_id.clone(),
            "hours".to_string(),
            admin,
            Some("Food coop treasury".to_string()),
        );

        assert!(result.is_ok());
        let treasury = result.unwrap();

        // Entity ID should be set
        assert!(treasury.entity_id().is_some());
        assert_eq!(treasury.entity_id().unwrap(), &entity_id);

        // coop_id should be derived from entity_id
        assert_eq!(treasury.coop_id(), "food-coop");
        assert_eq!(&treasury.coop_id, "food-coop");

        // Should be retrievable by coop_id
        assert!(manager.get_treasury_by_coop("food-coop").is_some());
    }

    #[test]
    fn test_duplicate_treasury_fails() {
        let mut manager = TreasuryManager::new();
        let treasury_did = test_did("treasury");
        let admin = test_did("admin");

        manager
            .register_treasury(
                treasury_did.clone(),
                "test-coop".to_string(),
                "hours".to_string(),
                admin.clone(),
                None,
            )
            .unwrap();

        // Same DID should fail
        let result = manager.register_treasury(
            treasury_did,
            "other-coop".to_string(),
            "hours".to_string(),
            admin.clone(),
            None,
        );
        assert!(result.is_err());

        // Same coop should fail
        let result = manager.register_treasury(
            test_did("other"),
            "test-coop".to_string(),
            "hours".to_string(),
            admin,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_create_budget() {
        let mut manager = TreasuryManager::new();
        let treasury_did = test_did("treasury");
        let admin = test_did("admin");

        manager
            .register_treasury(
                treasury_did.clone(),
                "test-coop".to_string(),
                "hours".to_string(),
                admin.clone(),
                None,
            )
            .unwrap();

        let budget = manager
            .create_budget(
                treasury_did.clone(),
                "Operations".to_string(),
                1000,
                "hours".to_string(),
                None,
                admin,
                None,
            )
            .unwrap();

        assert_eq!(budget.allocated_amount, 1000);
        assert_eq!(budget.spent_amount, 0);
        assert_eq!(budget.remaining(), 1000);
        assert!(budget.can_spend());

        // Should be retrievable
        let budgets = manager.list_budgets(&treasury_did);
        assert_eq!(budgets.len(), 1);
    }

    #[test]
    fn test_budget_spending() {
        let mut manager = TreasuryManager::new();
        let treasury_did = test_did("treasury");
        let admin = test_did("admin");

        manager
            .register_treasury(
                treasury_did.clone(),
                "test-coop".to_string(),
                "hours".to_string(),
                admin.clone(),
                None,
            )
            .unwrap();

        let budget = manager
            .create_budget(
                treasury_did,
                "Operations".to_string(),
                100,
                "hours".to_string(),
                None,
                admin,
                None,
            )
            .unwrap();

        let budget_id = budget.id.clone();
        let entry_hash = ContentHash::from_bytes([0u8; 32]);

        // Record spending (should trigger 50% threshold)
        let thresholds = manager
            .record_spending(&budget_id, 50, entry_hash.clone())
            .unwrap();
        assert!(thresholds.contains(&50));

        let budget = manager.get_budget(&budget_id).unwrap();
        assert_eq!(budget.spent_amount, 50);
        assert_eq!(budget.remaining(), 50);

        // Record more spending (should trigger 80% and 100%)
        let thresholds = manager.record_spending(&budget_id, 50, entry_hash).unwrap();
        assert!(thresholds.contains(&80));
        assert!(thresholds.contains(&100));

        let budget = manager.get_budget(&budget_id).unwrap();
        assert_eq!(budget.spent_amount, 100);
        assert_eq!(budget.remaining(), 0);
    }

    #[test]
    fn test_spending_rules() {
        let mut manager = TreasuryManager::new();
        let treasury_did = test_did("treasury");
        let admin = test_did("admin");

        manager
            .register_treasury(
                treasury_did.clone(),
                "test-coop".to_string(),
                "hours".to_string(),
                admin,
                None,
            )
            .unwrap();

        // Add spending rule
        let rule = SpendingRule::new(
            treasury_did.clone(),
            "Large withdrawal".to_string(),
            1000,
            "hours".to_string(),
            ApprovalType::SimpleMajority,
        );
        manager.add_spending_rule(rule).unwrap();

        // Below threshold - no approval needed
        assert!(manager
            .requires_approval(&treasury_did, 500, "hours")
            .is_none());

        // Above threshold - approval required
        assert_eq!(
            manager.requires_approval(&treasury_did, 1500, "hours"),
            Some(ApprovalType::SimpleMajority)
        );

        // Different currency - no approval needed
        assert!(manager
            .requires_approval(&treasury_did, 1500, "usd")
            .is_none());
    }

    #[test]
    fn test_multiple_spending_rules() {
        let mut manager = TreasuryManager::new();
        let treasury_did = test_did("treasury");
        let admin = test_did("admin");

        manager
            .register_treasury(
                treasury_did.clone(),
                "test-coop".to_string(),
                "hours".to_string(),
                admin,
                None,
            )
            .unwrap();

        // Add two rules with different thresholds
        let rule1 = SpendingRule::new(
            treasury_did.clone(),
            "Medium withdrawal".to_string(),
            500,
            "hours".to_string(),
            ApprovalType::SimpleMajority,
        );
        let rule2 = SpendingRule::new(
            treasury_did.clone(),
            "Large withdrawal".to_string(),
            2000,
            "hours".to_string(),
            ApprovalType::Emergency,
        );

        manager.add_spending_rule(rule1).unwrap();
        manager.add_spending_rule(rule2).unwrap();

        // Below 500 - no approval
        assert!(manager
            .requires_approval(&treasury_did, 499, "hours")
            .is_none());

        // Exactly at 500 threshold - simple majority (threshold is inclusive)
        assert_eq!(
            manager.requires_approval(&treasury_did, 500, "hours"),
            Some(ApprovalType::SimpleMajority)
        );

        // Above 500, below 2000 - simple majority
        assert_eq!(
            manager.requires_approval(&treasury_did, 1000, "hours"),
            Some(ApprovalType::SimpleMajority)
        );

        // Above 2000 - emergency (highest)
        assert_eq!(
            manager.requires_approval(&treasury_did, 3000, "hours"),
            Some(ApprovalType::Emergency)
        );
    }

    #[test]
    fn test_budget_expiration() {
        let budget = TreasuryBudget::new(
            test_did("treasury"),
            "Test".to_string(),
            1000,
            "hours".to_string(),
            Some(icn_time::current_timestamp_secs() + 3600), // 1 hour from now
            test_did("admin"),
            None,
        );

        assert!(!budget.is_expired(icn_time::current_timestamp_secs()));
        assert!(budget.can_spend());

        // Create already-expired budget
        let expired = TreasuryBudget {
            period_end: Some(0), // Unix epoch = expired
            ..budget.clone()
        };
        assert!(expired.is_expired(icn_time::current_timestamp_secs()));
    }

    #[test]
    fn test_approval_type_priority() {
        assert!(
            approval_type_priority(ApprovalType::Emergency)
                > approval_type_priority(ApprovalType::SuperMajority)
        );
        assert!(
            approval_type_priority(ApprovalType::SuperMajority)
                > approval_type_priority(ApprovalType::SimpleMajority)
        );
        assert!(
            approval_type_priority(ApprovalType::SimpleMajority)
                > approval_type_priority(ApprovalType::None)
        );
    }

    #[test]
    fn test_budget_percentage_used() {
        let mut budget = TreasuryBudget::new(
            test_did("treasury"),
            "Test".to_string(),
            100,
            "hours".to_string(),
            None,
            test_did("admin"),
            None,
        );

        assert_eq!(budget.percentage_used(), 0.0);

        budget.spent_amount = 25;
        assert_eq!(budget.percentage_used(), 25.0);

        budget.spent_amount = 100;
        assert_eq!(budget.percentage_used(), 100.0);
    }

    #[test]
    fn test_negative_amount_validation() {
        let mut manager = TreasuryManager::new();
        let treasury_did = test_did("treasury");
        let admin = test_did("admin");

        manager
            .register_treasury(
                treasury_did.clone(),
                "test-coop".to_string(),
                "hours".to_string(),
                admin.clone(),
                None,
            )
            .unwrap();

        let budget = manager
            .create_budget(
                treasury_did.clone(),
                "Test budget".to_string(),
                1000,
                "hours".to_string(),
                None,
                admin.clone(),
                None,
            )
            .unwrap();

        let entry_hash = ContentHash::from_bytes([0u8; 32]);

        // Negative amount should fail
        let result = manager.record_spending(&budget.id, -100, entry_hash.clone());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be positive"));

        // Zero amount should also fail
        let result = manager.record_spending(&budget.id, 0, entry_hash);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be positive"));

        // Negative budget creation should fail
        let result = manager.create_budget(
            treasury_did,
            "Bad budget".to_string(),
            -500,
            "hours".to_string(),
            None,
            admin,
            None,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be positive"));
    }

    #[test]
    fn test_two_phase_commit_snapshot_methods() {
        let mut manager = TreasuryManager::new();
        let treasury_did = test_did("treasury");
        let admin = test_did("admin");

        manager
            .register_treasury(
                treasury_did.clone(),
                "test-coop".to_string(),
                "hours".to_string(),
                admin.clone(),
                None,
            )
            .unwrap();

        let budget = manager
            .create_budget(
                treasury_did.clone(),
                "Test budget".to_string(),
                1000,
                "hours".to_string(),
                None,
                admin,
                None,
            )
            .unwrap();

        let budget_id = budget.id.clone();

        // Phase 1: Clone and modify
        let mut budget_clone = manager.get_budget(&budget_id).unwrap().clone();
        assert_eq!(budget_clone.allocated_amount, 1000);
        budget_clone.allocated_amount = 500;

        // Phase 2: Persist snapshot (no-op for in-memory manager, but API should work)
        let result = manager.save_budget_snapshot(&budget_clone);
        assert!(result.is_ok());

        // In-memory state should still be 1000 (unchanged)
        assert_eq!(
            manager.get_budget(&budget_id).unwrap().allocated_amount,
            1000
        );

        // Phase 3: Apply snapshot to in-memory state
        let result = manager.apply_budget_snapshot(&budget_clone);
        assert!(result.is_ok());

        // Now in-memory state should be 500
        assert_eq!(
            manager.get_budget(&budget_id).unwrap().allocated_amount,
            500
        );
    }

    #[test]
    fn test_apply_snapshot_nonexistent_budget_fails() {
        let mut manager = TreasuryManager::new();
        let treasury_did = test_did("treasury");
        let admin = test_did("admin");

        manager
            .register_treasury(
                treasury_did.clone(),
                "test-coop".to_string(),
                "hours".to_string(),
                admin.clone(),
                None,
            )
            .unwrap();

        // Create a fake budget that doesn't exist in manager
        let fake_budget = TreasuryBudget::new(
            treasury_did,
            "Fake".to_string(),
            1000,
            "hours".to_string(),
            None,
            admin,
            None,
        );

        // Applying snapshot for non-existent budget should fail
        let result = manager.apply_budget_snapshot(&fake_budget);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not found in-memory"));
    }

    #[test]
    fn test_two_phase_commit_simulates_transfer() {
        let mut manager = TreasuryManager::new();
        let treasury_did = test_did("treasury");
        let admin = test_did("admin");

        manager
            .register_treasury(
                treasury_did.clone(),
                "test-coop".to_string(),
                "hours".to_string(),
                admin.clone(),
                None,
            )
            .unwrap();

        // Create source budget with 1000
        let from_budget = manager
            .create_budget(
                treasury_did.clone(),
                "Source".to_string(),
                1000,
                "hours".to_string(),
                None,
                admin.clone(),
                None,
            )
            .unwrap();

        // Create destination budget with 500
        let to_budget = manager
            .create_budget(
                treasury_did,
                "Destination".to_string(),
                500,
                "hours".to_string(),
                None,
                admin,
                None,
            )
            .unwrap();

        let from_id = from_budget.id.clone();
        let to_id = to_budget.id.clone();

        // Simulate transfer of 300 using two-phase commit pattern
        let transfer_amount = 300;

        // Phase 1: Clone and modify
        let mut from_clone = manager.get_budget(&from_id).unwrap().clone();
        let mut to_clone = manager.get_budget(&to_id).unwrap().clone();

        from_clone.allocated_amount -= transfer_amount;
        to_clone.allocated_amount += transfer_amount;

        // Phase 2: Persist (no-op for in-memory, but validates API)
        manager.save_budget_snapshot(&from_clone).unwrap();
        manager.save_budget_snapshot(&to_clone).unwrap();

        // Verify in-memory state unchanged during persist phase
        assert_eq!(manager.get_budget(&from_id).unwrap().allocated_amount, 1000);
        assert_eq!(manager.get_budget(&to_id).unwrap().allocated_amount, 500);

        // Phase 3: Apply both snapshots
        manager.apply_budget_snapshot(&from_clone).unwrap();
        manager.apply_budget_snapshot(&to_clone).unwrap();

        // Verify transfer completed
        assert_eq!(manager.get_budget(&from_id).unwrap().allocated_amount, 700);
        assert_eq!(manager.get_budget(&to_id).unwrap().allocated_amount, 800);
    }

    // === Labor Share Tests ===

    #[test]
    fn test_create_labor_share() {
        let mut manager = TreasuryManager::new();
        let holder = test_did("member");
        let coop_id = "food-coop".to_string();

        let share = manager
            .create_labor_share(holder.clone(), coop_id.clone(), "hours".to_string())
            .unwrap();

        assert_eq!(share.holder, holder);
        assert_eq!(share.cooperative_id, coop_id);
        assert_eq!(share.currency, "hours");
        assert_eq!(share.labor_days, 0);
        assert_eq!(share.accumulated_surplus, 0);
        assert!(share.is_active());

        // Should be retrievable
        assert!(manager.get_labor_share(&share.id).is_some());
    }

    #[test]
    fn test_record_labor_contribution() {
        let mut manager = TreasuryManager::new();
        let holder = test_did("member");

        let share = manager
            .create_labor_share(holder, "food-coop".to_string(), "hours".to_string())
            .unwrap();

        let share_id = share.id.clone();

        // Record labor contribution
        manager
            .record_labor_contribution(&share_id, 10, Some("Week 1 work".to_string()))
            .unwrap();

        let share = manager.get_labor_share(&share_id).unwrap();
        assert_eq!(share.labor_days, 10);
        assert_eq!(share.provenance.len(), 2); // Created + LaborRecorded

        // Record more labor
        manager
            .record_labor_contribution(&share_id, 5, None)
            .unwrap();
        let share = manager.get_labor_share(&share_id).unwrap();
        assert_eq!(share.labor_days, 15);
    }

    #[test]
    fn test_list_holder_shares() {
        let mut manager = TreasuryManager::new();
        let holder1 = test_did("member1");
        let holder2 = test_did("member2");

        // Create shares for holder1 in two coops
        manager
            .create_labor_share(holder1.clone(), "coop-a".to_string(), "hours".to_string())
            .unwrap();
        manager
            .create_labor_share(holder1.clone(), "coop-b".to_string(), "hours".to_string())
            .unwrap();

        // Create share for holder2
        manager
            .create_labor_share(holder2.clone(), "coop-a".to_string(), "hours".to_string())
            .unwrap();

        // holder1 should have 2 shares
        let holder1_shares = manager.list_holder_shares(&holder1);
        assert_eq!(holder1_shares.len(), 2);

        // holder2 should have 1 share
        let holder2_shares = manager.list_holder_shares(&holder2);
        assert_eq!(holder2_shares.len(), 1);
    }

    #[test]
    fn test_list_coop_shares() {
        let mut manager = TreasuryManager::new();
        let holder1 = test_did("member1");
        let holder2 = test_did("member2");

        // Create shares in coop-a for both holders
        manager
            .create_labor_share(holder1.clone(), "coop-a".to_string(), "hours".to_string())
            .unwrap();
        manager
            .create_labor_share(holder2.clone(), "coop-a".to_string(), "hours".to_string())
            .unwrap();

        // Create share in coop-b for holder1 only
        manager
            .create_labor_share(holder1, "coop-b".to_string(), "hours".to_string())
            .unwrap();

        // coop-a should have 2 shares
        let coop_a_shares = manager.list_coop_shares("coop-a");
        assert_eq!(coop_a_shares.len(), 2);

        // coop-b should have 1 share
        let coop_b_shares = manager.list_coop_shares("coop-b");
        assert_eq!(coop_b_shares.len(), 1);
    }

    #[test]
    fn test_create_and_activate_bond() {
        use crate::labor_shares::{BondStatus, PaymentSchedule};

        let mut manager = TreasuryManager::new();
        let holder = test_did("investor");
        let now = icn_time::current_timestamp_secs();

        let bond = CooperativeBond::new_offering(
            BondId::new("bond-001".to_string()),
            "food-coop".to_string(),
            holder.clone(),
            10000,                                               // principal
            300,                                                 // 3% interest (basis points)
            now + 31536000,                                      // 1 year maturity
            PaymentSchedule::InterestOnly { interval_days: 90 }, // Quarterly interest
            "hours".to_string(),
            "proposal-001".to_string(),
            now,
        );

        manager.create_bond(bond.clone()).unwrap();

        // Should be retrievable
        let retrieved = manager.get_bond(&bond.id).unwrap();
        assert_eq!(retrieved.principal, 10000);
        assert!(matches!(retrieved.status, BondStatus::Offering { .. }));

        // Activate the bond
        manager.activate_bond(&bond.id).unwrap();
        let activated = manager.get_bond(&bond.id).unwrap();
        assert!(matches!(activated.status, BondStatus::Active));
    }

    #[test]
    fn test_record_bond_payment() {
        use crate::labor_shares::PaymentSchedule;

        let mut manager = TreasuryManager::new();
        let holder = test_did("investor");
        let now = icn_time::current_timestamp_secs();

        let bond = CooperativeBond::new_offering(
            BondId::new("bond-002".to_string()),
            "tech-coop".to_string(),
            holder,
            5000,
            200, // 2% interest
            now + 31536000,
            PaymentSchedule::InterestOnly { interval_days: 30 }, // Monthly interest
            "hours".to_string(),
            "proposal-002".to_string(),
            now,
        );

        manager.create_bond(bond.clone()).unwrap();
        manager.activate_bond(&bond.id).unwrap();

        // Record interest payment
        manager
            .record_bond_payment(&bond.id, BondPaymentType::Interest, 25)
            .unwrap();

        let bond = manager.get_bond(&bond.id).unwrap();
        assert_eq!(bond.payments.len(), 1);
        assert_eq!(bond.payments[0].amount, 25);
        assert!(matches!(
            bond.payments[0].payment_type,
            BondPaymentType::Interest
        ));
    }

    #[test]
    fn test_list_issuer_bonds() {
        use crate::labor_shares::PaymentSchedule;

        let mut manager = TreasuryManager::new();
        let holder1 = test_did("investor1");
        let holder2 = test_did("investor2");
        let now = icn_time::current_timestamp_secs();

        // Create bonds for food-coop
        manager
            .create_bond(CooperativeBond::new_offering(
                BondId::new("food-bond-1".to_string()),
                "food-coop".to_string(),
                holder1.clone(),
                1000,
                100,
                now + 31536000,
                PaymentSchedule::Bullet, // All at maturity
                "hours".to_string(),
                "proposal-f1".to_string(),
                now,
            ))
            .unwrap();
        manager
            .create_bond(CooperativeBond::new_offering(
                BondId::new("food-bond-2".to_string()),
                "food-coop".to_string(),
                holder2,
                2000,
                150,
                now + 31536000,
                PaymentSchedule::Bullet,
                "hours".to_string(),
                "proposal-f2".to_string(),
                now,
            ))
            .unwrap();

        // Create bond for tech-coop
        manager
            .create_bond(CooperativeBond::new_offering(
                BondId::new("tech-bond-1".to_string()),
                "tech-coop".to_string(),
                holder1,
                1500,
                120,
                now + 31536000,
                PaymentSchedule::Amortizing { interval_days: 90 }, // Quarterly amortizing
                "hours".to_string(),
                "proposal-t1".to_string(),
                now,
            ))
            .unwrap();

        // food-coop should have 2 bonds
        let food_bonds = manager.list_issuer_bonds("food-coop");
        assert_eq!(food_bonds.len(), 2);

        // tech-coop should have 1 bond
        let tech_bonds = manager.list_issuer_bonds("tech-coop");
        assert_eq!(tech_bonds.len(), 1);
    }

    #[test]
    fn test_surplus_allocation_execution() {
        let mut manager = TreasuryManager::new();
        let holder1 = test_did("member1");
        let holder2 = test_did("member2");
        let coop_id = "workers-coop".to_string();
        let now = icn_time::current_timestamp_secs();

        // Create shares with different labor contributions
        let share1 = manager
            .create_labor_share(holder1.clone(), coop_id.clone(), "hours".to_string())
            .unwrap();
        manager
            .record_labor_contribution(&share1.id, 100, None)
            .unwrap(); // 100 labor days

        let share2 = manager
            .create_labor_share(holder2.clone(), coop_id.clone(), "hours".to_string())
            .unwrap();
        manager
            .record_labor_contribution(&share2.id, 50, None)
            .unwrap(); // 50 labor days

        // Get the shares to pass to the allocation
        let shares: Vec<LaborShare> = manager
            .list_coop_shares(&coop_id)
            .into_iter()
            .cloned()
            .collect();

        // Create a surplus allocation (1500 total surplus, 150 total labor days = 10 per day)
        let allocation = SurplusAllocation::new(
            "alloc-2025-q1".to_string(),
            coop_id,
            1500, // total surplus
            "2025-Q1".to_string(),
            &shares,
            "proposal-123".to_string(),
            now,
            "hours".to_string(),
        )
        .unwrap();

        // Execute the allocation
        manager.execute_surplus_allocation(allocation).unwrap();

        // Check that surplus was distributed proportionally
        // holder1: 100/150 * 1500 = 1000
        // holder2: 50/150 * 1500 = 500
        let share1 = manager.get_labor_share(&share1.id).unwrap();
        let share2 = manager.get_labor_share(&share2.id).unwrap();

        assert_eq!(share1.accumulated_surplus, 1000);
        assert_eq!(share2.accumulated_surplus, 500);
    }
}
