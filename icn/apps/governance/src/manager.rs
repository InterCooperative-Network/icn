//! Governance Manager for Gateway API
//!
//! Moved from `icn-gateway` to the app layer so the manager lives next to the
//! domain it manages. The only structural change relative to the gateway version
//! is that `ReceiptStore` (a gateway-internal type) is replaced by the
//! [`GovernanceReceiptBackend`] trait so this crate does not depend on
//! `icn-gateway`.

use crate::receipt_backend::GovernanceReceiptBackend;
use anyhow::Result;
use icn_federation::BilateralClearingAgreement;
use icn_governance::{
    scopes_overlap, ActionItem, ActionItemFilter, ActionItemId, ActionItemPriority,
    ActionItemStatus, ActionItemStoreBackend, Activity, ActivityId, ActivityKind,
    ActivityStoreBackend, Comment, CommentId, Delegation, DelegationId, DelegationScope,
    Discussion, DiscussionStore, GovernanceConfig, GovernanceDecisionReceipt, GovernanceDomain,
    GovernanceDomainId, GovernanceError, GovernanceOps, GovernanceParams, GovernanceProfileId,
    InMemoryActionItemStore, InMemoryActivityStore, InMemoryDiscussionStore, InMemoryMeetingStore,
    InMemoryMilestoneStore, InMemoryProgramStore, InMemoryStructureStore, Meeting, MeetingId,
    MeetingStoreBackend, MembershipConfig, MembershipSource, Milestone, MilestoneId,
    MilestoneStatus, MilestoneStoreBackend, PaginatedResult, Program, ProgramId, ProgramKind,
    ProgramStoreBackend, ProofOutcome, Proposal, ProposalDomainLookup, ProposalId, ProposalPayload,
    ProposalScope, ProposalState, RoleAssignment, Structure, StructureId, StructureKind,
    StructureStoreBackend, Timestamp, Vote, VoteChoice, VoteTally, DEFAULT_MAX_DELEGATION_DEPTH,
};
use icn_identity::Did;
use icn_kernel_api::{AllocationReceipt, ScopeLevel, SettlementIntent};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tracing::debug;

// ============================================================================
// Notification digest types
// ============================================================================

/// A pending vote in the digest: an Open proposal the caller has not yet voted on.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingVoteDigest {
    /// The proposal ID
    pub proposal_id: String,
    /// Governance domain the proposal lives in
    pub domain_id: String,
    /// Human-readable title
    pub title: String,
    /// Unix timestamp when voting closes (`None` if not set)
    pub closes_at: Option<u64>,
}

/// An overdue action item in the digest: assigned to the caller, past due, not completed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OverdueItemDigest {
    /// The action item ID
    pub item_id: String,
    /// Governance domain the item belongs to
    pub domain_id: String,
    /// Human-readable title
    pub title: String,
    /// Unix timestamp of the due date
    pub due_date: u64,
}

/// An upcoming meeting in the digest: scheduled within the lookahead window
/// and the caller is in the attendee list.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpcomingMeetingDigest {
    /// The meeting ID
    pub meeting_id: String,
    /// Governance domain the meeting belongs to
    pub domain_id: String,
    /// Human-readable title
    pub title: String,
    /// Unix timestamp when the meeting is scheduled to start
    pub scheduled_at: u64,
}

/// DID-scoped notification digest returned by `GET /gov/digest`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DigestSummary {
    /// The DID this digest was generated for
    pub did: String,
    /// Count of pending votes
    pub pending_vote_count: usize,
    /// Open proposals the caller has not yet voted on
    pub pending_votes: Vec<PendingVoteDigest>,
    /// Count of overdue action items
    pub overdue_item_count: usize,
    /// Action items assigned to caller that are past their due date
    pub overdue_items: Vec<OverdueItemDigest>,
    /// Count of upcoming meetings
    pub upcoming_meeting_count: usize,
    /// Meetings in the next 48 h where the caller is listed as an attendee
    pub upcoming_meetings: Vec<UpcomingMeetingDigest>,
}

/// Full provenance chain for a governance proposal (INV-5).
///
/// Links governance decision → allocation receipts for independent verification.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProvenanceChain {
    /// Governance decision receipt (present if proposal was closed)
    pub governance_receipt: Option<icn_governance::GovernanceDecisionReceipt>,
    /// Allocation receipts linking decision to economic intents
    pub allocations: Vec<AllocationReceipt>,
    /// True if the chain is complete for this proposal type
    pub chain_complete: bool,
}

// ============================================================================
// Program dashboard aggregate types
// ============================================================================

/// Action item status counts for the program dashboard.
///
/// Only counts items whose `parent` is an `InstitutionalParent::Activity`
/// belonging to the program. Domain items not attached to a program activity
/// are excluded.
#[derive(Debug, Clone, Default)]
pub struct ProgramActionItemCounts {
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
    pub deferred: usize,
    pub cancelled: usize,
}

impl ProgramActionItemCounts {
    pub fn total(&self) -> usize {
        self.pending + self.in_progress + self.completed + self.deferred + self.cancelled
    }
}

/// Composite program dashboard: program + ordered milestones + linked
/// activities + action item counts + meetings linked through those activities.
///
/// Assembled by [`GovernanceManager::get_program_dashboard`] and converted to
/// [`crate::http::models::ProgramDashboardResponse`] by the HTTP handler.
pub struct ProgramDashboard {
    pub program: Program,
    pub milestones: Vec<Milestone>,
    pub activities: Vec<Activity>,
    pub action_item_counts: ProgramActionItemCounts,
    /// All meetings linked to at least one activity that belongs to this
    /// program. Deduped (a meeting linked to two activities appears once),
    /// sorted earliest `scheduled_at` first; unscheduled meetings sort last.
    pub meetings: Vec<Meeting>,
}

// ============================================================================
// Sled-Backed Action Item Store
// ============================================================================

/// Sled-backed storage for action items
///
/// # Performance Characteristics
///
/// The current implementation uses prefix scanning for list operations, which is O(n)
/// where n is the number of action items in the domain. This is acceptable for
/// typical cooperative workloads (< 1000 items per domain).
///
/// For larger deployments with 10K+ items per domain, consider adding secondary
/// indexes for common filter fields.
pub struct SledActionItemStore {
    db: Arc<sled::Db>,
}

impl SledActionItemStore {
    /// Create a new Sled-backed action item store
    pub fn new(db: Arc<sled::Db>) -> Self {
        Self { db }
    }

    /// Generate key for an action item
    fn item_key(domain_id: &GovernanceDomainId, id: &ActionItemId) -> String {
        format!("action_item:{}:{}", domain_id.0, id.0)
    }

    /// Generate prefix for all action items in a domain
    fn domain_prefix(domain_id: &GovernanceDomainId) -> String {
        format!("action_item:{}:", domain_id.0)
    }

    /// Generate the assignee secondary index key.
    ///
    /// Format: `action_item_by_assignee:{assignee_did}:{domain_id}:{item_id}`
    /// Value: `b"1"` (tombstone — presence signals membership, no data stored)
    fn assignee_idx_key(
        assignee: &icn_identity::Did,
        domain_id: &GovernanceDomainId,
        id: &ActionItemId,
    ) -> String {
        format!(
            "action_item_by_assignee:{}:{}:{}",
            assignee.as_str(),
            domain_id.0,
            id.0
        )
    }

    /// Prefix for scanning all assignee-index entries for a given DID.
    fn assignee_idx_prefix(assignee: &icn_identity::Did) -> String {
        format!("action_item_by_assignee:{}:", assignee.as_str())
    }
}

impl ActionItemStoreBackend for SledActionItemStore {
    fn save(&self, item: &ActionItem) -> std::result::Result<(), GovernanceError> {
        let key = Self::item_key(&item.domain_id, &item.id);

        // Remove stale assignee index entry if the assignee changed on update.
        if let Ok(Some(existing)) = self.get(&item.domain_id, &item.id) {
            if existing.assignee != item.assignee {
                if let Some(ref old_assignee) = existing.assignee {
                    let old_idx = Self::assignee_idx_key(old_assignee, &item.domain_id, &item.id);
                    self.db.remove(old_idx.as_bytes()).map_err(|e| {
                        GovernanceError::Internal(format!(
                            "Sled assignee-idx remove (stale) failed: {e}"
                        ))
                    })?;
                }
            }
        }

        let value = icn_encoding::encode_versioned(item)
            .map_err(|e| GovernanceError::Internal(format!("Failed to encode action item: {e}")))?;
        self.db
            .insert(key.as_bytes(), value)
            .map_err(|e| GovernanceError::Internal(format!("Sled insert failed: {e}")))?;

        // Maintain assignee secondary index
        if let Some(ref assignee) = item.assignee {
            let idx_key = Self::assignee_idx_key(assignee, &item.domain_id, &item.id);
            self.db
                .insert(idx_key.as_bytes(), b"1" as &[u8])
                .map_err(|e| {
                    GovernanceError::Internal(format!("Sled assignee-idx insert failed: {e}"))
                })?;
        }
        Ok(())
    }

    fn get(
        &self,
        domain_id: &GovernanceDomainId,
        id: &ActionItemId,
    ) -> std::result::Result<Option<ActionItem>, GovernanceError> {
        let key = Self::item_key(domain_id, id);
        match self.db.get(key.as_bytes()) {
            Ok(Some(value)) => {
                let item = icn_encoding::decode_versioned(&value).map_err(|e| {
                    GovernanceError::Internal(format!("Failed to decode action item: {e}"))
                })?;
                Ok(Some(item))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(GovernanceError::Internal(format!("Sled get failed: {e}"))),
        }
    }

    fn list(
        &self,
        domain_id: &GovernanceDomainId,
        filter: &ActionItemFilter,
    ) -> std::result::Result<Vec<ActionItem>, GovernanceError> {
        let prefix = Self::domain_prefix(domain_id);
        let mut items = Vec::new();

        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) =
                result.map_err(|e| GovernanceError::Internal(format!("Sled scan failed: {e}")))?;
            let item: ActionItem = icn_encoding::decode_versioned(&value).map_err(|e| {
                GovernanceError::Internal(format!("Failed to decode action item: {e}"))
            })?;
            if filter.matches(&item) {
                items.push(item);
            }
        }

        // Sort by created_at descending (newest first)
        items.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(items)
    }

    fn delete(
        &self,
        domain_id: &GovernanceDomainId,
        id: &ActionItemId,
    ) -> std::result::Result<bool, GovernanceError> {
        let key = Self::item_key(domain_id, id);

        // Load item first so we can clean up the assignee index
        if let Ok(Some(existing)) = self.get(domain_id, id) {
            if let Some(ref assignee) = existing.assignee {
                let idx_key = Self::assignee_idx_key(assignee, domain_id, id);
                self.db.remove(idx_key.as_bytes()).map_err(|e| {
                    GovernanceError::Internal(format!("Sled assignee-idx delete failed: {e}"))
                })?;
            }
        }

        self.db
            .remove(key.as_bytes())
            .map(|opt| opt.is_some())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete failed: {e}")))
    }

    /// Scan the assignee secondary index and return all items assigned to `assignee`.
    ///
    /// Index key format: `action_item_by_assignee:{did}:{domain_id}:{item_id}`
    /// Each entry is a tombstone; the actual item is read from the primary key.
    fn list_by_assignee(
        &self,
        assignee: &icn_identity::Did,
    ) -> std::result::Result<Vec<ActionItem>, GovernanceError> {
        let prefix = Self::assignee_idx_prefix(assignee);
        let mut items = Vec::new();

        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (raw_key, _) = result.map_err(|e| {
                GovernanceError::Internal(format!("Sled scan (assignee idx) failed: {e}"))
            })?;

            // Key format after prefix: "{domain_id}:{item_id}"
            // Use rsplitn so domain IDs containing ':' are parsed correctly.
            // UUIDs (item IDs) never contain ':', so splitting from the right
            // always yields the UUID last.
            let key_str = std::str::from_utf8(&raw_key).map_err(|e| {
                GovernanceError::Internal(format!("Invalid UTF-8 in assignee idx key: {e}"))
            })?;
            let suffix = key_str.strip_prefix(&prefix).unwrap_or(key_str);
            let mut parts = suffix.rsplitn(2, ':');
            let item_id_str = parts.next().unwrap_or("");
            let domain_id_str = parts.next().unwrap_or("");

            let domain_id = GovernanceDomainId(domain_id_str.to_string());
            let item_id: ActionItemId = item_id_str.parse().map_err(|e| {
                GovernanceError::Internal(format!("Invalid item_id in assignee idx: {e}"))
            })?;

            match self.get(&domain_id, &item_id) {
                Ok(Some(item)) => items.push(item),
                Ok(None) => {
                    // Stale index entry — primary was deleted without cleaning index.
                    // Skip silently; a background sweep would clean this.
                    tracing::debug!(
                        assignee = %assignee.as_str(),
                        domain = %domain_id_str,
                        item = %item_id_str,
                        "stale assignee index entry; primary key not found"
                    );
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        Ok(items)
    }

    fn count(
        &self,
        domain_id: &GovernanceDomainId,
        filter: &ActionItemFilter,
    ) -> std::result::Result<usize, GovernanceError> {
        let prefix = Self::domain_prefix(domain_id);
        let mut count = 0;

        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) =
                result.map_err(|e| GovernanceError::Internal(format!("Sled scan failed: {e}")))?;
            let item: ActionItem = icn_encoding::decode_versioned(&value).map_err(|e| {
                GovernanceError::Internal(format!("Failed to decode action item: {e}"))
            })?;
            if filter.matches(&item) {
                count += 1;
            }
        }

        Ok(count)
    }

    fn delete_all(
        &self,
        domain_id: &GovernanceDomainId,
    ) -> std::result::Result<usize, GovernanceError> {
        let prefix = Self::domain_prefix(domain_id);
        let mut deleted_count = 0;

        // Collect keys first to avoid iterator invalidation
        let keys: Vec<_> = self
            .db
            .scan_prefix(prefix.as_bytes())
            .filter_map(|r| r.ok().map(|(k, _)| k))
            .collect();

        for key in keys {
            if self
                .db
                .remove(key)
                .map_err(|e| GovernanceError::Internal(format!("Sled delete failed: {e}")))?
                .is_some()
            {
                deleted_count += 1;
            }
        }

        Ok(deleted_count)
    }
}

// ========== Sled store for Structures (Tranche 2) ==========

/// Sled-backed storage for internal structures and role assignments.
///
/// # Key scheme (dual-key — no suffix scanning)
///
/// Structure:
///   - Primary:      `structure:{structure_id}`                     → Structure
///   - Entity index: `structure_by_entity:{entity_id}:{structure_id}` → b"1"
///
/// Role:
///   - Primary:           `role:{role_id}`                             → RoleAssignment
///   - Structure index:   `role_by_structure:{structure_id}:{role_id}` → b"1"
pub struct SledStructureStore {
    db: Arc<sled::Db>,
}

impl SledStructureStore {
    /// Create a new Sled-backed structure store
    pub fn new(db: Arc<sled::Db>) -> Self {
        Self { db }
    }

    // --- Structure keys ---

    fn structure_primary_key(id: &icn_governance::StructureId) -> String {
        format!("structure:{}", id.0)
    }

    fn structure_idx_key(entity_id: &str, id: &icn_governance::StructureId) -> String {
        format!("structure_by_entity:{}:{}", entity_id, id.0)
    }

    fn entity_structure_index_prefix(entity_id: &str) -> String {
        format!("structure_by_entity:{}:", entity_id)
    }

    // --- Role keys ---

    fn role_primary_key(rid: &icn_governance::RoleAssignmentId) -> String {
        format!("role:{}", rid.0)
    }

    fn role_idx_key(
        sid: &icn_governance::StructureId,
        rid: &icn_governance::RoleAssignmentId,
    ) -> String {
        format!("role_by_structure:{}:{}", sid.0, rid.0)
    }

    fn structure_role_index_prefix(sid: &icn_governance::StructureId) -> String {
        format!("role_by_structure:{}:", sid.0)
    }
}

impl icn_governance::StructureStoreBackend for SledStructureStore {
    fn save_structure(
        &self,
        s: &icn_governance::Structure,
    ) -> std::result::Result<(), GovernanceError> {
        let primary = Self::structure_primary_key(&s.id);
        let idx = Self::structure_idx_key(&s.parent_entity_id, &s.id);
        let value = icn_encoding::encode_versioned(s)
            .map_err(|e| GovernanceError::Internal(format!("Failed to encode structure: {e}")))?;
        self.db
            .insert(primary.as_bytes(), value)
            .map_err(|e| GovernanceError::Internal(format!("Sled insert failed: {e}")))?;
        self.db
            .insert(idx.as_bytes(), b"1".as_ref())
            .map_err(|e| GovernanceError::Internal(format!("Sled insert index failed: {e}")))?;
        Ok(())
    }

