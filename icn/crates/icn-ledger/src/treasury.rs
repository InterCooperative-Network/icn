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
use crate::principal_rows::{
    refuse_unless_one_spelling_per_principal, PrincipalRowsRefusal, TREASURY_KEYSPACE,
};
use crate::types::JournalEntry;
use anyhow::{bail, Result};
use icn_entity::EntityId;
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
/// Persisted velocity-limit rows: `ledger:treasury:vlimit:<limit-id>`.
const VELOCITY_LIMIT_PREFIX: &str = "ledger:treasury:vlimit:";

/// Every subspace that shares the lexical parent `ledger:treasury:` with the
/// primary treasury rows, by the exact prefix its writer produces.
///
/// The loader classifies a key beneath the parent as a primary row unless it
/// begins with one of these, so a subspace missing from this list would have
/// its rows refused as unreadable primaries (fail closed) rather than adopted
/// as treasuries. `ledger:treasury:vlimit:` was absent from the loader's
/// previous skip list and survived only because its values do not parse as a
/// [`Treasury`]; it is named here so the classification is by key shape and
/// never by whether a value happens to deserialize. The scanner descriptor
/// `icn-ledger/treasury` claims none of these prefixes either.
const TREASURY_SIBLING_SUBSPACES: [&str; 6] = [
    BUDGET_PREFIX,
    SPENDING_RULE_PREFIX,
    TREASURY_AUDIT_PREFIX,
    TREASURY_IDX_COOP_PREFIX,
    TREASURY_IDX_BUDGETS_PREFIX,
    VELOCITY_LIMIT_PREFIX,
];

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

    /// Index: entity_id -> treasury DID (only for treasuries with
    /// `entity_id: Some(...)`). Kept alongside `coop_treasuries` so entity-keyed
    /// lookup and uniqueness work even when a treasury's legacy `coop_id` differs
    /// from `entity_id.identifier()` — e.g. a surrogate backfill where
    /// `coop:<uuid>` is bound to `coop-legacy-*`. The legacy `coop_id` is never
    /// rewritten; this is an additional index, not a replacement.
    entity_treasuries: HashMap<EntityId, Did>,

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

/// Why persisted treasury state could not be hydrated (#2627 M1).
///
/// Companion to [`crate::principal_rows::PrincipalRowsRefusal`], which carries
/// the refusals every principal-keyed ledger keyspace shares — an alias pair,
/// an unreadable key, a key whose spelling disagrees with its own value. This
/// enum carries what is specific to the treasury layout. Carried through
/// `anyhow` so `with_store` keeps its signature; recover it with
/// `anyhow::Error::downcast_ref::<TreasuryHydrationRefusal>()`.
///
/// Payload-safe by construction: a variant carries a row count and nothing
/// else — never a spelling, a coop id, an entity id or a stored value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TreasuryHydrationRefusal {
    /// A primary row whose key names a treasury principal holds a value that
    /// cannot be read as a [`Treasury`].
    ///
    /// Refused rather than skipped: were it the only row for a principal,
    /// skipping it would turn unreadable state into absent state; were it one
    /// of an alias pair, skipping it would make the surviving row look
    /// unambiguous. Before M1 the loader skipped such rows silently — which
    /// is how about half of anchor-derived cooperative treasuries dropped out
    /// of the maps on every reload (inventory §10.1, owned by #2628); M1 makes
    /// that state visible rather than absent, and does not repair it.
    #[error(
        "{}: {rows} primary treasury row(s) whose key names a treasury principal but whose \
         value cannot be read as a treasury record; refusing to hydrate, because skipping \
         the row would turn unreadable state into absent state",
        TREASURY_KEYSPACE
    )]
    UnreadablePrimaryValue {
        /// How many primary rows held an unreadable value.
        rows: usize,
    },

    /// Two primary rows naming distinct principals bind one `coop_id`.
    #[error(
        "{}: {rows} primary treasury row(s) bind a coop_id that another primary row already \
         binds; refusing to hydrate (fail closed) rather than let the cooperative index keep \
         whichever row scanned last",
        TREASURY_KEYSPACE
    )]
    DuplicateCoopId {
        /// How many rows bind a `coop_id` an earlier row bound.
        rows: usize,
    },

    /// Two primary rows naming distinct principals bind one `entity_id`.
    #[error(
        "{}: {rows} primary treasury row(s) bind an entity_id that another primary row \
         already binds; refusing to hydrate (fail closed) rather than let the entity index \
         keep whichever row scanned last",
        TREASURY_KEYSPACE
    )]
    DuplicateEntityId {
        /// How many rows bind an `entity_id` an earlier row bound.
        rows: usize,
    },

    /// A `ledger:treasury:idx:coop:` row whose key or value cannot be read as
    /// (coop id, treasury principal spelling).
    #[error(
        "{}: {rows} cooperative index row(s) (ledger:treasury:idx:coop:) whose key or value \
         cannot be read as a coop id and a treasury principal spelling; refusing to hydrate, \
         because an index that cannot be read cannot be checked against the primary rows",
        TREASURY_KEYSPACE
    )]
    CoopIndexUnreadable {
        /// How many index rows could not be read.
        rows: usize,
    },

    /// A `ledger:treasury:idx:coop:` row names a primary treasury row — through
    /// the coop id it is filed under, or through the principal its value
    /// decodes to — under a spelling that is not that row's physical key.
    ///
    /// Under I7 the two spellings compare equal as `Did`, which is exactly
    /// why the comparison here is on bytes: an index must not silently
    /// retarget one spelling of a principal to another.
    #[error(
        "{}: {rows} cooperative index row(s) whose stored spelling differs from the physical \
         key spelling of the primary treasury row it names; refusing to hydrate, because a \
         persisted index must not retarget one spelling of a principal to another",
        TREASURY_KEYSPACE
    )]
    CoopIndexSpellingMismatch {
        /// How many index rows disagree with the primary row they name.
        rows: usize,
    },
}

