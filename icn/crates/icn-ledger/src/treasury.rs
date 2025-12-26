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

use crate::types::{ContentHash, JournalEntry};
use anyhow::{bail, Result};
use icn_entity::EntityId;
use icn_identity::Did;
use icn_obs::metrics::treasury as treasury_metrics;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

// Storage key prefixes
const TREASURY_PREFIX: &str = "ledger:treasury:";
const BUDGET_PREFIX: &str = "ledger:treasury:budget:";
const SPENDING_RULE_PREFIX: &str = "ledger:treasury:rule:";
const TREASURY_AUDIT_PREFIX: &str = "ledger:treasury:audit:";
const TREASURY_IDX_COOP_PREFIX: &str = "ledger:treasury:idx:coop:";
const TREASURY_IDX_BUDGETS_PREFIX: &str = "ledger:treasury:idx:budgets:";

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

/// Budget allocation within a treasury
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryBudget {
    /// Unique budget ID
    pub id: String,

    /// Treasury this budget belongs to
    pub treasury_did: Did,

    /// Purpose of this budget (e.g., "operations", "grants", "emergency")
    pub purpose: String,

    /// Allocated amount from treasury
    pub allocated_amount: i64,

    /// Amount spent from this budget
    pub spent_amount: i64,

    /// Currency
    pub currency: String,

    /// Budget period (start timestamp)
    pub period_start: u64,

    /// Budget period (end timestamp, None = indefinite)
    pub period_end: Option<u64>,

    /// Current status
    pub status: BudgetStatus,

    /// Governance proposal that created this budget (if any)
    pub proposal_id: Option<String>,

    /// When created
    pub created_at: u64,

    /// Who created it
    pub created_by: Did,

    /// Notification thresholds (e.g., [50, 80, 100] = notify at 50%, 80%, 100%)
    pub notification_thresholds: Vec<u8>,

    /// Thresholds already notified
    pub notified_thresholds: Vec<u8>,
}

impl TreasuryBudget {
    /// Create a new budget allocation
    pub fn new(
        treasury_did: Did,
        purpose: String,
        allocated_amount: i64,
        currency: String,
        period_end: Option<u64>,
        created_by: Did,
        proposal_id: Option<String>,
    ) -> Self {
        let now = icn_time::current_timestamp_secs();
        Self {
            id: format!("budget-{}-{}", now, uuid_simple()),
            treasury_did,
            purpose,
            allocated_amount,
            spent_amount: 0,
            currency,
            period_start: now,
            period_end,
            status: BudgetStatus::Active,
            proposal_id,
            created_at: now,
            created_by,
            notification_thresholds: vec![50, 80, 100],
            notified_thresholds: Vec::new(),
        }
    }

    /// Calculate remaining budget
    pub fn remaining(&self) -> i64 {
        self.allocated_amount - self.spent_amount
    }

    /// Calculate percentage used
    pub fn percentage_used(&self) -> f64 {
        if self.allocated_amount == 0 {
            return 0.0;
        }
        (self.spent_amount as f64 / self.allocated_amount as f64) * 100.0
    }

    /// Check if budget is exceeded
    pub fn is_exceeded(&self) -> bool {
        self.spent_amount > self.allocated_amount
    }

    /// Check if budget period has expired
    pub fn is_expired(&self, now: u64) -> bool {
        self.period_end.is_some_and(|end| now >= end)
    }

    /// Check if spending is allowed (active and not expired)
    pub fn can_spend(&self) -> bool {
        self.status == BudgetStatus::Active && !self.is_expired(icn_time::current_timestamp_secs())
    }