    fn get_structure(
        &self,
        id: &icn_governance::StructureId,
    ) -> std::result::Result<Option<icn_governance::Structure>, GovernanceError> {
        let key = Self::structure_primary_key(id);
        match self.db.get(key.as_bytes()) {
            Ok(Some(value)) => {
                let s: icn_governance::Structure =
                    icn_encoding::decode_versioned(&value).map_err(|e| {
                        GovernanceError::Internal(format!("Failed to decode structure: {e}"))
                    })?;
                Ok(Some(s))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(GovernanceError::Internal(format!("Sled get failed: {e}"))),
        }
    }

    fn list_structures_by_entity(
        &self,
        entity_id: &str,
    ) -> std::result::Result<Vec<icn_governance::Structure>, GovernanceError> {
        let prefix = Self::entity_structure_index_prefix(entity_id);
        let prefix_len = prefix.len();
        let mut out = Vec::new();
        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (idx_key, _) =
                result.map_err(|e| GovernanceError::Internal(format!("Sled scan failed: {e}")))?;
            let idx_str = std::str::from_utf8(&idx_key)
                .map_err(|e| GovernanceError::Internal(format!("Invalid UTF-8 key: {e}")))?;
            // Extract structure_id from the tail of the index key
            let sid_str = &idx_str[prefix_len..];
            let sid = icn_governance::StructureId(sid_str.to_string());
            let primary = Self::structure_primary_key(&sid);
            if let Some(value) = self
                .db
                .get(primary.as_bytes())
                .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
            {
                let s: icn_governance::Structure =
                    icn_encoding::decode_versioned(&value).map_err(|e| {
                        GovernanceError::Internal(format!("Failed to decode structure: {e}"))
                    })?;
                out.push(s);
            }
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    fn delete_structure(
        &self,
        id: &icn_governance::StructureId,
    ) -> std::result::Result<bool, GovernanceError> {
        // First fetch to know the entity_id (needed for the index key)
        let primary = Self::structure_primary_key(id);
        let existing = self
            .db
            .get(primary.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?;
        let s: icn_governance::Structure = match existing {
            Some(v) => icn_encoding::decode_versioned(&v).map_err(|e| {
                GovernanceError::Internal(format!("Failed to decode structure: {e}"))
            })?,
            None => return Ok(false),
        };

        // Cascade: delete all roles for this structure
        let role_idx_prefix = Self::structure_role_index_prefix(id);
        let role_idx_prefix_len = role_idx_prefix.len();
        let role_idx_keys: Vec<sled::IVec> = self
            .db
            .scan_prefix(role_idx_prefix.as_bytes())
            .filter_map(|r| r.ok().map(|(k, _)| k))
            .collect();
        for idx_key in role_idx_keys {
            let idx_str = std::str::from_utf8(&idx_key)
                .map_err(|e| GovernanceError::Internal(format!("Invalid UTF-8 key: {e}")))?;
            let rid_str = &idx_str[role_idx_prefix_len..];
            // Parse as UUID for RoleAssignmentId
            if let Ok(uuid) = uuid::Uuid::parse_str(rid_str) {
                let rid = icn_governance::RoleAssignmentId::from_uuid(uuid);
                let role_primary = Self::role_primary_key(&rid);
                self.db
                    .remove(role_primary.as_bytes())
                    .map_err(|e| GovernanceError::Internal(format!("Sled delete failed: {e}")))?;
            }
            self.db
                .remove(&idx_key)
                .map_err(|e| GovernanceError::Internal(format!("Sled delete index failed: {e}")))?;
        }

        // Delete entity index key
        let idx = Self::structure_idx_key(&s.parent_entity_id, id);
        self.db
            .remove(idx.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete index failed: {e}")))?;

        // Delete primary
        self.db
            .remove(primary.as_bytes())
            .map(|opt| opt.is_some())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete failed: {e}")))
    }

    fn save_role(
        &self,
        r: &icn_governance::RoleAssignment,
    ) -> std::result::Result<(), GovernanceError> {
        let primary = Self::role_primary_key(&r.id);
        let idx = Self::role_idx_key(&r.structure_id, &r.id);
        let value = icn_encoding::encode_versioned(r)
            .map_err(|e| GovernanceError::Internal(format!("Failed to encode role: {e}")))?;
        self.db
            .insert(primary.as_bytes(), value)
            .map_err(|e| GovernanceError::Internal(format!("Sled insert failed: {e}")))?;
        self.db
            .insert(idx.as_bytes(), b"1".as_ref())
            .map_err(|e| GovernanceError::Internal(format!("Sled insert index failed: {e}")))?;
        Ok(())
    }

    fn get_role(
        &self,
        id: &icn_governance::RoleAssignmentId,
    ) -> std::result::Result<Option<icn_governance::RoleAssignment>, GovernanceError> {
        let key = Self::role_primary_key(id);
        match self.db.get(key.as_bytes()) {
            Ok(Some(value)) => {
                let r: icn_governance::RoleAssignment = icn_encoding::decode_versioned(&value)
                    .map_err(|e| {
                        GovernanceError::Internal(format!("Failed to decode role: {e}"))
                    })?;
                Ok(Some(r))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(GovernanceError::Internal(format!("Sled get failed: {e}"))),
        }
    }

    fn list_roles_by_structure(
        &self,
        sid: &icn_governance::StructureId,
    ) -> std::result::Result<Vec<icn_governance::RoleAssignment>, GovernanceError> {
        let prefix = Self::structure_role_index_prefix(sid);
        let prefix_len = prefix.len();
        let mut out = Vec::new();
        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (idx_key, _) =
                result.map_err(|e| GovernanceError::Internal(format!("Sled scan failed: {e}")))?;
            let idx_str = std::str::from_utf8(&idx_key)
                .map_err(|e| GovernanceError::Internal(format!("Invalid UTF-8 key: {e}")))?;
            let rid_str = &idx_str[prefix_len..];
            if let Ok(uuid) = uuid::Uuid::parse_str(rid_str) {
                let rid = icn_governance::RoleAssignmentId::from_uuid(uuid);
                let primary = Self::role_primary_key(&rid);
                if let Some(value) = self
                    .db
                    .get(primary.as_bytes())
                    .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
                {
                    let r: icn_governance::RoleAssignment = icn_encoding::decode_versioned(&value)
                        .map_err(|e| {
                            GovernanceError::Internal(format!("Failed to decode role: {e}"))
                        })?;
                    out.push(r);
                }
            }
        }
        out.sort_by(|a, b| a.start_date.cmp(&b.start_date));
        Ok(out)
    }

    fn delete_role(
        &self,
        id: &icn_governance::RoleAssignmentId,
    ) -> std::result::Result<bool, GovernanceError> {
        // Fetch role to know its structure_id (needed for index key)
        let primary = Self::role_primary_key(id);
        let existing = self
            .db
            .get(primary.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?;
        let r: icn_governance::RoleAssignment = match existing {
            Some(v) => icn_encoding::decode_versioned(&v)
                .map_err(|e| GovernanceError::Internal(format!("Failed to decode role: {e}")))?,
            None => return Ok(false),
        };
        // Delete index key
        let idx = Self::role_idx_key(&r.structure_id, id);
        self.db
            .remove(idx.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete index failed: {e}")))?;
        // Delete primary
        self.db
            .remove(primary.as_bytes())
            .map(|opt| opt.is_some())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete failed: {e}")))
    }

    /// Scan all `role:` primary keys and collect those belonging to the given DID.
    ///
    /// This is a full scan (O(roles)) — acceptable for cooperative-scale deployments.
    /// A `role_by_person` secondary index can be added later if profiling warrants it.
    fn list_roles_by_person(
        &self,
        did: &icn_identity::Did,
    ) -> std::result::Result<Vec<icn_governance::RoleAssignment>, GovernanceError> {
        let prefix = "role:";
        let mut out = Vec::new();
        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) = result
                .map_err(|e| GovernanceError::Internal(format!("Sled scan (roles) failed: {e}")))?;
            let r: icn_governance::RoleAssignment = icn_encoding::decode_versioned(&value)
                .map_err(|e| GovernanceError::Internal(format!("Failed to decode role: {e}")))?;
            if &r.person_did == did {
                out.push(r);
            }
        }
        out.sort_by(|a, b| a.start_date.cmp(&b.start_date));
        Ok(out)
    }
}

// ========== Sled store for Activities (Tranche 2) ==========

/// Sled-backed storage for activities.
///
/// # Key scheme (dual-key — no suffix scanning)
///
///   - Primary:      `activity:{activity_id}`                        → Activity
///   - Entity index: `activity_by_entity:{entity_id}:{activity_id}` → b"1"
pub struct SledActivityStore {
    db: Arc<sled::Db>,
}

impl SledActivityStore {
    /// Create a new Sled-backed activity store
    pub fn new(db: Arc<sled::Db>) -> Self {
        Self { db }
    }

    fn activity_primary_key(id: &icn_governance::ActivityId) -> String {
        format!("activity:{}", id.0)
    }

    fn activity_idx_key(entity_id: &str, id: &icn_governance::ActivityId) -> String {
        format!("activity_by_entity:{}:{}", entity_id, id.0)
    }

    fn entity_activity_index_prefix(entity_id: &str) -> String {
        format!("activity_by_entity:{}:", entity_id)
    }

    /// Decode an Activity from stored bytes with V0 → V1 migration support.
    ///
    /// Postcard encodes structs positionally, so records written before
    /// `parent_program_id` was added (`ActivityV0`) will fail to decode as the
    /// current `Activity` struct with `DeserializeUnexpectedEnd`. This function
    /// falls back to decoding as `ActivityV0` and upgrading with
    /// `parent_program_id: None` so reads of old records succeed without a
    /// full store migration scan.
    fn decode_activity(
        bytes: &[u8],
    ) -> std::result::Result<icn_governance::Activity, GovernanceError> {
        // Fast path: current layout (all records written after parent_program_id landed).
        match icn_encoding::decode_versioned::<icn_governance::Activity>(bytes) {
            Ok(a) => return Ok(a),
            Err(icn_encoding::Error::Postcard(_)) => {
                // May be a V0 record (no parent_program_id). Try the old layout.
                tracing::debug!("activity decode failed with postcard error; trying V0 migration");
            }
            Err(e) => {
                return Err(GovernanceError::Internal(format!(
                    "Failed to decode activity (non-postcard error): {e}"
                )));
            }
        }

        // V0 layout: same fields as Activity minus `parent_program_id`.
        #[derive(serde::Deserialize)]
        struct ActivityV0 {
            id: icn_governance::ActivityId,
            parent_entity_id: String,
            kind: icn_governance::ActivityKind,
            name: String,
            description: Option<String>,
            status: icn_governance::ActivityStatus,
            start_date: Option<icn_governance::Timestamp>,
            end_date: Option<icn_governance::Timestamp>,
            linked_structures: Vec<icn_governance::StructureId>,
            created_at: icn_governance::Timestamp,
            created_by_decision: Option<icn_governance::ProposalId>,
        }

        let v0: ActivityV0 = icn_encoding::decode_versioned(bytes).map_err(|e| {
            GovernanceError::Internal(format!(
                "Failed to decode activity (V0 fallback also failed): {e}"
            ))
        })?;

        Ok(icn_governance::Activity {
            id: v0.id,
            parent_entity_id: v0.parent_entity_id,
            kind: v0.kind,
            name: v0.name,
            description: v0.description,
            status: v0.status,
            start_date: v0.start_date,
            end_date: v0.end_date,
            linked_structures: v0.linked_structures,
            created_at: v0.created_at,
            created_by_decision: v0.created_by_decision,
            parent_program_id: None,
        })
    }
}

impl icn_governance::ActivityStoreBackend for SledActivityStore {
    fn save(&self, a: &icn_governance::Activity) -> std::result::Result<(), GovernanceError> {
        let primary = Self::activity_primary_key(&a.id);
        let idx = Self::activity_idx_key(&a.parent_entity_id, &a.id);
        let value = icn_encoding::encode_versioned(a)
            .map_err(|e| GovernanceError::Internal(format!("Failed to encode activity: {e}")))?;
        self.db
            .insert(primary.as_bytes(), value)
            .map_err(|e| GovernanceError::Internal(format!("Sled insert failed: {e}")))?;
        self.db
            .insert(idx.as_bytes(), b"1".as_ref())
            .map_err(|e| GovernanceError::Internal(format!("Sled insert index failed: {e}")))?;
        Ok(())
    }

    fn get(
        &self,
        id: &icn_governance::ActivityId,
    ) -> std::result::Result<Option<icn_governance::Activity>, GovernanceError> {
        let key = Self::activity_primary_key(id);
        match self.db.get(key.as_bytes()) {
            Ok(Some(value)) => Ok(Some(Self::decode_activity(&value)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(GovernanceError::Internal(format!("Sled get failed: {e}"))),
        }
    }

    fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> std::result::Result<Vec<icn_governance::Activity>, GovernanceError> {
        let prefix = Self::entity_activity_index_prefix(entity_id);
        let prefix_len = prefix.len();
        let mut out = Vec::new();
        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (idx_key, _) =
                result.map_err(|e| GovernanceError::Internal(format!("Sled scan failed: {e}")))?;
            let idx_str = std::str::from_utf8(&idx_key)
                .map_err(|e| GovernanceError::Internal(format!("Invalid UTF-8 key: {e}")))?;
            let aid_str = &idx_str[prefix_len..];
            let aid = icn_governance::ActivityId(aid_str.to_string());
            let primary = Self::activity_primary_key(&aid);
            if let Some(value) = self
                .db
                .get(primary.as_bytes())
                .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
            {
                out.push(Self::decode_activity(&value)?);
            }
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    fn delete(
        &self,
        id: &icn_governance::ActivityId,
    ) -> std::result::Result<bool, GovernanceError> {
        // Fetch to know entity_id for the index key
        let primary = Self::activity_primary_key(id);
        let existing = self
            .db
            .get(primary.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?;
        let a: icn_governance::Activity = match existing {
            Some(v) => Self::decode_activity(&v)?,
            None => return Ok(false),
        };
        // Delete index key
        let idx = Self::activity_idx_key(&a.parent_entity_id, id);
        self.db
            .remove(idx.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete index failed: {e}")))?;
        // Delete primary
        self.db
            .remove(primary.as_bytes())
            .map(|opt| opt.is_some())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete failed: {e}")))
    }
}

// ============================================================================
// Sled-Backed Program Store
// ============================================================================

/// Sled-backed storage for programs.
///
/// Key scheme (triple-key):
/// - Primary:        `program:{program_id}`                         → postcard-encoded Program
/// - Domain index:   `program_by_domain:{domain_id}:{program_id}`   → empty (membership marker)
/// - Entity index:   `program_by_entity:{entity_id}:{program_id}`   → empty (membership marker)
///
/// Matches the `<thing>_by_<scope>:...` secondary-index convention used by
/// `SledActivityStore`/`SledMeetingStore`/`SledStructureStore`. Programs are
/// queryable both by governance domain and by parent entity; both indexes are
/// maintained by [`save`](SledProgramStore::save) and cleaned by
/// [`delete`](SledProgramStore::delete).
pub struct SledProgramStore {
    db: Arc<sled::Db>,
}

impl SledProgramStore {
    /// Create a new Sled-backed program store.
    pub fn new(db: Arc<sled::Db>) -> Self {
        Self { db }
    }

    fn program_primary_key(id: &icn_governance::ProgramId) -> String {
        format!("program:{}", id.0)
    }

    fn program_domain_idx_key(
        domain_id: &GovernanceDomainId,
        id: &icn_governance::ProgramId,
    ) -> String {
        format!("program_by_domain:{}:{}", domain_id.0, id.0)
    }

    fn program_entity_idx_key(entity_id: &str, id: &icn_governance::ProgramId) -> String {
        format!("program_by_entity:{}:{}", entity_id, id.0)
    }

    fn domain_idx_prefix(domain_id: &GovernanceDomainId) -> String {
        format!("program_by_domain:{}:", domain_id.0)
    }

    fn entity_idx_prefix(entity_id: &str) -> String {
        format!("program_by_entity:{}:", entity_id)
    }
}

impl icn_governance::ProgramStoreBackend for SledProgramStore {
    fn save(&self, p: &icn_governance::Program) -> std::result::Result<(), GovernanceError> {
        let primary = Self::program_primary_key(&p.id);

        // Remove stale scope index entries if domain_id or parent_entity_id changed.
        // Propagate read errors rather than silently treating them as "no prior record".
        if let Some(existing) = self.get(&p.id)? {
            if existing.domain_id != p.domain_id {
                let old = Self::program_domain_idx_key(&existing.domain_id, &p.id);
                self.db.remove(old.as_bytes()).map_err(|e| {
                    GovernanceError::Internal(format!("Sled remove (stale domain idx) failed: {e}"))
                })?;
            }
            if existing.parent_entity_id != p.parent_entity_id {
                let old = Self::program_entity_idx_key(&existing.parent_entity_id, &p.id);
                self.db.remove(old.as_bytes()).map_err(|e| {
                    GovernanceError::Internal(format!("Sled remove (stale entity idx) failed: {e}"))
                })?;
            }
        }

        let domain_idx = Self::program_domain_idx_key(&p.domain_id, &p.id);
        let entity_idx = Self::program_entity_idx_key(&p.parent_entity_id, &p.id);
        let value = icn_encoding::encode_versioned(p)
            .map_err(|e| GovernanceError::Internal(format!("Failed to encode program: {e}")))?;
        self.db
            .insert(primary.as_bytes(), value)
            .map_err(|e| GovernanceError::Internal(format!("Sled insert (primary) failed: {e}")))?;
        self.db
            .insert(domain_idx.as_bytes(), b"1".as_ref())
            .map_err(|e| {
                GovernanceError::Internal(format!("Sled insert (domain idx) failed: {e}"))
            })?;
        self.db
            .insert(entity_idx.as_bytes(), b"1".as_ref())
            .map_err(|e| {
                GovernanceError::Internal(format!("Sled insert (entity idx) failed: {e}"))
            })?;
        Ok(())
    }

    fn get(
        &self,
        id: &icn_governance::ProgramId,
    ) -> std::result::Result<Option<icn_governance::Program>, GovernanceError> {
        let key = Self::program_primary_key(id);
        match self
            .db
            .get(key.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
        {
            Some(value) => {
                let p: icn_governance::Program =
                    icn_encoding::decode_versioned(&value).map_err(|e| {
                        GovernanceError::Internal(format!("Failed to decode program: {e}"))
                    })?;
                Ok(Some(p))
            }
            None => Ok(None),
        }
    }

    fn list_by_domain(
        &self,
        domain_id: &GovernanceDomainId,
    ) -> std::result::Result<Vec<icn_governance::Program>, GovernanceError> {
        let prefix = Self::domain_idx_prefix(domain_id);
        // The expected left side when splitting at the final ':'.
        let expected_left = format!("program_by_domain:{}", domain_id.0);
        let mut out = Vec::new();
        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (idx_key, _) =
                result.map_err(|e| GovernanceError::Internal(format!("Sled scan failed: {e}")))?;
            let idx_str = std::str::from_utf8(&idx_key)
                .map_err(|e| GovernanceError::Internal(format!("Invalid UTF-8 key: {e}")))?;
            // Use rsplit_once to isolate the program ID from the domain part.
            // This guards against prefix collisions where domain "coop" would
            // match keys belonging to domain "coop:nycn".
            let Some((left, pid_str)) = idx_str.rsplit_once(':') else {
                continue;
            };
            if left != expected_left {
                continue;
            }
            let pid = icn_governance::ProgramId(pid_str.to_string());
            let primary = Self::program_primary_key(&pid);
            if let Some(v) = self
                .db
                .get(primary.as_bytes())
                .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
            {
                let p: icn_governance::Program =
                    icn_encoding::decode_versioned(&v).map_err(|e| {
                        GovernanceError::Internal(format!("Failed to decode program: {e}"))
                    })?;
                out.push(p);
            }
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> std::result::Result<Vec<icn_governance::Program>, GovernanceError> {
        let prefix = Self::entity_idx_prefix(entity_id);
        let expected_left = format!("program_by_entity:{}", entity_id);
        let mut out = Vec::new();
        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (idx_key, _) =
                result.map_err(|e| GovernanceError::Internal(format!("Sled scan failed: {e}")))?;
            let idx_str = std::str::from_utf8(&idx_key)
                .map_err(|e| GovernanceError::Internal(format!("Invalid UTF-8 key: {e}")))?;
            // Use rsplit_once to guard against prefix collisions (entity "a" vs "a:b").
            let Some((left, pid_str)) = idx_str.rsplit_once(':') else {
                continue;
            };
            if left != expected_left {
                continue;
            }
            let pid = icn_governance::ProgramId(pid_str.to_string());
            let primary = Self::program_primary_key(&pid);
            if let Some(v) = self
                .db
                .get(primary.as_bytes())
                .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
            {
                let p: icn_governance::Program =
                    icn_encoding::decode_versioned(&v).map_err(|e| {
                        GovernanceError::Internal(format!("Failed to decode program: {e}"))
                    })?;
                out.push(p);
            }
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    fn delete(&self, id: &icn_governance::ProgramId) -> std::result::Result<bool, GovernanceError> {
        let primary = Self::program_primary_key(id);
        let Some(value) = self
            .db
            .get(primary.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
        else {
            return Ok(false);
        };
        let p: icn_governance::Program = icn_encoding::decode_versioned(&value)
            .map_err(|e| GovernanceError::Internal(format!("Failed to decode program: {e}")))?;
        let domain_idx = Self::program_domain_idx_key(&p.domain_id, id);
        let entity_idx = Self::program_entity_idx_key(&p.parent_entity_id, id);

        self.db
            .remove(primary.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete (primary) failed: {e}")))?;
        self.db.remove(domain_idx.as_bytes()).map_err(|e| {
            GovernanceError::Internal(format!("Sled delete (domain idx) failed: {e}"))
        })?;
        self.db.remove(entity_idx.as_bytes()).map_err(|e| {
            GovernanceError::Internal(format!("Sled delete (entity idx) failed: {e}"))
        })?;
        Ok(true)
    }
}

// ============================================================================
// Sled-Backed Milestone Store
// ============================================================================

/// Sled-backed storage for milestones.
///
/// Key scheme (dual-key):
/// - Primary:        `milestone:{milestone_id}`                       → postcard-encoded Milestone
/// - Program index:  `milestone_by_program:{program_id}:{milestone_id}` → empty (membership marker)
pub struct SledMilestoneStore {
    db: Arc<sled::Db>,
}

impl SledMilestoneStore {
    pub fn new(db: Arc<sled::Db>) -> Self {
        Self { db }
    }

    fn milestone_primary_key(id: &icn_governance::MilestoneId) -> String {
        format!("milestone:{}", id.0)
    }

    fn milestone_program_idx_key(
        program_id: &icn_governance::ProgramId,
        id: &icn_governance::MilestoneId,
    ) -> String {
        format!("milestone_by_program:{}:{}", program_id.0, id.0)
    }

    fn program_idx_prefix(program_id: &icn_governance::ProgramId) -> String {
        format!("milestone_by_program:{}:", program_id.0)
    }
}

impl icn_governance::MilestoneStoreBackend for SledMilestoneStore {
    fn save(&self, m: &icn_governance::Milestone) -> std::result::Result<(), GovernanceError> {
        let primary = Self::milestone_primary_key(&m.id);

        // Remove stale program index entry if program_id changed on update.
        // Propagate read errors rather than silently treating them as "no prior record".
        if let Some(existing) = self.get(&m.id)? {
            if existing.program_id != m.program_id {
                let old = Self::milestone_program_idx_key(&existing.program_id, &m.id);
                self.db.remove(old.as_bytes()).map_err(|e| {
                    GovernanceError::Internal(format!(
                        "Sled remove (stale program idx) failed: {e}"
                    ))
                })?;
            }
        }

        let idx = Self::milestone_program_idx_key(&m.program_id, &m.id);
        let value = icn_encoding::encode_versioned(m)
            .map_err(|e| GovernanceError::Internal(format!("Failed to encode milestone: {e}")))?;
        self.db
            .insert(primary.as_bytes(), value)
            .map_err(|e| GovernanceError::Internal(format!("Sled insert (primary) failed: {e}")))?;
        self.db
            .insert(idx.as_bytes(), b"1".as_ref())
            .map_err(|e| GovernanceError::Internal(format!("Sled insert (idx) failed: {e}")))?;
        Ok(())
    }

    fn get(
        &self,
        id: &icn_governance::MilestoneId,
    ) -> std::result::Result<Option<icn_governance::Milestone>, GovernanceError> {
        let key = Self::milestone_primary_key(id);
        match self
            .db
            .get(key.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
        {
            Some(value) => {
                let m: icn_governance::Milestone =
                    icn_encoding::decode_versioned(&value).map_err(|e| {
                        GovernanceError::Internal(format!("Failed to decode milestone: {e}"))
                    })?;
                Ok(Some(m))
            }
            None => Ok(None),
        }
    }

    fn list_by_program(
        &self,
        program_id: &icn_governance::ProgramId,
    ) -> std::result::Result<Vec<icn_governance::Milestone>, GovernanceError> {
        let prefix = Self::program_idx_prefix(program_id);
        let expected_left = format!("milestone_by_program:{}", program_id.0);
        let mut out = Vec::new();
        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (idx_key, _) =
                result.map_err(|e| GovernanceError::Internal(format!("Sled scan failed: {e}")))?;
            let idx_str = std::str::from_utf8(&idx_key)
                .map_err(|e| GovernanceError::Internal(format!("Invalid UTF-8 key: {e}")))?;
            // Use rsplit_once to guard against prefix collisions (program "p" vs "p:sub").
            let Some((left, mid_str)) = idx_str.rsplit_once(':') else {
                continue;
            };
            if left != expected_left {
                continue;
            }
            let mid = icn_governance::MilestoneId(mid_str.to_string());
            let primary = Self::milestone_primary_key(&mid);
            if let Some(v) = self
                .db
                .get(primary.as_bytes())
                .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
            {
                let m: icn_governance::Milestone =
                    icn_encoding::decode_versioned(&v).map_err(|e| {
                        GovernanceError::Internal(format!("Failed to decode milestone: {e}"))
                    })?;
                out.push(m);
            }
        }
        out.sort_by_key(|m| m.phase_index);
        Ok(out)
    }

    fn delete(
        &self,
        id: &icn_governance::MilestoneId,
    ) -> std::result::Result<bool, GovernanceError> {
        let primary = Self::milestone_primary_key(id);
        let Some(value) = self
            .db
            .get(primary.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
        else {
            return Ok(false);
        };
        let m: icn_governance::Milestone = icn_encoding::decode_versioned(&value)
            .map_err(|e| GovernanceError::Internal(format!("Failed to decode milestone: {e}")))?;
        let idx = Self::milestone_program_idx_key(&m.program_id, id);

        self.db
            .remove(primary.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete (primary) failed: {e}")))?;
        self.db
            .remove(idx.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete (idx) failed: {e}")))?;
        Ok(true)
    }
}

// ============================================================================
// Sled-Backed Meeting Store
// ============================================================================

/// Sled-backed storage for meetings.
///
/// Key scheme (dual-key):
/// - Primary:       `meeting:{meeting_id}`                     → postcard-encoded Meeting
/// - Domain index:  `meeting_by_domain:{domain_id}:{meeting_id}` → empty (membership marker)
///
/// This matches the `<thing>_by_<scope>:...` secondary-index convention used by
/// `SledStructureStore` and `SledActivityStore`. Lookups by meeting ID are O(1)
/// via the primary key; lookups by domain scan the index prefix. The scheme is
/// unambiguous under domain IDs containing `:` because the meeting ID lives in
/// its own primary key namespace, not embedded as a suffix of the domain key.
pub struct SledMeetingStore {
    db: Arc<sled::Db>,
}

impl SledMeetingStore {
    /// Create a new Sled-backed meeting store.
    pub fn new(db: Arc<sled::Db>) -> Self {
        Self { db }
    }

    fn meeting_key(id: &MeetingId) -> String {
        format!("meeting:{}", id.0)
    }

    fn index_key(domain_id: &str, id: &MeetingId) -> String {
        format!("meeting_by_domain:{}:{}", domain_id, id.0)
    }

    fn domain_index_prefix(domain_id: &str) -> String {
        format!("meeting_by_domain:{}:", domain_id)
    }

    fn activity_index_key(activity_id: &ActivityId, meeting_id: &MeetingId) -> String {
        format!("meeting_by_activity:{}:{}", activity_id.0, meeting_id.0)
    }

    fn activity_index_prefix(activity_id: &ActivityId) -> String {
        format!("meeting_by_activity:{}:", activity_id.0)
    }
}

impl MeetingStoreBackend for SledMeetingStore {
    fn save(&self, m: &Meeting) -> std::result::Result<(), GovernanceError> {
        let primary_key = Self::meeting_key(&m.id);
        let index_key = Self::index_key(&m.domain_id, &m.id);

        // Load the existing record (if any) to diff linked_activities for
        // index maintenance. On first save this returns empty. If an existing
        // record cannot be decoded, surface the decode failure rather than
        // silently treating it as "no prior activities" — that would leave
        // stale activity-index rows pointing at a now-absent meeting.
        let old_activities: Vec<ActivityId> =
            match self.db.get(primary_key.as_bytes()).map_err(|e| {
                GovernanceError::Internal(format!("Sled get (pre-save) failed: {e}"))
            })? {
                Some(v) => {
                    icn_encoding::decode_versioned::<Meeting>(&v)
                        .map_err(|e| {
                            GovernanceError::Internal(format!(
                                "Failed to decode existing meeting for index diff: {e}"
                            ))
                        })?
                        .linked_activities
                }
                None => Vec::new(),
            };

        let value = icn_encoding::encode_versioned(m)
            .map_err(|e| GovernanceError::Internal(format!("Failed to encode meeting: {e}")))?;

        // Best-effort atomic: write primary, then domain index. If either
        // fails the caller sees an error and can retry. sled::Batch would be
        // stronger but is not used elsewhere in this store for this pattern.
        self.db
            .insert(primary_key.as_bytes(), value)
            .map_err(|e| GovernanceError::Internal(format!("Sled insert (primary) failed: {e}")))?;
        self.db
            .insert(index_key.as_bytes(), &[] as &[u8])
            .map_err(|e| GovernanceError::Internal(format!("Sled insert (index) failed: {e}")))?;

        // Reconcile the activity secondary index against the *desired* set
        // (`m.linked_activities`). We take the union of the old record's
        // activities and the new record's activities as the set to touch —
        // this means a retry after a partial-failure previous save still
        // rebuilds any missing index rows for activities that are listed in
        // the new primary record.
        let desired: std::collections::HashSet<&ActivityId> = m.linked_activities.iter().collect();
        let union: std::collections::HashSet<&ActivityId> = old_activities
            .iter()
            .chain(m.linked_activities.iter())
            .collect();
        for act_id in union {
            let k = Self::activity_index_key(act_id, &m.id);
            if desired.contains(act_id) {
                self.db.insert(k.as_bytes(), &[] as &[u8]).map_err(|e| {
                    GovernanceError::Internal(format!("Sled insert (activity index) failed: {e}"))
                })?;
            } else {
                self.db.remove(k.as_bytes()).map_err(|e| {
                    GovernanceError::Internal(format!("Sled remove (activity index) failed: {e}"))
                })?;
            }
        }
        Ok(())
    }

    fn get(&self, id: &MeetingId) -> std::result::Result<Option<Meeting>, GovernanceError> {
        let key = Self::meeting_key(id);
        match self
            .db
            .get(key.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
        {
            Some(value) => {
                let m: Meeting = icn_encoding::decode_versioned(&value).map_err(|e| {
                    GovernanceError::Internal(format!("Failed to decode meeting: {e}"))
                })?;
                Ok(Some(m))
            }
            None => Ok(None),
        }
    }

    fn list_by_domain(
        &self,
        domain_id: &str,
    ) -> std::result::Result<Vec<Meeting>, GovernanceError> {
        let prefix = Self::domain_index_prefix(domain_id);
        // The exact prefix the key's "domain portion" must match — i.e., the
        // prefix minus its trailing ':'. This is necessary because sled's
        // `scan_prefix` matches any byte-prefix, so `meeting_by_domain:coop:`
        // also matches `meeting_by_domain:coop:nycn:{id}` (a different domain).
        // We must reject index rows whose domain is not exactly `domain_id`.
        let expected_domain_prefix = format!("meeting_by_domain:{}", domain_id);
        let mut out = Vec::new();
        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, _) =
                result.map_err(|e| GovernanceError::Internal(format!("Sled scan failed: {e}")))?;
            let key_str = std::str::from_utf8(&key)
                .map_err(|e| GovernanceError::Internal(format!("Invalid UTF-8 key: {e}")))?;
            // Split at the last ':' to recover (domain-portion, meeting-id).
            let Some((domain_portion, meeting_id_str)) = key_str.rsplit_once(':') else {
                continue;
            };
            // Reject keys whose domain portion doesn't exactly match. This
            // filters out keys from domains whose name happens to start with
            // `{domain_id}` (e.g., scanning `coop:` must not pick up `coop:nycn`).
            if domain_portion != expected_domain_prefix {
                continue;
            }
            let primary_key = format!("meeting:{}", meeting_id_str);
            let Some(value) = self
                .db
                .get(primary_key.as_bytes())
                .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
            else {
                // Index entry without primary — treat as dangling and skip.
                continue;
            };
            let m: Meeting = icn_encoding::decode_versioned(&value)
                .map_err(|e| GovernanceError::Internal(format!("Failed to decode meeting: {e}")))?;
            out.push(m);
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    fn delete(&self, id: &MeetingId) -> std::result::Result<bool, GovernanceError> {
        // Load the meeting to discover domain_id + linked_activities so we
        // can clean up both the domain index and all activity index entries.
        let primary_key = Self::meeting_key(id);
        let Some(value) = self
            .db
            .get(primary_key.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
        else {
            return Ok(false);
        };
        let m: Meeting = icn_encoding::decode_versioned(&value)
            .map_err(|e| GovernanceError::Internal(format!("Failed to decode meeting: {e}")))?;
        let index_key = Self::index_key(&m.domain_id, id);

        self.db
            .remove(primary_key.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete (primary) failed: {e}")))?;
        self.db
            .remove(index_key.as_bytes())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete (index) failed: {e}")))?;

        // Clean up all activity index entries for this meeting.
        for act_id in &m.linked_activities {
            let k = Self::activity_index_key(act_id, id);
            self.db.remove(k.as_bytes()).map_err(|e| {
                GovernanceError::Internal(format!(
                    "Sled remove (activity index on delete) failed: {e}"
                ))
            })?;
        }
        Ok(true)
    }

    /// Scan all meeting primary keys and return those whose `scheduled_at`
    /// falls within `[now_secs, now_secs + window_secs]` and whose status is
    /// not `Cancelled` or `Completed`.
    ///
    /// Note: this is an O(N) full scan over the meetings table. For the near
    /// term (few meetings per node), this is acceptable. A dedicated
    /// `meeting_by_scheduled:{bucket}:{id}` index can be added later as a
    /// straight mirror of the `action_item_by_assignee` pattern.
    fn list_upcoming(
        &self,
        now_secs: u64,
        window_secs: u64,
    ) -> std::result::Result<Vec<Meeting>, GovernanceError> {
        let upper = now_secs.saturating_add(window_secs);
        let mut out = Vec::new();
        for result in self.db.scan_prefix(b"meeting:") {
            let (_, value) =
                result.map_err(|e| GovernanceError::Internal(format!("Sled scan failed: {e}")))?;
            let m: Meeting = icn_encoding::decode_versioned(&value)
                .map_err(|e| GovernanceError::Internal(format!("Failed to decode meeting: {e}")))?;
            if matches!(
                m.status,
                icn_governance::MeetingStatus::Cancelled | icn_governance::MeetingStatus::Completed
            ) {
                continue;
            }
            match m.scheduled_at {
                Some(t) if t >= now_secs && t <= upper => out.push(m),
                _ => continue,
            }
        }
        out.sort_by_key(|m| m.scheduled_at.unwrap_or(0));
        Ok(out)
    }

    fn list_by_activity(
        &self,
        activity_id: &ActivityId,
    ) -> std::result::Result<Vec<Meeting>, GovernanceError> {
        let prefix = Self::activity_index_prefix(activity_id);
        // `meeting_by_activity:{activity_id}:` — split on last ':' to get the
        // meeting ID, then load from primary. Activity IDs may contain ':' so
        // we use rsplit_once (same convention as list_by_domain).
        let expected_prefix = format!("meeting_by_activity:{}", activity_id.0);
        let mut out = Vec::new();
        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, _) =
                result.map_err(|e| GovernanceError::Internal(format!("Sled scan failed: {e}")))?;
            let key_str = std::str::from_utf8(&key)
                .map_err(|e| GovernanceError::Internal(format!("Invalid UTF-8 key: {e}")))?;
            let Some((activity_portion, meeting_id_str)) = key_str.rsplit_once(':') else {
                continue;
            };
            // Reject keys whose activity portion doesn't exactly match — guards
            // against activity IDs that are prefixes of other activity IDs.
            if activity_portion != expected_prefix {
                continue;
            }
            let primary_key = format!("meeting:{}", meeting_id_str);
            let Some(value) = self
                .db
                .get(primary_key.as_bytes())
                .map_err(|e| GovernanceError::Internal(format!("Sled get failed: {e}")))?
            else {
                // Dangling index entry — skip.
                continue;
            };
            let m: Meeting = icn_encoding::decode_versioned(&value)
                .map_err(|e| GovernanceError::Internal(format!("Failed to decode meeting: {e}")))?;
            out.push(m);
        }
        // Earliest scheduled first; unscheduled (None) sort last.
        out.sort_by_key(|m| m.scheduled_at.unwrap_or(u64::MAX));
        Ok(out)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod sled_meeting_store_tests {
    use super::*;
    use icn_governance::{Meeting, MeetingId};

    fn open_temp_db() -> (Arc<sled::Db>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = sled::Config::new()
            .path(dir.path())
            .temporary(true)
            .open()
            .expect("sled open");
        (Arc::new(db), dir)
    }

    fn mk_meeting(domain: &str, title: &str) -> Meeting {
        Meeting::new(
            MeetingId::generate(),
            domain.to_string(),
            title.to_string(),
            "did:icn:creator".to_string(),
            1_700_000_000,
        )
    }

    #[test]
    fn save_and_get_roundtrip() {
        let (db, _dir) = open_temp_db();
        let store = SledMeetingStore::new(db);
        let m = mk_meeting("dom-a", "Kickoff");
        store.save(&m).unwrap();

        let got = store.get(&m.id).unwrap().expect("meeting present");
        assert_eq!(got.id, m.id);
        assert_eq!(got.domain_id, "dom-a");
        assert_eq!(got.title, "Kickoff");
    }

    #[test]
    fn get_is_o1_and_does_not_scan() {
        // Regression guard: the previous implementation full-scanned `meeting:`
        // and could return the wrong meeting if two domains carried the same
        // MeetingId. Primary keys are now meeting-id-only, so collisions are
        // impossible within a single store.
        let (db, _dir) = open_temp_db();
        let store = SledMeetingStore::new(db);
        let m1 = mk_meeting("dom-a", "alpha");
        let m2 = mk_meeting("dom-b", "beta");
        store.save(&m1).unwrap();
        store.save(&m2).unwrap();

        let got1 = store.get(&m1.id).unwrap().unwrap();
        let got2 = store.get(&m2.id).unwrap().unwrap();
        assert_eq!(got1.title, "alpha");
        assert_eq!(got2.title, "beta");
    }

    #[test]
    fn list_by_domain_isolates_domains() {
        let (db, _dir) = open_temp_db();
        let store = SledMeetingStore::new(db);
        let m_a1 = mk_meeting("dom-a", "a1");
        let m_a2 = mk_meeting("dom-a", "a2");
        let m_b1 = mk_meeting("dom-b", "b1");
        store.save(&m_a1).unwrap();
        store.save(&m_a2).unwrap();
        store.save(&m_b1).unwrap();

        let a = store.list_by_domain("dom-a").unwrap();
        let b = store.list_by_domain("dom-b").unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 1);
        assert!(a.iter().all(|m| m.domain_id == "dom-a"));
        assert!(b.iter().all(|m| m.domain_id == "dom-b"));
    }

    #[test]
    fn domain_ids_with_colons_do_not_leak_across_domains() {
        // Critical regression test: the previous `meeting:{domain}:{id}` scheme
        // could confuse `meeting:foo:bar:baz` (domain `foo` / id `bar:baz` vs.
        // domain `foo:bar` / id `baz`). The dual-key scheme is immune because
        // domain IDs live only in the index prefix and meeting IDs live only in
        // the primary key.
        let (db, _dir) = open_temp_db();
        let store = SledMeetingStore::new(db);
        let m_colon = mk_meeting("coop:nycn", "colon-domain");
        let m_plain = mk_meeting("coop", "plain-domain");
        store.save(&m_colon).unwrap();
        store.save(&m_plain).unwrap();

        let colon_listing = store.list_by_domain("coop:nycn").unwrap();
        let plain_listing = store.list_by_domain("coop").unwrap();
        assert_eq!(colon_listing.len(), 1);
        assert_eq!(plain_listing.len(), 1);
        assert_eq!(colon_listing[0].title, "colon-domain");
        assert_eq!(plain_listing[0].title, "plain-domain");
    }

    #[test]
    fn delete_removes_primary_and_index() {
        let (db, _dir) = open_temp_db();
        let store = SledMeetingStore::new(db.clone());
        let m = mk_meeting("dom-a", "to-delete");
        store.save(&m).unwrap();

        let removed = store.delete(&m.id).unwrap();
        assert!(removed);

        assert!(store.get(&m.id).unwrap().is_none());
        assert!(store.list_by_domain("dom-a").unwrap().is_empty());

        // Verify both keys physically removed (no dangling index).
        let primary = format!("meeting:{}", m.id.0);
        let index = format!("meeting_by_domain:dom-a:{}", m.id.0);
        assert!(db.get(primary.as_bytes()).unwrap().is_none());
        assert!(db.get(index.as_bytes()).unwrap().is_none());
    }

    #[test]
    fn delete_missing_returns_false() {
        let (db, _dir) = open_temp_db();
        let store = SledMeetingStore::new(db);
        let missing = MeetingId::from_raw("nope");
        let removed = store.delete(&missing).unwrap();
        assert!(!removed);
    }

    #[test]
    fn list_by_domain_sorted_newest_first() {
        let (db, _dir) = open_temp_db();
        let store = SledMeetingStore::new(db);
        let mut m_old = mk_meeting("dom-a", "old");
        m_old.created_at = 1_700_000_000;
        let mut m_new = mk_meeting("dom-a", "new");
        m_new.created_at = 1_700_000_999;
        store.save(&m_old).unwrap();
        store.save(&m_new).unwrap();

        let listing = store.list_by_domain("dom-a").unwrap();
        assert_eq!(listing[0].title, "new");
        assert_eq!(listing[1].title, "old");
    }

    #[test]
    fn list_by_activity_filters_and_sorts() {
        use icn_governance::ActivityId;
        let (db, _dir) = open_temp_db();
        let store = SledMeetingStore::new(db.clone());

        let act_a = ActivityId("act-alpha".to_string());
        let act_b = ActivityId("act-beta".to_string());

        // m1: linked to act_a, scheduled at 2000
        let mut m1 = mk_meeting("dom", "meeting-1");
        m1.scheduled_at = Some(2000);
        m1.linked_activities = vec![act_a.clone()];
        store.save(&m1).unwrap();

        // m2: linked to act_a + act_b, scheduled at 1000 (earlier)
        let mut m2 = mk_meeting("dom", "meeting-2");
        m2.scheduled_at = Some(1000);
        m2.linked_activities = vec![act_a.clone(), act_b.clone()];
        store.save(&m2).unwrap();

        // m3: linked only to act_b, unscheduled
        let mut m3 = mk_meeting("dom", "meeting-3");
        m3.linked_activities = vec![act_b.clone()];
        store.save(&m3).unwrap();

        // m4: no linked activities
        store.save(&mk_meeting("dom", "meeting-4")).unwrap();

        let by_a = store.list_by_activity(&act_a).unwrap();
        assert_eq!(by_a.len(), 2, "act_a has two meetings");
        assert_eq!(
            by_a[0].title, "meeting-2",
            "earlier scheduled_at sorts first"
        );
        assert_eq!(by_a[1].title, "meeting-1");

        let by_b = store.list_by_activity(&act_b).unwrap();
        assert_eq!(by_b.len(), 2, "act_b has two meetings");
        assert_eq!(by_b[0].title, "meeting-2");
        assert_eq!(by_b[1].title, "meeting-3", "unscheduled sorts last");

        assert!(store
            .list_by_activity(&ActivityId("no-such".to_string()))
            .unwrap()
            .is_empty());

        // Verify index keys physically exist in sled
        let idx = format!("meeting_by_activity:act-alpha:{}", m1.id.0);
        assert!(
            db.get(idx.as_bytes()).unwrap().is_some(),
            "activity index entry present"
        );
    }

    #[test]
    fn list_by_activity_activity_id_prefix_isolation() {
        // Regression: "act-a:" must not match "act-alpha:" entries
        use icn_governance::ActivityId;
        let (db, _dir) = open_temp_db();
        let store = SledMeetingStore::new(db);

        let act_a = ActivityId("act-a".to_string());
        let act_alpha = ActivityId("act-alpha".to_string());

        let mut m_a = mk_meeting("dom", "for-act-a");
        m_a.linked_activities = vec![act_a.clone()];
        store.save(&m_a).unwrap();

        let mut m_alpha = mk_meeting("dom", "for-act-alpha");
        m_alpha.linked_activities = vec![act_alpha.clone()];
        store.save(&m_alpha).unwrap();

        let by_a = store.list_by_activity(&act_a).unwrap();
        let by_alpha = store.list_by_activity(&act_alpha).unwrap();
        assert_eq!(by_a.len(), 1, "act-a must not pick up act-alpha entries");
        assert_eq!(by_alpha.len(), 1);
        assert_eq!(by_a[0].title, "for-act-a");
        assert_eq!(by_alpha[0].title, "for-act-alpha");
    }

    #[test]
    fn activity_index_cleaned_on_update() {
        // Removing an activity from linked_activities on re-save must remove
        // the corresponding index entry.
        use icn_governance::ActivityId;
        let (db, _dir) = open_temp_db();
        let store = SledMeetingStore::new(db.clone());

        let act_a = ActivityId("act-a".to_string());
        let act_b = ActivityId("act-b".to_string());

        let mut m = mk_meeting("dom", "relink");
        m.linked_activities = vec![act_a.clone()];
        store.save(&m).unwrap();

        // Confirm act_a index entry is present
        let k_a = format!("meeting_by_activity:act-a:{}", m.id.0);
        assert!(db.get(k_a.as_bytes()).unwrap().is_some());

        // Re-save with act_b only
        m.linked_activities = vec![act_b.clone()];
        store.save(&m).unwrap();

        // act_a index must be removed; act_b must be present
        assert!(
            db.get(k_a.as_bytes()).unwrap().is_none(),
            "stale act-a index entry should be removed"
        );
        let k_b = format!("meeting_by_activity:act-b:{}", m.id.0);
        assert!(
            db.get(k_b.as_bytes()).unwrap().is_some(),
            "act-b index entry added"
        );

        assert!(store.list_by_activity(&act_a).unwrap().is_empty());
        assert_eq!(store.list_by_activity(&act_b).unwrap().len(), 1);
    }

    #[test]
    fn activity_index_cleaned_on_delete() {
        use icn_governance::ActivityId;
        let (db, _dir) = open_temp_db();
        let store = SledMeetingStore::new(db.clone());

        let act = ActivityId("act-x".to_string());
        let mut m = mk_meeting("dom", "will-delete");
        m.linked_activities = vec![act.clone()];
        store.save(&m).unwrap();

        let k = format!("meeting_by_activity:act-x:{}", m.id.0);
        assert!(db.get(k.as_bytes()).unwrap().is_some());

        store.delete(&m.id).unwrap();

        assert!(
            db.get(k.as_bytes()).unwrap().is_none(),
            "activity index entry must be removed on delete"
        );
        assert!(store.list_by_activity(&act).unwrap().is_empty());
    }
}

// ============================================================================
// Sled Program/Milestone store tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod sled_program_store_tests {
    use super::*;
    use icn_governance::{
        Milestone, MilestoneId, MilestoneStoreBackend, Program, ProgramId, ProgramKind,
        ProgramStatus, ProgramStoreBackend,
    };

    fn open_temp_db() -> (Arc<sled::Db>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = sled::Config::new()
            .path(dir.path())
            .temporary(true)
            .open()
            .expect("sled open");
        (Arc::new(db), dir)
    }

    fn mk_program(id: &str, domain: &str, entity: &str) -> Program {
        Program::new(
            ProgramId::from_raw(id),
            GovernanceDomainId::new(domain),
            entity,
            ProgramKind::Cycle,
            format!("Program {id}"),
            1_700_000_000,
        )
    }

    // ---------- Program store ----------

    #[test]
    fn program_save_and_get_roundtrip() {
        let (db, _dir) = open_temp_db();
        let store = SledProgramStore::new(db);
        let p = mk_program("cycle-2026", "dom-a", "ent-a");
        store.save(&p).unwrap();

        let got = store.get(&p.id).unwrap().expect("program present");
        assert_eq!(got.id, p.id);
        assert_eq!(got.domain_id, p.domain_id);
        assert_eq!(got.parent_entity_id, "ent-a");
        assert_eq!(got.status, ProgramStatus::Draft);
    }

    #[test]
    fn program_list_by_domain_isolates_domains() {
        let (db, _dir) = open_temp_db();
        let store = SledProgramStore::new(db);
        store.save(&mk_program("a1", "dom-a", "ent-a")).unwrap();
        store.save(&mk_program("a2", "dom-a", "ent-a")).unwrap();
        store.save(&mk_program("b1", "dom-b", "ent-b")).unwrap();

        let dom_a = store
            .list_by_domain(&GovernanceDomainId::new("dom-a"))
            .unwrap();
        let dom_b = store
            .list_by_domain(&GovernanceDomainId::new("dom-b"))
            .unwrap();
        assert_eq!(dom_a.len(), 2);
        assert_eq!(dom_b.len(), 1);
    }

    #[test]
    fn program_list_by_domain_rejects_prefix_collisions() {
        // Regression guard: scan_prefix("program_by_domain:coop:") must NOT
        // match keys under `program_by_domain:coop:nycn:...` (a different
        // domain whose name starts with "coop").
        let (db, _dir) = open_temp_db();
        let store = SledProgramStore::new(db);
        store.save(&mk_program("p1", "coop", "ent")).unwrap();
        store.save(&mk_program("p2", "coop:nycn", "ent")).unwrap();

        let coop_only = store
            .list_by_domain(&GovernanceDomainId::new("coop"))
            .unwrap();
        let nycn_only = store
            .list_by_domain(&GovernanceDomainId::new("coop:nycn"))
            .unwrap();
        assert_eq!(coop_only.len(), 1);
        assert_eq!(coop_only[0].id.0, "p1");
        assert_eq!(nycn_only.len(), 1);
        assert_eq!(nycn_only[0].id.0, "p2");
    }

    #[test]
    fn program_list_by_entity_isolates_entities() {
        let (db, _dir) = open_temp_db();
        let store = SledProgramStore::new(db);
        store.save(&mk_program("p1", "dom-a", "ent-a")).unwrap();
        store.save(&mk_program("p2", "dom-a", "ent-b")).unwrap();

        let ent_a = store.list_by_entity("ent-a").unwrap();
        let ent_b = store.list_by_entity("ent-b").unwrap();
        assert_eq!(ent_a.len(), 1);
        assert_eq!(ent_b.len(), 1);
        assert_eq!(ent_a[0].parent_entity_id, "ent-a");
        assert_eq!(ent_b[0].parent_entity_id, "ent-b");
    }

    #[test]
    fn program_delete_cleans_both_indexes() {
        let (db, _dir) = open_temp_db();
        let store = SledProgramStore::new(db.clone());
        let p = mk_program("cycle-2026", "dom-a", "ent-a");
        store.save(&p).unwrap();

        assert!(store.delete(&p.id).unwrap());
        assert!(store.get(&p.id).unwrap().is_none());
        assert!(
            store
                .list_by_domain(&GovernanceDomainId::new("dom-a"))
                .unwrap()
                .is_empty(),
            "domain index row must be removed on delete"
        );
        assert!(
            store.list_by_entity("ent-a").unwrap().is_empty(),
            "entity index row must be removed on delete"
        );
        // No primary or index rows left.
        for result in db.iter() {
            let (k, _) = result.unwrap();
            let k_str = std::str::from_utf8(&k).unwrap();
            assert!(
                !k_str.starts_with("program:") && !k_str.starts_with("program_by_"),
                "dangling key after delete: {k_str}"
            );
        }
    }

    #[test]
    fn program_delete_missing_is_idempotent() {
        let (db, _dir) = open_temp_db();
        let store = SledProgramStore::new(db);
        assert!(!store
            .delete(&ProgramId::from_raw("does-not-exist"))
            .unwrap());
    }

    #[test]
    fn program_list_by_domain_sorted_newest_first() {
        let (db, _dir) = open_temp_db();
        let store = SledProgramStore::new(db);
        let mut old = mk_program("old", "dom-a", "ent-a");
        old.created_at = 1_700_000_000;
        let mut new = mk_program("new", "dom-a", "ent-a");
        new.created_at = 1_700_000_999;
        store.save(&old).unwrap();
        store.save(&new).unwrap();

        let listing = store
            .list_by_domain(&GovernanceDomainId::new("dom-a"))
            .unwrap();
        assert_eq!(listing[0].id.0, "new");
        assert_eq!(listing[1].id.0, "old");
    }

    // ---------- Milestone store ----------

    #[test]
    fn milestone_save_and_get_roundtrip() {
        let (db, _dir) = open_temp_db();
        let store = SledMilestoneStore::new(db);
        let prog = ProgramId::from_raw("cycle-2026");
        let m = Milestone::new(
            MilestoneId::from_raw("m-venue"),
            prog,
            "Venue confirmed",
            0,
            1_700_000_000,
        );
        store.save(&m).unwrap();

        let got = store.get(&m.id).unwrap().expect("milestone present");
        assert_eq!(got.name, "Venue confirmed");
        assert_eq!(got.phase_index, 0);
    }

    #[test]
    fn milestone_list_by_program_orders_by_phase_index() {
        let (db, _dir) = open_temp_db();
        let store = SledMilestoneStore::new(db);
        let prog = ProgramId::from_raw("cycle-2026");

        // Save out of phase order.
        store
            .save(&Milestone::new(
                MilestoneId::from_raw("m-launch"),
                prog.clone(),
                "Launch",
                2,
                1_000,
            ))
            .unwrap();
        store
            .save(&Milestone::new(
                MilestoneId::from_raw("m-venue"),
                prog.clone(),
                "Venue",
                0,
                1_000,
            ))
            .unwrap();
        store
            .save(&Milestone::new(
                MilestoneId::from_raw("m-budget"),
                prog.clone(),
                "Budget",
                1,
                1_000,
            ))
            .unwrap();

        let listing = store.list_by_program(&prog).unwrap();
        assert_eq!(listing.len(), 3);
        assert_eq!(listing[0].name, "Venue");
        assert_eq!(listing[1].name, "Budget");
        assert_eq!(listing[2].name, "Launch");
    }

    #[test]
    fn milestone_list_by_program_isolates_programs() {
        let (db, _dir) = open_temp_db();
        let store = SledMilestoneStore::new(db);
        let prog_a = ProgramId::from_raw("cycle-2026");
        let prog_b = ProgramId::from_raw("campaign-q3");

        store
            .save(&Milestone::new(
                MilestoneId::from_raw("m-a"),
                prog_a.clone(),
                "A",
                0,
                1_000,
            ))
            .unwrap();
        store
            .save(&Milestone::new(
                MilestoneId::from_raw("m-b"),
                prog_b.clone(),
                "B",
                0,
                1_000,
            ))
            .unwrap();

        let a = store.list_by_program(&prog_a).unwrap();
        let b = store.list_by_program(&prog_b).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert_eq!(a[0].name, "A");
        assert_eq!(b[0].name, "B");
    }

    #[test]
    fn milestone_list_by_program_rejects_prefix_collisions() {
        // Same regression guard as program_list_by_domain: scanning
        // `milestone_by_program:cycle:` must not match keys from
        // `cycle-2026` (a different program whose id starts with "cycle").
        let (db, _dir) = open_temp_db();
        let store = SledMilestoneStore::new(db);
        let short = ProgramId::from_raw("cycle");
        let long = ProgramId::from_raw("cycle-2026");

        store
            .save(&Milestone::new(
                MilestoneId::from_raw("m-short"),
                short.clone(),
                "short-side",
                0,
                1_000,
            ))
            .unwrap();
        store
            .save(&Milestone::new(
                MilestoneId::from_raw("m-long"),
                long.clone(),
                "long-side",
                0,
                1_000,
            ))
            .unwrap();

        let short_only = store.list_by_program(&short).unwrap();
        assert_eq!(short_only.len(), 1);
        assert_eq!(short_only[0].name, "short-side");
        let long_only = store.list_by_program(&long).unwrap();
        assert_eq!(long_only.len(), 1);
        assert_eq!(long_only[0].name, "long-side");
    }

    #[test]
    fn milestone_delete_cleans_index() {
        let (db, _dir) = open_temp_db();
        let store = SledMilestoneStore::new(db.clone());
        let prog = ProgramId::from_raw("cycle-2026");
        let m = Milestone::new(
            MilestoneId::from_raw("m-venue"),
            prog.clone(),
            "Venue",
            0,
            1_000,
        );
        store.save(&m).unwrap();

        assert!(store.delete(&m.id).unwrap());
        assert!(store.get(&m.id).unwrap().is_none());
        assert!(
            store.list_by_program(&prog).unwrap().is_empty(),
            "index row must be removed on delete"
        );
        for result in db.iter() {
            let (k, _) = result.unwrap();
            let k_str = std::str::from_utf8(&k).unwrap();
            assert!(
                !k_str.starts_with("milestone:") && !k_str.starts_with("milestone_by_"),
                "dangling key after delete: {k_str}"
            );
        }
    }

    #[test]
    fn milestone_delete_missing_is_idempotent() {
        let (db, _dir) = open_temp_db();
        let store = SledMilestoneStore::new(db);
        assert!(!store.delete(&MilestoneId::from_raw("nope")).unwrap());
    }

    /// Regression: SledProgramStore::save leaked stale domain/entity index rows
    /// when a program was updated with different domain_id or parent_entity_id.
    #[test]
    fn program_stale_scope_index_cleaned_on_update() {
        let (db, _dir) = open_temp_db();
        let store = SledProgramStore::new(db);

        let domain_a = GovernanceDomainId::new("dom-alpha");
        let domain_b = GovernanceDomainId::new("dom-beta");
        let entity_a = "entity-a";
        let entity_b = "entity-b";

        let mut p = mk_program("prog-1", "dom-alpha", entity_a);
        store.save(&p).unwrap();
        assert_eq!(store.list_by_domain(&domain_a).unwrap().len(), 1);
        assert_eq!(store.list_by_entity(entity_a).unwrap().len(), 1);

        // Move to a different domain and entity
        p.domain_id = domain_b.clone();
        p.parent_entity_id = entity_b.to_string();
        store.save(&p).unwrap();

        assert_eq!(
            store.list_by_domain(&domain_a).unwrap().len(),
            0,
            "old domain index row must be cleaned on update"
        );
        assert_eq!(store.list_by_domain(&domain_b).unwrap().len(), 1);
        assert_eq!(
            store.list_by_entity(entity_a).unwrap().len(),
            0,
            "old entity index row must be cleaned on update"
        );
        assert_eq!(store.list_by_entity(entity_b).unwrap().len(), 1);
    }

    /// Regression: SledMilestoneStore::save leaked stale program index rows
    /// when a milestone's program_id changed on update.
    #[test]
    fn milestone_stale_program_index_cleaned_on_update() {
        let (db, _dir) = open_temp_db();
        let store = SledMilestoneStore::new(db);

        let prog_a = ProgramId::from_raw("prog-a");
        let prog_b = ProgramId::from_raw("prog-b");

        let mut m = Milestone::new(
            MilestoneId::from_raw("m-shared"),
            prog_a.clone(),
            "Milestone",
            0,
            1_000,
        );
        store.save(&m).unwrap();
        assert_eq!(store.list_by_program(&prog_a).unwrap().len(), 1);
        assert_eq!(store.list_by_program(&prog_b).unwrap().len(), 0);

        // Move milestone to a different program
        m.program_id = prog_b.clone();
        store.save(&m).unwrap();

        assert_eq!(
            store.list_by_program(&prog_a).unwrap().len(),
            0,
            "old program index row must be cleaned on update"
        );
        assert_eq!(store.list_by_program(&prog_b).unwrap().len(), 1);
    }
}

/// Handle type for actor-backed governance
///
/// This uses the `GovernanceOps` trait to avoid direct dependency on `icn-core`.
/// Any type implementing `GovernanceOps` can be used (e.g., icn-core's GovernanceHandle).
pub type GovernanceHandle = Arc<dyn GovernanceOps + Send + Sync>;

/// Governance manager for gateway API
///
/// Supports two modes:
/// - **Standalone mode** (`new()`): In-memory storage, for testing only
/// - **Actor-backed mode** (`with_handle()`): Delegates to daemon's GovernanceActor
///
/// Note: In actor-backed mode, the in-memory fields (domains, proposals, votes,
/// delegations) are initialized but unused - all operations delegate to the
/// daemon's GovernanceActor. They exist for standalone testing fallback.
///
/// Action items are always managed locally (not gossiped) and can use either
/// in-memory or Sled-backed persistent storage.
pub struct GovernanceManager {
    /// In-memory storage for domains (standalone mode only)
    domains: RwLock<HashMap<GovernanceDomainId, GovernanceDomain>>,
    /// In-memory storage for proposals (standalone mode only)
    proposals: RwLock<HashMap<ProposalId, Proposal>>,
    /// In-memory storage for votes (standalone mode only)
    votes: RwLock<HashMap<ProposalId, Vec<Vote>>>,
    /// In-memory storage for delegations (standalone mode only)
    delegations: RwLock<HashMap<DelegationId, Delegation>>,
    /// In-memory storage for discussions (standalone mode only)
    discussions: RwLock<InMemoryDiscussionStore>,
    /// Action item storage backend (in-memory or Sled-backed)
    action_items: Box<dyn ActionItemStoreBackend>,
    /// Structure store backend (committees, working groups, etc.)
    structure_store: Arc<dyn StructureStoreBackend>,
    /// Activity store backend (events, programs, projects)
    activity_store: Arc<dyn ActivityStoreBackend>,
    /// Meeting store backend (deliberation trace objects)
    meeting_store: Arc<dyn MeetingStoreBackend>,
    /// Program store backend (multi-phase institutional endeavors)
    program_store: Arc<dyn ProgramStoreBackend>,
    /// Milestone store backend (stage-gates within programs)
    milestone_store: Arc<dyn MilestoneStoreBackend>,
    /// Optional handle to daemon's GovernanceActor (actor-backed mode)
    governance_handle: Option<GovernanceHandle>,
    /// Optional receipt store for persisting GovernanceDecisionReceipts
    receipt_store: Option<Arc<dyn GovernanceReceiptBackend>>,
}

impl GovernanceManager {
    /// Create a new governance manager with in-memory storage
    ///
    /// **Warning**: This mode is for testing only. State is lost on restart
    /// and not synchronized via gossip.
    pub fn new() -> Self {
        debug!("GovernanceManager created in standalone mode (in-memory only)");
        GovernanceManager {
            domains: RwLock::new(HashMap::new()),
            proposals: RwLock::new(HashMap::new()),
            votes: RwLock::new(HashMap::new()),
            delegations: RwLock::new(HashMap::new()),
            discussions: RwLock::new(InMemoryDiscussionStore::new()),
            action_items: Box::new(InMemoryActionItemStore::new()),
            structure_store: Arc::new(InMemoryStructureStore::new()),
            activity_store: Arc::new(InMemoryActivityStore::new()),
            meeting_store: Arc::new(InMemoryMeetingStore::new()),
            program_store: Arc::new(InMemoryProgramStore::new()),
            milestone_store: Arc::new(InMemoryMilestoneStore::new()),
            governance_handle: None,
            receipt_store: None,
        }
    }

    /// Create a governance manager backed by the daemon's GovernanceActor
    ///
    /// This is the recommended mode for production. All operations delegate
    /// to the daemon's GovernanceActor, ensuring:
    /// - Persistence across restarts
    /// - Gossip synchronization
    /// - Single source of truth
    ///
    /// Note: The in-memory HashMaps are initialized but never used in this mode.
    /// They exist only for API consistency with standalone mode.
    ///
    /// Action items use in-memory storage by default. Call `set_action_item_store`
    /// to use persistent storage.
    pub fn with_handle(handle: GovernanceHandle) -> Self {
        debug!("GovernanceManager created with daemon GovernanceActor handle");
        GovernanceManager {
            // These fields are unused in actor-backed mode - all operations
            // delegate to the GovernanceActor via governance_handle
            domains: RwLock::new(HashMap::new()),
            proposals: RwLock::new(HashMap::new()),
            votes: RwLock::new(HashMap::new()),
            delegations: RwLock::new(HashMap::new()),
            discussions: RwLock::new(InMemoryDiscussionStore::new()),
            // Action items are always stored in gateway (not synced via gossip)
            // Use set_action_item_store() to configure persistent storage
            action_items: Box::new(InMemoryActionItemStore::new()),
            structure_store: Arc::new(InMemoryStructureStore::new()),
            activity_store: Arc::new(InMemoryActivityStore::new()),
            meeting_store: Arc::new(InMemoryMeetingStore::new()),
            program_store: Arc::new(InMemoryProgramStore::new()),
            milestone_store: Arc::new(InMemoryMilestoneStore::new()),
            governance_handle: Some(handle),
            receipt_store: None,
        }
    }

    /// Set a custom action item store backend
    ///
    /// Use this to configure Sled-backed persistent storage for action items.
    /// Must be called before any action items are created.
    pub fn set_action_item_store(&mut self, store: Box<dyn ActionItemStoreBackend>) {
        self.action_items = store;
    }

    /// Attach a receipt store for persisting GovernanceDecisionReceipts.
    ///
    /// When set, `close_proposal()` in standalone mode will automatically
    /// generate and store a `GovernanceDecisionReceipt` after closing.
    pub fn with_receipt_store(mut self, store: Arc<dyn GovernanceReceiptBackend>) -> Self {
        self.receipt_store = Some(store);
        self
    }

    /// Replace the structure store backend.
    ///
    /// Use this to configure Sled-backed persistent storage for structures.
    pub fn with_structure_store(mut self, store: Arc<dyn StructureStoreBackend>) -> Self {
        self.structure_store = store;
        self
    }

    /// Replace the activity store backend.
    ///
    /// Use this to configure Sled-backed persistent storage for activities.
    pub fn with_activity_store(mut self, store: Arc<dyn ActivityStoreBackend>) -> Self {
        self.activity_store = store;
        self
    }

    /// Replace the meeting store backend.
    ///
    /// Use this to configure Sled-backed persistent storage for meetings.
    pub fn with_meeting_store(mut self, store: Arc<dyn MeetingStoreBackend>) -> Self {
        self.meeting_store = store;
        self
    }

    /// Replace the program store backend.
    ///
    /// Use this to configure Sled-backed persistent storage for programs.
    pub fn with_program_store(mut self, store: Arc<dyn ProgramStoreBackend>) -> Self {
        self.program_store = store;
        self
    }

    /// Replace the milestone store backend.
    ///
    /// Use this to configure Sled-backed persistent storage for milestones.
    pub fn with_milestone_store(mut self, store: Arc<dyn MilestoneStoreBackend>) -> Self {
        self.milestone_store = store;
        self
    }

    /// Create a governance manager with Sled-backed action item storage
    ///
    /// This is the recommended mode for production with persistent action items.
    pub fn with_sled_action_items(handle: GovernanceHandle, db: Arc<sled::Db>) -> Self {
        debug!("GovernanceManager created with daemon handle + Sled action item store");
        GovernanceManager {
            domains: RwLock::new(HashMap::new()),
            proposals: RwLock::new(HashMap::new()),
            votes: RwLock::new(HashMap::new()),
            delegations: RwLock::new(HashMap::new()),
            discussions: RwLock::new(InMemoryDiscussionStore::new()),
            action_items: Box::new(SledActionItemStore::new(db.clone())),
            structure_store: Arc::new(InMemoryStructureStore::new()),
            activity_store: Arc::new(InMemoryActivityStore::new()),
            meeting_store: Arc::new(InMemoryMeetingStore::new()),
            // Programs and milestones use Sled-backed stores so they persist
            // across restarts. Structure/activity/meeting are overridden by
            // the caller (server.rs) via the with_*_store() builder methods;
            // program/milestone use the same db here to avoid needing further
            // builder calls for the happy path.
            program_store: Arc::new(SledProgramStore::new(db.clone())),
            milestone_store: Arc::new(SledMilestoneStore::new(db)),
            governance_handle: Some(handle),
            receipt_store: None,
        }
    }

    /// Create a standalone governance manager with Sled-backed action item storage
    ///
    /// Useful for testing persistence without a daemon connection.
    pub fn new_with_sled(db: Arc<sled::Db>) -> Self {
        debug!("GovernanceManager created in standalone mode with Sled action item store");
        GovernanceManager {
            domains: RwLock::new(HashMap::new()),
            proposals: RwLock::new(HashMap::new()),
            votes: RwLock::new(HashMap::new()),
            delegations: RwLock::new(HashMap::new()),
            discussions: RwLock::new(InMemoryDiscussionStore::new()),
            action_items: Box::new(SledActionItemStore::new(db.clone())),
            structure_store: Arc::new(InMemoryStructureStore::new()),
            activity_store: Arc::new(InMemoryActivityStore::new()),
            meeting_store: Arc::new(InMemoryMeetingStore::new()),
            // Programs and milestones use Sled-backed stores so they persist
            // across restarts (same db instance, separate key namespaces).
            program_store: Arc::new(SledProgramStore::new(db.clone())),
            milestone_store: Arc::new(SledMilestoneStore::new(db)),
            governance_handle: None,
            receipt_store: None,
        }
    }

    /// Check if running in actor-backed mode
    pub fn is_actor_backed(&self) -> bool {
        self.governance_handle.is_some()
    }

    /// Create a new governance domain
    pub async fn create_domain(
        &self,
        domain_id: GovernanceDomainId,
        name: String,
        profile: String,
        params: GovernanceParams,
        membership: MembershipConfig,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            // Actor-backed mode: delegate to GovernanceActor
            return handle
                .create_domain(domain_id, name, profile, params, membership)
                .await;
        }

        // Standalone mode: in-memory storage
        let profile_id = if profile.starts_with("contract:") {
            let did = profile.strip_prefix("contract:").unwrap_or(&profile);
            GovernanceProfileId::contract(did)
        } else {
            GovernanceProfileId::builtin(&profile)
        };

        let config = GovernanceConfig::new(profile_id, membership, params);
        // Use with_id so the domain's own `id` field matches the caller-supplied
        // domain_id rather than generating a new UUID (which would make the map
        // key and the domain's id field diverge).
        let domain = GovernanceDomain::with_id(domain_id.clone(), name, config);

        let mut domains = self.domains.write().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;

        if domains.contains_key(&domain_id) {
            anyhow::bail!(
                "Domain '{}' already exists. Use a unique domain ID or update the existing domain.",
                domain_id.0
            );
        }

        domains.insert(domain_id, domain);
        Ok(())
    }

    /// Add or remove a member from a governance domain
    pub async fn update_domain_membership(
        &self,
        domain_id: GovernanceDomainId,
        member: icn_identity::Did,
        action: icn_governance::MembershipAction,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            return handle
                .update_domain_membership(domain_id, member, action)
                .await;
        }

        let mut domains = self.domains.write().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;

        let domain = domains
            .get_mut(&domain_id)
            .ok_or_else(|| anyhow::anyhow!("Domain '{}' not found", domain_id.0))?;

        match &mut domain.config.membership.source {
            MembershipSource::StaticList(members) => match action {
                icn_governance::MembershipAction::Add => {
                    if !members.contains(&member) {
                        members.push(member);
                    }
                }
                icn_governance::MembershipAction::Remove => {
                    if let Some(pos) = members.iter().position(|m| m == &member) {
                        members.remove(pos);
                    }
                }
            },
            MembershipSource::TrustThreshold(_) => {
                anyhow::bail!(
                    "Cannot modify members of trust-based membership domain '{}'. \
                     Convert to static membership first.",
                    domain_id.0
                );
            }
        }

        domain.updated_at = icn_time::current_timestamp_secs();
        Ok(())
    }

    /// Get a governance domain
    pub async fn get_domain(
        &self,
        domain_id: &GovernanceDomainId,
    ) -> Result<Option<GovernanceDomain>> {
        if let Some(ref handle) = self.governance_handle {
            return handle.get_domain(domain_id).await;
        }

        let domains = self.domains.read().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;
        Ok(domains.get(domain_id).cloned())
    }

    /// List all governance domains
    pub async fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        if let Some(ref handle) = self.governance_handle {
            return handle.list_domains().await;
        }

        let domains = self.domains.read().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;
        Ok(domains.values().cloned().collect())
    }

    /// List governance domains with pagination
    pub async fn list_domains_paginated(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<PaginatedResult<GovernanceDomain>> {
        if let Some(ref handle) = self.governance_handle {
            return handle.list_domains_paginated(cursor, limit).await;
        }

        let domains = self.domains.read().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;

        let offset: usize = cursor
            .and_then(|c| c.strip_prefix("offset:"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let mut keys: Vec<_> = domains.keys().cloned().collect();
        keys.sort_by(|a, b| a.0.cmp(&b.0));

        let total = keys.len();
        let items: Vec<GovernanceDomain> = keys
            .into_iter()
            .skip(offset)
            .take(limit)
            .filter_map(|k| domains.get(&k).cloned())
            .collect();

        let next_offset = offset + items.len();
        let next_cursor = if next_offset < total {
            Some(format!("offset:{next_offset}"))
        } else {
            None
        };

        Ok(PaginatedResult::new(items, next_cursor).with_total(total))
    }

    /// Create a new proposal
    pub async fn create_proposal(
        &self,
        proposal_id: ProposalId,
        domain_id: GovernanceDomainId,
        proposer: Did,
        title: String,
        description: String,
        payload: ProposalPayload,
        scope: ProposalScope,
    ) -> Result<ProposalId> {
        self.create_proposal_with_actions(
            proposal_id,
            domain_id,
            proposer,
            title,
            description,
            payload,
            scope,
            Vec::new(),
        )
        .await
    }

    /// Create a proposal with action item specs that will be materialized on acceptance.
    pub async fn create_proposal_with_actions(
        &self,
        proposal_id: ProposalId,
        domain_id: GovernanceDomainId,
        proposer: Did,
        title: String,
        description: String,
        payload: ProposalPayload,
        scope: ProposalScope,
        action_items_on_accept: Vec<icn_governance::ActionItemSpec>,
    ) -> Result<ProposalId> {
        if let Some(ref handle) = self.governance_handle {
            let generated_id = handle
                .create_proposal_with_actions(
                    domain_id,
                    title,
                    description,
                    payload,
                    scope,
                    action_items_on_accept,
                )
                .await?;
            return Ok(generated_id);
        }

        let domains = self.domains.read().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;
        if !domains.contains_key(&domain_id) {
            anyhow::bail!(
                "Domain '{}' not found. Create the domain first using create_domain().",
                domain_id.0
            );
        }
        drop(domains);

        let mut proposal = Proposal::new(domain_id, proposer, title, description, payload)
            .with_scope(scope)
            .with_action_items(action_items_on_accept);
        proposal.id = proposal_id.clone();

        let mut proposals = self.proposals.write().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;

        if proposals.contains_key(&proposal_id) {
            anyhow::bail!(
                "Proposal '{}' already exists. Use a unique proposal ID.",
                proposal_id.0
            );
        }

        proposals.insert(proposal_id.clone(), proposal);
        Ok(proposal_id)
    }

    /// Convenience wrapper for the `POST /federation/clearing/{id}/propose-adoption` flow.
    ///
    /// Constructs a `ProposalPayload::Federation(FederationProposal::EstablishClearing)`
    /// from the source agreement's fields, keeping icn_governance type construction
    /// inside the app layer (meaning firewall boundary).
    ///
    /// Returns the created proposal ID as a plain `String`.
    pub async fn propose_clearing_adoption(
        &self,
        domain_id_str: &str,
        proposer: Did,
        title: String,
        description: String,
        agreement: &BilateralClearingAgreement,
        source_agreement_id: String,
    ) -> Result<String> {
        let currency = agreement
            .exchange_rates
            .keys()
            .min()
            .and_then(|k| k.split(':').next())
            .unwrap_or("HOURS")
            .to_ascii_uppercase();

        let payload =
            ProposalPayload::Federation(icn_governance::FederationProposal::EstablishClearing {
                partner_coop_id: agreement.coop_b.clone(),
                partner_coop_did: agreement.coop_b_did.clone(),
                max_imbalance: agreement.max_imbalance,
                settlement_interval: agreement.settlement_interval,
                currency,
                source_agreement_id: Some(source_agreement_id.clone()),
            });

        let proposal_id_str = format!("prop-adoption-{}", uuid::Uuid::new_v4());
        let proposed_id = ProposalId(proposal_id_str);
        let domain_id = GovernanceDomainId(domain_id_str.to_string());

        let created_id = self
            .create_proposal(
                proposed_id,
                domain_id,
                proposer,
                title,
                description,
                payload,
                ProposalScope::Local,
            )
            .await?;
        Ok(created_id.0)
    }

    /// Convenience wrapper for `create_domain` that accepts primitive types.
    ///
    /// Uses `GovernanceParams::default()` and a static-list membership sourced from
    /// the provided DIDs. Intended for tests and simple integrations that do not
    /// need fine-grained governance parameter control.
    pub async fn create_domain_simple(
        &self,
        domain_id_str: &str,
        name: &str,
        members: Vec<Did>,
    ) -> Result<()> {
        self.create_domain(
            GovernanceDomainId(domain_id_str.to_string()),
            name.to_string(),
            "default".to_string(),
            GovernanceParams::default(),
            MembershipConfig {
                source: MembershipSource::StaticList(members),
            },
        )
        .await
    }

    /// Get a specific proposal
    pub async fn get_proposal(&self, proposal_id: &ProposalId) -> Result<Option<Proposal>> {
        if let Some(ref handle) = self.governance_handle {
            return handle.get_proposal(proposal_id).await;
        }

        let proposals = self.proposals.read().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;
        Ok(proposals.get(proposal_id).cloned())
    }

    /// Insert a proposal with any state (test helper)
    #[allow(clippy::expect_used)]
    pub fn insert_test_proposal(&self, proposal: Proposal) {
        if self.governance_handle.is_some() {
            panic!("insert_test_proposal can only be used in standalone mode");
        }

        let mut proposals = self
            .proposals
            .write()
            .expect("Proposals lock poisoned in test");
        proposals.insert(proposal.id.clone(), proposal);
    }

    /// List all proposals
    pub async fn list_proposals(&self) -> Result<Vec<Proposal>> {
        if let Some(ref handle) = self.governance_handle {
            return handle.list_proposals().await;
        }

        let proposals = self.proposals.read().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;
        Ok(proposals.values().cloned().collect())
    }

    /// Open a proposal for voting
    pub async fn open_proposal(
        &self,
        proposal_id: ProposalId,
        voting_period_seconds: u64,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            return handle
                .open_proposal(proposal_id, voting_period_seconds)
                .await;
        }

        let mut proposals = self.proposals.write().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;

        if let Some(proposal) = proposals.get_mut(&proposal_id) {
            proposal.open(voting_period_seconds)?;
            Ok(())
        } else {
            anyhow::bail!(
                "Proposal '{}' not found. Create the proposal first using create_proposal().",
                proposal_id.0
            )
        }
    }

    /// Close a proposal and finalize voting
    pub async fn close_proposal(&self, proposal_id: ProposalId) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            return handle.close_proposal(proposal_id).await;
        }
        self.close_proposal_inner(proposal_id, None)
    }

    /// Close a proposal counting only votes from currently-eligible members.
    ///
    /// Called by the HTTP handler after revalidating voter commons standing at
    /// close time. Votes from members who lost standing (Suspended/Candidate)
    /// after casting are excluded from the effective tally. This ensures
    /// institutional legitimacy holds across the full proposal lifecycle —
    /// standing must be valid at resolution, not just at vote-cast time.
    ///
    /// Only valid when the in-memory store is active (`governance_handle` must
    /// be `None`). The handle-backed path computes its own tally internally.
    pub async fn close_proposal_filtered(
        &self,
        proposal_id: ProposalId,
        eligible_voters: &HashSet<Did>,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            // Delegate to the handle, which propagates the eligibility filter through
            // GovernanceCommand::CloseProposal { eligible_voters: Some(...) } so the
            // actor applies it before tallying. This is the production path.
            return handle
                .close_proposal_filtered(proposal_id, eligible_voters)
                .await;
        }
        self.close_proposal_inner(proposal_id, Some(eligible_voters))
    }

    /// Close a proposal with both standing revalidation and suspension-based
    /// delegation exclusion. Passes both filters through to the actor when the
    /// actor path is active. In the in-memory path, suspension exclusion is
    /// a no-op (no delegation expansion occurs in-memory).
    pub async fn close_proposal_with_suspension(
        &self,
        proposal_id: ProposalId,
        eligible_voters: Option<HashSet<Did>>,
        excluded_delegators: Option<HashSet<Did>>,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            return handle
                .close_proposal_with_suspension(proposal_id, eligible_voters, excluded_delegators)
                .await;
        }
        // In-memory path: no delegation expansion, so suspension exclusion is a no-op.
        // Just apply the eligible_voters filter if present.
        match eligible_voters.as_ref() {
            Some(eligible) => self.close_proposal_inner(proposal_id, Some(eligible)),
            None => self.close_proposal_inner(proposal_id, None),
        }
    }

    fn close_proposal_inner(
        &self,
        proposal_id: ProposalId,
        eligible_voters: Option<&HashSet<Did>>,
    ) -> Result<()> {
        let mut proposals = self.proposals.write().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;
        let votes = self
            .votes
            .read()
            .map_err(|e| anyhow::anyhow!("Votes storage lock poisoned (concurrent panic?): {e}"))?;
        let domains = self.domains.read().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;

        if let Some(proposal) = proposals.get_mut(&proposal_id) {
            if !proposal.state.is_open() {
                anyhow::bail!(
                    "Proposal '{}' cannot be closed: not open for voting (current state: {:?}). \
                     Only proposals in 'Open' state can be closed.",
                    proposal_id.0,
                    proposal.state
                );
            }

            let domain = domains.get(&proposal.domain_id).ok_or_else(|| {
                anyhow::anyhow!(
                    "Domain '{}' not found for proposal '{}'. Domain may have been deleted.",
                    proposal.domain_id.0,
                    proposal_id.0
                )
            })?;

            let raw_votes = votes.get(&proposal_id).cloned().unwrap_or_default();
            // If an eligibility filter was provided (close-time standing revalidation),
            // exclude votes from members who no longer hold active commons standing.
            let proposal_votes: Vec<_> = match eligible_voters {
                Some(eligible) => raw_votes
                    .into_iter()
                    .filter(|v| eligible.contains(&v.voter))
                    .collect(),
                None => raw_votes,
            };
            let tally = VoteTally::from(proposal_votes.clone());

            let total_members = match &domain.config.membership.source {
                MembershipSource::StaticList(members) => members.len(),
                MembershipSource::TrustThreshold(_) => tally.total_votes().max(1),
            };

            let quorum_percentage = if total_members > 0 {
                let total_votes = tally.total_votes();
                let percentage = total_votes
                    .checked_mul(100)
                    .and_then(|v| v.checked_div(total_members));

                match percentage {
                    Some(p) => p.min(100) as u8,
                    None => {
                        tracing::error!(
                            proposal_id = %proposal_id.0,
                            total_votes = total_votes,
                            total_members = total_members,
                            "Integer overflow in quorum calculation"
                        );
                        anyhow::bail!(
                            "Integer overflow calculating quorum for proposal '{}': \
                             {} votes * 100 overflowed. This indicates corrupted vote data.",
                            proposal_id.0,
                            total_votes
                        );
                    }
                }
            } else {
                0
            };

            let now = icn_time::current_timestamp_secs();

            let final_state = if quorum_percentage < domain.config.params.quorum_percentage {
                ProposalState::NoQuorum { closed_at: now }
            } else if tally.approval_percentage()
                >= domain.config.params.approval_threshold_percentage
            {
                ProposalState::Accepted { closed_at: now }
            } else {
                ProposalState::Rejected { closed_at: now }
            };

            let outcome = match &final_state {
                ProposalState::Accepted { .. } => ProofOutcome::Accepted,
                ProposalState::Rejected { .. } => ProofOutcome::Rejected,
                ProposalState::NoQuorum { .. } => ProofOutcome::NoQuorum,
                _ => unreachable!("final_state is always Accepted/Rejected/NoQuorum"),
            };

            proposal.close(final_state)?;

            if let Some(ref store) = self.receipt_store {
                let receipt = GovernanceDecisionReceipt::new(
                    proposal_id.0.clone(),
                    proposal.domain_id.0.clone(),
                    outcome,
                    tally,
                    &proposal_votes,
                );
                if let Err(e) = store.put_governance(&receipt) {
                    tracing::error!(
                        proposal_id = %proposal_id.0,
                        error = %e,
                        "Failed to store governance decision receipt — provenance chain broken"
                    );
                    // Escalated from warn to error: receipt store failure means
                    // the governance→economics provenance chain is broken (PS-3).
                }

                // Wire governance→economics binding (INV-2: Allocation Completeness)
                // When a budget/treasury/allocation proposal is accepted, create an
                // AllocationReceipt linking the decision to economic intents.
                if matches!(outcome, ProofOutcome::Accepted) {
                    let decision_hash = receipt.decision_hash;
                    if let Some(allocation_receipt) = self.create_allocation_receipt(
                        &proposal.payload,
                        decision_hash,
                        &proposal_id,
                        &proposal.domain_id,
                    ) {
                        if let Err(e) = store.put_allocation(&allocation_receipt) {
                            tracing::error!(
                                proposal_id = %proposal_id.0,
                                error = %e,
                                "Failed to store allocation receipt — economics binding broken"
                            );
                        } else {
                            tracing::info!(
                                proposal_id = %proposal_id.0,
                                intent_count = allocation_receipt.intents.len(),
                                "Allocation receipt created: governance→economics chain bound"
                            );
                        }
                    }
                }
            }

            // Decision→action bridge (standalone path).
            //
            // When the proposal is Accepted and carries `action_items_on_accept`
            // specs, materialize them into concrete ActionItems now. This mirrors
            // what the GovernanceActor does in the actor-backed path (see
            // `actor.rs::materialize_action_items`). Without this, the
            // in-memory/standalone path (used in tests and HTTP-only deployments)
            // would silently skip obligation creation.
            //
            // Lock scope note: this block runs while the proposals write lock is
            // still held (the lock is taken earlier in close_proposal_inner and
            // held for the whole function). The action_items store I/O here adds
            // to the lock duration. For the pilot's single-gateway deployment this
            // is acceptable; a future refactor can clone the needed fields and drop
            // the proposals lock before the I/O if contention becomes a concern.
            //
            // Dedup guard: if items already exist for this proposal (e.g. because
            // the actor path ran first on a re-close), skip to avoid duplicates.
            if matches!(outcome, ProofOutcome::Accepted)
                && !proposal.action_items_on_accept.is_empty()
            {
                let filter = icn_governance::ActionItemFilter {
                    linked_proposal: Some(proposal_id.clone()),
                    ..Default::default()
                };
                let already_exists = match self.action_items.list(&proposal.domain_id, &filter) {
                    Ok(existing) => !existing.is_empty(),
                    Err(e) => {
                        // List failure means we cannot confirm absence of duplicates.
                        // Log a warning and proceed — risk is a duplicate item, not
                        // a missing one. A duplicate is recoverable; a missing
                        // obligation is silent data loss.
                        tracing::warn!(
                            proposal_id = %proposal_id.0,
                            domain_id = %proposal.domain_id.0,
                            error = %e,
                            "Failed to check for existing action items before materialization; \
                             proceeding (risk: duplicate items)"
                        );
                        false
                    }
                };

                if !already_exists {
                    let domain_id = proposal.domain_id.clone();
                    let proposer = proposal.proposer.clone();
                    for spec in &proposal.action_items_on_accept {
                        let item = spec.materialize(
                            domain_id.clone(),
                            proposal_id.clone(),
                            proposer.clone(),
                            now,
                        );
                        if let Err(e) = self.action_items.save(&item) {
                            tracing::warn!(
                                proposal_id = %proposal_id.0,
                                action_item_title = %spec.title,
                                error = %e,
                                "Failed to materialize action item from accepted proposal — \
                                 obligation may be lost"
                            );
                        }
                    }
                }
            }

            Ok(())
        } else {
            anyhow::bail!(
                "Proposal '{}' not found. It may not exist or was already deleted.",
                proposal_id.0
            )
        }
    }

    /// Get the full provenance chain for a proposal (INV-5: Chain Walkability).
    ///
    /// Returns:
    /// - `governance_receipt`: The decision receipt for the proposal (if closed)
    /// - `allocations`: AllocationReceipts linked to this decision (if any)
    /// - `chain_complete`: true if governance receipt AND at least one allocation exist for economic proposals
    ///
    /// This endpoint makes the governance→economics link independently verifiable
    /// without SSH access to the node.
    pub async fn get_chain(&self, proposal_id: &ProposalId) -> Result<ProvenanceChain> {
        let governance_receipt = if let Some(ref store) = self.receipt_store {
            match store.get_governance_by_proposal(&proposal_id.0) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(proposal_id = %proposal_id.0, error = %e, "Receipt store error in get_chain");
                    None
                }
            }
        } else {
            None
        };

        let allocations = if let Some(ref receipt) = governance_receipt {
            if let Some(ref store) = self.receipt_store {
                let decision_hash = receipt.decision_hash;
                match store.list_allocations_by_decision(&decision_hash) {
                    Ok(a) => a,
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to query allocations in get_chain");
                        vec![]
                    }
                }
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        let has_governance = governance_receipt.is_some();
        let has_allocations = !allocations.is_empty();

        // chain_complete requires correctly determining if this proposal type
        // was expected to produce allocation receipts.
        //
        // Logic:
        // - No governance receipt: always incomplete.
        // - Rejected/NoQuorum: complete with just the governance receipt (no allocations expected).
        // - Accepted + economic payload: complete iff allocations were stored.
        // - Accepted + non-economic payload: complete (no allocations needed).
        // - Accepted + unknown proposal (actor mode, no local state): conservative false.
        let chain_complete = if let Some(ref receipt) = governance_receipt {
            match receipt.outcome {
                icn_governance::ProofOutcome::Rejected | icn_governance::ProofOutcome::NoQuorum => {
                    true
                }
                icn_governance::ProofOutcome::Accepted => {
                    // Look up the proposal to determine if it requires allocations.
                    match self.get_proposal(proposal_id).await.ok().flatten() {
                        Some(p) => {
                            let is_economic_payload = matches!(
                                p.payload,
                                ProposalPayload::Budget { .. }
                                    | ProposalPayload::Treasury { .. }
                                    | ProposalPayload::Allocation { .. }
                                    | ProposalPayload::SurplusAllocation { .. }
                            );
                            if is_economic_payload {
                                has_allocations
                            } else {
                                true // non-economic proposals complete without allocations
                            }
                        }
                        // Proposal not found (actor-backed mode, no local state):
                        // fall back to presence of allocations as the completeness signal.
                        None => has_governance && has_allocations,
                    }
                }
            }
        } else {
            false
        };

        Ok(ProvenanceChain {
            governance_receipt,
            allocations,
            chain_complete,
        })
    }

    /// Create an AllocationReceipt from an accepted proposal's payload.
    ///
    /// Returns `None` for proposal types that don't produce economic effects
    /// (e.g., Text, Membership, ConfigChange).
    ///
    /// This is the governance→economics binding point (INV-2).
    fn create_allocation_receipt(
        &self,
        payload: &ProposalPayload,
        decision_hash: icn_kernel_api::Hash,
        proposal_id: &ProposalId,
        domain_id: &GovernanceDomainId,
    ) -> Option<AllocationReceipt> {
        let now = icn_time::current_timestamp_secs();

        match payload {
            ProposalPayload::Budget {
                amount,
                currency,
                recipient,
                purpose,
            } => {
                let intent = SettlementIntent::new(
                    &proposal_id.0,
                    decision_hash,
                    &domain_id.0,          // from: domain treasury
                    recipient.to_string(), // to: recipient
                    *amount as u64,
                    currency,
                )
                .with_memo(purpose.clone())
                .with_timestamp(now);

                let receipt =
                    AllocationReceipt::new(decision_hash, ScopeLevel::Org).add_intent(intent);
                // Set timestamp
                let mut receipt = receipt;
                receipt.created_at = now;
                Some(receipt)
            }

            ProposalPayload::Treasury { operation } => {
                // Treasury operations that produce economic effects
                match operation {
                    icn_governance::TreasuryProposalOperation::CreateBudget {
                        treasury_did,
                        amount,
                        currency,
                        purpose,
                        ..
                    } => {
                        let intent = SettlementIntent::new(
                            &proposal_id.0,
                            decision_hash,
                            treasury_did.to_string(),
                            format!("budget:{}", proposal_id.0),
                            *amount as u64,
                            currency,
                        )
                        .with_memo(purpose.clone())
                        .with_timestamp(now);

                        let mut receipt = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
                            .add_intent(intent);
                        receipt.created_at = now;
                        Some(receipt)
                    }
                    icn_governance::TreasuryProposalOperation::Spend {
                        treasury_did,
                        recipient,
                        amount,
                        currency,
                        memo,
                        ..
                    } => {
                        let intent = SettlementIntent::new(
                            &proposal_id.0,
                            decision_hash,
                            treasury_did.to_string(),
                            recipient.to_string(),
                            *amount as u64,
                            currency,
                        )
                        .with_memo(memo.clone())
                        .with_timestamp(now);

                        let mut receipt = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
                            .add_intent(intent);
                        receipt.created_at = now;
                        Some(receipt)
                    }
                    _ => {
                        tracing::debug!(
                            proposal_id = %proposal_id.0,
                            "Treasury operation does not produce allocation receipt"
                        );
                        None
                    }
                }
            }

            ProposalPayload::Allocation {
                pool_amount: _,
                unit,
                options,
                purpose: _,
            } => {
                // Participatory budget: create one intent per option
                let intents: Vec<SettlementIntent> = options
                    .iter()
                    .map(|opt| {
                        SettlementIntent::new(
                            &proposal_id.0,
                            decision_hash,
                            &domain_id.0,
                            opt.recipient.to_string(),
                            opt.requested_amount as u64,
                            unit,
                        )
                        .with_memo(opt.label.clone())
                        .with_timestamp(now)
                    })
                    .collect();

                let mut receipt =
                    AllocationReceipt::new(decision_hash, ScopeLevel::Org).with_intents(intents);
                receipt.created_at = now;
                Some(receipt)
            }

            ProposalPayload::SurplusAllocation { .. } => {
                // Surplus allocation produces economic effects
                // For now, create a placeholder receipt — the actual distribution
                // is handled by the ledger's patronage module
                let mut receipt = AllocationReceipt::new(decision_hash, ScopeLevel::Org);
                receipt.created_at = now;
                Some(receipt)
            }

            // Non-economic proposal types
            ProposalPayload::Text { .. }
            | ProposalPayload::Membership { .. }
            | ProposalPayload::ConfigChange { .. }
            | ProposalPayload::SchedulingPolicy { .. }
            | ProposalPayload::FreezeMember { .. }
            | ProposalPayload::UnfreezeMember { .. }
            | ProposalPayload::VetoProposal { .. }
            | ProposalPayload::ForceCloseProposal { .. }
            | ProposalPayload::DisputeResolution { .. }
            | ProposalPayload::Sdis { .. }
            | ProposalPayload::ProtocolUpgrade { .. }
            | ProposalPayload::ProtocolChange { .. }
            | ProposalPayload::ResourceAccess { .. }
            | ProposalPayload::Charter { .. }
            | ProposalPayload::RollbackLedger { .. }
            | ProposalPayload::ShareRedemption { .. }
            | ProposalPayload::BondIssuance { .. }
            | ProposalPayload::Federation(_) => None,
        }
    }

    /// Get vote tally for a proposal
    pub async fn get_vote_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally> {
        if let Some(ref handle) = self.governance_handle {
            return handle.get_vote_tally(proposal_id).await;
        }

        let votes = self
            .votes
            .read()
            .map_err(|e| anyhow::anyhow!("Votes storage lock poisoned (concurrent panic?): {e}"))?;
        let proposal_votes = votes.get(proposal_id).cloned().unwrap_or_default();
        Ok(VoteTally::from(proposal_votes))
    }

    /// Get list of voter DIDs for a proposal
    pub async fn get_voter_dids(&self, proposal_id: &ProposalId) -> Result<Vec<Did>> {
        if let Some(ref handle) = self.governance_handle {
            return handle.get_voter_dids(proposal_id).await;
        }

        let votes = self
            .votes
            .read()
            .map_err(|e| anyhow::anyhow!("Votes storage lock poisoned (concurrent panic?): {e}"))?;
        let voter_dids = votes
            .get(proposal_id)
            .map(|votes| votes.iter().map(|v| v.voter.clone()).collect())
            .unwrap_or_default();
        Ok(voter_dids)
    }

    /// Get the GovernanceProofV2 for a closed proposal
    pub async fn get_proof(
        &self,
        proposal_id: &ProposalId,
    ) -> Result<Option<icn_governance::GovernanceProofV2>> {
        if let Some(ref handle) = self.governance_handle {
            return handle.get_proof(proposal_id).await;
        }
        if let Some(ref store) = self.receipt_store {
            match store.get_governance_by_proposal(&proposal_id.0) {
                Ok(Some(receipt)) => {
                    return Ok(Some(icn_governance::GovernanceProofV2::new(
                        receipt,
                        Vec::new(),
                    )));
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        proposal_id = %proposal_id.0,
                        error = %e,
                        "Failed to query receipt store for proof"
                    );
                }
            }
        }
        Ok(None)
    }

    /// Cast a vote on a proposal
    pub async fn cast_vote(
        &self,
        proposal_id: ProposalId,
        voter: Did,
        choice: VoteChoice,
        comment: Option<String>,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            return handle
                .cast_vote(proposal_id, voter.clone(), choice, comment)
                .await;
        }

        let proposals = self.proposals.read().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;
        let proposal = proposals.get(&proposal_id).ok_or_else(|| {
            anyhow::anyhow!(
                "Proposal '{}' not found. Cannot cast vote on non-existent proposal.",
                proposal_id.0
            )
        })?;

        if !proposal.state.is_open() {
            anyhow::bail!(
                "Cannot vote on proposal '{}': not open for voting (current state: {:?}). \
                 Proposal may have been closed or not yet opened.",
                proposal_id.0,
                proposal.state
            );
        }

        let domains = self.domains.read().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;
        let domain = domains.get(&proposal.domain_id).ok_or_else(|| {
            anyhow::anyhow!(
                "Domain '{}' not found. Cannot verify voter membership.",
                proposal.domain_id.0
            )
        })?;

        let is_member = match &domain.config.membership.source {
            MembershipSource::StaticList(members) => members.contains(&voter),
            MembershipSource::TrustThreshold(_) => true,
        };

        if !is_member {
            anyhow::bail!(
                "Voter {} is not a member of domain {}",
                voter,
                proposal.domain_id.0
            );
        }

        drop(domains);
        drop(proposals);

        let mut votes = self
            .votes
            .write()
            .map_err(|e| anyhow::anyhow!("Votes storage lock poisoned (concurrent panic?): {e}"))?;

        // TOCTOU check: re-verify proposal is still open
        let proposals_recheck = self.proposals.read().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;
        let proposal_recheck = proposals_recheck.get(&proposal_id).ok_or_else(|| {
            anyhow::anyhow!(
                "Proposal '{}' was deleted during vote submission.",
                proposal_id.0
            )
        })?;

        if !proposal_recheck.state.is_open() {
            anyhow::bail!(
                "Proposal was closed during vote submission (current state: {:?})",
                proposal_recheck.state
            );
        }
        drop(proposals_recheck);

        let proposal_votes = votes.entry(proposal_id.clone()).or_insert_with(Vec::new);

        if proposal_votes.iter().any(|v| v.voter == voter) {
            anyhow::bail!(
                "Voter {} has already voted on proposal {}",
                voter,
                proposal_id.0
            );
        }

        let mut vote = Vote::new(proposal_id, voter, choice);
        if let Some(c) = comment {
            vote = vote.with_comment(c);
        }
        proposal_votes.push(vote);

        Ok(())
    }

    // ============================================================================
    // Delegation Methods
    // ============================================================================

    /// Create a new vote delegation
    pub async fn create_delegation(&self, delegation: Delegation) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            return handle.create_delegation(delegation).await;
        }

        if delegation.delegator == delegation.delegate {
            anyhow::bail!(
                "Cannot delegate to yourself. Delegator and delegate must be different DIDs."
            );
        }

        let mut delegations = self.delegations.write().map_err(|e| {
            anyhow::anyhow!("Delegations storage lock poisoned (concurrent panic?): {e}")
        })?;

        let proposals = self.proposals.read().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;

        if delegations.contains_key(&delegation.id) {
            anyhow::bail!(
                "Delegation '{}' already exists. Use a unique delegation ID.",
                delegation.id.0
            );
        }

        let now = icn_time::current_timestamp_secs();
        let has_existing = delegations.values().any(|d| {
            d.delegator == delegation.delegator && d.scope == delegation.scope && d.is_active(now)
        });
        if has_existing {
            anyhow::bail!(
                "Active delegation already exists for scope {:?}. Revoke the existing delegation first.",
                delegation.scope
            );
        }

        let cycle_path = find_delegation_cycle(
            &delegation.delegator,
            &delegation.delegate,
            &delegation.scope,
            &delegations,
            &proposals,
            now,
        );
        if let Some(path) = cycle_path {
            let path_str = path
                .iter()
                .map(|did| did.to_string())
                .collect::<Vec<_>>()
                .join(" → ");
            anyhow::bail!(
                "Delegation would create a cycle: {path_str}. Remove an existing delegation to break the cycle.",
            );
        }

        let incoming_depth = compute_incoming_depth(
            &delegation.delegator,
            &delegation.scope,
            &delegations,
            &proposals,
            now,
        );
        let max_depth = DEFAULT_MAX_DELEGATION_DEPTH;
        if incoming_depth >= max_depth {
            anyhow::bail!(
                "Maximum delegation depth ({max_depth}) exceeded. The delegation chain is too long.",
            );
        }

        drop(proposals);
        delegations.insert(delegation.id.clone(), delegation);
        Ok(())
    }

    /// Get a delegation by ID
    pub async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        if let Some(ref handle) = self.governance_handle {
            return handle.get_delegation(id).await;
        }

        let delegations = self.delegations.read().map_err(|e| {
            anyhow::anyhow!("Delegations storage lock poisoned (concurrent panic?): {e}")
        })?;
        Ok(delegations.get(id).cloned())
    }

    /// Get all delegations given by a specific DID
    pub async fn get_delegations_from(&self, delegator: &Did) -> Result<Vec<Delegation>> {
        if let Some(ref handle) = self.governance_handle {
            return handle.get_delegations_from(delegator).await;
        }

        let delegations = self.delegations.read().map_err(|e| {
            anyhow::anyhow!("Delegations storage lock poisoned (concurrent panic?): {e}")
        })?;
        Ok(delegations
            .values()
            .filter(|d| d.delegator == *delegator)
            .cloned()
            .collect())
    }

    /// Get all delegations received by a specific DID
    pub async fn get_delegations_to(&self, delegate: &Did) -> Result<Vec<Delegation>> {
        if let Some(ref handle) = self.governance_handle {
            return handle.get_delegations_to(delegate).await;
        }

        let delegations = self.delegations.read().map_err(|e| {
            anyhow::anyhow!("Delegations storage lock poisoned (concurrent panic?): {e}")
        })?;
        Ok(delegations
            .values()
            .filter(|d| d.delegate == *delegate)
            .cloned()
            .collect())
    }

    /// Revoke a delegation
    pub async fn revoke_delegation(&self, id: &DelegationId, revoked_at: Timestamp) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            return handle.revoke_delegation(id, revoked_at).await;
        }

        let mut delegations = self.delegations.write().map_err(|e| {
            anyhow::anyhow!("Delegations storage lock poisoned (concurrent panic?): {e}")
        })?;

        if let Some(delegation) = delegations.get_mut(id) {
            delegation.revoked_at = Some(revoked_at);
            Ok(())
        } else {
            anyhow::bail!(
                "Delegation '{}' not found. It may not exist or was already deleted.",
                id.0
            )
        }
    }

    // ============================================================================
    // Deliberation Methods
    // ============================================================================

    /// Start deliberation period for a proposal
    pub async fn start_deliberation(
        &self,
        proposal_id: &ProposalId,
        deliberation_period_seconds: u64,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            return handle
                .start_deliberation(proposal_id.clone(), deliberation_period_seconds)
                .await;
        }

        let mut proposals = self.proposals.write().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;

        let proposal = proposals
            .get_mut(proposal_id)
            .ok_or_else(|| anyhow::anyhow!("Proposal '{}' not found", proposal_id.0))?;

        proposal.start_deliberation(deliberation_period_seconds)?;
        Ok(())
    }

    /// End deliberation and open for voting
    pub async fn end_deliberation_and_open(
        &self,
        proposal_id: &ProposalId,
        voting_period_seconds: u64,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            return handle
                .end_deliberation_and_open(proposal_id.clone(), voting_period_seconds)
                .await;
        }

        let mut proposals = self.proposals.write().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;

        let proposal = proposals
            .get_mut(proposal_id)
            .ok_or_else(|| anyhow::anyhow!("Proposal '{}' not found", proposal_id.0))?;

        proposal.end_deliberation_and_open(voting_period_seconds)?;
        Ok(())
    }

    // ============================================================================
    // Discussion Methods
    // ============================================================================

    /// Add a comment to a proposal's discussion
    pub async fn add_comment(&self, comment: Comment) -> Result<CommentId> {
        {
            let proposals = self
                .proposals
                .read()
                .map_err(|e| anyhow::anyhow!("Proposals storage lock poisoned: {e}"))?;

            let proposal = proposals
                .get(&comment.proposal_id)
                .ok_or_else(|| anyhow::anyhow!("Proposal '{}' not found", comment.proposal_id.0))?;

            if !proposal.state.allows_comments() {
                anyhow::bail!(
                    "Cannot add comment: proposal '{}' is not open for discussion (state: {:?})",
                    comment.proposal_id.0,
                    proposal.state
                );
            }
        }

        let mut discussions = self
            .discussions
            .write()
            .map_err(|e| anyhow::anyhow!("Discussions storage lock poisoned: {e}"))?;

        discussions
            .add_comment(comment)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Get a comment by ID
    pub async fn get_comment(&self, comment_id: &CommentId) -> Result<Option<Comment>> {
        let discussions = self
            .discussions
            .read()
            .map_err(|e| anyhow::anyhow!("Discussions storage lock poisoned: {e}"))?;
        Ok(discussions.get_comment(comment_id).cloned())
    }

    /// Edit a comment
    pub async fn edit_comment(
        &self,
        comment_id: &CommentId,
        new_content: String,
        editor: &Did,
    ) -> Result<()> {
        let mut discussions = self
            .discussions
            .write()
            .map_err(|e| anyhow::anyhow!("Discussions storage lock poisoned: {e}"))?;
        discussions
            .edit_comment(comment_id, new_content, editor)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Delete a comment (soft delete)
    pub async fn delete_comment(&self, comment_id: &CommentId, deleter: &Did) -> Result<()> {
        let mut discussions = self
            .discussions
            .write()
            .map_err(|e| anyhow::anyhow!("Discussions storage lock poisoned: {e}"))?;
        discussions
            .delete_comment(comment_id, deleter)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Add a reaction to a comment
    pub async fn add_reaction(
        &self,
        comment_id: &CommentId,
        reactor: &Did,
        emoji: &str,
    ) -> Result<()> {
        let mut discussions = self
            .discussions
            .write()
            .map_err(|e| anyhow::anyhow!("Discussions storage lock poisoned: {e}"))?;
        discussions
            .add_reaction(comment_id, reactor, emoji)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Remove a reaction from a comment
    pub async fn remove_reaction(
        &self,
        comment_id: &CommentId,
        reactor: &Did,
        emoji: &str,
    ) -> Result<()> {
        let mut discussions = self
            .discussions
            .write()
            .map_err(|e| anyhow::anyhow!("Discussions storage lock poisoned: {e}"))?;
        discussions
            .remove_reaction(comment_id, reactor, emoji)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Get the full discussion for a proposal
    pub async fn get_discussion(&self, proposal_id: &ProposalId) -> Result<Option<Discussion>> {
        let discussions = self
            .discussions
            .read()
            .map_err(|e| anyhow::anyhow!("Discussions storage lock poisoned: {e}"))?;
        Ok(discussions.get_discussion(proposal_id))
    }

    /// List comments for a proposal with pagination
    pub async fn list_comments(
        &self,
        proposal_id: &ProposalId,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Comment>> {
        let discussions = self
            .discussions
            .read()
            .map_err(|e| anyhow::anyhow!("Discussions storage lock poisoned: {e}"))?;
        Ok(discussions
            .list_comments(proposal_id, limit, offset)
            .into_iter()
            .cloned()
            .collect())
    }

    /// Get comment count for a proposal
    pub async fn count_comments(&self, proposal_id: &ProposalId) -> Result<usize> {
        let discussions = self
            .discussions
            .read()
            .map_err(|e| anyhow::anyhow!("Discussions storage lock poisoned: {e}"))?;
        Ok(discussions.count_comments(proposal_id))
    }

    /// Get participants in a proposal's discussion
    pub async fn get_discussion_participants(&self, proposal_id: &ProposalId) -> Result<Vec<Did>> {
        let discussions = self
            .discussions
            .read()
            .map_err(|e| anyhow::anyhow!("Discussions storage lock poisoned: {e}"))?;
        Ok(discussions.get_participants(proposal_id))
    }

    // ========================================================================
    // Action Item Management
    // ========================================================================

    /// Create a new action item
    pub fn create_action_item(
        &self,
        domain_id: GovernanceDomainId,
        title: String,
        description: Option<String>,
        created_by: Did,
        assignee: Option<Did>,
        due_date: Option<u64>,
        priority: ActionItemPriority,
        linked_proposal: Option<ProposalId>,
        meeting_context: Option<String>,
        tags: Vec<String>,
    ) -> Result<ActionItem> {
        let now = icn_time::current_timestamp_secs();
        let mut item = ActionItem::new(domain_id, title, created_by, now);
        item.description = description;
        item.assignee = assignee;
        item.due_date = due_date;
        item.priority = priority;
        item.linked_proposal = linked_proposal;
        item.meeting_context = meeting_context;
        item.tags = tags;

        self.action_items
            .save(&item)
            .map_err(|e| anyhow::anyhow!("Failed to save action item: {e}"))?;

        Ok(item)
    }

    /// Get an action item by ID
    pub fn get_action_item(
        &self,
        domain_id: &GovernanceDomainId,
        id: &ActionItemId,
    ) -> Result<Option<ActionItem>> {
        self.action_items
            .get(domain_id, id)
            .map_err(|e| anyhow::anyhow!("Failed to get action item: {e}"))
    }

    /// List action items with optional filtering
    pub fn list_action_items(
        &self,
        domain_id: &GovernanceDomainId,
        filter: &ActionItemFilter,
    ) -> Result<Vec<ActionItem>> {
        self.action_items
            .list(domain_id, filter)
            .map_err(|e| anyhow::anyhow!("Failed to list action items: {e}"))
    }

    /// Update an action item
    pub fn update_action_item(&self, item: &ActionItem) -> Result<()> {
        self.action_items
            .save(item)
            .map_err(|e| anyhow::anyhow!("Failed to update action item: {e}"))
    }

    /// Delete an action item
    pub fn delete_action_item(
        &self,
        domain_id: &GovernanceDomainId,
        id: &ActionItemId,
    ) -> Result<bool> {
        self.action_items
            .delete(domain_id, id)
            .map_err(|e| anyhow::anyhow!("Failed to delete action item: {e}"))
    }

    /// Count action items matching a filter
    pub fn count_action_items(
        &self,
        domain_id: &GovernanceDomainId,
        filter: &ActionItemFilter,
    ) -> Result<usize> {
        self.action_items
            .count(domain_id, filter)
            .map_err(|e| anyhow::anyhow!("Failed to count action items: {e}"))
    }

    /// Add a note to an action item
    pub fn add_action_item_note(
        &self,
        domain_id: &GovernanceDomainId,
        id: &ActionItemId,
        author: Did,
        content: String,
    ) -> Result<ActionItem> {
        let mut item = self
            .action_items
            .get(domain_id, id)
            .map_err(|e| anyhow::anyhow!("Failed to get action item: {e}"))?
            .ok_or_else(|| anyhow::anyhow!("Action item not found: {id}"))?;

        let now = icn_time::current_timestamp_secs();
        item.add_note(author, content, now);

        self.action_items
            .save(&item)
            .map_err(|e| anyhow::anyhow!("Failed to save action item: {e}"))?;

        Ok(item)
    }

    /// Update the status of an action item
    pub fn update_action_item_status(
        &self,
        domain_id: &GovernanceDomainId,
        id: &ActionItemId,
        status: ActionItemStatus,
    ) -> Result<ActionItem> {
        let mut item = self
            .action_items
            .get(domain_id, id)
            .map_err(|e| anyhow::anyhow!("Failed to get action item: {e}"))?
            .ok_or_else(|| anyhow::anyhow!("Action item not found: {id}"))?;

        item.status = status;
        item.updated_at = icn_time::current_timestamp_secs();

        self.action_items
            .save(&item)
            .map_err(|e| anyhow::anyhow!("Failed to save action item: {e}"))?;

        Ok(item)
    }

    // ========================================================================
    // Notification digest
    // ========================================================================

    /// Digest lookahead window for upcoming meetings: 48 h.
    const DIGEST_UPCOMING_WINDOW_SECS: u64 = 48 * 60 * 60;

    /// Generate a DID-scoped notification digest.
    ///
    /// Returns a summary of pending votes (Open proposals not yet voted on by
    /// `did`), overdue action items (assigned to `did`, past due, not done),
    /// and upcoming meetings (scheduled in the next 48 h where `did` is on
    /// the attendee list).
    pub async fn generate_digest(&self, did: &Did, now_secs: u64) -> DigestSummary {
        let pending_votes = self.digest_pending_votes(did).await;
        let overdue_items = self.digest_overdue_items(did, now_secs);
        let upcoming_meetings = self.digest_upcoming_meetings(did, now_secs);

        DigestSummary {
            did: did.to_string(),
            pending_vote_count: pending_votes.len(),
            pending_votes,
            overdue_item_count: overdue_items.len(),
            overdue_items,
            upcoming_meeting_count: upcoming_meetings.len(),
            upcoming_meetings,
        }
    }

    /// Collect meetings scheduled in the next `DIGEST_UPCOMING_WINDOW_SECS`
    /// where `did` appears in the attendee list.
    ///
    /// Linked-structure / linked-activity membership expansion is out of
    /// scope for the digest PR — that join lives with `/me/scopes`/`/me/work`
    /// in Tranche 1. For now, only explicit attendance counts.
    fn digest_upcoming_meetings(&self, did: &Did, now_secs: u64) -> Vec<UpcomingMeetingDigest> {
        let meetings = match self
            .meeting_store
            .list_upcoming(now_secs, Self::DIGEST_UPCOMING_WINDOW_SECS)
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "digest: failed to list upcoming meetings");
                return vec![];
            }
        };

        let did_str = did.as_str();
        meetings
            .into_iter()
            .filter(|m| m.attendees.iter().any(|a| a.did == did_str))
            .filter_map(|m| {
                m.scheduled_at.map(|scheduled_at| UpcomingMeetingDigest {
                    meeting_id: m.id.0.clone(),
                    domain_id: m.domain_id.clone(),
                    title: m.title.clone(),
                    scheduled_at,
                })
            })
            .collect()
    }

    /// Collect Open proposals the DID has not yet voted on.
    async fn digest_pending_votes(&self, did: &Did) -> Vec<PendingVoteDigest> {
        let proposals = match self.list_proposals().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "digest: failed to list proposals");
                return vec![];
            }
        };

        let mut result = Vec::new();
        for proposal in proposals {
            if !matches!(proposal.state, ProposalState::Open { .. }) {
                continue;
            }
            let voter_dids = match self.get_voter_dids(&proposal.id).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        proposal_id = %proposal.id.0,
                        error = %e,
                        "digest: failed to get voter dids"
                    );
                    continue;
                }
            };
            if voter_dids.contains(did) {
                continue; // already voted
            }
            let closes_at = match &proposal.state {
                ProposalState::Open { closes_at, .. } => Some(*closes_at),
                _ => None,
            };
            result.push(PendingVoteDigest {
                proposal_id: proposal.id.0.clone(),
                domain_id: proposal.domain_id.0.clone(),
                title: proposal.title.clone(),
                closes_at,
            });
        }
        result
    }