/// Outcome of the treasury `entity_id` populate storage seam
/// ([`TreasuryManager::populate_treasury_entity_id_for_did`]). Reached by the
/// contract-bound operator backfill (ADR-0084) and by activation-time population
/// ([`TreasuryManager::populate_entity_id_at_activation`], #2082); both go through
/// the same fail-closed write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreasuryEntityIdPopulateResult {
    /// `entity_id` was populated (`None → Some`) and persisted.
    Populated,
    /// The treasury already carried an `entity_id`; nothing was changed
    /// (idempotent guard — apply never overwrites an existing identity target).
    AlreadyPopulated,
    /// No treasury is registered for the planned `treasury_did`; nothing was
    /// changed.
    TreasuryNotFound,
    /// The planned `treasury_did` resolves to a treasury whose `coop_id` no
    /// longer matches the planned `coop_id` byte-for-byte (rows/index drifted);
    /// nothing was changed (fail closed).
    CoopIdMismatch,
    /// The target `EntityId` is already indexed to a *different* treasury;
    /// populating would violate entity uniqueness. Nothing was changed (fail
    /// closed).
    EntityIdConflict,
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
            entity_treasuries: HashMap::new(),
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
            entity_treasuries: HashMap::new(),
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

        // Reject a duplicate by the EXACT entity, not just by its derived coop_id.
        // A surrogate-backfilled treasury keeps its legacy `coop_id` (which differs
        // from `entity_id.identifier()`), so the coop_id check alone would miss it
        // and allow a second treasury for the same entity. Check the entity index.
        if self.entity_treasuries.contains_key(&entity_id) {
            bail!("Treasury already exists for entity: {entity_id}");
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
        self.entity_treasuries
            .insert(entity_id.clone(), treasury_did.clone());
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

    /// Get treasury by owning `EntityId`.
    ///
    /// Uses the dedicated entity index, so it finds treasuries whose `entity_id`
    /// was populated by a surrogate backfill (where the legacy `coop_id` differs
    /// from `entity_id.identifier()`) — not just those where they happen to match.
    pub fn get_treasury_by_entity(&self, entity_id: &EntityId) -> Option<&Treasury> {
        self.entity_treasuries
            .get(entity_id)
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

    /// Rebuild the in-memory state from the store: classify first, adopt last.
    ///
    /// Everything the store holds is read and validated into a
    /// [`PersistedTreasuryState`] before the first in-memory map is touched, so
    /// a refusal on any row leaves `self` exactly as constructed — no map is
    /// partially hydrated from the rows that came before it. The primary
    /// treasury rows are classified through `crate::principal_rows`
    /// ([`classify_primary_treasury_rows`]); the persisted cooperative index is
    /// checked against them ([`validate_coop_index`]); the sibling subspaces
    /// are read as they were before #2627 M1.
    fn load_from_store(&mut self) -> Result<()> {
        let Some(store) = self.store.clone() else {
            return Ok(());
        };

        let state = read_persisted_state(store.as_ref())?;
        self.adopt_persisted_state(state);

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

    /// Adopt a fully classified and validated [`PersistedTreasuryState`].
    ///
    /// Infallible by construction: every refusal has already been raised by
    /// [`read_persisted_state`], so nothing here can fail after a map has been
    /// mutated. Each treasury is keyed by the spelling its own row carries,
    /// which [`classify_primary_treasury_rows`] has proven equal to the
    /// physical key spelling, so `persist_treasury` addresses the row that
    /// was loaded and never opens a second spelling.
    fn adopt_persisted_state(&mut self, state: PersistedTreasuryState) {
        for treasury in state.treasuries {
            let treasury_did = treasury.treasury_did.clone();
            if let Some(entity_id) = treasury.entity_id.clone() {
                self.entity_treasuries
                    .insert(entity_id, treasury_did.clone());
            }
            self.coop_treasuries
                .insert(treasury.coop_id.clone(), treasury_did.clone());
            self.treasury_budgets
                .insert(treasury_did.clone(), Vec::new());
            self.treasury_rules.insert(treasury_did.clone(), Vec::new());
            self.treasuries.insert(treasury_did, treasury);
        }

        for budget in state.budgets {
            self.treasury_budgets
                .entry(budget.treasury_did.clone())
                .or_default()
                .push(budget.id.clone());
            self.budgets.insert(budget.id.clone(), budget);
        }

        for rule in state.spending_rules {
            self.treasury_rules
                .entry(rule.treasury_did.clone())
                .or_default()
                .push(rule.id.clone());
            self.spending_rules.insert(rule.id.clone(), rule);
        }

        for share in state.labor_shares {
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

        for bond in state.bonds {
            self.issuer_bonds
                .entry(bond.issuer_id.clone())
                .or_default()
                .push(bond.id.clone());
            self.bonds.insert(bond.id.clone(), bond);
        }

        for allocation in state.surplus_allocations {
            self.surplus_allocations
                .insert(allocation.id.clone(), allocation);
        }
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

    /// Storage seam for the controlled treasury `entity_id` backfill **apply**
    /// (ADR-0084, #2082 lane). Populates the `entity_id` of the treasury
    /// identified by the **planned `treasury_did`** and persists it, **preserving
    /// `coop_id` byte-for-byte**: only the `entity_id` field changes.
    ///
    /// The write targets the exact planned DID — never whichever row currently
    /// wins the `coop_id → DID` index — so a store whose index no longer points
    /// at the planned treasury (e.g. an inconsistent/duplicate `coop_id`) cannot
    /// mutate the wrong row or leave the audit disagreeing with the write.
    /// `expected_coop_id` is re-verified against the located row byte-for-byte; a
    /// mismatch fails closed.
    ///
    /// A `coop_id ↔ EntityId` mapping grants **zero** authority — this writes an
    /// identity *target*, never a permission. The method stays `pub(crate)`:
    /// external callers reach this seam only through the contract-bound
    /// [`TreasuryManager::apply_entity_id_backfill`] orchestrator (which restricts
    /// mutation to the planner's `WouldPopulate` rows) or the activation-time
    /// [`TreasuryManager::populate_entity_id_at_activation`] wrapper — both pass a
    /// target derived from a trusted binding, never an arbitrary one. The
    /// fail-closed checks below hold regardless of caller.
    ///
    /// Fail-closed and idempotent:
    /// - a missing planned DID is a no-op
    ///   ([`TreasuryNotFound`](TreasuryEntityIdPopulateResult::TreasuryNotFound));
    /// - a located row whose `coop_id` differs from `expected_coop_id` fails
    ///   closed ([`CoopIdMismatch`](TreasuryEntityIdPopulateResult::CoopIdMismatch));
    /// - a treasury that already carries an `entity_id` is left untouched
    ///   ([`AlreadyPopulated`](TreasuryEntityIdPopulateResult::AlreadyPopulated));
    /// - a target `EntityId` already bound to a different treasury fails closed
    ///   ([`EntityIdConflict`](TreasuryEntityIdPopulateResult::EntityIdConflict));
    /// - the durable write happens **before** the in-memory commit, so a storage
    ///   error returns `Err` with both the store and the cache left unchanged.
    pub(crate) fn populate_treasury_entity_id_for_did(
        &mut self,
        treasury_did: &Did,
        expected_coop_id: &str,
        entity_id: EntityId,
    ) -> Result<TreasuryEntityIdPopulateResult> {
        // Locate by the EXACT planned DID — never by the coop_id index winner.
        let Some(existing) = self.treasuries.get(treasury_did) else {
            return Ok(TreasuryEntityIdPopulateResult::TreasuryNotFound);
        };

        // The located row must still carry the exact coop_id the planner
        // classified, byte-for-byte. A mismatch means the rows/index drifted since
        // planning; fail closed rather than mutate a different identity.
        if existing.coop_id != expected_coop_id {
            return Ok(TreasuryEntityIdPopulateResult::CoopIdMismatch);
        }

        // Idempotent: never overwrite an already-populated identity target.
        if existing.entity_id.is_some() {
            return Ok(TreasuryEntityIdPopulateResult::AlreadyPopulated);
        }

        // Entity uniqueness: the target EntityId must not already be indexed to a
        // DIFFERENT treasury. A surrogate-backfilled row keeps its legacy coop_id,
        // so the coop_id index cannot catch an entity collision — check the entity
        // index and fail closed on a conflict.
        if let Some(other_did) = self.entity_treasuries.get(&entity_id) {
            if other_did != treasury_did {
                return Ok(TreasuryEntityIdPopulateResult::EntityIdConflict);
            }
            // Already indexed to THIS treasury (inconsistent with the entity_id:
            // None checked above, but treat as an idempotent no-op rather than
            // re-indexing).
            return Ok(TreasuryEntityIdPopulateResult::AlreadyPopulated);
        }

        // Populate entity_id only; coop_id (and every other field) is carried
        // through unchanged.
        let mut updated = existing.clone();
        updated.entity_id = Some(entity_id.clone());
        debug_assert_eq!(
            updated.coop_id, expected_coop_id,
            "apply must preserve coop_id byte-for-byte"
        );

        // Persist first: a storage failure must leave the durable store and the
        // in-memory cache consistent (both unchanged) — fail closed.
        if let Some(ref store) = self.store {
            self.persist_treasury(&updated, store)?;
        }
        // Commit in-memory only after a durable write: update the treasury cache
        // and the entity index together so they stay consistent.
        self.treasuries.insert(treasury_did.clone(), updated);
        self.entity_treasuries
            .insert(entity_id, treasury_did.clone());
        Ok(TreasuryEntityIdPopulateResult::Populated)
    }

    /// Activation-time entry point (#2082) to the fail-closed `entity_id` populate
    /// seam ([`populate_treasury_entity_id_for_did`](Self::populate_treasury_entity_id_for_did)).
    ///
    /// A cooperative's treasury is registered with `entity_id: None`, its
    /// activation record is committed, the canonical `coop_id ↔ EntityId` binding is
    /// recorded, and only then is this called to populate the treasury from the
    /// EntityId that binding produced (or the direct projection when no map store is
    /// wired). It verifies `coop_id` byte-for-byte, never overwrites an existing
    /// target, and enforces entity uniqueness — so a treasury never carries an
    /// identity the map did not record. It sets an identity *target* only and grants
    /// no authority; the caller must not fail activation on a non-`Populated`
    /// outcome (the operator backfill can complete it later).
    pub fn populate_entity_id_at_activation(
        &mut self,
        treasury_did: &Did,
        expected_coop_id: &str,
        entity_id: EntityId,
    ) -> Result<TreasuryEntityIdPopulateResult> {
        self.populate_treasury_entity_id_for_did(treasury_did, expected_coop_id, entity_id)
    }

    /// Creation-time entry point (#2082, `CreateTreasury`) to the same fail-closed
    /// `entity_id` populate seam
    /// ([`populate_treasury_entity_id_for_did`](Self::populate_treasury_entity_id_for_did)).
    ///
    /// Used by the coop_id-preserving two-step defined in
    /// `docs/design/create-treasury-entity-id-semantics.md`: the treasury is first
    /// registered with the plain `register_treasury` under the byte-exact original
    /// `coop_id` (never `register_treasury_with_entity`, which would re-derive the
    /// `coop_id` from `entity_id.identifier()` and mis-file surrogate-bound rows),
    /// then this populates `entity_id` from an **already-recorded trusted binding**
    /// the caller consulted read-only. Unlike activation there is no projection
    /// fallback: no trusted binding means the caller never reaches this method.
    /// Same guarantees as the activation entry point: byte-for-byte `coop_id`
    /// verification, never overwrites an existing target, entity uniqueness
    /// enforced. It sets an identity *target* only and grants no authority; the
    /// caller must not fail treasury creation on a non-`Populated` outcome (the
    /// operator backfill can complete it later).
    pub fn populate_entity_id_at_creation(
        &mut self,
        treasury_did: &Did,
        expected_coop_id: &str,
        entity_id: EntityId,
    ) -> Result<TreasuryEntityIdPopulateResult> {
        self.populate_treasury_entity_id_for_did(treasury_did, expected_coop_id, entity_id)
    }
}

impl Default for TreasuryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything `TreasuryManager::load_from_store` reads, classified and
/// validated before any in-memory map is touched.
///
/// The primary treasury rows carry the refusal machinery of #2627 M1; the
/// other vectors are the sibling subspaces read exactly as before, so a
/// storage error in any scan — or any refusal — surfaces before adoption
/// begins rather than between two maps.
struct PersistedTreasuryState {
    /// One record per treasury principal, each proven to carry the spelling
    /// its physical key carries.
    treasuries: Vec<Treasury>,
    budgets: Vec<TreasuryBudget>,
    spending_rules: Vec<SpendingRule>,
    labor_shares: Vec<LaborShare>,
    bonds: Vec<CooperativeBond>,
    surplus_allocations: Vec<SurplusAllocation>,
}

/// A primary treasury row after classification: the exact spelling its
/// physical key carries, and the record its value holds.
///
/// The key spelling is kept beside the record until adoption because it is
/// the row's identity — what `persist_treasury` will address — and the guard
/// must group by it, not by whatever the body happens to spell.
struct ClassifiedTreasuryRow {
    key_spelling: String,
    treasury: Treasury,
}

/// What one key beneath `ledger:treasury:` is, to the loader.
enum TreasuryKey {
    /// `ledger:treasury:<spelling>`: the authoritative treasury row, with the
    /// spelling exactly as the key carries it.
    Primary(String),
    /// A row of a sibling subspace ([`TREASURY_SIBLING_SUBSPACES`]). It is not
    /// a treasury-principal row and belongs to its own loader.
    Sibling,
    /// Neither: not UTF-8, not beneath the parent, or a remainder that is not
    /// a treasury principal. Never skipped — an unreadable row is evidence.
    Unreadable,
}

/// Classify one stored key beneath `ledger:treasury:` by its shape alone.
///
/// The classification is by key structure, never by whether the value
/// happens to deserialize: a primary row whose value cannot be read is still
/// a primary row, and a sibling row is a sibling whatever it holds. The key is
/// decoded strictly — `persist_treasury` writes a `Did`'s `Display`, always
/// UTF-8 — so a key that is not UTF-8 is one the writer never produced, and
/// normalizing it would let an invalid byte pass as a spelling the guard
/// accepts while the raw row stayed on disk under a key nothing addresses.
fn classify_treasury_key(key: &[u8]) -> TreasuryKey {
    let Ok(key_str) = std::str::from_utf8(key) else {
        return TreasuryKey::Unreadable;
    };
    let Some(remainder) = key_str.strip_prefix(TREASURY_PREFIX) else {
        return TreasuryKey::Unreadable;
    };
    if TREASURY_SIBLING_SUBSPACES
        .iter()
        .any(|sibling| key_str.starts_with(sibling))
    {
        return TreasuryKey::Sibling;
    }
    // The key must name a principal before its value is read. `persist_treasury`
    // writes a `Did`'s `Display`, and a body is adopted only if its own
    // `treasury_did` — which deserializes through `Did::from_str` — spells the
    // same as the key, so a remainder `Did::from_str` rejects can never be one
    // of this keyspace's rows, whatever its value holds. An anchor-derived
    // spelling whose bytes are no Ed25519 point (inventory §10.1) fails here,
    // as it fails `Deserialize`; before M1 such a row vanished silently.
    if Did::from_str(remainder).is_err() {
        return TreasuryKey::Unreadable;
    }
    TreasuryKey::Primary(remainder.to_string())
}

/// Read and classify every primary `ledger:treasury:<did>` row, refusing
/// before anything is adopted (#2627 M1).
///
/// ```text
/// physical rows
/// → classify every key by shape (primary / sibling / unreadable)
/// → read a value only behind a key that names a principal
/// → prove one spelling per principal   (`principal_rows`)
/// → prove the key spelling and the body spelling are the same bytes
/// → prove one treasury per coop_id and per entity_id
/// → only then hand the rows to the Did-keyed maps
/// ```
///
/// Refusals are raised in evidence order: an unreadable key or value first,
/// because a classification computed over an incomplete view proves nothing;
/// then the alias collision; then the rows whose body disagrees with their
/// key; then the institutional duplicates. Two rows naming one principal
/// under two spellings are two treasury records that can disagree about every
/// field, and no economics rule authorizes choosing, summing or combining
/// them: the only correct answer is the typed refusal. No spelling is
/// normalized, no row is re-keyed, nothing is deleted.
fn classify_primary_treasury_rows(store: &dyn Store) -> Result<Vec<ClassifiedTreasuryRow>> {
    let pairs = store.scan(TREASURY_PREFIX.as_bytes())?;

    // `pairs` is consumed so each raw row is dropped once classified; the
    // guard holds the parsed rows only.
    let mut rows = Vec::with_capacity(pairs.len());
    let mut unreadable_keys = 0usize;
    let mut unreadable_values = 0usize;
    for (key, value) in pairs {
        let key_spelling = match classify_treasury_key(&key) {
            TreasuryKey::Sibling => continue,
            TreasuryKey::Unreadable => {
                unreadable_keys += 1;
                continue;
            }
            TreasuryKey::Primary(spelling) => spelling,
        };
        // Behind an accepted key, an unreadable value is refused rather than
        // skipped: were it the only row for a principal, skipping it would turn
        // unreadable state into absent state; were it one of an alias pair,
        // skipping it would make the remaining row look unambiguous.
        match serde_json::from_slice::<Treasury>(&value) {
            Ok(treasury) => rows.push(ClassifiedTreasuryRow {
                key_spelling,
                treasury,
            }),
            Err(_) => unreadable_values += 1,
        }
    }

    if unreadable_keys > 0 {
        return Err(PrincipalRowsRefusal::UnreadableKey {
            keyspace: TREASURY_KEYSPACE,
            rows: unreadable_keys,
        }
        .into());
    }
    if unreadable_values > 0 {
        return Err(TreasuryHydrationRefusal::UnreadablePrimaryValue {
            rows: unreadable_values,
        }
        .into());
    }

    // Grouped by the stored **key**, not by the spelling inside the record:
    // the key is what a later `persist_treasury` addresses.
    refuse_unless_one_spelling_per_principal(
        TREASURY_KEYSPACE,
        rows.iter().map(|row| (row.key_spelling.as_str(), "")),
    )?;

    // Exact bytes, never `Did` equality: under I7 two spellings of one
    // principal compare equal, and the question here is not whether the key
    // and the body name the same principal but whether this physical row is
    // the row `persist_treasury` claims to have written — which derives the
    // key from `treasury.treasury_did`'s `Display`. A row that disagrees with
    // itself is the residue of a collapse that already happened.
    let strayed = rows
        .iter()
        .filter(|row| row.treasury.treasury_did.as_str() != row.key_spelling)
        .count();
    if strayed > 0 {
        return Err(PrincipalRowsRefusal::KeyValueSpellingMismatch {
            keyspace: TREASURY_KEYSPACE,
            rows: strayed,
        }
        .into());
    }

    // The pre-M1 institutional guards, now over the classified set and before
    // any map exists. Every row here names a distinct principal, so a shared
    // coop_id or entity_id is two treasuries for one institution — a store the
    // apply paths would mutate ambiguously. Fail closed rather than let the
    // index keep whichever row scanned last.
    let mut seen_coops = HashSet::new();
    let duplicate_coops = rows
        .iter()
        .filter(|row| !seen_coops.insert(row.treasury.coop_id.as_str()))
        .count();
    if duplicate_coops > 0 {
        return Err(TreasuryHydrationRefusal::DuplicateCoopId {
            rows: duplicate_coops,
        }
        .into());
    }
    let mut seen_entities = HashSet::new();
    let duplicate_entities = rows
        .iter()
        .filter_map(|row| row.treasury.entity_id.as_ref())
        .filter(|entity_id| !seen_entities.insert(*entity_id))
        .count();
    if duplicate_entities > 0 {
        return Err(TreasuryHydrationRefusal::DuplicateEntityId {
            rows: duplicate_entities,
        }
        .into());
    }

    Ok(rows)
}

/// Check every persisted `ledger:treasury:idx:coop:<coop_id>` row against the
/// classified primary rows (#2627 M1, idx:coop integrity).
///
/// The index is a write-only projection: `persist_coop_index` writes it at
/// registration, and hydration rebuilds the coop map from the primary rows
/// rather than from it. It is not authority, and this check gives it none.
/// It is still persisted evidence that can preserve a representation
/// disagreement, so before adoption every row must (a) decode — a value that
/// is no treasury principal spelling is refused, not skipped — and (b) agree
/// byte-for-byte with the physical key spelling of the primary row it names,
/// whether it names it through the coop_id it is filed under or through the
/// principal its value decodes to. An index row filed under a coop_id with no
/// primary row and naming no known principal is an orphan: nothing consumes
/// it, so it is tolerated and not adopted.
fn validate_coop_index(store: &dyn Store, primaries: &[ClassifiedTreasuryRow]) -> Result<()> {
    let pairs = store.scan(TREASURY_IDX_COOP_PREFIX.as_bytes())?;

    let mut unreadable = 0usize;
    let mut mismatched = 0usize;
    for (key, value) in pairs {
        let (Ok(key_str), Ok(stored_spelling)) =
            (std::str::from_utf8(&key), std::str::from_utf8(&value))
        else {
            unreadable += 1;
            continue;
        };
        let Some(coop_id) = key_str.strip_prefix(TREASURY_IDX_COOP_PREFIX) else {
            unreadable += 1;
            continue;
        };
        let Ok(pointed) = Did::from_str(stored_spelling) else {
            unreadable += 1;
            continue;
        };

        let registered_for_coop = primaries.iter().find(|row| row.treasury.coop_id == coop_id);
        // `Did` equality here is deliberate: it finds the primary row for the
        // principal the value names under *any* spelling, so that the exact
        // comparison below can refuse the alias.
        let named_by_value = primaries
            .iter()
            .find(|row| row.treasury.treasury_did == pointed);
        let disagrees = |row: &ClassifiedTreasuryRow| row.key_spelling != stored_spelling;
        if registered_for_coop.is_some_and(disagrees) || named_by_value.is_some_and(disagrees) {
            mismatched += 1;
        }
    }

    if unreadable > 0 {
        return Err(TreasuryHydrationRefusal::CoopIndexUnreadable { rows: unreadable }.into());
    }
    if mismatched > 0 {
        return Err(
            TreasuryHydrationRefusal::CoopIndexSpellingMismatch { rows: mismatched }.into(),
        );
    }
    Ok(())
}

/// Read every treasury subspace into a [`PersistedTreasuryState`].
///
/// All scans complete before the caller adopts anything. The primary rows go
/// through [`classify_primary_treasury_rows`] and [`validate_coop_index`];
/// the budget, rule, labor-share, bond and allocation rows are read exactly as
/// before M1, a row that does not parse being skipped. That permissiveness is
/// pre-existing and recorded as follow-up in the N2-A migration gate; M1
/// classifies the primary rows only.
fn read_persisted_state(store: &dyn Store) -> Result<PersistedTreasuryState> {
    let primaries = classify_primary_treasury_rows(store)?;
    validate_coop_index(store, &primaries)?;
    let treasuries = primaries.into_iter().map(|row| row.treasury).collect();

    let budgets = store
        .scan(BUDGET_PREFIX.as_bytes())?
        .into_iter()
        .filter_map(|(_, value)| serde_json::from_slice::<TreasuryBudget>(&value).ok())
        .collect();

    let spending_rules = store
        .scan(SPENDING_RULE_PREFIX.as_bytes())?
        .into_iter()
        .filter_map(|(_, value)| serde_json::from_slice::<SpendingRule>(&value).ok())
        .collect();

    let labor_shares = store
        .scan(LABOR_SHARE_PREFIX.as_bytes())?
        .into_iter()
        .filter(|(key, _)| !String::from_utf8_lossy(key).contains(":idx:"))
        .filter_map(|(_, value)| serde_json::from_slice::<LaborShare>(&value).ok())
        .collect();

    let bonds = store
        .scan(BOND_PREFIX.as_bytes())?
        .into_iter()
        .filter(|(key, _)| !String::from_utf8_lossy(key).contains(":idx:"))
        .filter_map(|(_, value)| serde_json::from_slice::<CooperativeBond>(&value).ok())
        .collect();

    let surplus_allocations = store
        .scan(SURPLUS_ALLOCATION_PREFIX.as_bytes())?
        .into_iter()
        .filter_map(|(_, value)| serde_json::from_slice::<SurplusAllocation>(&value).ok())
        .collect();

    Ok(PersistedTreasuryState {
        treasuries,
        budgets,
        spending_rules,
        labor_shares,
        bonds,
        surplus_allocations,
    })
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

    // ------------------------------------------------------------------
    // Entity-id index / uniqueness (Codex P2: preserve entity uniqueness).
    // ------------------------------------------------------------------

    #[test]
    fn register_treasury_with_entity_rejects_duplicate_entity() {
        let mut mgr = TreasuryManager::new();
        let creator = test_did("creator");
        let entity = EntityId::cooperative("food-coop").unwrap();

        // Normal registration (coop_id == entity_id.identifier()) works and is
        // entity-indexed.
        mgr.register_treasury_with_entity(
            test_did("t1"),
            entity.clone(),
            "USD".to_string(),
            creator.clone(),
            None,
        )
        .unwrap();
        assert_eq!(
            mgr.get_treasury_by_entity(&entity).unwrap().coop_id(),
            "food-coop"
        );

        // A second registration for the same entity is rejected (no second row).
        let result = mgr.register_treasury_with_entity(
            test_did("t2"),
            entity,
            "USD".to_string(),
            creator,
            None,
        );
        assert!(result.is_err(), "duplicate entity registration must reject");
    }

    #[test]
    fn hydration_fails_closed_on_duplicate_entity_id() {
        use icn_store::{SledStore, Store};
        use std::sync::Arc;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let store: Arc<dyn Store> = Arc::new(SledStore::open(tmp.path()).unwrap());
        let entity = EntityId::cooperative("food-coop").unwrap();
        let creator = test_did("creator");

        // Seed TWO persisted treasuries bound to the SAME entity_id (different DIDs
        // and coop_ids) — an inconsistent store.
        let t1 = Treasury::new_with_entity(
            test_did("t1"),
            entity.clone(),
            "USD".to_string(),
            creator.clone(),
            None,
        );
        let mut t2 =
            Treasury::new_with_entity(test_did("t2"), entity, "USD".to_string(), creator, None);
        t2.coop_id = "coop:other".to_string(); // same entity, distinct coop_id

        for t in [&t1, &t2] {
            let key = format!("{}{}", TREASURY_PREFIX, t.treasury_did);
            store
                .put(key.as_bytes(), &serde_json::to_vec(t).unwrap())
                .unwrap();
        }

        // Hydration must fail closed rather than silently keep one mapping.
        assert!(
            TreasuryManager::with_store(store).is_err(),
            "hydration must fail closed on a duplicate entity_id in the store"
        );
    }

    #[test]
    fn hydration_fails_closed_on_duplicate_coop_id() {
        use icn_store::{SledStore, Store};
        use std::sync::Arc;
        use tempfile::TempDir;

        let tmp = TempDir::new().unwrap();
        let store: Arc<dyn Store> = Arc::new(SledStore::open(tmp.path()).unwrap());
        let creator = test_did("creator");

        // Two legacy treasuries (entity_id: None) sharing ONE coop_id, different
        // DIDs — an inconsistent store. Must fail closed rather than silently
        // collapse into one last-writer-wins coop index entry.
        let t1 = Treasury::new(
            test_did("t1"),
            "food-coop".to_string(),
            "USD".to_string(),
            creator.clone(),
            None,
        );
        let t2 = Treasury::new(
            test_did("t2"),
            "food-coop".to_string(),
            "USD".to_string(),
            creator,
            None,
        );
        for t in [&t1, &t2] {
            let key = format!("{}{}", TREASURY_PREFIX, t.treasury_did);
            store
                .put(key.as_bytes(), &serde_json::to_vec(t).unwrap())
                .unwrap();
        }

        assert!(
            TreasuryManager::with_store(store).is_err(),
            "hydration must fail closed on a duplicate coop_id in the store"
        );
    }

    #[test]
    fn populate_entity_id_writes_the_planned_did() {
        let mut mgr = TreasuryManager::new();
        let creator = test_did("creator");
        let did = test_did("t1");
        mgr.register_treasury(
            did.clone(),
            "coop:xyz".to_string(),
            "USD".to_string(),
            creator,
            None,
        )
        .unwrap();
        let entity = EntityId::cooperative("coop-legacy-abcd").unwrap();

        let result = mgr
            .populate_treasury_entity_id_for_did(&did, "coop:xyz", entity.clone())
            .unwrap();
        assert_eq!(result, TreasuryEntityIdPopulateResult::Populated);

        // The exact planned DID's row was mutated; coop_id preserved byte-for-byte.
        let t = mgr.get_treasury(&did).unwrap();
        assert_eq!(t.entity_id(), Some(&entity));
        assert_eq!(t.coop_id(), "coop:xyz");
        // Entity index resolves to that same row.
        assert_eq!(
            mgr.get_treasury_by_entity(&entity).unwrap().coop_id(),
            "coop:xyz"
        );
    }

    #[test]
    fn populate_entity_id_fails_closed_on_coop_id_mismatch_or_missing_did() {
        let mut mgr = TreasuryManager::new();
        let creator = test_did("creator");
        let did = test_did("t1");
        mgr.register_treasury(
            did.clone(),
            "coop:xyz".to_string(),
            "USD".to_string(),
            creator,
            None,
        )
        .unwrap();
        let entity = EntityId::cooperative("coop-legacy-abcd").unwrap();

        // Planned coop_id no longer matches the located row's coop_id -> fail
        // closed, nothing written, entity not indexed.
        let mismatch = mgr
            .populate_treasury_entity_id_for_did(&did, "coop:WRONG", entity.clone())
            .unwrap();
        assert_eq!(mismatch, TreasuryEntityIdPopulateResult::CoopIdMismatch);
        assert!(mgr.get_treasury(&did).unwrap().entity_id().is_none());
        assert!(mgr.get_treasury_by_entity(&entity).is_none());

        // A DID that is not registered -> TreasuryNotFound, nothing written.
        let ghost = test_did("ghost");
        let notfound = mgr
            .populate_treasury_entity_id_for_did(&ghost, "coop:xyz", entity.clone())
            .unwrap();
        assert_eq!(notfound, TreasuryEntityIdPopulateResult::TreasuryNotFound);
        assert!(mgr.get_treasury_by_entity(&entity).is_none());
    }

    // ── #2627 M1: classify before adopt ─────────────────────────────────────

    fn sled_store() -> (tempfile::TempDir, Arc<dyn Store>) {
        let tmp = tempfile::TempDir::new().unwrap();
        let store: Arc<dyn Store> = Arc::new(icn_store::SledStore::open(tmp.path()).unwrap());
        (tmp, store)
    }

    fn alias_of(did: &Did) -> Did {
        Did::from_str(&format!(
            "did:icn:f{}",
            hex::encode(did.identifier_bytes().unwrap())
        ))
        .unwrap()
    }

    fn put_primary(store: &Arc<dyn Store>, t: &Treasury) {
        let key = format!("{}{}", TREASURY_PREFIX, t.treasury_did);
        store
            .put(key.as_bytes(), &serde_json::to_vec(t).unwrap())
            .unwrap();
    }

    #[test]
    fn a_refused_hydration_leaves_every_map_untouched() {
        // The maps are private, so this pin lives in-module: it calls the
        // loader on a manager it can inspect. Rows are laid out so that a
        // loader adopting as it scans would have hydrated a valid treasury,
        // its budget and its index entries before reaching the alias pair.
        let (_tmp, store) = sled_store();
        let creator = test_did("creator");
        let first = KeyPair::generate().unwrap().did().clone();
        let colliding = KeyPair::generate().unwrap().did().clone();
        // `first` sorts before the pair only if its spelling does; make the
        // pair's spellings the byte-greatest by using the `z` spelling of
        // `colliding` and its `f` alias, and give `first` an `F` spelling.
        let first_upper = Did::from_str(&format!(
            "did:icn:F{}",
            hex::encode_upper(first.identifier_bytes().unwrap())
        ))
        .unwrap();
        put_primary(
            &store,
            &Treasury::new(
                first_upper.clone(),
                "coop-1".into(),
                "USD".into(),
                creator.clone(),
                None,
            ),
        );
        put_primary(
            &store,
            &Treasury::new(
                colliding.clone(),
                "coop-2".into(),
                "USD".into(),
                creator.clone(),
                None,
            ),
        );
        put_primary(
            &store,
            &Treasury::new(
                alias_of(&colliding),
                "coop-2".into(),
                "USD".into(),
                creator.clone(),
                None,
            ),
        );
        let budget = TreasuryBudget::new(
            first_upper.clone(),
            "ops".into(),
            100,
            "USD".into(),
            None,
            creator.clone(),
            None,
        );
        store
            .put(
                format!("{}{}", BUDGET_PREFIX, budget.id).as_bytes(),
                &serde_json::to_vec(&budget).unwrap(),
            )
            .unwrap();

        let mut mgr = TreasuryManager::new();
        mgr.store = Some(store);
        let err = mgr.load_from_store().expect_err("the alias pair refuses");
        assert!(
            err.downcast_ref::<PrincipalRowsRefusal>().is_some(),
            "{err}"
        );

        assert!(mgr.treasuries.is_empty(), "treasuries partially adopted");
        assert!(
            mgr.coop_treasuries.is_empty(),
            "coop index partially adopted"
        );
        assert!(mgr.entity_treasuries.is_empty());
        assert!(
            mgr.treasury_budgets.is_empty(),
            "budget index partially adopted"
        );
        assert!(mgr.treasury_rules.is_empty());
        assert!(mgr.budgets.is_empty(), "budgets adopted before the refusal");
        assert!(mgr.spending_rules.is_empty());
    }

    #[test]
    fn a_refused_coop_index_leaves_every_map_untouched() {
        // The index is validated after the primary rows classify cleanly, so
        // this is the last refusal before adoption; it must still adopt nothing.
        let (_tmp, store) = sled_store();
        let creator = test_did("creator");
        let did = KeyPair::generate().unwrap().did().clone();
        put_primary(
            &store,
            &Treasury::new(did.clone(), "coop-1".into(), "USD".into(), creator, None),
        );
        store
            .put(
                format!("{TREASURY_IDX_COOP_PREFIX}coop-1").as_bytes(),
                alias_of(&did).as_str().as_bytes(),
            )
            .unwrap();

        let mut mgr = TreasuryManager::new();
        mgr.store = Some(store);
        let err = mgr.load_from_store().expect_err("the index disagrees");
        assert!(matches!(
            err.downcast_ref::<TreasuryHydrationRefusal>(),
            Some(TreasuryHydrationRefusal::CoopIndexSpellingMismatch { rows: 1 })
        ));
        assert!(mgr.treasuries.is_empty());
        assert!(mgr.coop_treasuries.is_empty());
        assert!(mgr.treasury_budgets.is_empty());
    }

    #[test]
    fn treasury_keys_classify_by_shape_alone() {
        let did = KeyPair::generate().unwrap().did().clone();
        let primary = format!("{TREASURY_PREFIX}{did}");
        assert!(matches!(
            classify_treasury_key(primary.as_bytes()),
            TreasuryKey::Primary(ref s) if s == did.as_str()
        ));

        for sibling in TREASURY_SIBLING_SUBSPACES {
            let key = format!("{sibling}{did}:anything");
            assert!(
                matches!(classify_treasury_key(key.as_bytes()), TreasuryKey::Sibling),
                "{sibling} is a sibling subspace whatever follows it"
            );
        }

        for unreadable in [
            TREASURY_PREFIX.to_string(),
            format!("{TREASURY_PREFIX}did:icn:"),
            format!("{TREASURY_PREFIX}did:icn:zNOTAKEY"),
            format!("{TREASURY_PREFIX}{did}junk"),
            format!("{TREASURY_PREFIX}idx:other:{did}"),
            format!("ledger:other:{did}"),
        ] {
            assert!(
                matches!(
                    classify_treasury_key(unreadable.as_bytes()),
                    TreasuryKey::Unreadable
                ),
                "{unreadable} must be unreadable, not a primary or a sibling"
            );
        }
        let mut non_utf8 = primary.into_bytes();
        non_utf8.push(0xFF);
        assert!(matches!(
            classify_treasury_key(&non_utf8),
            TreasuryKey::Unreadable
        ));
    }

    #[test]
    fn the_sibling_list_names_every_prefix_the_writers_produce() {
        // Every `ledger:treasury:`-rooted prefix constant in this module and
        // its submodules is either the primary prefix or in the sibling list.
        // A writer that gains a new subspace must add it here, or its rows
        // will refuse hydration as unreadable primaries — deliberately.
        for prefix in [
            BUDGET_PREFIX,
            SPENDING_RULE_PREFIX,
            TREASURY_AUDIT_PREFIX,
            TREASURY_IDX_COOP_PREFIX,
            TREASURY_IDX_BUDGETS_PREFIX,
            VELOCITY_LIMIT_PREFIX,
        ] {
            assert!(prefix.starts_with(TREASURY_PREFIX));
            assert!(TREASURY_SIBLING_SUBSPACES.contains(&prefix), "{prefix}");
        }
        assert_eq!(TREASURY_SIBLING_SUBSPACES.len(), 6);
    }

    #[test]
    fn the_scanner_descriptor_claims_exactly_the_primary_row_shape() {
        // Gate ↔ loader agreement, pinned against the ledger's own constants:
        // the registered prefix is the primary prefix run through the DID
        // scheme, so it matches every key `persist_treasury` writes and no
        // key of any sibling subspace; the key ends with the spelling; the
        // disposition is the loader's — fail closed, established in code.
        use icn_store::did_collision_scan::{
            n2a_keyspaces, MergeDisposition, PrincipalRegion, RuleBasis,
        };
        let descriptor = n2a_keyspaces()
            .into_iter()
            .find(|d| d.name == TREASURY_KEYSPACE)
            .expect("registered");

        assert_eq!(
            descriptor.prefix,
            format!("{TREASURY_PREFIX}did:icn:").as_bytes(),
            "the descriptor claims the primary rows through the DID scheme"
        );
        let did = KeyPair::generate().unwrap().did().clone();
        let written = format!("{}{}", TREASURY_PREFIX, did);
        assert!(written.as_bytes().starts_with(descriptor.prefix));
        for sibling in TREASURY_SIBLING_SUBSPACES {
            assert!(
                !sibling.as_bytes().starts_with(descriptor.prefix)
                    && !descriptor.prefix.starts_with(sibling.as_bytes()),
                "{sibling} must be neither inside nor around the descriptor prefix"
            );
        }
        assert!(descriptor.did_ends_key, "nothing follows the spelling");
        assert!(!descriptor.slash_ends_did);
        assert!(matches!(
            descriptor.principal_region,
            PrincipalRegion::WholeKey
        ));
        assert_eq!(descriptor.disposition, MergeDisposition::FailClosed);
        assert_eq!(descriptor.basis, RuleBasis::Established);
        assert!(descriptor.inventory_rows.contains(&10));
        assert!(descriptor.inventory_rows.contains(&41));
    }

    #[test]
    fn the_refusal_texts_carry_no_spelling_no_coop_id_and_no_value() {
        let (_tmp, store) = sled_store();
        let creator = test_did("creator");
        let did = KeyPair::generate().unwrap().did().clone();
        let alias = alias_of(&did);
        put_primary(
            &store,
            &Treasury::new(
                did.clone(),
                "secret-coop".into(),
                "USD".into(),
                creator.clone(),
                Some("secret description".into()),
            ),
        );
        store
            .put(
                format!("{TREASURY_IDX_COOP_PREFIX}secret-coop").as_bytes(),
                alias.as_str().as_bytes(),
            )
            .unwrap();

        let text = TreasuryManager::with_store(store)
            .err()
            .expect("index mismatch refuses")
            .to_string();
        assert!(text.contains(TREASURY_KEYSPACE));
        for leaked in [
            did.as_str(),
            alias.as_str(),
            "secret-coop",
            "secret description",
        ] {
            assert!(!text.contains(leaked), "leaked {leaked:?} in {text}");
        }
        assert_eq!(text.lines().count(), 1);
    }
}