    /// Record spending and return any newly triggered thresholds
    ///
    /// Uses saturating addition to prevent integer overflow.
    pub fn record_spending(&mut self, amount: i64) -> Vec<u8> {
        self.spent_amount = self.spent_amount.saturating_add(amount);

        // Check for exceeded status
        if self.is_exceeded() {
            self.status = BudgetStatus::Exceeded;
        }

        // Check for threshold notifications
        // Round and clamp to valid u8 range (0-100)
        let percentage = self.percentage_used().round().clamp(0.0, 100.0) as u8;
        let mut triggered = Vec::new();

        for threshold in &self.notification_thresholds {
            if percentage >= *threshold && !self.notified_thresholds.contains(threshold) {
                self.notified_thresholds.push(*threshold);
                triggered.push(*threshold);
            }
        }

        triggered
    }
}

/// Budget status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetStatus {
    /// Budget is active and can be spent from
    Active,
    /// Budget is paused (no spending allowed)
    Paused,
    /// Budget has been exceeded
    Exceeded,
    /// Budget period has expired
    Expired,
    /// Budget was cancelled
    Cancelled,
}

/// Spending rule for treasury operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingRule {
    /// Unique rule ID
    pub id: String,

    /// Treasury this rule applies to
    pub treasury_did: Did,

    /// Rule name/description
    pub name: String,

    /// Amount threshold (spending above this requires governance)
    pub threshold_amount: i64,

    /// Currency this rule applies to
    pub currency: String,

    /// Required governance approval type
    pub approval_type: ApprovalType,

    /// Whether the rule is active
    pub is_active: bool,

    /// When created
    pub created_at: u64,

    /// Governance proposal that created/modified this rule
    pub proposal_id: Option<String>,
}

impl SpendingRule {
    /// Create a new spending rule
    pub fn new(
        treasury_did: Did,
        name: String,
        threshold_amount: i64,
        currency: String,
        approval_type: ApprovalType,
    ) -> Self {
        let now = icn_time::current_timestamp_secs();
        Self {
            id: format!("rule-{}-{}", now, uuid_simple()),
            treasury_did,
            name,
            threshold_amount,
            currency,
            approval_type,
            is_active: true,
            created_at: now,
            proposal_id: None,
        }
    }

    /// Create with proposal reference
    pub fn with_proposal(mut self, proposal_id: String) -> Self {
        self.proposal_id = Some(proposal_id);
        self
    }
}

/// Velocity limit for treasury withdrawals
///
/// Prevents treasury drain attacks by limiting total withdrawals
/// within a rolling time window. Unlike spending rules (which require
/// governance approval for large individual withdrawals), velocity limits
/// block rapid small withdrawals that could cumulatively drain a treasury.
///
/// # Example
/// ```text
/// // Limit: max 5000 hours per hour
/// VelocityLimit {
///     window_seconds: 3600,   // 1 hour window
///     max_amount: 5000,       // max 5000 per window
///     ..
/// }
///
/// // Over 1 hour, user makes 10 withdrawals of 400 each
/// // After 5000 cumulative, further withdrawals are blocked
/// // until older withdrawals expire from the rolling window
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VelocityLimit {
    /// Unique limit ID
    pub id: String,

    /// Treasury this limit applies to
    pub treasury_did: Did,

    /// Currency this limit applies to
    pub currency: String,

    /// Rolling window size in seconds (e.g., 3600 for hourly, 86400 for daily)
    pub window_seconds: u64,

    /// Maximum total amount allowed within the window
    pub max_amount: i64,

    /// Whether the limit is active
    pub is_active: bool,

    /// When created
    pub created_at: u64,

    /// Governance proposal that created/modified this limit
    pub proposal_id: Option<String>,
}

impl VelocityLimit {
    /// Create a new velocity limit
    ///
    /// # Arguments
    /// * `treasury_did` - Treasury this applies to
    /// * `currency` - Currency to track
    /// * `window_seconds` - Rolling window size (e.g., 3600 for hourly)
    /// * `max_amount` - Maximum amount allowed in window
    pub fn new(treasury_did: Did, currency: String, window_seconds: u64, max_amount: i64) -> Self {
        let now = icn_time::current_timestamp_secs();
        Self {
            id: format!("vlimit-{}-{}", now, uuid_simple()),
            treasury_did,
            currency,
            window_seconds,
            max_amount,
            is_active: true,
            created_at: now,
            proposal_id: None,
        }
    }