    /// Collect action items assigned to the DID that are overdue.
    fn digest_overdue_items(&self, did: &Did, now_secs: u64) -> Vec<OverdueItemDigest> {
        let items = match self.action_items.list_by_assignee(did) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "digest: failed to list items by assignee");
                return vec![];
            }
        };

        items
            .into_iter()
            .filter(|item| item.is_overdue(now_secs))
            .filter_map(|item| {
                item.due_date.map(|due| OverdueItemDigest {
                    item_id: item.id.to_string(),
                    domain_id: item.domain_id.0.clone(),
                    title: item.title.clone(),
                    due_date: due,
                })
            })
            .collect()
    }

    // ========================================================================
    // Structure management (committees, working groups, teams)
    // ========================================================================

    /// Create a new internal structure owned by an entity.
    pub fn create_structure(
        &self,
        parent_entity_id: String,
        kind: StructureKind,
        name: String,
        mandate: Option<String>,
    ) -> Result<Structure> {
        let now = icn_time::current_timestamp_secs();
        let id = StructureId::generate();
        let mut s = Structure::new(id, parent_entity_id, kind, name, now);
        s.mandate = mandate;
        self.structure_store
            .save_structure(&s)
            .map_err(|e| anyhow::anyhow!("Failed to save structure: {e}"))?;
        Ok(s)
    }

    /// Get a structure by ID.
    pub fn get_structure(&self, id: &StructureId) -> Result<Option<Structure>> {
        self.structure_store
            .get_structure(id)
            .map_err(|e| anyhow::anyhow!("Failed to get structure: {e}"))
    }

    /// List all structures owned by an entity.
    pub fn list_structures(&self, entity_id: &str) -> Result<Vec<Structure>> {
        self.structure_store
            .list_structures_by_entity(entity_id)
            .map_err(|e| anyhow::anyhow!("Failed to list structures: {e}"))
    }

    /// Assign a role in a structure.
    pub fn assign_role(
        &self,
        structure_id: StructureId,
        person_did: icn_identity::Did,
        role: String,
    ) -> Result<RoleAssignment> {
        // Validate the structure exists before persisting the role
        let exists = self
            .structure_store
            .get_structure(&structure_id)
            .map_err(|e| anyhow::anyhow!("Failed to look up structure: {e}"))?;
        if exists.is_none() {
            return Err(anyhow::anyhow!("Structure {} not found", structure_id));
        }
        let now = icn_time::current_timestamp_secs();
        let assignment = RoleAssignment::new(structure_id, person_did, role, now);
        self.structure_store
            .save_role(&assignment)
            .map_err(|e| anyhow::anyhow!("Failed to save role assignment: {e}"))?;
        Ok(assignment)
    }

    /// List role assignments for a structure.
    pub fn list_roles(&self, structure_id: &StructureId) -> Result<Vec<RoleAssignment>> {
        self.structure_store
            .list_roles_by_structure(structure_id)
            .map_err(|e| anyhow::anyhow!("Failed to list roles: {e}"))
    }

    /// List all role assignments held by the given DID across all structures.
    ///
    /// Used by `GET /gov/me/scopes` to return the caller's authority scope without
    /// requiring them to know which structures they belong to.
    pub fn list_roles_for_person(&self, did: &icn_identity::Did) -> Result<Vec<RoleAssignment>> {
        self.structure_store
            .list_roles_by_person(did)
            .map_err(|e| anyhow::anyhow!("Failed to list roles for person: {e}"))
    }

    /// List action items assigned to the given DID across all domains, with optional filtering.
    ///
    /// Used by `GET /gov/me/work`. The filter is applied in-memory after the index scan;
    /// all filter fields default to `None` (no restriction). Results are sorted
    /// oldest-first (`created_at` ascending) so callers see their longest-outstanding
    /// items at the top.
    ///
    /// The `filter.assignee` field is **ignored** here — the caller DID is always used
    /// as the assignee query key. Setting it would be a no-op or would produce empty results.
    pub fn list_work_for_person(
        &self,
        did: &icn_identity::Did,
        filter: &icn_governance::ActionItemFilter,
    ) -> Result<Vec<ActionItem>> {
        let mut items = self
            .action_items
            .list_by_assignee(did)
            .map_err(|e| anyhow::anyhow!("Failed to list work for person: {e}"))?;

        // Apply filter predicates (status, priority, overdue, tag, open_only).
        // The assignee check is skipped — items from the index are already scoped to `did`.
        let filter_no_assignee = icn_governance::ActionItemFilter {
            assignee: None,
            ..filter.clone()
        };
        items.retain(|item| filter_no_assignee.matches(item));

        // Oldest-first: surfaces the longest-outstanding work at the top.
        items.sort_by_key(|item| item.created_at);

        Ok(items)
    }

    // ========================================================================
    // Activity management (events, programs, projects, initiatives)
    // ========================================================================

    /// Create a new activity owned by an entity.
    ///
    /// If `parent_program_id` is `Some`, the new activity is registered on the
    /// program's forward-link list (`program.activities`) in addition to having
    /// its own `parent_program_id` set. This keeps both sides of the
    /// Program↔Activity relationship in sync from the moment of creation.
    ///
    /// If the referenced program does not exist the activity is still created
    /// (soft-reference semantics) but the forward-link update is skipped.
    pub fn create_activity(
        &self,
        parent_entity_id: String,
        kind: ActivityKind,
        name: String,
        description: Option<String>,
        start_date: Option<u64>,
        end_date: Option<u64>,
        parent_program_id: Option<icn_governance::program::ProgramId>,
    ) -> Result<Activity> {
        // Validate date range when both are provided
        if let (Some(start), Some(end)) = (start_date, end_date) {
            if end < start {
                return Err(anyhow::anyhow!("Activity end_date must be >= start_date"));
            }
        }
        let now = icn_time::current_timestamp_secs();
        let id = ActivityId::generate();
        let mut a = Activity::new(id, parent_entity_id, kind, name, now);
        a.description = description;
        a.start_date = start_date;
        a.end_date = end_date;
        a.parent_program_id = parent_program_id.clone();
        self.activity_store
            .save(&a)
            .map_err(|e| anyhow::anyhow!("Failed to save activity: {e}"))?;

        // Keep Program.activities forward list in sync with the reverse link.
        if let Some(prog_id) = &parent_program_id {
            if let Some(mut p) = self
                .program_store
                .get(prog_id)
                .map_err(|e| anyhow::anyhow!("Failed to look up program: {e}"))?
            {
                if !p.activities.contains(&a.id) {
                    p.activities.push(a.id.clone());
                    self.program_store
                        .save(&p)
                        .map_err(|e| anyhow::anyhow!("Failed to update program activities: {e}"))?;
                }
            }
            // Program not found → soft-reference, activity still created.
        }

        Ok(a)
    }

    /// Link an existing activity to a program, keeping both sides in sync.
    ///
    /// Idempotent when the relationship already exists.
    ///
    /// If the activity is currently linked to a *different* program, this
    /// performs a move: the activity ID is removed from the previous
    /// program's `activities` list before the new forward link is written.
    /// That preserves the single-parent invariant across Program↔Activity.
    ///
    /// Errors if either the program or the activity does not exist.
    pub fn link_activity_to_program(
        &self,
        program_id: &ProgramId,
        activity_id: &ActivityId,
    ) -> Result<()> {
        let mut p = self
            .program_store
            .get(program_id)
            .map_err(|e| anyhow::anyhow!("Failed to look up program: {e}"))?
            .ok_or_else(|| anyhow::anyhow!("Program not found: {program_id}"))?;

        let mut a = self
            .activity_store
            .get(activity_id)
            .map_err(|e| anyhow::anyhow!("Failed to look up activity: {e}"))?
            .ok_or_else(|| anyhow::anyhow!("Activity not found: {activity_id}"))?;

        // If moving from a different program, remove the stale forward link
        // on the old program first so it does not claim the activity.
        if let Some(old_program_id) = a.parent_program_id.clone() {
            if &old_program_id != program_id {
                if let Some(mut old_p) = self
                    .program_store
                    .get(&old_program_id)
                    .map_err(|e| anyhow::anyhow!("Failed to look up old program: {e}"))?
                {
                    if old_p.activities.contains(activity_id) {
                        old_p.activities.retain(|id| id != activity_id);
                        self.program_store.save(&old_p).map_err(|e| {
                            anyhow::anyhow!("Failed to update old program activities: {e}")
                        })?;
                    }
                }
            }
        }

        // Forward link: program → activity
        if !p.activities.contains(activity_id) {
            p.activities.push(activity_id.clone());
            self.program_store
                .save(&p)
                .map_err(|e| anyhow::anyhow!("Failed to update program activities: {e}"))?;
        }

        // Reverse link: activity → program
        if a.parent_program_id.as_ref() != Some(program_id) {
            a.parent_program_id = Some(program_id.clone());
            self.activity_store
                .save(&a)
                .map_err(|e| anyhow::anyhow!("Failed to update activity parent: {e}"))?;
        }

        Ok(())
    }

    /// Remove the link between an activity and a program, clearing both sides.
    ///
    /// Returns `true` if either side changed (forward list entry removed or
    /// reverse `parent_program_id` cleared), `false` if neither side needed
    /// updating. This deliberately handles the legacy / inconsistent case
    /// where only the reverse link was set (pre-consistency-fix records):
    /// the reverse link is always cleared if it points at `program_id`,
    /// even when the forward list has no entry.
    ///
    /// Errors if the program does not exist.
    pub fn unlink_activity_from_program(
        &self,
        program_id: &ProgramId,
        activity_id: &ActivityId,
    ) -> Result<bool> {
        let mut p = self
            .program_store
            .get(program_id)
            .map_err(|e| anyhow::anyhow!("Failed to look up program: {e}"))?
            .ok_or_else(|| anyhow::anyhow!("Program not found: {program_id}"))?;

        let mut changed = false;

        // Forward link: remove from program if present
        if p.activities.contains(activity_id) {
            p.activities.retain(|id| id != activity_id);
            self.program_store
                .save(&p)
                .map_err(|e| anyhow::anyhow!("Failed to update program activities: {e}"))?;
            changed = true;
        }

        // Reverse link: clear on activity when it points to this program,
        // regardless of whether the forward list had an entry. This cleans
        // up legacy records that set only one direction.
        if let Some(mut a) = self
            .activity_store
            .get(activity_id)
            .map_err(|e| anyhow::anyhow!("Failed to look up activity: {e}"))?
        {
            if a.parent_program_id.as_ref() == Some(program_id) {
                a.parent_program_id = None;
                self.activity_store
                    .save(&a)
                    .map_err(|e| anyhow::anyhow!("Failed to clear activity parent: {e}"))?;
                changed = true;
            }
        }

        Ok(changed)
    }

    /// Get an activity by ID.
    pub fn get_activity(&self, id: &ActivityId) -> Result<Option<Activity>> {
        self.activity_store
            .get(id)
            .map_err(|e| anyhow::anyhow!("Failed to get activity: {e}"))
    }

    /// List all activities owned by an entity.
    pub fn list_activities(&self, entity_id: &str) -> Result<Vec<Activity>> {
        self.activity_store
            .list_by_entity(entity_id)
            .map_err(|e| anyhow::anyhow!("Failed to list activities: {e}"))
    }

    // ========================================================================
    // Meeting management (deliberation trace objects)
    // ========================================================================

    /// Create a new meeting in a governance domain.
    pub fn create_meeting(
        &self,
        domain_id: String,
        title: String,
        description: Option<String>,
        scheduled_at: Option<u64>,
        created_by: String,
    ) -> Result<Meeting> {
        let now = icn_time::current_timestamp_secs();
        let id = MeetingId::generate();
        let mut m = Meeting::new(id, domain_id, title, created_by, now);
        m.description = description;
        m.scheduled_at = scheduled_at;
        self.meeting_store
            .save(&m)
            .map_err(|e| anyhow::anyhow!("Failed to save meeting: {e}"))?;
        Ok(m)
    }

    /// Get a meeting by ID.
    pub fn get_meeting(&self, id: &MeetingId) -> Result<Option<Meeting>> {
        self.meeting_store
            .get(id)
            .map_err(|e| anyhow::anyhow!("Failed to get meeting: {e}"))
    }

    /// List all meetings in a governance domain, newest first.
    pub fn list_meetings(&self, domain_id: &str) -> Result<Vec<Meeting>> {
        self.meeting_store
            .list_by_domain(domain_id)
            .map_err(|e| anyhow::anyhow!("Failed to list meetings: {e}"))
    }

    /// Update a meeting record (full save — caller mutates then calls this).
    pub fn update_meeting(&self, m: &Meeting) -> Result<()> {
        self.meeting_store
            .save(m)
            .map_err(|e| anyhow::anyhow!("Failed to update meeting: {e}"))
    }

    /// Delete a meeting (hard delete).
    pub fn delete_meeting(&self, id: &MeetingId) -> Result<bool> {
        self.meeting_store
            .delete(id)
            .map_err(|e| anyhow::anyhow!("Failed to delete meeting: {e}"))
    }

    // ========================================================================
    // Program management (multi-phase institutional endeavors)
    // ========================================================================

    /// Create a new program in a governance domain.
    pub fn create_program(
        &self,
        domain_id: GovernanceDomainId,
        parent_entity_id: String,
        kind: ProgramKind,
        name: String,
        description: Option<String>,
        start_at: Option<u64>,
        end_at: Option<u64>,
        created_by_decision: Option<icn_governance::ProposalId>,
    ) -> Result<Program> {
        let now = icn_time::current_timestamp_secs();
        let id = ProgramId::generate();
        let mut p = Program::new(id, domain_id, parent_entity_id, kind, name, now);
        p.description = description;
        p.start_at = start_at;
        p.end_at = end_at;
        p.created_by_decision = created_by_decision;
        self.program_store
            .save(&p)
            .map_err(|e| anyhow::anyhow!("Failed to save program: {e}"))?;
        Ok(p)
    }

    /// Get a program by ID.
    pub fn get_program(&self, id: &ProgramId) -> Result<Option<Program>> {
        self.program_store
            .get(id)
            .map_err(|e| anyhow::anyhow!("Failed to get program: {e}"))
    }

    /// List all programs in a governance domain, newest first.
    pub fn list_programs_by_domain(&self, domain_id: &GovernanceDomainId) -> Result<Vec<Program>> {
        self.program_store
            .list_by_domain(domain_id)
            .map_err(|e| anyhow::anyhow!("Failed to list programs: {e}"))
    }

    // ========================================================================
    // Milestone management (stage-gates within programs)
    // ========================================================================

    /// Create a new milestone in a program.
    pub fn create_milestone(
        &self,
        program_id: ProgramId,
        name: String,
        description: Option<String>,
        phase_index: u32,
        target_date: Option<u64>,
        completion_criteria: Vec<String>,
    ) -> Result<Milestone> {
        // Verify the program exists before creating a milestone against it.
        let mut p = self
            .program_store
            .get(&program_id)
            .map_err(|e| anyhow::anyhow!("Failed to look up program: {e}"))?
            .ok_or_else(|| anyhow::anyhow!("Program not found: {program_id}"))?;

        let now = icn_time::current_timestamp_secs();
        let id = MilestoneId::generate();
        let mut m = Milestone::new(id.clone(), program_id.clone(), name, phase_index, now);
        m.description = description;
        m.target_date = target_date;
        m.completion_criteria = completion_criteria;
        self.milestone_store
            .save(&m)
            .map_err(|e| anyhow::anyhow!("Failed to save milestone: {e}"))?;

        // Register the milestone ID on the program record in phase-index order.
        // Load the full set of existing milestones (including the newly saved one)
        // so the milestones list is always sorted by phase_index, not insertion order.
        {
            let mut indexed: Vec<(u32, MilestoneId)> = p
                .milestones
                .iter()
                .filter_map(|mid| {
                    self.milestone_store
                        .get(mid)
                        .ok()
                        .flatten()
                        .map(|ms| (ms.phase_index, mid.clone()))
                })
                .collect();
            indexed.push((m.phase_index, id));
            indexed.sort_by_key(|&(pi, _)| pi);
            p.milestones = indexed.into_iter().map(|(_, mid)| mid).collect();
        }
        self.program_store
            .save(&p)
            .map_err(|e| anyhow::anyhow!("Failed to update program milestones: {e}"))?;

        Ok(m)
    }

    /// Get a milestone by ID.
    pub fn get_milestone(&self, id: &MilestoneId) -> Result<Option<Milestone>> {
        self.milestone_store
            .get(id)
            .map_err(|e| anyhow::anyhow!("Failed to get milestone: {e}"))
    }

    /// List milestones belonging to a program, ordered by phase_index.
    pub fn list_milestones_by_program(&self, program_id: &ProgramId) -> Result<Vec<Milestone>> {
        self.milestone_store
            .list_by_program(program_id)
            .map_err(|e| anyhow::anyhow!("Failed to list milestones: {e}"))
    }

    /// Update a milestone's status.
    ///
    /// `actor` is the DID of the caller performing the transition. When the
    /// milestone moves into `Completed`, the actor is recorded as
    /// `completed_by` for audit. When the milestone is reopened, both
    /// `completed_at` and `completed_by` are cleared.
    pub fn update_milestone_status(
        &self,
        id: &MilestoneId,
        status: MilestoneStatus,
        actor: &Did,
    ) -> Result<Milestone> {
        let mut m = self
            .milestone_store
            .get(id)
            .map_err(|e| anyhow::anyhow!("Failed to get milestone: {e}"))?
            .ok_or_else(|| anyhow::anyhow!("Milestone not found: {id}"))?;

        if status == MilestoneStatus::Completed {
            if m.status != MilestoneStatus::Completed {
                m.completed_at = Some(icn_time::current_timestamp_secs());
                m.completed_by = Some(actor.clone());
            }
        } else {
            // Clear completion metadata when reopening (Pending/InProgress/Blocked).
            m.completed_at = None;
            m.completed_by = None;
        }
        m.status = status;
        self.milestone_store
            .save(&m)
            .map_err(|e| anyhow::anyhow!("Failed to save milestone: {e}"))?;
        Ok(m)
    }

    // ========================================================================
    // Gateway-level string wrappers
    //
    // These thin helpers let the gateway avoid importing icn_governance types
    // (ProposalId, VoteChoice) directly. They are the meaning-firewall boundary
    // for the proposal vote flow.
    // ========================================================================

    /// Cast a vote using string IDs and choices (gateway boundary helper).
    ///
    /// `choice_str` must be "for", "against", or "abstain".
    pub async fn cast_vote_str(
        &self,
        proposal_id_str: String,
        voter: Did,
        choice_str: &str,
        comment: Option<String>,
    ) -> Result<()> {
        let choice = match choice_str.to_lowercase().as_str() {
            "for" => VoteChoice::For,
            "against" => VoteChoice::Against,
            "abstain" => VoteChoice::Abstain,
            other => anyhow::bail!("Invalid vote choice: {other}"),
        };
        self.cast_vote(ProposalId(proposal_id_str), voter, choice, comment)
            .await
    }

    /// Get a proposal by its string ID (gateway boundary helper).
    pub async fn get_proposal_str(&self, id: &str) -> Result<Option<Proposal>> {
        self.get_proposal(&ProposalId(id.to_string())).await
    }

    /// Get the proof for a proposal by its string ID (gateway boundary helper).
    pub async fn get_proof_str(
        &self,
        id: &str,
    ) -> Result<Option<icn_governance::GovernanceProofV2>> {
        self.get_proof(&ProposalId(id.to_string())).await
    }

    // ========================================================================
    // Program dashboard (composite read surface)
    // ========================================================================

    /// Compose a compact dashboard for a program.
    ///
    /// Returns `None` when the program does not exist.
    ///
    /// Composition strategy:
    /// - **Program**: single `get_program` lookup.
    /// - **Milestones**: `list_milestones_by_program` (already sorted by
    ///   `phase_index`).
    /// - **Activities**: fetched individually from `program.activities` (the
    ///   program record is the source of truth for which activities belong to
    ///   it). Missing activity records are silently skipped (soft-reference
    ///   semantics: the program outlives individual activity deletions).
    /// - **Action item counts**: all items in the program's domain are loaded
    ///   once, then filtered in memory to those whose `parent` is an
    ///   `InstitutionalParent::Activity` with an ID present in
    ///   `program.activities`. Grouped by `ActionItemStatus`.
    /// - **Meetings**: all meetings linked to at least one program activity,
    ///   deduped and sorted by `scheduled_at` (earliest first).
    pub fn get_program_dashboard(
        &self,
        program_id: &ProgramId,
    ) -> Result<Option<ProgramDashboard>> {
        // 1. Program record
        let program = match self.get_program(program_id)? {
            Some(p) => p,
            None => return Ok(None),
        };

        // 2. Milestones (sorted by phase_index from the store)
        let milestones = self.list_milestones_by_program(program_id)?;

        // 3. Activities — two complementary sources, merged and deduped:
        //    (a) program.activities: explicit list maintained by the program lead.
        //    (b) list_activities(parent_entity_id) filtered by parent_program_id:
        //        activities that declared their own parent program at creation time.
        //
        // Both sources are truthful; either can be populated independently.
        // The union ensures dashboard completeness regardless of which
        // linkage direction was used.
        let mut seen_ids: HashSet<ActivityId> = program.activities.iter().cloned().collect();
        let entity_activities = self.list_activities(&program.parent_entity_id)?;
        for a in &entity_activities {
            if a.parent_program_id.as_ref() == Some(program_id) {
                seen_ids.insert(a.id.clone());
            }
        }

        let mut activities: Vec<Activity> = Vec::with_capacity(seen_ids.len());
        for act_id in &seen_ids {
            if let Some(a) = self.get_activity(act_id)? {
                activities.push(a);
            }
        }
        // Stable ordering: sort by name so response is deterministic
        activities.sort_by(|a, b| a.name.cmp(&b.name));

        // 4. Action item counts — one domain list, in-memory filter
        let activity_ids: HashSet<ActivityId> = seen_ids;
        let all_items = self.list_action_items(&program.domain_id, &ActionItemFilter::default())?;

        let mut counts = ProgramActionItemCounts::default();
        for item in &all_items {
            let belongs = match &item.parent {
                Some(icn_governance::InstitutionalParent::Activity { id }) => {
                    activity_ids.contains(id)
                }
                _ => false,
            };
            if belongs {
                match item.status {
                    ActionItemStatus::Pending => counts.pending += 1,
                    ActionItemStatus::InProgress => counts.in_progress += 1,
                    ActionItemStatus::Completed => counts.completed += 1,
                    ActionItemStatus::Deferred => counts.deferred += 1,
                    ActionItemStatus::Cancelled => counts.cancelled += 1,
                }
            }
        }

        // 5. Meetings — one list_by_activity call per discovered activity,
        //    deduped by meeting ID (same meeting may link to several activities).
        let mut seen_meeting_ids: HashSet<MeetingId> = HashSet::new();
        let mut meetings: Vec<Meeting> = Vec::new();
        for act_id in &activity_ids {
            let act_meetings = self.meeting_store.list_by_activity(act_id)?;
            for m in act_meetings {
                if seen_meeting_ids.insert(m.id.clone()) {
                    meetings.push(m);
                }
            }
        }
        // Deterministic ordering: primary key is `scheduled_at` (earliest
        // first, `None` sorts last), secondary key is `MeetingId` so meetings
        // with identical timestamps (common for `None`) have a stable order
        // independent of `HashSet` iteration order.
        meetings.sort_by(|a, b| {
            a.scheduled_at
                .unwrap_or(u64::MAX)
                .cmp(&b.scheduled_at.unwrap_or(u64::MAX))
                .then_with(|| a.id.0.cmp(&b.id.0))
        });

        Ok(Some(ProgramDashboard {
            program,
            milestones,
            activities,
            action_item_counts: counts,
            meetings,
        }))
    }
}

impl Default for GovernanceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Delegation Cycle Detection Helpers
// ============================================================================

struct ProposalMapLookup<'a>(&'a HashMap<ProposalId, Proposal>);

impl ProposalDomainLookup for ProposalMapLookup<'_> {
    fn lookup_proposal_domain(&self, proposal_id: &ProposalId) -> Option<GovernanceDomainId> {
        self.0.get(proposal_id).map(|p| p.domain_id.clone())
    }
}

fn manager_scopes_overlap(
    a: &DelegationScope,
    b: &DelegationScope,
    proposals: &HashMap<ProposalId, Proposal>,
) -> bool {
    let lookup = ProposalMapLookup(proposals);
    scopes_overlap(a, b, &lookup, false)
}

fn find_delegation_cycle(
    delegator: &Did,
    delegate: &Did,
    scope: &DelegationScope,
    delegations: &HashMap<DelegationId, Delegation>,
    proposals: &HashMap<ProposalId, Proposal>,
    now: Timestamp,
) -> Option<Vec<Did>> {
    let mut path = vec![delegator.clone(), delegate.clone()];
    let mut visited = HashSet::new();
    visited.insert(delegator.clone());
    let mut current = delegate.clone();

    for _ in 0..=DEFAULT_MAX_DELEGATION_DEPTH {
        if visited.contains(&current) {
            return Some(path);
        }
        visited.insert(current.clone());

        let next = delegations
            .values()
            .find(|d| {
                d.delegator == current
                    && d.is_active(now)
                    && manager_scopes_overlap(&d.scope, scope, proposals)
            })
            .map(|d| d.delegate.clone());

        match next {
            Some(d) => {
                if d == *delegator {
                    path.push(d);
                    return Some(path);
                }
                path.push(d.clone());
                current = d;
            }
            None => return None,
        }
    }

    None
}