    /// Create with proposal reference
    pub fn with_proposal(mut self, proposal_id: String) -> Self {
        self.proposal_id = Some(proposal_id);
        self
    }
}

/// Tracks withdrawal history for velocity limit checking
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VelocityWindow {
    /// Withdrawals within the tracking period: (timestamp, amount)
    pub withdrawals: Vec<(u64, i64)>,

    /// When this window was last updated
    pub last_updated: u64,
}

impl VelocityWindow {
    /// Calculate total withdrawn within the window, pruning expired entries
    pub fn total_in_window(&mut self, window_seconds: u64, now: u64) -> i64 {
        let window_start = now.saturating_sub(window_seconds);

        // Prune expired entries
        self.withdrawals
            .retain(|(timestamp, _)| *timestamp > window_start);
        self.last_updated = now;

        // Sum remaining
        self.withdrawals.iter().map(|(_, amount)| *amount).sum()
    }

    /// Record a new withdrawal
    pub fn record_withdrawal(&mut self, amount: i64) {
        let now = icn_time::current_timestamp_secs();
        self.withdrawals.push((now, amount));
        self.last_updated = now;
    }

    /// Check if adding `amount` would exceed `max_amount` in window
    pub fn would_exceed(&mut self, window_seconds: u64, max_amount: i64, amount: i64) -> bool {
        let now = icn_time::current_timestamp_secs();
        let current_total = self.total_in_window(window_seconds, now);
        current_total.saturating_add(amount) > max_amount
    }
}

/// Type of approval required for treasury spending
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalType {
    /// No approval needed (within threshold)
    None,
    /// Simple majority vote (>50%)
    SimpleMajority,
    /// Super-majority (>66%)
    SuperMajority,
    /// Board/council approval only
    BoardOnly,
    /// Emergency threshold (75%+)
    Emergency,
}

/// Treasury operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TreasuryOperation {
    /// Deposit funds into treasury
    Deposit {
        from: Did,
        amount: i64,
        currency: String,
        memo: Option<String>,
    },
    /// Withdraw funds from treasury
    Withdraw {
        to: Did,
        amount: i64,
        currency: String,
        purpose: String,
        budget_id: Option<String>,
    },
    /// Allocate funds to a budget
    AllocateBudget {
        budget_id: String,
        amount: i64,
        currency: String,
        purpose: String,
    },
    /// Transfer between budgets
    TransferBetweenBudgets {
        from_budget: String,
        to_budget: String,
        amount: i64,
        currency: String,
        reason: String,
    },
    /// Reallocate unspent budget back to treasury
    ReclaimBudget {
        budget_id: String,
        amount: i64,
        currency: String,
        reason: String,
    },
    /// Create a new budget
    CreateBudget {
        budget_id: String,
        purpose: String,
        amount: i64,
        currency: String,
    },
    /// Cancel a budget
    CancelBudget {
        budget_id: String,
        reason: String,
        return_to_treasury: bool,
    },
    /// Modify spending rule
    ModifySpendingRule {
        rule_id: String,
        new_threshold: Option<i64>,
        new_approval_type: Option<ApprovalType>,
        is_active: Option<bool>,
    },
}

/// Audit record for treasury operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryAuditRecord {
    /// Unique audit ID
    pub id: String,

    /// Treasury DID
    pub treasury_did: Did,

    /// Operation performed
    pub operation: TreasuryOperation,

    /// Ledger entry hash (if applicable)
    pub ledger_entry_hash: Option<ContentHash>,

    /// Governance proposal ID (if required approval)
    pub proposal_id: Option<String>,

    /// Who performed the operation
    pub performed_by: Did,

    /// When performed
    pub performed_at: u64,

    /// Treasury balance after operation
    pub balance_after: i64,

    /// Additional notes
    pub notes: Option<String>,
}