fn compute_incoming_depth(
    delegate: &Did,
    scope: &DelegationScope,
    delegations: &HashMap<DelegationId, Delegation>,
    proposals: &HashMap<ProposalId, Proposal>,
    now: Timestamp,
) -> usize {
    let mut visited = HashSet::new();
    compute_incoming_depth_recursive(delegate, scope, delegations, proposals, now, &mut visited)
}

fn compute_incoming_depth_recursive(
    delegate: &Did,
    scope: &DelegationScope,
    delegations: &HashMap<DelegationId, Delegation>,
    proposals: &HashMap<ProposalId, Proposal>,
    now: Timestamp,
    visited: &mut HashSet<Did>,
) -> usize {
    if visited.contains(delegate) {
        return 0;
    }
    visited.insert(delegate.clone());

    if visited.len() > DEFAULT_MAX_DELEGATION_DEPTH + 10 {
        return 0;
    }

    let delegators: Vec<Did> = delegations
        .values()
        .filter(|d| {
            d.delegate == *delegate
                && d.is_active(now)
                && manager_scopes_overlap(&d.scope, scope, proposals)
        })
        .map(|d| d.delegator.clone())
        .collect();

    if delegators.is_empty() {
        return 0;
    }

    delegators
        .into_iter()
        .map(|d| {
            1 + compute_incoming_depth_recursive(&d, scope, delegations, proposals, now, visited)
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_domain_with_builtin_profile() {
        let mgr = GovernanceManager::new();
        let domain_id = GovernanceDomainId("test-coop".to_string());
        let membership = MembershipConfig {
            source: MembershipSource::StaticList(vec![]),
        };
        let params = GovernanceParams::new(50, 50, 86400);

        let result = mgr
            .create_domain(
                domain_id.clone(),
                "Test Coop".to_string(),
                "cooperative_default".to_string(),
                params,
                membership,
            )
            .await;

        assert!(result.is_ok());
        let domain = mgr.get_domain(&domain_id).await.unwrap().unwrap();
        assert_eq!(domain.config.profile.0, "cooperative_default");
    }

    #[tokio::test]
    async fn test_create_domain_with_contract_profile() {
        let mgr = GovernanceManager::new();
        let domain_id = GovernanceDomainId("contract-coop".to_string());
        let membership = MembershipConfig {
            source: MembershipSource::StaticList(vec![]),
        };
        let params = GovernanceParams::new(50, 50, 86400);

        mgr.create_domain(
            domain_id.clone(),
            "Contract Coop".to_string(),
            "contract:did:icn:abc123".to_string(),
            params,
            membership,
        )
        .await
        .unwrap();

        let domain = mgr.get_domain(&domain_id).await.unwrap().unwrap();
        assert_eq!(domain.config.profile.0, "contract:did:icn:abc123");
        assert!(domain.config.profile.is_contract());
    }

    fn test_did(seed: u8) -> Did {
        Did::from_anchor_id(&[seed; 32])
    }

    #[tokio::test]
    async fn test_delegation_direct_cycle_rejected() {
        let mgr = GovernanceManager::new();
        let alice = test_did(1);
        let bob = test_did(2);

        let d1 = Delegation::new(alice.clone(), bob.clone(), DelegationScope::Blanket);
        mgr.create_delegation(d1).await.unwrap();

        let d2 = Delegation::new(bob.clone(), alice.clone(), DelegationScope::Blanket);
        let result = mgr.create_delegation(d2).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[tokio::test]
    async fn test_delegation_transitive_cycle_rejected() {
        let mgr = GovernanceManager::new();
        let alice = test_did(1);
        let bob = test_did(2);
        let charlie = test_did(3);

        mgr.create_delegation(Delegation::new(
            alice.clone(),
            bob.clone(),
            DelegationScope::Blanket,
        ))
        .await
        .unwrap();
        mgr.create_delegation(Delegation::new(
            bob.clone(),
            charlie.clone(),
            DelegationScope::Blanket,
        ))
        .await
        .unwrap();

        let d3 = Delegation::new(charlie.clone(), alice.clone(), DelegationScope::Blanket);
        let result = mgr.create_delegation(d3).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[tokio::test]
    async fn test_delegation_max_depth_enforced() {
        let mgr = GovernanceManager::new();
        let alice = test_did(1);
        let bob = test_did(2);
        let charlie = test_did(3);
        let dave = test_did(4);
        let eve = test_did(5);

        mgr.create_delegation(Delegation::new(
            alice.clone(),
            bob.clone(),
            DelegationScope::Blanket,
        ))
        .await
        .unwrap();
        mgr.create_delegation(Delegation::new(
            bob.clone(),
            charlie.clone(),
            DelegationScope::Blanket,
        ))
        .await
        .unwrap();
        mgr.create_delegation(Delegation::new(
            charlie.clone(),
            dave.clone(),
            DelegationScope::Blanket,
        ))
        .await
        .unwrap();

        let result = mgr
            .create_delegation(Delegation::new(
                dave.clone(),
                eve.clone(),
                DelegationScope::Blanket,
            ))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("depth"));
    }

    #[tokio::test]
    async fn test_delegation_different_scopes_no_cycle() {
        let mgr = GovernanceManager::new();
        let alice = test_did(1);
        let bob = test_did(2);
        let domain1 = GovernanceDomainId::new("domain1");
        let domain2 = GovernanceDomainId::new("domain2");

        mgr.create_delegation(Delegation::new(
            alice.clone(),
            bob.clone(),
            DelegationScope::Domain(domain1.clone()),
        ))
        .await
        .unwrap();

        let result = mgr
            .create_delegation(Delegation::new(
                bob.clone(),
                alice.clone(),
                DelegationScope::Domain(domain2.clone()),
            ))
            .await;
        assert!(result.is_ok());
    }

    // ============================================================================
    // Stage 4 Regression Tests — Hardening Pass 2026-03-25
    // ============================================================================

    /// In-memory receipt backend for tests (no sled dependency required).
    struct InMemoryReceiptBackend {
        governance: std::sync::Mutex<Vec<icn_governance::GovernanceDecisionReceipt>>,
        allocations: std::sync::Mutex<Vec<AllocationReceipt>>,
    }

    impl InMemoryReceiptBackend {
        fn new() -> Self {
            Self {
                governance: std::sync::Mutex::new(vec![]),
                allocations: std::sync::Mutex::new(vec![]),
            }
        }
    }

    impl crate::receipt_backend::GovernanceReceiptBackend for InMemoryReceiptBackend {
        fn put_governance(
            &self,
            receipt: &icn_governance::GovernanceDecisionReceipt,
        ) -> Result<(), String> {
            self.governance.lock().unwrap().push(receipt.clone());
            Ok(())
        }
        fn get_governance_by_proposal(
            &self,
            proposal_id: &str,
        ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
            Ok(self
                .governance
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.proposal_id == proposal_id)
                .cloned())
        }
        fn put_allocation(
            &self,
            receipt: &AllocationReceipt,
        ) -> Result<icn_kernel_api::Hash, String> {
            let hash = [1u8; 32]; // deterministic test hash
            self.allocations.lock().unwrap().push(receipt.clone());
            Ok(hash)
        }
        fn get_governance_by_decision(
            &self,
            decision_hash: &icn_kernel_api::Hash,
        ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
            Ok(self
                .governance
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.decision_hash == *decision_hash)
                .cloned())
        }
        fn list_allocations_by_decision(
            &self,
            decision_hash: &icn_kernel_api::Hash,
        ) -> Result<Vec<AllocationReceipt>, String> {
            Ok(self
                .allocations
                .lock()
                .unwrap()
                .iter()
                .filter(|a| a.decision_hash == *decision_hash)
                .cloned()
                .collect())
        }
    }

    /// Build a minimal manager with a single domain and single member.
    async fn make_manager_with_domain() -> (GovernanceManager, GovernanceDomainId, Did) {
        let kp = icn_identity::KeyPair::generate().unwrap();
        let member_did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-coop");

        let mgr = GovernanceManager::new();
        mgr.create_domain(
            domain_id.clone(),
            "Test Coop".to_string(),
            "default".to_string(),
            GovernanceParams {
                quorum_percentage: 1,
                approval_threshold_percentage: 51,
                voting_period_seconds: 86400,
                require_deliberation: false,
                ..GovernanceParams::default()
            },
            MembershipConfig {
                source: MembershipSource::StaticList(vec![member_did.clone()]),
            },
        )
        .await
        .unwrap();

        (mgr, domain_id, member_did)
    }

    /// INV-2 TEST: Accepted Budget proposal creates AllocationReceipt.
    #[tokio::test]
    async fn test_inv2_allocation_receipt_created_on_budget_acceptance() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;

        // Attach in-memory receipt backend
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend.clone());

        // Create and open a Budget proposal
        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Fund server".to_string(),
                "Buy a server".to_string(),
                ProposalPayload::Budget {
                    amount: 1000,
                    currency: "HOURS".to_string(),
                    recipient: member_did.clone(),
                    purpose: "Infrastructure".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        mgr.open_proposal(proposal_id.clone(), 86400).await.unwrap();
        mgr.cast_vote(
            proposal_id.clone(),
            member_did.clone(),
            VoteChoice::For,
            None,
        )
        .await
        .unwrap();
        mgr.close_proposal(proposal_id.clone()).await.unwrap();

        // Verify governance receipt created
        let gov_receipt = backend.get_governance_by_proposal(&proposal_id.0).unwrap();
        assert!(
            gov_receipt.is_some(),
            "INV-2: governance receipt must be created on close"
        );
        let gov_receipt = gov_receipt.unwrap();
        assert_eq!(
            gov_receipt.outcome,
            icn_governance::ProofOutcome::Accepted,
            "INV-2: outcome must be Accepted"
        );

        // Verify allocation receipt created (INV-2)
        let allocations = backend
            .list_allocations_by_decision(&gov_receipt.decision_hash)
            .unwrap();
        assert!(
            !allocations.is_empty(),
            "INV-2: AllocationReceipt must be created for accepted Budget proposal"
        );
        assert_eq!(
            allocations[0].decision_hash, gov_receipt.decision_hash,
            "INV-2: AllocationReceipt decision_hash must match GovernanceDecisionReceipt"
        );
        assert!(
            !allocations[0].intents.is_empty(),
            "INV-2: AllocationReceipt must have at least one SettlementIntent"
        );
    }

    /// INV-2 TEST: Non-economic proposals (Text) do NOT create AllocationReceipt.
    #[tokio::test]
    async fn test_inv2_no_allocation_receipt_for_text_proposal() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend.clone());

        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Policy change".to_string(),
                "Update meeting schedule".to_string(),
                ProposalPayload::Text {
                    body: "Meet bi-weekly".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        mgr.open_proposal(proposal_id.clone(), 86400).await.unwrap();
        mgr.cast_vote(
            proposal_id.clone(),
            member_did.clone(),
            VoteChoice::For,
            None,
        )
        .await
        .unwrap();
        mgr.close_proposal(proposal_id.clone()).await.unwrap();

        let gov_receipt = backend
            .get_governance_by_proposal(&proposal_id.0)
            .unwrap()
            .unwrap();
        let allocations = backend
            .list_allocations_by_decision(&gov_receipt.decision_hash)
            .unwrap();
        assert!(
            allocations.is_empty(),
            "INV-2: Text proposal must NOT create AllocationReceipt"
        );
    }

    /// INV-6 TEST: Duplicate vote is rejected.
    #[tokio::test]
    async fn test_inv6_duplicate_vote_rejected() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;

        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Test".to_string(),
                "Test proposal".to_string(),
                ProposalPayload::Text {
                    body: "test".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        mgr.open_proposal(proposal_id.clone(), 86400).await.unwrap();

        // First vote succeeds
        mgr.cast_vote(
            proposal_id.clone(),
            member_did.clone(),
            VoteChoice::For,
            None,
        )
        .await
        .expect("First vote must succeed");

        // Second vote from same DID must fail (INV-6)
        let result = mgr
            .cast_vote(
                proposal_id.clone(),
                member_did.clone(),
                VoteChoice::Against,
                None,
            )
            .await;
        assert!(result.is_err(), "INV-6: Duplicate vote must be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("already voted"),
            "INV-6: Error must mention 'already voted', got: {err}"
        );
    }

    /// INV-5 TEST: get_chain returns chain after accepted proposal.
    #[tokio::test]
    async fn test_inv5_chain_walkable_after_close() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend.clone());

        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Fund server".to_string(),
                "Buy a server".to_string(),
                ProposalPayload::Budget {
                    amount: 500,
                    currency: "CREDITS".to_string(),
                    recipient: member_did.clone(),
                    purpose: "Server".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        mgr.open_proposal(proposal_id.clone(), 86400).await.unwrap();
        mgr.cast_vote(
            proposal_id.clone(),
            member_did.clone(),
            VoteChoice::For,
            None,
        )
        .await
        .unwrap();
        mgr.close_proposal(proposal_id.clone()).await.unwrap();

        // INV-5: chain must be walkable
        let chain = mgr.get_chain(&proposal_id).await.unwrap();
        assert!(
            chain.governance_receipt.is_some(),
            "INV-5: governance_receipt must be present"
        );
        assert!(
            !chain.allocations.is_empty(),
            "INV-5: allocations must be present for Budget proposal"
        );

        // Verify chain_complete is true for accepted Budget with allocations stored
        assert!(
            chain.chain_complete,
            "INV-5: chain_complete must be true for accepted Budget with stored allocations"
        );

        // Verify decision_hash binds governance to allocation
        let gov_hash = chain.governance_receipt.unwrap().decision_hash;
        assert_eq!(
            chain.allocations[0].decision_hash, gov_hash,
            "INV-5: allocation.decision_hash must equal governance.decision_hash"
        );
    }

    /// INV-5 TEST: chain_complete is correct for accepted Text proposal (no allocations expected).
    #[tokio::test]
    async fn test_inv5_chain_complete_text_proposal() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend.clone());

        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Text proposal".to_string(),
                "No economic effect".to_string(),
                ProposalPayload::Text {
                    body: "Just a decision".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        mgr.open_proposal(proposal_id.clone(), 86400).await.unwrap();
        mgr.cast_vote(
            proposal_id.clone(),
            member_did.clone(),
            VoteChoice::For,
            None,
        )
        .await
        .unwrap();
        mgr.close_proposal(proposal_id.clone()).await.unwrap();

        let chain = mgr.get_chain(&proposal_id).await.unwrap();
        assert!(
            chain.governance_receipt.is_some(),
            "INV-5: governance_receipt must exist"
        );
        assert!(
            chain.allocations.is_empty(),
            "INV-5: Text proposal has no allocations"
        );
        assert!(
            chain.chain_complete,
            "INV-5: chain_complete must be true for accepted Text proposal (no allocations needed)"
        );
    }

    // ========================================================================
    // Notification digest tests (Phase 4)
    // ========================================================================

    fn sled_db_tmp() -> (Arc<sled::Db>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = sled::Config::new()
            .path(dir.path())
            .temporary(true)
            .open()
            .expect("sled open");
        (Arc::new(db), dir)
    }

    #[tokio::test]
    async fn digest_empty_for_did_with_no_activity() {
        let (mgr, _domain, _member) = make_manager_with_domain().await;
        let stranger = test_did(99);

        let digest = mgr.generate_digest(&stranger, 1_700_000_000).await;

        assert_eq!(digest.did, stranger.to_string());
        assert_eq!(digest.pending_vote_count, 0);
        assert!(digest.pending_votes.is_empty());
        assert_eq!(digest.overdue_item_count, 0);
        assert!(digest.overdue_items.is_empty());
        assert_eq!(digest.upcoming_meeting_count, 0);
        assert!(digest.upcoming_meetings.is_empty());
    }

    #[tokio::test]
    async fn digest_pending_vote_includes_open_proposal_not_yet_voted() {
        let (mgr, domain_id, proposer) = make_manager_with_domain().await;
        let voter = test_did(7);

        let prop_id = mgr
            .create_proposal(
                ProposalId("prop-digest-1".to_string()),
                domain_id.clone(),
                proposer.clone(),
                "Pending".to_string(),
                "body".to_string(),
                ProposalPayload::Text {
                    body: "hello".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();
        mgr.open_proposal(prop_id.clone(), 86400).await.unwrap();

        let digest = mgr.generate_digest(&voter, 1_700_000_000).await;

        assert_eq!(digest.pending_vote_count, 1);
        assert_eq!(digest.pending_votes[0].proposal_id, prop_id.0);
        assert!(digest.pending_votes[0].closes_at.is_some());
    }

    #[tokio::test]
    async fn digest_pending_vote_excluded_after_voting() {
        let (mgr, domain_id, member) = make_manager_with_domain().await;

        let prop_id = mgr
            .create_proposal(
                ProposalId("prop-digest-2".to_string()),
                domain_id.clone(),
                member.clone(),
                "Voted".to_string(),
                "body".to_string(),
                ProposalPayload::Text {
                    body: "hello".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();
        mgr.open_proposal(prop_id.clone(), 86400).await.unwrap();
        mgr.cast_vote(prop_id.clone(), member.clone(), VoteChoice::For, None)
            .await
            .unwrap();

        let digest = mgr.generate_digest(&member, 1_700_000_000).await;
        assert_eq!(
            digest.pending_vote_count, 0,
            "member who already voted should not see a pending vote"
        );
    }

    #[tokio::test]
    async fn digest_overdue_item_appears_for_assignee() {
        // Use Sled-backed action-item store so list_by_assignee actually works.
        let (db, _dir) = sled_db_tmp();
        let (mut mgr, domain_id, _member) = make_manager_with_domain().await;
        mgr.set_action_item_store(Box::new(SledActionItemStore::new(db)));
        let assignee = fresh_did();
        let creator = fresh_did();

        // Create an assigned item with a due date in the past.
        let mut item = ActionItem::new(
            domain_id.clone(),
            "Write minutes".to_string(),
            creator,
            1_000,
        );
        item.assignee = Some(assignee.clone());
        item.due_date = Some(1_500);
        mgr.action_items.save(&item).unwrap();

        // Now = 2000, past due.
        let digest = mgr.generate_digest(&assignee, 2_000).await;
        assert_eq!(digest.overdue_item_count, 1);
        assert_eq!(digest.overdue_items[0].title, "Write minutes");
        assert_eq!(digest.overdue_items[0].due_date, 1_500);
    }

    #[tokio::test]
    async fn digest_overdue_excludes_completed_item() {
        let (db, _dir) = sled_db_tmp();
        let (mut mgr, domain_id, _member) = make_manager_with_domain().await;
        mgr.set_action_item_store(Box::new(SledActionItemStore::new(db)));
        let assignee = fresh_did();
        let creator = fresh_did();

        let mut item = ActionItem::new(domain_id.clone(), "Done".to_string(), creator, 1_000);
        item.assignee = Some(assignee.clone());
        item.due_date = Some(1_500);
        item.status = ActionItemStatus::Completed;
        mgr.action_items.save(&item).unwrap();

        let digest = mgr.generate_digest(&assignee, 2_000).await;
        assert_eq!(digest.overdue_item_count, 0);
    }

    #[tokio::test]
    async fn digest_upcoming_meetings_filter_by_window_and_attendee() {
        let (mgr, domain_id, _member) = make_manager_with_domain().await;
        let invitee = test_did(21);
        let bystander_did_str = "did:icn:stranger".to_string();

        let now = 1_700_000_000u64;

        // In-window meeting with invitee listed as attendee.
        let mut m_in = Meeting::new(
            MeetingId::generate(),
            domain_id.0.clone(),
            "In-window",
            "did:icn:organizer",
            now,
        );
        m_in.scheduled_at = Some(now + 3_600); // +1h
        m_in.attendees = vec![icn_governance::MeetingAttendee {
            did: invitee.as_str().to_owned(),
            status: icn_governance::AttendanceStatus::Invited,
            meeting_role: icn_governance::MeetingRole::Participant,
        }];
        mgr.meeting_store.save(&m_in).unwrap();

        // Out-of-window meeting (3 days out).
        let mut m_far = Meeting::new(
            MeetingId::generate(),
            domain_id.0.clone(),
            "Far",
            "did:icn:organizer",
            now,
        );
        m_far.scheduled_at = Some(now + 3 * 86_400);
        m_far.attendees = vec![icn_governance::MeetingAttendee {
            did: invitee.as_str().to_owned(),
            status: icn_governance::AttendanceStatus::Invited,
            meeting_role: icn_governance::MeetingRole::Participant,
        }];
        mgr.meeting_store.save(&m_far).unwrap();

        // In-window meeting but invitee is NOT an attendee.
        let mut m_no_invite = Meeting::new(
            MeetingId::generate(),
            domain_id.0.clone(),
            "Not invited",
            "did:icn:organizer",
            now,
        );
        m_no_invite.scheduled_at = Some(now + 1_800);
        m_no_invite.attendees = vec![icn_governance::MeetingAttendee {
            did: bystander_did_str,
            status: icn_governance::AttendanceStatus::Invited,
            meeting_role: icn_governance::MeetingRole::Participant,
        }];
        mgr.meeting_store.save(&m_no_invite).unwrap();

        let digest = mgr.generate_digest(&invitee, now).await;
        assert_eq!(digest.upcoming_meeting_count, 1);
        assert_eq!(digest.upcoming_meetings[0].title, "In-window");
    }

    // ------------------------------------------------------------------------
    // SledActionItemStore assignee-secondary-index coverage
    // ------------------------------------------------------------------------

    /// Helper: generate a valid DID from a fresh Ed25519 key.
    /// (test_did() uses from_anchor_id which may not decompress as a valid
    /// Edwards point on deserialize — avoid for Sled roundtrip tests.)
    fn fresh_did() -> Did {
        icn_identity::KeyPair::generate().unwrap().did().clone()
    }

    #[tokio::test]
    async fn sled_action_items_list_by_assignee_across_domains() {
        use icn_governance::ActionItemStoreBackend;
        let (db, _dir) = sled_db_tmp();
        let store = SledActionItemStore::new(db);
        let assignee = fresh_did();
        let creator = fresh_did();
        let other_assignee = fresh_did();

        let domain_a = GovernanceDomainId::new("dom-a");
        let domain_b = GovernanceDomainId::new("dom-b");

        let mut item_a = ActionItem::new(domain_a.clone(), "A".to_string(), creator.clone(), 1);
        item_a.assignee = Some(assignee.clone());
        let mut item_b = ActionItem::new(domain_b.clone(), "B".to_string(), creator.clone(), 1);
        item_b.assignee = Some(assignee.clone());
        let mut item_c = ActionItem::new(domain_a.clone(), "C".to_string(), creator.clone(), 1);
        item_c.assignee = Some(other_assignee); // different assignee

        store.save(&item_a).unwrap();
        store.save(&item_b).unwrap();
        store.save(&item_c).unwrap();

        let items = store.list_by_assignee(&assignee).unwrap();
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(
            items.len(),
            2,
            "should find assignee's items in both domains"
        );
        assert!(titles.contains(&"A"));
        assert!(titles.contains(&"B"));
        assert!(!titles.contains(&"C"));
    }

    #[tokio::test]
    async fn sled_action_items_assignee_index_cleaned_on_delete() {
        use icn_governance::ActionItemStoreBackend;
        let (db, _dir) = sled_db_tmp();
        let store = SledActionItemStore::new(db);
        let assignee = fresh_did();
        let creator = fresh_did();
        let domain = GovernanceDomainId::new("dom-x");

        let mut item = ActionItem::new(domain.clone(), "Title".to_string(), creator, 1);
        item.assignee = Some(assignee.clone());
        store.save(&item).unwrap();
        assert_eq!(store.list_by_assignee(&assignee).unwrap().len(), 1);

        let removed = store.delete(&domain, &item.id).unwrap();
        assert!(removed);
        assert_eq!(
            store.list_by_assignee(&assignee).unwrap().len(),
            0,
            "assignee index row must be removed on delete"
        );
    }

    /// Regression: list_by_assignee used splitn(2) which failed when domain_id
    /// contained ':' characters (e.g. "did:icn:..." style IDs). rsplitn from
    /// the right correctly isolates the UUID item_id.
    #[tokio::test]
    async fn sled_action_items_list_by_assignee_colon_domain_id() {
        use icn_governance::ActionItemStoreBackend;
        let (db, _dir) = sled_db_tmp();
        let store = SledActionItemStore::new(db);
        let assignee = fresh_did();
        let creator = fresh_did();

        // Domain ID with colons — would break splitn(2, ':')
        let domain = GovernanceDomainId::new("did:icn:coop:local");

        let mut item = ActionItem::new(domain.clone(), "Colon domain".to_string(), creator, 1);
        item.assignee = Some(assignee.clone());
        store.save(&item).unwrap();

        let found = store.list_by_assignee(&assignee).unwrap();
        assert_eq!(
            found.len(),
            1,
            "must find item even when domain_id contains colons"
        );
        assert_eq!(found[0].id, item.id);
    }

    /// Regression: SledActionItemStore::save leaked the old assignee index entry
    /// when an item's assignee was changed on update.
    #[tokio::test]
    async fn sled_action_items_assignee_index_updated_on_reassign() {
        use icn_governance::ActionItemStoreBackend;
        let (db, _dir) = sled_db_tmp();
        let store = SledActionItemStore::new(db);
        let original_assignee = fresh_did();
        let new_assignee = fresh_did();
        let creator = fresh_did();
        let domain = GovernanceDomainId::new("dom-reassign");

        let mut item = ActionItem::new(domain.clone(), "Reassign test".to_string(), creator, 1);
        item.assignee = Some(original_assignee.clone());
        store.save(&item).unwrap();

        assert_eq!(store.list_by_assignee(&original_assignee).unwrap().len(), 1);
        assert_eq!(store.list_by_assignee(&new_assignee).unwrap().len(), 0);

        // Reassign to new_assignee
        item.assignee = Some(new_assignee.clone());
        store.save(&item).unwrap();

        assert_eq!(
            store.list_by_assignee(&original_assignee).unwrap().len(),
            0,
            "old assignee index row must be removed after reassignment"
        );
        assert_eq!(
            store.list_by_assignee(&new_assignee).unwrap().len(),
            1,
            "new assignee must appear in index"
        );
    }

    // =========================================================================
    // Program ↔ Activity consistency tests
    // =========================================================================

    /// `create_activity` with a valid `parent_program_id` must add the new
    /// activity's ID into `Program.activities` immediately.
    #[tokio::test]
    async fn create_activity_with_parent_program_id_updates_forward_list() {
        let (mgr, domain_id, _member) = make_manager_with_domain().await;

        let prog = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "Prog".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        assert!(
            prog.activities.is_empty(),
            "fresh program has no activities"
        );

        let act = mgr
            .create_activity(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "Act A".to_string(),
                None,
                None,
                None,
                Some(prog.id.clone()),
            )
            .unwrap();

        let reloaded = mgr.get_program(&prog.id).unwrap().unwrap();
        assert!(
            reloaded.activities.contains(&act.id),
            "program.activities must contain the new activity id"
        );
    }

    /// If the referenced program does not exist, `create_activity` must still
    /// succeed (soft-reference semantics, consistent with milestone behavior).
    #[tokio::test]
    async fn create_activity_program_not_found_still_succeeds() {
        let (mgr, _domain_id, _member) = make_manager_with_domain().await;

        let ghost_program_id = ProgramId::generate();

        let act = mgr
            .create_activity(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "Orphan Act".to_string(),
                None,
                None,
                None,
                Some(ghost_program_id.clone()),
            )
            .unwrap();

        // Activity was created and carries the reverse link.
        assert_eq!(act.parent_program_id.as_ref(), Some(&ghost_program_id));
    }

    /// `link_activity_to_program` must write both directions: program gains the
    /// activity ID and the activity gains the program ID.
    #[tokio::test]
    async fn link_activity_to_program_both_sides_updated() {
        let (mgr, domain_id, _member) = make_manager_with_domain().await;

        let prog = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "P".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        // Activity created WITHOUT a parent — no forward/reverse link yet.
        let act = mgr
            .create_activity(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "A".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        assert!(act.parent_program_id.is_none());

        mgr.link_activity_to_program(&prog.id, &act.id).unwrap();

        let p2 = mgr.get_program(&prog.id).unwrap().unwrap();
        let a2 = mgr.get_activity(&act.id).unwrap().unwrap();
        assert!(p2.activities.contains(&act.id), "forward link must exist");
        assert_eq!(
            a2.parent_program_id.as_ref(),
            Some(&prog.id),
            "reverse link must exist"
        );
    }

    /// `link_activity_to_program` must be a no-op (not an error) when the
    /// relationship already exists.
    #[tokio::test]
    async fn link_activity_to_program_idempotent() {
        let (mgr, domain_id, _member) = make_manager_with_domain().await;

        let prog = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "P".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let act = mgr
            .create_activity(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "A".to_string(),
                None,
                None,
                None,
                Some(prog.id.clone()),
            )
            .unwrap();

        // Link again — must not error or duplicate the ID in the list.
        mgr.link_activity_to_program(&prog.id, &act.id).unwrap();
        mgr.link_activity_to_program(&prog.id, &act.id).unwrap();

        let p2 = mgr.get_program(&prog.id).unwrap().unwrap();
        let occurrences = p2.activities.iter().filter(|id| *id == &act.id).count();
        assert_eq!(occurrences, 1, "activity id must appear exactly once");
    }

    /// `link_activity_to_program` must return an error when the activity does
    /// not exist.
    #[tokio::test]
    async fn link_activity_to_program_activity_not_found_errors() {
        let (mgr, domain_id, _member) = make_manager_with_domain().await;

        let prog = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "P".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let ghost = ActivityId::generate();
        let result = mgr.link_activity_to_program(&prog.id, &ghost);
        assert!(result.is_err(), "linking non-existent activity must error");
    }

    /// `unlink_activity_from_program` must remove both directions.
    #[tokio::test]
    async fn unlink_activity_from_program_both_sides_cleared() {
        let (mgr, domain_id, _member) = make_manager_with_domain().await;

        let prog = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "P".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let act = mgr
            .create_activity(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "A".to_string(),
                None,
                None,
                None,
                Some(prog.id.clone()),
            )
            .unwrap();

        // Verify both sides are set before unlinking.
        let p1 = mgr.get_program(&prog.id).unwrap().unwrap();
        assert!(p1.activities.contains(&act.id));

        let was_linked = mgr.unlink_activity_from_program(&prog.id, &act.id).unwrap();
        assert!(was_linked, "must return true when actually unlinked");

        let p2 = mgr.get_program(&prog.id).unwrap().unwrap();
        let a2 = mgr.get_activity(&act.id).unwrap().unwrap();
        assert!(
            !p2.activities.contains(&act.id),
            "forward link must be removed"
        );
        assert!(
            a2.parent_program_id.is_none(),
            "reverse link must be cleared"
        );
    }

    /// `unlink_activity_from_program` must return `false` (not an error) when
    /// the activity is not linked to that program.
    #[tokio::test]
    async fn unlink_activity_not_linked_returns_false() {
        let (mgr, domain_id, _member) = make_manager_with_domain().await;

        let prog = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "P".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let act = mgr
            .create_activity(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "A".to_string(),
                None,
                None,
                None,
                None, // not linked to prog
            )
            .unwrap();

        let was_linked = mgr.unlink_activity_from_program(&prog.id, &act.id).unwrap();
        assert!(!was_linked, "must return false when not linked");
    }

    /// Relinking an activity to a different program must move both directions
    /// atomically: old program loses the ID, new program gains it, and the
    /// activity's reverse link points to the new program.
    #[tokio::test]
    async fn relink_to_different_program_updates_both() {
        let (mgr, domain_id, _member) = make_manager_with_domain().await;

        let prog_a = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "Prog A".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let prog_b = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "Prog B".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let act = mgr
            .create_activity(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "Migrating Act".to_string(),
                None,
                None,
                None,
                Some(prog_a.id.clone()),
            )
            .unwrap();

        // Unlink from A, link to B.
        mgr.unlink_activity_from_program(&prog_a.id, &act.id)
            .unwrap();
        mgr.link_activity_to_program(&prog_b.id, &act.id).unwrap();

        let pa = mgr.get_program(&prog_a.id).unwrap().unwrap();
        let pb = mgr.get_program(&prog_b.id).unwrap().unwrap();
        let a2 = mgr.get_activity(&act.id).unwrap().unwrap();

        assert!(
            !pa.activities.contains(&act.id),
            "prog_a must no longer contain the activity"
        );
        assert!(
            pb.activities.contains(&act.id),
            "prog_b must contain the activity"
        );
        assert_eq!(
            a2.parent_program_id.as_ref(),
            Some(&prog_b.id),
            "activity reverse link must point to prog_b"
        );
    }

    /// After `create_activity` with a `parent_program_id`, the dashboard for
    /// that program must include the activity via the forward list alone — no
    /// reverse-scan-only rescue needed for newly created activities.
    #[tokio::test]
    async fn dashboard_sees_activity_via_forward_link_immediately() {
        let (mgr, domain_id, _member) = make_manager_with_domain().await;

        let prog = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "FwdProg".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let act = mgr
            .create_activity(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "FwdAct".to_string(),
                None,
                None,
                None,
                Some(prog.id.clone()),
            )
            .unwrap();

        // Verify forward link is set — the dashboard does not rely on the
        // reverse scan to discover this activity.
        let p = mgr.get_program(&prog.id).unwrap().unwrap();
        assert!(
            p.activities.contains(&act.id),
            "forward link must be set immediately after create_activity"
        );

        // Dashboard must include the activity.
        let dash = mgr
            .get_program_dashboard(&prog.id)
            .unwrap()
            .expect("program must exist");
        assert!(
            dash.activities.iter().any(|a| a.id == act.id),
            "dashboard must expose the activity"
        );
    }

    /// Legacy / pre-consistency-fix data: the activity's reverse link points at
    /// `program_id`, but the program's forward list does NOT contain the
    /// activity. `unlink_activity_from_program` must still clear the reverse
    /// link and report that a change occurred.
    #[tokio::test]
    async fn unlink_clears_reverse_only_legacy_link() {
        let (mgr, domain_id, _member) = make_manager_with_domain().await;

        let prog = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "P".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let mut act = mgr
            .create_activity(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "Legacy".to_string(),
                None,
                None,
                None,
                None, // not initially linked
            )
            .unwrap();

        // Simulate legacy inconsistent state: reverse set but forward missing.
        act.parent_program_id = Some(prog.id.clone());
        mgr.activity_store.save(&act).unwrap();
        let p_before = mgr.get_program(&prog.id).unwrap().unwrap();
        assert!(
            !p_before.activities.contains(&act.id),
            "precondition: forward list must be empty"
        );

        let changed = mgr.unlink_activity_from_program(&prog.id, &act.id).unwrap();
        assert!(
            changed,
            "unlink must report change even when only reverse link existed"
        );

        let a_after = mgr.get_activity(&act.id).unwrap().unwrap();
        assert!(
            a_after.parent_program_id.is_none(),
            "reverse link must be cleared for legacy records"
        );
    }

    /// Directly relinking an activity (without calling unlink first) must
    /// remove the activity ID from the previous program's forward list so
    /// no program holds a stale entry.
    #[tokio::test]
    async fn link_moves_activity_between_programs() {
        let (mgr, domain_id, _member) = make_manager_with_domain().await;

        let prog_a = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "A".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();
        let prog_b = mgr
            .create_program(
                domain_id.clone(),
                "ent-1".to_string(),
                ProgramKind::Cycle,
                "B".to_string(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

        let act = mgr
            .create_activity(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "Mover".to_string(),
                None,
                None,
                None,
                Some(prog_a.id.clone()),
            )
            .unwrap();

        // Link directly to B without calling unlink first.
        mgr.link_activity_to_program(&prog_b.id, &act.id).unwrap();

        let pa = mgr.get_program(&prog_a.id).unwrap().unwrap();
        let pb = mgr.get_program(&prog_b.id).unwrap().unwrap();
        let a2 = mgr.get_activity(&act.id).unwrap().unwrap();
        assert!(
            !pa.activities.contains(&act.id),
            "prog_a must lose the stale forward entry"
        );
        assert!(pb.activities.contains(&act.id));
        assert_eq!(a2.parent_program_id.as_ref(), Some(&prog_b.id));
    }
}