impl TreasuryAuditRecord {
    /// Create a new audit record
    pub fn new(
        treasury_did: Did,
        operation: TreasuryOperation,
        performed_by: Did,
        balance_after: i64,
    ) -> Self {
        let now = icn_time::current_timestamp_secs();
        Self {
            id: format!("audit-{}-{}", now, uuid_simple()),
            treasury_did,
            operation,
            ledger_entry_hash: None,
            proposal_id: None,
            performed_by,
            performed_at: now,
            balance_after,
            notes: None,
        }
    }

    /// Add ledger entry reference
    pub fn with_ledger_entry(mut self, hash: ContentHash) -> Self {
        self.ledger_entry_hash = Some(hash);
        self
    }

    /// Add proposal reference
    pub fn with_proposal(mut self, proposal_id: String) -> Self {
        self.proposal_id = Some(proposal_id);
        self
    }

    /// Add notes
    pub fn with_notes(mut self, notes: String) -> Self {
        self.notes = Some(notes);
        self
    }
}

/// Paginated audit trail response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedAuditTrail {
    /// Audit records for the current page
    pub records: Vec<TreasuryAuditRecord>,
    /// Total number of records (for pagination UI)
    pub total: usize,
    /// Current offset
    pub offset: usize,
    /// Page size limit
    pub limit: usize,
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

    // === Budget Operations ===

    /// Create a budget allocation
    pub fn create_budget(
        &mut self,
        treasury_did: Did,
        purpose: String,
        amount: i64,
        currency: String,
        period_end: Option<u64>,
        created_by: Did,
        proposal_id: Option<String>,
    ) -> Result<TreasuryBudget> {
        // Validate inputs
        if !self.treasuries.contains_key(&treasury_did) {
            bail!("Treasury not found: {treasury_did}");
        }

        if amount <= 0 {
            bail!("Budget amount must be positive, got: {amount}");
        }

        if purpose.trim().is_empty() {
            bail!("Budget purpose cannot be empty");
        }

        if currency.trim().is_empty() {
            bail!("Budget currency cannot be empty");
        }

        // Validate period_end is in the future if provided
        if let Some(end) = period_end {
            let now = icn_time::current_timestamp_secs();
            if end <= now {
                bail!("Budget period end must be in the future");
            }
        }

        let budget = TreasuryBudget::new(
            treasury_did.clone(),
            purpose.clone(),
            amount,
            currency,
            period_end,
            created_by,
            proposal_id,
        );

        info!(
            budget_id = %budget.id,
            treasury_did = %treasury_did,
            purpose = %purpose,
            amount = amount,
            "Creating treasury budget"
        );

        let budget_id = budget.id.clone();
        self.budgets.insert(budget_id.clone(), budget.clone());
        self.treasury_budgets
            .entry(treasury_did.clone())
            .or_default()
            .push(budget_id.clone());

        // Persist
        if let Some(ref store) = self.store {
            self.persist_budget(&budget, store)?;
            self.persist_budget_index(&treasury_did, &budget_id, store)?;
        }

        // Emit metrics
        treasury_metrics::budget_created_inc();
        treasury_metrics::budget_remaining_set(&budget_id, amount);

        Ok(budget)
    }

    /// Get budget by ID
    pub fn get_budget(&self, budget_id: &str) -> Option<&TreasuryBudget> {
        self.budgets.get(budget_id)
    }

    /// Get mutable budget by ID
    pub fn get_budget_mut(&mut self, budget_id: &str) -> Option<&mut TreasuryBudget> {
        self.budgets.get_mut(budget_id)
    }

    /// List budgets for a treasury
    pub fn list_budgets(&self, treasury_did: &Did) -> Vec<&TreasuryBudget> {
        self.treasury_budgets
            .get(treasury_did)
            .map(|ids| ids.iter().filter_map(|id| self.budgets.get(id)).collect())
            .unwrap_or_default()
    }

    /// Record spending against a budget
    pub fn record_spending(
        &mut self,
        budget_id: &str,
        amount: i64,
        ledger_entry_hash: ContentHash,
    ) -> Result<Vec<u8>> {
        // Validate amount is positive
        if amount <= 0 {
            bail!("Spending amount must be positive, got: {amount}");
        }

        // First validate without mutable borrow
        {
            let budget = self
                .budgets
                .get(budget_id)
                .ok_or_else(|| anyhow::anyhow!("Budget not found: {budget_id}"))?;

            if !budget.can_spend() {
                bail!("Budget {budget_id} is not available for spending");
            }

            if amount > budget.remaining() {
                bail!(
                    "Amount {} exceeds remaining budget {}",
                    amount,
                    budget.remaining()
                );
            }
        }

        info!(
            budget_id = %budget_id,
            amount = amount,
            entry_hash = %ledger_entry_hash,
            "Recording budget spending"
        );

        // Now mutate
        let budget = self
            .budgets
            .get_mut(budget_id)
            .ok_or_else(|| anyhow::anyhow!("Budget not found: {budget_id}"))?;
        let triggered_thresholds = budget.record_spending(amount);
        let budget_clone = budget.clone();

        // Persist updated budget
        if let Some(ref store) = self.store {
            self.persist_budget(&budget_clone, store)?;
        }

        Ok(triggered_thresholds)
    }

    /// Update budget status
    pub fn update_budget_status(&mut self, budget_id: &str, status: BudgetStatus) -> Result<()> {
        let budget = self
            .budgets
            .get_mut(budget_id)
            .ok_or_else(|| anyhow::anyhow!("Budget not found: {budget_id}"))?;

        info!(
            budget_id = %budget_id,
            old_status = ?budget.status,
            new_status = ?status,
            "Updating budget status"
        );

        budget.status = status;
        let budget_clone = budget.clone();

        if let Some(ref store) = self.store {
            self.persist_budget(&budget_clone, store)?;
        }

        Ok(())
    }

    /// Save budget changes to persistent storage
    ///
    /// Call this after making direct mutations to budget fields (e.g., allocated_amount).
    /// This is a public wrapper around the internal persist_budget method.
    pub fn save_budget(&self, budget_id: &str) -> Result<()> {
        let budget = self
            .budgets
            .get(budget_id)
            .ok_or_else(|| anyhow::anyhow!("Budget not found: {budget_id}"))?;

        if let Some(ref store) = self.store {
            self.persist_budget(budget, store)?;
        }

        Ok(())
    }

    /// Persist a budget snapshot directly to storage without updating in-memory state.
    ///
    /// This is used for two-phase commit patterns where we want to persist changes
    /// before committing them to in-memory state. The budget's ID is used as the key.
    ///
    /// # Two-Phase Commit Pattern
    ///
    /// 1. Clone budgets from in-memory state
    /// 2. Modify the clones
    /// 3. Call `save_budget_snapshot` to persist clones (if this fails, no state corruption)
    /// 4. Call `apply_budget_snapshot` to update in-memory state
    ///
    /// This ensures that if persistence fails, in-memory state remains consistent.
    pub fn save_budget_snapshot(&self, budget: &TreasuryBudget) -> Result<()> {
        if let Some(ref store) = self.store {
            self.persist_budget(budget, store)?;
        }
        Ok(())
    }

    /// Apply a budget snapshot to in-memory state.
    ///
    /// Used after the persist phase of two-phase commit to sync in-memory state
    /// with storage. Returns error if the budget ID doesn't exist in-memory.
    ///
    /// Takes a reference and clones internally to avoid consuming the caller's budget,
    /// allowing the caller to continue using the budget if needed (e.g., for logging).
    ///
    /// # Safety
    ///
    /// Only call this after successfully calling `save_budget_snapshot` for the same budget.
    /// This ensures storage and memory remain consistent.
    pub fn apply_budget_snapshot(&mut self, budget: &TreasuryBudget) -> Result<()> {
        if !self.budgets.contains_key(&budget.id) {
            bail!("Budget not found in-memory: {}", budget.id);
        }
        self.budgets.insert(budget.id.clone(), budget.clone());
        Ok(())
    }

    // === Spending Rules ===

    /// Add a spending rule
    pub fn add_spending_rule(&mut self, rule: SpendingRule) -> Result<()> {
        if !self.treasuries.contains_key(&rule.treasury_did) {
            bail!("Treasury not found: {}", rule.treasury_did);
        }

        info!(
            rule_id = %rule.id,
            treasury_did = %rule.treasury_did,
            threshold = rule.threshold_amount,
            approval_type = ?rule.approval_type,
            "Adding spending rule"
        );

        let rule_id = rule.id.clone();
        let treasury_did = rule.treasury_did.clone();

        self.spending_rules.insert(rule_id.clone(), rule.clone());
        self.treasury_rules
            .entry(treasury_did.clone())
            .or_default()
            .push(rule_id);

        if let Some(ref store) = self.store {
            self.persist_spending_rule(&rule, store)?;
        }

        Ok(())
    }

    /// Check if an amount requires governance approval
    /// Returns the highest approval type required, or None if no approval needed
    pub fn requires_approval(
        &self,
        treasury_did: &Did,
        amount: i64,
        currency: &str,
    ) -> Option<ApprovalType> {
        let rule_ids = self.treasury_rules.get(treasury_did)?;

        let mut highest_approval: Option<ApprovalType> = None;

        for rule_id in rule_ids {
            if let Some(rule) = self.spending_rules.get(rule_id) {
                if rule.is_active && rule.currency == currency && amount >= rule.threshold_amount {
                    // Return the highest approval type
                    let current = highest_approval.unwrap_or(ApprovalType::None);
                    if approval_type_priority(rule.approval_type) > approval_type_priority(current)
                    {
                        highest_approval = Some(rule.approval_type);
                    }
                }
            }
        }

        // Only return if it requires some approval
        let result = match highest_approval {
            Some(ApprovalType::None) => None,
            other => other,
        };

        // Emit metric if approval is required
        if let Some(ref approval_type) = result {
            let type_str = match approval_type {
                ApprovalType::None => "none",
                ApprovalType::SimpleMajority => "simple_majority",
                ApprovalType::SuperMajority => "super_majority",
                ApprovalType::BoardOnly => "board_only",
                ApprovalType::Emergency => "emergency",
            };
            treasury_metrics::approval_required_inc(type_str);
        }

        result
    }

    /// List spending rules for a treasury
    pub fn list_spending_rules(&self, treasury_did: &Did) -> Vec<&SpendingRule> {
        self.treasury_rules
            .get(treasury_did)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.spending_rules.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Update a spending rule
    pub fn update_spending_rule(
        &mut self,
        rule_id: &str,
        threshold_amount: Option<i64>,
        approval_type: Option<ApprovalType>,
        is_active: Option<bool>,
    ) -> Result<()> {
        let rule = self
            .spending_rules
            .get_mut(rule_id)
            .ok_or_else(|| anyhow::anyhow!("Spending rule not found: {rule_id}"))?;

        if let Some(threshold) = threshold_amount {
            rule.threshold_amount = threshold;
        }
        if let Some(approval) = approval_type {
            rule.approval_type = approval;
        }
        if let Some(active) = is_active {
            rule.is_active = active;
        }

        info!(
            rule_id = %rule_id,
            threshold = rule.threshold_amount,
            approval_type = ?rule.approval_type,
            is_active = rule.is_active,
            "Updated spending rule"
        );

        let rule_clone = rule.clone();

        if let Some(ref store) = self.store {
            self.persist_spending_rule(&rule_clone, store)?;
        }

        Ok(())
    }

    // === Velocity Limits ===

    /// Add a velocity limit to a treasury
    ///
    /// Velocity limits prevent treasury drain attacks by limiting the total
    /// amount that can be withdrawn within a rolling time window.
    pub fn add_velocity_limit(&mut self, limit: VelocityLimit) -> Result<()> {
        if !self.treasuries.contains_key(&limit.treasury_did) {
            bail!("Treasury not found: {}", limit.treasury_did);
        }

        info!(
            limit_id = %limit.id,
            treasury_did = %limit.treasury_did,
            window_seconds = limit.window_seconds,
            max_amount = limit.max_amount,
            currency = %limit.currency,
            "Adding velocity limit"
        );

        let limit_id = limit.id.clone();
        let treasury_did = limit.treasury_did.clone();

        self.velocity_limits.insert(limit_id.clone(), limit.clone());
        self.treasury_velocity_limits
            .entry(treasury_did)
            .or_default()
            .push(limit_id);

        if let Some(ref store) = self.store {
            self.persist_velocity_limit(&limit, store)?;
        }

        Ok(())
    }

    /// List velocity limits for a treasury
    pub fn list_velocity_limits(&self, treasury_did: &Did) -> Vec<&VelocityLimit> {
        self.treasury_velocity_limits
            .get(treasury_did)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.velocity_limits.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if a withdrawal would violate velocity limits
    ///
    /// Returns Some(limit) if the withdrawal would exceed a velocity limit,
    /// or None if the withdrawal is allowed.
    pub fn check_velocity(
        &mut self,
        treasury_did: &Did,
        amount: i64,
        currency: &str,
    ) -> Option<&VelocityLimit> {
        let limit_ids = self.treasury_velocity_limits.get(treasury_did)?.clone();

        for limit_id in &limit_ids {
            if let Some(limit) = self.velocity_limits.get(limit_id) {
                if !limit.is_active || limit.currency != currency {
                    continue;
                }

                // Get or create velocity window for this treasury/currency pair
                let window_key = (treasury_did.clone(), currency.to_string());
                let window = self.velocity_windows.entry(window_key).or_default();

                if window.would_exceed(limit.window_seconds, limit.max_amount, amount) {
                    // Emit metric for velocity limit exceeded
                    treasury_metrics::velocity_limit_exceeded_inc(currency);
                    // Return the limit that would be violated
                    return self.velocity_limits.get(limit_id);
                }
            }
        }

        None
    }

    /// Record a withdrawal for velocity tracking
    ///
    /// Call this after a successful withdrawal to update the velocity window.
    pub fn record_withdrawal_for_velocity(
        &mut self,
        treasury_did: &Did,
        amount: i64,
        currency: &str,
    ) {
        let window_key = (treasury_did.clone(), currency.to_string());
        let window = self.velocity_windows.entry(window_key).or_default();
        window.record_withdrawal(amount);
    }

    /// Persist velocity limit to storage
    fn persist_velocity_limit(&self, limit: &VelocityLimit, store: &Arc<dyn Store>) -> Result<()> {
        let key = format!("{}vlimit:{}", TREASURY_PREFIX, limit.id);
        let value = serde_json::to_vec(limit)?;
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

    // === Audit Trail ===

    /// Execute a treasury operation with audit logging
    pub fn record_audit(
        &mut self,
        treasury_did: &Did,
        operation: TreasuryOperation,
        performed_by: Did,
        balance_after: i64,
        proposal_id: Option<String>,
        ledger_entry_hash: Option<ContentHash>,
    ) -> Result<TreasuryAuditRecord> {
        if !self.treasuries.contains_key(treasury_did) {
            bail!("Treasury not found: {treasury_did}");
        }

        let mut record =
            TreasuryAuditRecord::new(treasury_did.clone(), operation, performed_by, balance_after);

        if let Some(hash) = ledger_entry_hash {
            record = record.with_ledger_entry(hash);
        }
        if let Some(proposal) = proposal_id {
            record = record.with_proposal(proposal);
        }

        info!(
            audit_id = %record.id,
            treasury_did = %treasury_did,
            operation_type = ?std::mem::discriminant(&record.operation),
            "Recording treasury audit"
        );

        if let Some(ref store) = self.store {
            self.persist_audit_record(&record, store)?;
        }

        Ok(record)
    }

    /// Get audit trail for a treasury with pagination
    ///
    /// Returns a `PaginatedAuditTrail` containing records and total count for UI pagination.
    /// Uses reverse iteration for efficiency - only loads the requested records instead
    /// of loading all records into memory for sorting.
    ///
    /// Note: Records are returned most recent first, based on key ordering
    /// (keys include timestamps in ascending order, so reverse iteration
    /// yields most recent first).
    pub fn get_audit_trail(
        &self,
        treasury_did: &Did,
        limit: usize,
        offset: usize,
    ) -> Result<PaginatedAuditTrail> {
        let Some(ref store) = self.store else {
            return Ok(PaginatedAuditTrail {
                records: Vec::new(),
                total: 0,
                offset,
                limit,
            });
        };

        let prefix = format!("{TREASURY_AUDIT_PREFIX}{treasury_did}");

        // Use optimized reverse pagination - only loads requested records
        // Keys are ordered by timestamp, so reverse gives most recent first
        let (pairs, total) = store.scan_reverse_paginated(prefix.as_bytes(), offset, limit)?;

        let records: Vec<TreasuryAuditRecord> = pairs
            .into_iter()
            .filter_map(|(_, value)| serde_json::from_slice(&value).ok())
            .collect();

        Ok(PaginatedAuditTrail {
            records,
            total,
            offset,
            limit,
        })
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

        info!(
            treasuries = self.treasuries.len(),
            budgets = self.budgets.len(),
            rules = self.spending_rules.len(),
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

    fn persist_budget(&self, budget: &TreasuryBudget, store: &Arc<dyn Store>) -> Result<()> {
        let key = format!("{}{}", BUDGET_PREFIX, budget.id);
        let value = serde_json::to_vec(budget)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }

    fn persist_budget_index(
        &self,
        treasury_did: &Did,
        budget_id: &str,
        store: &Arc<dyn Store>,
    ) -> Result<()> {
        let key = format!("{TREASURY_IDX_BUDGETS_PREFIX}{treasury_did}:{budget_id}");
        store.put(key.as_bytes(), budget_id.as_bytes())?;
        Ok(())
    }

    fn persist_spending_rule(&self, rule: &SpendingRule, store: &Arc<dyn Store>) -> Result<()> {
        let key = format!("{}{}", SPENDING_RULE_PREFIX, rule.id);
        let value = serde_json::to_vec(rule)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }

    fn persist_audit_record(
        &self,
        record: &TreasuryAuditRecord,
        store: &Arc<dyn Store>,
    ) -> Result<()> {
        // Key includes timestamp for time-ordered retrieval
        let key = format!(
            "{}{}:{}:{}",
            TREASURY_AUDIT_PREFIX, record.treasury_did, record.performed_at, record.id
        );
        let value = serde_json::to_vec(record)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }
}

impl Default for TreasuryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Get priority value for approval types (higher = more strict)
fn approval_type_priority(approval: ApprovalType) -> u8 {
    match approval {
        ApprovalType::None => 0,
        ApprovalType::SimpleMajority => 1,
        ApprovalType::SuperMajority => 2,
        ApprovalType::BoardOnly => 3,
        ApprovalType::Emergency => 4,
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
    use super::*;
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
}
