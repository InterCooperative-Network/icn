//! Governance Manager for Gateway API
//!
//! Moved from `icn-gateway` to the app layer so the manager lives next to the
//! domain it manages. The only structural change relative to the gateway version
//! is that `ReceiptStore` (a gateway-internal type) is replaced by the
//! [`GovernanceReceiptBackend`] trait so this crate does not depend on
//! `icn-gateway`.

use crate::dispatch_evidence::{
    derive_reconciliation_status, EffectDispatchEvidence, ReconciliationStatus,
};
use crate::institutional_effect::InstitutionalEffectRecord;
use crate::receipt_backend::GovernanceReceiptBackend;
use crate::state_store::{GovernanceStateStore, SledGovernanceStateStore};
use anyhow::Result;
use icn_federation::BilateralClearingAgreement;
use icn_governance::{
    scopes_overlap, ActionItem, ActionItemFilter, ActionItemId, ActionItemPriority,
    ActionItemStatus, ActionItemStoreBackend, Activity, ActivityId, ActivityKind,
    ActivityStoreBackend, AttendanceStatus, Comment, CommentId, Delegation, DelegationId,
    DelegationScope, Discussion, DiscussionStore, GovernanceConfig, GovernanceDecisionReceipt,
    GovernanceDomain, GovernanceDomainId, GovernanceError, GovernanceOps, GovernanceParams,
    GovernanceProfileId, InMemoryActionItemStore, InMemoryActivityStore, InMemoryDiscussionStore,
    InMemoryMeetingStore, InMemoryMilestoneStore, InMemoryProgramStore, InMemoryStructureStore,
    Meeting, MeetingAttendanceTransition, MeetingId, MeetingStoreBackend, MembershipConfig,
    MembershipSource, Milestone, MilestoneId, MilestoneStatus, MilestoneStoreBackend,
    PaginatedResult, Program, ProgramId, ProgramKind, ProgramStatus, ProgramStoreBackend,
    ProofOutcome, Proposal, ProposalDomainLookup, ProposalId, ProposalPayload, ProposalScope,
    ProposalState, RoleAssignment, Structure, StructureId, StructureKind, StructureStoreBackend,
    Timestamp, Vote, VoteChoice, VoteTally, DEFAULT_MAX_DELEGATION_DEPTH,
};
use icn_identity::Did;
use icn_kernel_api::{AllocationReceipt, ScopeLevel, SettlementIntent};
use icn_store::SledStore;
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

/// A single meeting/agenda-item pair in a proposal's deliberation trail.
///
/// Produced by [`GovernanceManager::get_deliberation`] and rendered into
/// [`crate::http::models::DeliberationMeetingResponse`] at the HTTP boundary.
#[derive(Debug, Clone)]
pub struct DeliberationMeetingEntry {
    pub meeting_id: icn_governance::MeetingId,
    pub meeting_title: String,
    pub meeting_status: icn_governance::MeetingStatus,
    pub scheduled_at: Option<u64>,
    pub started_at: Option<u64>,
    pub ended_at: Option<u64>,
    pub agenda_item_id: icn_governance::AgendaItemId,
    pub agenda_item_title: String,
    pub presenter: Option<String>,
    pub discussion_notes: Option<String>,
    pub outcome: Option<String>,
    pub generated_action_items: Vec<icn_governance::ActionItemId>,
}

/// Reverse read-model: a proposal's institutional trail.
///
/// Returned by [`GovernanceManager::get_deliberation`]. The `effect_kind`
/// field labels which `http::configure::GovernanceEffect` variant this
/// proposal would translate into on acceptance — shape only, not a claim
/// that the effect was dispatched.
#[derive(Debug, Clone)]
pub struct ProposalDeliberation {
    pub proposal_id: ProposalId,
    pub domain_id: GovernanceDomainId,
    pub payload_type: &'static str,
    pub state_label: &'static str,
    /// Unix seconds the proposal reached a terminal state, if any.
    /// Sourced from `ProposalState`, not the receipt (receipts do not carry timestamps).
    pub decided_at: Option<u64>,
    pub effect_kind: &'static str,
    pub deliberations: Vec<DeliberationMeetingEntry>,
    pub governance_receipt: Option<icn_governance::GovernanceDecisionReceipt>,
    /// Institutional effect records emitted at acceptance plus their
    /// downstream reconciliation state, oldest-first.  Empty when the
    /// proposal was not accepted, when the payload translated to
    /// `Unhandled`, or when no receipt store is wired.
    pub emitted_effects: Vec<ReconciledEffectEntry>,
}

/// An emitted institutional effect record paired with its dispatch evidence
/// and derived reconciliation status.
#[derive(Debug, Clone)]
pub struct ReconciledEffectEntry {
    pub record: InstitutionalEffectRecord,
    pub dispatch_evidence: Vec<EffectDispatchEvidence>,
    pub reconciliation_status: ReconciliationStatus,
}

/// Label the translated [`crate::http::configure::GovernanceEffect`] shape
/// for a proposal payload. Pure projection — must stay in sync with the
/// match in `http::handlers::close_proposal`.
fn payload_effect_kind(payload: &ProposalPayload) -> &'static str {
    match payload {
        ProposalPayload::FreezeMember { .. } => "freeze_member",
        ProposalPayload::UnfreezeMember { .. } => "unfreeze_member",
        ProposalPayload::Charter { .. } => "deploy_charter",
        ProposalPayload::Sdis { proposal: sdis } => match sdis {
            icn_governance::sdis::SdisProposal::AppointSteward { .. } => "appoint_steward",
            icn_governance::sdis::SdisProposal::RemoveSteward { .. } => "revoke_steward",
            _ => "unhandled",
        },
        _ => "unhandled",
    }
}

fn payload_requires_allocation_receipt(payload: &ProposalPayload) -> bool {
    matches!(
        payload,
        ProposalPayload::Budget { .. }
            | ProposalPayload::Treasury {
                operation: icn_governance::TreasuryProposalOperation::CreateBudget { .. }
                    | icn_governance::TreasuryProposalOperation::Spend { .. },
            }
            | ProposalPayload::Allocation { .. }
            | ProposalPayload::SurplusAllocation { .. }
    )
}

/// Unix-seconds timestamp the proposal reached a terminal lifecycle state,
/// or `None` if still in-flight. Used by the deliberation endpoint to tag
/// when a decision was recorded without depending on receipt fields.
fn proposal_decided_at(state: &icn_governance::ProposalState) -> Option<u64> {
    match state {
        icn_governance::ProposalState::Accepted { closed_at }
        | icn_governance::ProposalState::Rejected { closed_at }
        | icn_governance::ProposalState::NoQuorum { closed_at } => Some(*closed_at),
        icn_governance::ProposalState::Cancelled { cancelled_at } => Some(*cancelled_at),
        icn_governance::ProposalState::Vetoed { vetoed_at, .. } => Some(*vetoed_at),
        icn_governance::ProposalState::ForceClosed { closed_at, .. } => Some(*closed_at),
        icn_governance::ProposalState::Draft
        | icn_governance::ProposalState::Deliberation { .. }
        | icn_governance::ProposalState::Open { .. } => None,
    }
}

/// Short lowercase label for a proposal lifecycle state.
fn proposal_state_label(state: &icn_governance::ProposalState) -> &'static str {
    match state {
        icn_governance::ProposalState::Draft => "draft",
        icn_governance::ProposalState::Deliberation { .. } => "deliberation",
        icn_governance::ProposalState::Open { .. } => "open",
        icn_governance::ProposalState::Accepted { .. } => "accepted",
        icn_governance::ProposalState::Rejected { .. } => "rejected",
        icn_governance::ProposalState::NoQuorum { .. } => "no_quorum",
        icn_governance::ProposalState::Cancelled { .. } => "cancelled",
        icn_governance::ProposalState::Vetoed { .. } => "vetoed",
        icn_governance::ProposalState::ForceClosed { .. } => "force_closed",
    }
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

        // If the assignee changed on update, the OLD by-assignee index entry must
        // be removed. Resolve it before the transaction (a read), then remove it
        // and (re)write the primary row + new index atomically below.
        let stale_assignee_idx: Option<String> = match self.get(&item.domain_id, &item.id) {
            Ok(Some(existing)) if existing.assignee != item.assignee => existing
                .assignee
                .as_ref()
                .map(|old| Self::assignee_idx_key(old, &item.domain_id, &item.id)),
            _ => None,
        };

        let value = icn_encoding::encode_versioned(item)
            .map_err(|e| GovernanceError::Internal(format!("Failed to encode action item: {e}")))?;
        let new_assignee_idx: Option<String> = item
            .assignee
            .as_ref()
            .map(|assignee| Self::assignee_idx_key(assignee, &item.domain_id, &item.id));

        // Atomic: the primary row, the assignee secondary index, and any stale
        // index removal commit together in a single sled transaction. A partial
        // write (primary row present but its assignee index missing) can
        // therefore never occur, so by-assignee readers never miss a persisted
        // item and close-journal recovery can trust a present primary row as a
        // fully-persisted obligation.
        self.db
            .transaction(|tx| {
                if let Some(ref stale) = stale_assignee_idx {
                    tx.remove(stale.as_bytes())?;
                }
                tx.insert(key.as_bytes(), value.as_slice())?;
                if let Some(ref idx) = new_assignee_idx {
                    tx.insert(idx.as_bytes(), b"1" as &[u8])?;
                }
                Ok::<(), sled::transaction::ConflictableTransactionError<()>>(())
            })
            .map_err(|e: sled::transaction::TransactionError<()>| {
                GovernanceError::Internal(format!("Sled action-item save tx failed: {e:?}"))
            })?;
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
        items.sort_by_key(|b| std::cmp::Reverse(b.created_at));

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

    fn flush(&self) -> std::result::Result<(), GovernanceError> {
        // Force buffered action-item writes durable so the governance close
        // journal cannot be cleared while a materialized item is still un-fsynced.
        self.db
            .flush()
            .map(|_| ())
            .map_err(|e| GovernanceError::Internal(format!("Sled action-item flush failed: {e}")))
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
        out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
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
        out.sort_by_key(|a| a.start_date);
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
        out.sort_by_key(|a| a.start_date);
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
        out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
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
        out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
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
        out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
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
// Milestone Event Log
// ============================================================================

/// A single status-transition event recorded when `update_milestone_status` runs.
///
/// This is an app-layer (not core-crate) record. It lives in `manager.rs`
/// because it is produced and consumed entirely within the governance app's
/// persistence layer.
///
/// Fields are limited to what is truthfully known at the write site:
/// `milestone_id`, `changed_at`, `changed_by`, `from_status`, `to_status` are
/// always present because `update_milestone_status` has all of them. No
/// "reason" or "comment" field is added here; those require a Layer-2
/// call-site decision that the substrate does not currently model.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MilestoneEvent {
    pub milestone_id: MilestoneId,
    /// Unix seconds when the transition was recorded.
    pub changed_at: u64,
    /// DID of the actor who triggered the transition.
    pub changed_by: Did,
    /// Status before the transition.
    pub from_status: MilestoneStatus,
    /// Status after the transition.
    pub to_status: MilestoneStatus,
}

/// Append-only log of milestone status transitions.
///
/// `append` is called by `update_milestone_status` after a successful save.
/// `list_by_milestone` is called by the history endpoint to populate the
/// ordered entry list.
///
/// Ordering contract: entries returned by `list_by_milestone` must be
/// oldest-to-newest by `changed_at`.
pub trait MilestoneEventLogBackend: Send + Sync {
    fn append(&self, event: &MilestoneEvent) -> std::result::Result<(), GovernanceError>;
    fn list_by_milestone(
        &self,
        milestone_id: &MilestoneId,
    ) -> std::result::Result<Vec<MilestoneEvent>, GovernanceError>;
}

// ── In-memory implementation (for tests and standalone mode) ─────────────────

#[derive(Default)]
pub struct InMemoryMilestoneEventLog {
    events: RwLock<HashMap<MilestoneId, Vec<MilestoneEvent>>>,
}

impl InMemoryMilestoneEventLog {
    pub fn new() -> Self {
        Self::default()
    }
}

impl MilestoneEventLogBackend for InMemoryMilestoneEventLog {
    fn append(&self, event: &MilestoneEvent) -> std::result::Result<(), GovernanceError> {
        let mut guard = self
            .events
            .write()
            .map_err(|e| GovernanceError::Internal(format!("event log lock poisoned: {e}")))?;
        guard
            .entry(event.milestone_id.clone())
            .or_default()
            .push(event.clone());
        Ok(())
    }

    fn list_by_milestone(
        &self,
        milestone_id: &MilestoneId,
    ) -> std::result::Result<Vec<MilestoneEvent>, GovernanceError> {
        let guard = self
            .events
            .read()
            .map_err(|e| GovernanceError::Internal(format!("event log lock poisoned: {e}")))?;
        Ok(guard.get(milestone_id).cloned().unwrap_or_default())
    }
}

// ── Sled-backed implementation (for production) ───────────────────────────────

/// Sled-backed append-only log for milestone status transitions.
///
/// Key scheme:
///   `milestone_event:{milestone_id}:{changed_at:020}:{seq:020}:{uuid}`
///
/// The timestamp is zero-padded to 20 digits so lexicographic scan order
/// matches chronological order. A monotonically increasing per-process
/// sequence number (`seq`) is appended so two appends in the same second
/// retain insertion order regardless of UUID byte layout. The UUID suffix
/// ensures uniqueness across process restarts (where `seq` resets to 0).
///
/// Scan prefix: `milestone_event:{milestone_id}:` — yields all events for
/// a milestone, in chronological order.
///
/// **ID-boundary note**: `MilestoneId::from_raw` allows `:` inside the id,
/// so `scan_prefix` alone could return keys for `m1:child` when asked for
/// `m1`. `list_by_milestone` therefore also checks the decoded
/// `event.milestone_id` equals the requested id before returning.
pub struct SledMilestoneEventLog {
    db: Arc<sled::Db>,
    seq: std::sync::atomic::AtomicU64,
}

impl SledMilestoneEventLog {
    pub fn new(db: Arc<sled::Db>) -> Self {
        Self {
            db,
            seq: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn event_key(&self, milestone_id: &MilestoneId, changed_at: u64) -> String {
        let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!(
            "milestone_event:{}:{:020}:{:020}:{}",
            milestone_id.0,
            changed_at,
            seq,
            uuid::Uuid::new_v4()
        )
    }

    fn events_prefix(milestone_id: &MilestoneId) -> String {
        format!("milestone_event:{}:", milestone_id.0)
    }
}

impl MilestoneEventLogBackend for SledMilestoneEventLog {
    fn append(&self, event: &MilestoneEvent) -> std::result::Result<(), GovernanceError> {
        let key = self.event_key(&event.milestone_id, event.changed_at);
        let value = icn_encoding::encode_versioned(event).map_err(|e| {
            GovernanceError::Internal(format!("Failed to encode milestone event: {e}"))
        })?;
        self.db.insert(key.as_bytes(), value).map_err(|e| {
            GovernanceError::Internal(format!("Sled insert (event log) failed: {e}"))
        })?;
        Ok(())
    }

    fn list_by_milestone(
        &self,
        milestone_id: &MilestoneId,
    ) -> std::result::Result<Vec<MilestoneEvent>, GovernanceError> {
        let prefix = Self::events_prefix(milestone_id);
        // (changed_at, key_bytes) tuple preserves same-second insertion order
        // via the `seq` component encoded in the key.
        let mut rows: Vec<(u64, Vec<u8>, MilestoneEvent)> = Vec::new();
        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, value) = result.map_err(|e| {
                GovernanceError::Internal(format!("Sled scan (event log) failed: {e}"))
            })?;
            let event: MilestoneEvent = icn_encoding::decode_versioned(&value).map_err(|e| {
                GovernanceError::Internal(format!("Failed to decode milestone event: {e}"))
            })?;
            // Guard against `:` inside milestone_id producing prefix collisions
            // (e.g. `m1:child` matching a scan for `m1`).
            if &event.milestone_id != milestone_id {
                continue;
            }
            rows.push((event.changed_at, key.to_vec(), event));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        Ok(rows.into_iter().map(|(_, _, e)| e).collect())
    }
}

// ============================================================================
// Program Event Log
// ============================================================================

/// A single status-transition event recorded when `update_program_status` runs.
///
/// App-layer record only — lives in `manager.rs` because it is produced and
/// consumed entirely within the governance app's persistence layer.
///
/// Fields are limited to what is truthfully known at the write site:
/// `program_id`, `changed_at`, `changed_by`, `from_status`, `to_status` are
/// always present. No "reason" or "comment" field is added; those require a
/// Layer-2 call-site decision the substrate does not currently model.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProgramEvent {
    pub program_id: ProgramId,
    /// Unix seconds when the transition was recorded.
    pub changed_at: u64,
    /// DID of the actor who triggered the transition.
    pub changed_by: Did,
    /// Status before the transition.
    pub from_status: ProgramStatus,
    /// Status after the transition.
    pub to_status: ProgramStatus,
}

/// Append-only log of program status transitions.
///
/// `append` is called by `update_program_status` after a successful save.
/// `list_by_program` is called by the history endpoint (future slice) to
/// populate the ordered entry list.
///
/// Ordering contract: entries returned by `list_by_program` must be
/// oldest-to-newest by `changed_at`.
pub trait ProgramEventLogBackend: Send + Sync {
    fn append(&self, event: &ProgramEvent) -> std::result::Result<(), GovernanceError>;
    fn list_by_program(
        &self,
        program_id: &ProgramId,
    ) -> std::result::Result<Vec<ProgramEvent>, GovernanceError>;
}

// ── In-memory implementation (for tests and standalone mode) ─────────────────

#[derive(Default)]
pub struct InMemoryProgramEventLog {
    events: RwLock<HashMap<ProgramId, Vec<ProgramEvent>>>,
}

impl InMemoryProgramEventLog {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ProgramEventLogBackend for InMemoryProgramEventLog {
    fn append(&self, event: &ProgramEvent) -> std::result::Result<(), GovernanceError> {
        let mut guard = self.events.write().map_err(|e| {
            GovernanceError::Internal(format!("program event log lock poisoned: {e}"))
        })?;
        guard
            .entry(event.program_id.clone())
            .or_default()
            .push(event.clone());
        Ok(())
    }

    fn list_by_program(
        &self,
        program_id: &ProgramId,
    ) -> std::result::Result<Vec<ProgramEvent>, GovernanceError> {
        let guard = self.events.read().map_err(|e| {
            GovernanceError::Internal(format!("program event log lock poisoned: {e}"))
        })?;
        Ok(guard.get(program_id).cloned().unwrap_or_default())
    }
}

// ── Sled-backed implementation (for production) ───────────────────────────────

/// Sled-backed append-only log for program status transitions.
///
/// Key scheme:
///   `program_event:{program_id}:{changed_at:020}:{seq:020}:{uuid}`
///
/// The timestamp is zero-padded to 20 digits so lexicographic scan order
/// matches chronological order. A monotonically increasing per-process
/// sequence number (`seq`) is appended so two appends in the same second
/// retain insertion order. The UUID suffix ensures uniqueness across
/// process restarts (where `seq` resets to 0).
///
/// Scan prefix: `program_event:{program_id}:` — yields all events for a
/// program in chronological order.
///
/// **ID-boundary note**: `ProgramId::from_raw` allows `:` inside the id,
/// so `scan_prefix` alone could return keys for `p1:child` when asked for
/// `p1`. `list_by_program` therefore also checks the decoded
/// `event.program_id` equals the requested id before returning.
pub struct SledProgramEventLog {
    db: Arc<sled::Db>,
    seq: std::sync::atomic::AtomicU64,
}

impl SledProgramEventLog {
    pub fn new(db: Arc<sled::Db>) -> Self {
        Self {
            db,
            seq: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn event_key(&self, program_id: &ProgramId, changed_at: u64) -> String {
        let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!(
            "program_event:{}:{:020}:{:020}:{}",
            program_id.0,
            changed_at,
            seq,
            uuid::Uuid::new_v4()
        )
    }

    fn events_prefix(program_id: &ProgramId) -> String {
        format!("program_event:{}:", program_id.0)
    }
}

impl ProgramEventLogBackend for SledProgramEventLog {
    fn append(&self, event: &ProgramEvent) -> std::result::Result<(), GovernanceError> {
        let key = self.event_key(&event.program_id, event.changed_at);
        let value = icn_encoding::encode_versioned(event).map_err(|e| {
            GovernanceError::Internal(format!("Failed to encode program event: {e}"))
        })?;
        self.db.insert(key.as_bytes(), value).map_err(|e| {
            GovernanceError::Internal(format!("Sled insert (program event log) failed: {e}"))
        })?;
        Ok(())
    }

    fn list_by_program(
        &self,
        program_id: &ProgramId,
    ) -> std::result::Result<Vec<ProgramEvent>, GovernanceError> {
        let prefix = Self::events_prefix(program_id);
        let mut rows: Vec<(u64, Vec<u8>, ProgramEvent)> = Vec::new();
        for result in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, value) = result.map_err(|e| {
                GovernanceError::Internal(format!("Sled scan (program event log) failed: {e}"))
            })?;
            let event: ProgramEvent = icn_encoding::decode_versioned(&value).map_err(|e| {
                GovernanceError::Internal(format!("Failed to decode program event: {e}"))
            })?;
            if &event.program_id != program_id {
                continue;
            }
            rows.push((event.changed_at, key.to_vec(), event));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        Ok(rows.into_iter().map(|(_, _, e)| e).collect())
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
        out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
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
    /// Optional persistent store for domains (standalone mode only).
    ///
    /// When `Some`, standalone-mode domain writes are written through to
    /// this store and the in-memory `domains` map is seeded from it on
    /// construction. Unused in actor-backed mode (the actor owns its own
    /// store). Re-uses `SledGovernanceStateStore`'s `gov:domain:*` key
    /// space so a future migration to actor-backed mode reads the same
    /// records.
    domain_store: Option<Arc<dyn GovernanceStateStore>>,
    /// Optional receipt store for persisting GovernanceDecisionReceipts
    receipt_store: Option<Arc<dyn GovernanceReceiptBackend>>,
    /// Optional append-only log of milestone status transitions.
    ///
    /// `None` in tests and standalone mode (no log written, history falls back
    /// to lifecycle bookmarks). Set by sled-backed constructors so production
    /// deployments record every status change.
    milestone_event_log: Option<Arc<dyn MilestoneEventLogBackend>>,
    /// Optional append-only log of program status transitions.
    ///
    /// `None` in tests and standalone mode. Set by sled-backed constructors so
    /// production deployments record every status change.
    program_event_log: Option<Arc<dyn ProgramEventLogBackend>>,
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
            domain_store: None,
            receipt_store: None,
            milestone_event_log: None,
            program_event_log: None,
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
            domain_store: None,
            receipt_store: None,
            milestone_event_log: None,
            program_event_log: None,
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

    /// Attach an append-only milestone event log.
    ///
    /// When set, `update_milestone_status` appends a [`MilestoneEvent`] on
    /// every successful status transition. The history endpoint uses this log
    /// when available, falling back to lifecycle bookmarks otherwise.
    pub fn with_milestone_event_log(mut self, log: Arc<dyn MilestoneEventLogBackend>) -> Self {
        self.milestone_event_log = Some(log);
        self
    }

    /// Attach an append-only program event log.
    ///
    /// When set, `update_program_status` appends a [`ProgramEvent`] on every
    /// successful status transition.
    pub fn with_program_event_log(mut self, log: Arc<dyn ProgramEventLogBackend>) -> Self {
        self.program_event_log = Some(log);
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
            milestone_store: Arc::new(SledMilestoneStore::new(db.clone())),
            governance_handle: Some(handle),
            receipt_store: None,
            // Actor-backed mode: the actor owns its own state store. Standalone-mode
            // domain persistence is intentionally not wired here.
            domain_store: None,
            milestone_event_log: Some(Arc::new(SledMilestoneEventLog::new(db.clone()))),
            program_event_log: Some(Arc::new(SledProgramEventLog::new(db))),
        }
    }

    /// Create a standalone governance manager with Sled-backed action item storage
    ///
    /// Useful for testing persistence without a daemon connection.
    ///
    /// Governance domains are persisted in the same `gov:domain:*` key space
    /// the GovernanceActor uses (see `state_store::SledGovernanceStateStore`).
    /// Existing domains are loaded into the in-memory cache on construction so
    /// reads stay fast; writes are written through to the store. This makes
    /// NYCN-style bootstrap apply survive gateway restarts in standalone mode
    /// (issue #1600).
    pub fn new_with_sled(db: Arc<sled::Db>) -> Self {
        debug!("GovernanceManager created in standalone mode with Sled stores");

        let store: Arc<dyn icn_store::Store> = Arc::new(SledStore::from_db((*db).clone()));
        let domain_store: Arc<dyn GovernanceStateStore> =
            Arc::new(SledGovernanceStateStore::new(store));

        // Seed the in-memory cache from the persistent store. If load fails
        // (e.g. a corrupt entry) we log and continue with an empty cache —
        // create_domain() will surface conflicts on duplicate IDs once the
        // failing rows are repaired.
        let mut domains_map = HashMap::new();
        match domain_store.list_domains() {
            Ok(loaded) => {
                debug!(
                    "Loaded {} persisted governance domain(s) from store",
                    loaded.len()
                );
                for d in loaded {
                    domains_map.insert(d.id.clone(), d);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load persisted governance domains; starting empty: {e}");
            }
        }

        GovernanceManager {
            domains: RwLock::new(domains_map),
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
            milestone_store: Arc::new(SledMilestoneStore::new(db.clone())),
            governance_handle: None,
            domain_store: Some(domain_store),
            receipt_store: None,
            milestone_event_log: Some(Arc::new(SledMilestoneEventLog::new(db.clone()))),
            program_event_log: Some(Arc::new(SledProgramEventLog::new(db))),
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

        // Write-through to persistent store before mutating the in-memory cache,
        // so a store failure aborts the create cleanly without leaving an
        // unpersisted domain that disappears on restart.
        if let Some(store) = self.domain_store.as_ref() {
            store
                .save_domain(&domain)
                .map_err(|e| anyhow::anyhow!("Failed to persist domain '{}': {e}", domain_id.0))?;
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

        // Mirror the membership change into the persistent store so it
        // survives gateway restart in standalone mode.
        if let Some(store) = self.domain_store.as_ref() {
            store.save_domain(domain).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to persist membership update for domain '{}': {e}",
                    domain_id.0
                )
            })?;
        }
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
        // No authenticated request scope flows through this entry point —
        // emit no process-authorized v3 (#1868).
        self.close_proposal_inner(proposal_id, None, None)
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
        // No authenticated request scope flows through this entry point —
        // emit no process-authorized v3 (#1868).
        self.close_proposal_inner(proposal_id, Some(eligible_voters), None)
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
        capability_scope: Option<String>,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            return handle
                .close_proposal_with_suspension(
                    proposal_id,
                    eligible_voters,
                    excluded_delegators,
                    capability_scope,
                )
                .await;
        }
        // In-memory path: no delegation expansion, so suspension exclusion is a no-op.
        // Just apply the eligible_voters filter if present. The presented scope
        // (if any) flows through so the in-memory path emits the same
        // process-authorized v3 as the actor path (#1868).
        let scope = capability_scope.as_deref();
        match eligible_voters.as_ref() {
            Some(eligible) => self.close_proposal_inner(proposal_id, Some(eligible), scope),
            None => self.close_proposal_inner(proposal_id, None, scope),
        }
    }

    fn close_proposal_inner(
        &self,
        proposal_id: ProposalId,
        eligible_voters: Option<&HashSet<Did>>,
        // Capability scope the caller presented at close time, when the close
        // was driven by an authenticated request (#1868). `Some` only for the
        // scoped HTTP close path; `None` for unscoped entry points. Drives the
        // process-authorized v3 emission (evidence — never a constant).
        capability_scope: Option<&str>,
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

            let requires_execution_closure = matches!(outcome, ProofOutcome::Accepted)
                && proposal.payload.requires_execution_closure();
            if requires_execution_closure && self.receipt_store.is_none() {
                anyhow::bail!(
                    "Proposal '{}' requires execution closure but no receipt store is wired. \
                     Refusing to finalize Accepted without a traceable closure artifact.",
                    proposal_id.0
                );
            }

            if let Some(ref store) = self.receipt_store {
                // Cross-store close provenance, emitted through the SHARED
                // `CloseReceipts::apply` helper so the standalone manager and the
                // actor close path cannot drift. The manager keeps proposals
                // in-memory (the terminal flip below is `proposal.close(...)`, not
                // a durable `save_proposal`), so unlike the actor path there is no
                // durable cross-store phantom to recover from here — but the
                // receipt-emission order and the fail-closed v3 / fatal-when-
                // execution v1 / best-effort allocation split are exactly the ones
                // the actor's write-ahead journal replays (crate::close_journal).

                // #1868 v3 — scoped closes only (a capability scope was actually
                // presented). `::new` validation fails closed before any durable
                // write; the field is evidence, never a constant.
                let decision_v3 = if let Some(scope) = capability_scope {
                    let receipt_v3 = icn_governance::GovernanceDecisionReceiptV3::new(
                        proposal_id.0.clone(),
                        proposal.domain_id.0.clone(),
                        outcome,
                        tally.clone(),
                        &proposal_votes,
                        scope.to_string(),
                        icn_governance::ReceiptMandateAttestation::ProcessAuthorized,
                    )
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Invalid v3 decision receipt for proposal {}: {e}",
                            proposal_id.0
                        )
                    })?;
                    Some(crate::close_journal::DecisionV3Entry {
                        receipt: receipt_v3,
                        recorded_at: now,
                    })
                } else {
                    None
                };

                // v1 GovernanceDecisionReceipt — the decision record, and the
                // source of `decision_hash` for the ADR-0014 mandate below.
                let receipt = GovernanceDecisionReceipt::new(
                    proposal_id.0.clone(),
                    proposal.domain_id.0.clone(),
                    outcome,
                    tally.clone(),
                    &proposal_votes,
                );

                // governance→economics allocation receipt (INV-2: Allocation
                // Completeness), for accepted budget/treasury/allocation proposals.
                // Built here; the "required but not generated" gate is preserved
                // after emission below.
                let allocation_receipt = if matches!(outcome, ProofOutcome::Accepted) {
                    Self::create_allocation_receipt(
                        &proposal.payload,
                        receipt.decision_hash,
                        &proposal_id,
                        &proposal.domain_id,
                    )
                } else {
                    None
                };

                // SHARED emission: v3 → v1 → allocation. v3 is fail-closed; v1 is
                // fatal only under execution closure (PS-3 provenance chain);
                // allocation is best-effort (a missing allocation yields a
                // detectably incomplete chain, never a falsely-verified one).
                let close_receipts = crate::close_journal::CloseReceipts {
                    decision_v3,
                    governance_receipt: Some(receipt.clone()),
                    allocation_receipt,
                };
                close_receipts
                    .apply(store.as_ref(), requires_execution_closure)
                    .map_err(|e| anyhow::anyhow!(e))?;

                // ADR-0014 constitutional-memory seam.
                //
                // An Accepted decision produces a bounded institutional
                // authorization to carry out its effects — a `Mandate`,
                // optionally composed with narrow `AuthorityGrant`
                // records. Both are persisted through the shared
                // [`crate::grant_minting::mint_and_persist_for_accepted`]
                // helper, which is the canonical seam called by this
                // standalone path AND by the actor-backed close handler
                // so both paths produce the same constitutional-memory
                // artifact.
                //
                // This sits upstream of any `InstitutionalEffectRecord`
                // or `EffectDispatchEvidence` (which are evidence-side,
                // see `institutional_effect.rs`) and is intentionally
                // **behavior-neutral**: no executor gating, no
                // dispatcher wiring changes.
                if matches!(outcome, ProofOutcome::Accepted) {
                    match crate::grant_minting::mint_and_persist_for_accepted(
                        store.as_ref(),
                        &proposal_id.0,
                        &proposal.domain_id,
                        receipt.decision_hash,
                        &proposal.payload,
                        now,
                    ) {
                        Ok(crate::grant_minting::MandateMintOutcome::Minted {
                            mandate_id,
                            grants_persisted,
                        }) => {
                            tracing::debug!(
                                proposal_id = %proposal_id.0,
                                %mandate_id,
                                grants_persisted,
                                "Minted ADR-0014 mandate (standalone path)"
                            );
                        }
                        Ok(crate::grant_minting::MandateMintOutcome::AlreadyMinted {
                            mandate_id,
                        }) => {
                            tracing::debug!(
                                proposal_id = %proposal_id.0,
                                %mandate_id,
                                "ADR-0014 mandate already present; idempotent no-op"
                            );
                        }
                        Ok(crate::grant_minting::MandateMintOutcome::HashFailed) => {
                            tracing::error!(
                                proposal_id = %proposal_id.0,
                                "Failed to hash proposal payload for mandate payload_hash — \
                                 declining to mint mandate to avoid breaking content-binding invariant"
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                proposal_id = %proposal_id.0,
                                error = %e,
                                "Failed to persist ADR-0014 mandate — constitutional-memory record lost"
                            );
                        }
                    }
                }

                // Allocation-required closure-artifact gate (preserved): an
                // accepted payload that demands an allocation receipt but produced
                // none is a fail-closed error. The successful allocation write
                // itself already happened in `close_receipts.apply` above.
                if matches!(outcome, ProofOutcome::Accepted)
                    && close_receipts.allocation_receipt.is_none()
                    && payload_requires_allocation_receipt(&proposal.payload)
                {
                    anyhow::bail!(
                        "Proposal '{}' requires an allocation receipt closure artifact, but no receipt was generated.",
                        proposal_id.0
                    );
                }
            }

            proposal.close(final_state)?;

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
                            let is_economic_payload =
                                payload_requires_allocation_receipt(&p.payload);
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

    /// Canonical acceptance-effect emission for a just-accepted proposal.
    ///
    /// This is the single app-layer entry point that turns an accepted
    /// proposal's payload into a durable `InstitutionalEffectRecord`.
    /// Idempotent on `(proposal_id, effect_kind)` — a second call for the
    /// same pair returns `AlreadyEmitted` without writing.
    ///
    /// Used by the HTTP `close_proposal` handler for both standalone and
    /// actor-backed normal closes, by the actor's force-close accept branch
    /// (when a receipt store is wired into the actor), and by tests that
    /// need to stamp emission without going through HTTP.
    ///
    /// Returns `AcceptanceEmissionOutcome::NoEffect` when no receipt store
    /// is wired; callers treat that as "not durably recorded".
    pub fn apply_acceptance_effects(
        &self,
        proposal: &icn_governance::Proposal,
        decision_hash: Option<icn_kernel_api::receipts::Hash>,
        now: u64,
    ) -> Result<crate::institutional_effect::AcceptanceEmissionOutcome, anyhow::Error> {
        let Some(ref store) = self.receipt_store else {
            return Ok(crate::institutional_effect::AcceptanceEmissionOutcome::NoEffect);
        };
        crate::institutional_effect::emit_accepted_effect(
            store.as_ref(),
            &proposal.id.0,
            &proposal.domain_id.0,
            decision_hash,
            &proposal.payload,
            now,
        )
        .map_err(|e| anyhow::anyhow!("apply_acceptance_effects: {e}"))
    }

    /// Persist an institutional effect record via the attached receipt store.
    ///
    /// No-op when no receipt store is wired (callers should consider the
    /// effect "not durably recorded" in that case — the HTTP handler logs
    /// accordingly). Backend write failures are returned — the caller
    /// decides whether to escalate or degrade.
    pub fn record_institutional_effect(
        &self,
        record: &InstitutionalEffectRecord,
    ) -> Result<(), anyhow::Error> {
        let Some(ref store) = self.receipt_store else {
            return Ok(());
        };
        store
            .put_institutional_effect(record)
            .map_err(|e| anyhow::anyhow!("Failed to persist institutional effect record: {e}"))
    }

    /// Retrieve all institutional effect records emitted for a proposal,
    /// oldest-first (backend contract). Returns an empty list when no store
    /// is wired.
    pub fn list_institutional_effects(
        &self,
        proposal_id: &ProposalId,
    ) -> Result<Vec<InstitutionalEffectRecord>, anyhow::Error> {
        let Some(ref store) = self.receipt_store else {
            return Ok(vec![]);
        };
        store
            .list_institutional_effects_by_proposal(&proposal_id.0)
            .map_err(|e| anyhow::anyhow!("Failed to list institutional effect records: {e}"))
    }

    /// Persist downstream dispatch evidence attached to a previously
    /// emitted institutional effect record.
    ///
    /// No-op when no receipt store is wired. Called by the HTTP close
    /// handler after a downstream subsystem returns synchronously with
    /// structured success/error data — currently only the SDIS test-path
    /// `appoint_steward` / `revoke_steward` calls.
    pub fn record_dispatch_evidence(
        &self,
        evidence: &EffectDispatchEvidence,
    ) -> Result<(), anyhow::Error> {
        let Some(ref store) = self.receipt_store else {
            return Ok(());
        };
        store
            .put_effect_dispatch_evidence(evidence)
            .map_err(|e| anyhow::anyhow!("Failed to persist dispatch evidence: {e}"))
    }

    /// List dispatch evidence for an effect record, oldest-first. Empty
    /// list when no evidence or no store wired.
    pub fn list_dispatch_evidence(
        &self,
        effect_record_id: &str,
    ) -> Result<Vec<EffectDispatchEvidence>, anyhow::Error> {
        let Some(ref store) = self.receipt_store else {
            return Ok(vec![]);
        };
        store
            .list_effect_dispatch_evidence_by_record(effect_record_id)
            .map_err(|e| anyhow::anyhow!("Failed to list dispatch evidence: {e}"))
    }

    /// Assemble a proposal's deliberation trail.
    ///
    /// Returns the proposal's header fields together with every meeting in
    /// the proposal's domain whose agenda references this proposal — each
    /// meeting entry carries the per-agenda-item discussion notes, outcome,
    /// and generated action items. When the proposal has been closed, the
    /// governance decision receipt is included.
    ///
    /// `effect_kind` is a pure label derived from `proposal.payload` describing
    /// which `GovernanceEffect` variant this proposal would translate into on
    /// acceptance (matching the mapping in `http::handlers::close_proposal`).
    /// It reports shape only — not whether the effect was actually dispatched.
    ///
    /// This is a reverse read-model (proposal → meetings); no new state is
    /// written. Implementation scans `list_meetings(domain_id)` and filters
    /// matching agenda items — acceptable at per-domain meeting scale.
    pub async fn get_deliberation(
        &self,
        proposal_id: &ProposalId,
    ) -> Result<Option<ProposalDeliberation>> {
        let Some(proposal) = self.get_proposal(proposal_id).await? else {
            return Ok(None);
        };

        let meetings = self
            .meeting_store
            .list_by_domain(&proposal.domain_id.0)
            .map_err(|e| anyhow::anyhow!("Failed to list meetings: {e}"))?;

        let mut entries: Vec<DeliberationMeetingEntry> = Vec::new();
        for m in &meetings {
            for item in &m.agenda {
                if item
                    .linked_proposal
                    .as_ref()
                    .is_some_and(|pid| pid == proposal_id)
                {
                    entries.push(DeliberationMeetingEntry {
                        meeting_id: m.id.clone(),
                        meeting_title: m.title.clone(),
                        meeting_status: m.status,
                        scheduled_at: m.scheduled_at,
                        started_at: m.started_at,
                        ended_at: m.ended_at,
                        agenda_item_id: item.id.clone(),
                        agenda_item_title: item.title.clone(),
                        presenter: item.presenter.clone(),
                        discussion_notes: item.discussion_notes.clone(),
                        outcome: item.outcome.clone(),
                        generated_action_items: item.generated_action_items.clone(),
                    });
                }
            }
        }

        // Order deliberations chronologically by a single effective timestamp:
        // prefer `started_at` (when the meeting actually began), fall back to
        // `scheduled_at` (when it was planned). Meetings with neither sort
        // last (u64::MAX). A secondary key on the raw `scheduled_at` breaks
        // ties deterministically when two meetings share the same effective
        // timestamp.
        //
        // Using `started_at.or(scheduled_at)` (rather than a tuple with
        // `started_at` as the first slot) means that a meeting with only
        // `scheduled_at` populated is interleaved correctly against meetings
        // with `started_at`, instead of being pushed to the end.
        entries.sort_by_key(|e| {
            (
                e.started_at.or(e.scheduled_at).unwrap_or(u64::MAX),
                e.scheduled_at.unwrap_or(u64::MAX),
            )
        });

        let governance_receipt = if let Some(ref store) = self.receipt_store {
            match store.get_governance_by_proposal(&proposal_id.0) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(
                        proposal_id = %proposal_id.0,
                        error = %e,
                        "Receipt store error in get_deliberation",
                    );
                    None
                }
            }
        } else {
            None
        };

        let effect_kind = payload_effect_kind(&proposal.payload);
        let decided_at = proposal_decided_at(&proposal.state);

        // Include any emitted institutional effect records. Store errors are
        // downgraded to empty rather than failing the read — the deliberation
        // trail remains useful even if the effect index is unavailable.
        let records = self
            .list_institutional_effects(proposal_id)
            .unwrap_or_else(|e| {
                tracing::warn!(
                    proposal_id = %proposal_id.0,
                    error = %e,
                    "get_deliberation: effect index read failed; returning empty list",
                );
                vec![]
            });

        let emitted_effects: Vec<ReconciledEffectEntry> = records
            .into_iter()
            .map(|record| {
                let dispatch_evidence = self
                    .list_dispatch_evidence(&record.record_id)
                    .unwrap_or_else(|e| {
                        tracing::warn!(
                            record_id = %record.record_id,
                            error = %e,
                            "get_deliberation: evidence read failed; treating as empty",
                        );
                        vec![]
                    });
                let reconciliation_status =
                    derive_reconciliation_status(&record, &dispatch_evidence);
                ReconciledEffectEntry {
                    record,
                    dispatch_evidence,
                    reconciliation_status,
                }
            })
            .collect();

        Ok(Some(ProposalDeliberation {
            proposal_id: proposal.id.clone(),
            domain_id: proposal.domain_id.clone(),
            payload_type: proposal.payload.type_name(),
            state_label: proposal_state_label(&proposal.state),
            decided_at,
            effect_kind,
            deliberations: entries,
            governance_receipt,
            emitted_effects,
        }))
    }

    /// Create an AllocationReceipt from an accepted proposal's payload.
    ///
    /// Returns `None` for proposal types that don't produce economic effects
    /// (e.g., Text, Membership, ConfigChange).
    ///
    /// This is the governance→economics binding point (INV-2).
    /// Shared with the actor-backed close path (`actor.rs`) so the daemon
    /// normal-close path persists the same allocation/contribution receipt the
    /// in-process path does (Gap C parity). Uses no `&self` state — derivation
    /// is a pure function of the accepted payload + decision hash.
    pub(crate) fn create_allocation_receipt(
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
                            domain_id.0.clone(),
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

    /// Get the latest [`ActionItemCompletionReceipt`] for an action item id.
    ///
    /// Reads from the wired receipt store backend. Returns `Ok(None)`
    /// when no receipt has been emitted for the item, when the manager
    /// has no receipt store configured, or when the backend's query
    /// fails (failures are logged so the caller does not have to
    /// distinguish absence from query error).
    pub fn get_action_item_completion_by_item(
        &self,
        item_id: &str,
    ) -> Result<Option<icn_governance::ActionItemCompletionReceipt>> {
        if let Some(ref store) = self.receipt_store {
            match store.get_action_item_completion_by_item(item_id) {
                Ok(receipt) => return Ok(receipt),
                Err(e) => {
                    tracing::warn!(
                        item_id = %item_id,
                        error = %e,
                        "Failed to query receipt store for action item completion"
                    );
                }
            }
        }
        Ok(None)
    }

    /// List all [`ActionItemCompletionReceipt`]s ever persisted for an
    /// action item id, oldest-first by `completed_at`.
    ///
    /// Useful for surfaces that need the full reopen/re-complete chain
    /// rather than just the latest receipt.
    pub fn list_action_item_completions_by_item(
        &self,
        item_id: &str,
    ) -> Result<Vec<icn_governance::ActionItemCompletionReceipt>> {
        if let Some(ref store) = self.receipt_store {
            match store.list_action_item_completions_by_item(item_id) {
                Ok(receipts) => return Ok(receipts),
                Err(e) => {
                    tracing::warn!(
                        item_id = %item_id,
                        error = %e,
                        "Failed to list receipt-store action item completions"
                    );
                }
            }
        }
        Ok(Vec::new())
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

    /// Update the status of an action item.
    ///
    /// `capability_scope` is the capability scope that actually authorized the
    /// request (e.g. `"governance:meeting:write"`, or the legacy
    /// `"governance:write"` during the compatibility period). It is recorded
    /// verbatim into the emitted [`ActionItemCompletionReceiptV2`]'s
    /// `capability_scope_presented` field — it is evidence, so callers must
    /// pass the accepted scope, not a canonical preferred one.
    pub fn update_action_item_status(
        &self,
        domain_id: &GovernanceDomainId,
        id: &ActionItemId,
        status: ActionItemStatus,
        actor: &icn_identity::Did,
        capability_scope: &str,
    ) -> Result<ActionItem> {
        let mut item = self
            .action_items
            .get(domain_id, id)
            .map_err(|e| anyhow::anyhow!("Failed to get action item: {e}"))?
            .ok_or_else(|| anyhow::anyhow!("Action item not found: {id}"))?;

        let was_completed = matches!(item.status, ActionItemStatus::Completed);
        let now = icn_time::current_timestamp_secs();

        // On the first transition into Completed (was_completed: false →
        // status: Completed), persist the ADR-0026 Layer 2 receipt
        // BEFORE committing the status change. If the backend rejects
        // the receipt, the status save does not run — the holder's
        // standing never advertises a completion that has no provenance.
        // This makes `receipt_expected: true` honest under storage-fault
        // conditions; the alternative of "log and continue" can drop
        // receipts permanently because the `was_completed` guard skips
        // re-emission on subsequent re-saves.
        if matches!(status, ActionItemStatus::Completed) && !was_completed {
            if let Some(ref store) = self.receipt_store {
                let receipt = icn_governance::ActionItemCompletionReceipt::new(
                    item.id.to_string(),
                    item.domain_id.0.clone(),
                    actor.to_string(),
                    icn_governance::ActionItemTransition::Completed,
                    now,
                );
                store.put_action_item_completion(&receipt).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to persist action item completion receipt for {id}: {e}"
                    )
                })?;

                // #1868: emit the v2 receipt alongside v1. Action-item
                // completion is a membership-standing-only act (decomposition
                // §6 — governance:meeting:write, "no mandate beyond
                // membership-in-good-standing"), so the attestation is the
                // explicit `NoMandateRequired { MembershipStandingOnly }`
                // discriminator — no MandateGate call, no grant.
                // `capability_scope` is recorded verbatim as the scope that
                // authorized this request.
                let receipt_v2 = icn_governance::ActionItemCompletionReceiptV2::new(
                    item.id.to_string(),
                    item.domain_id.0.clone(),
                    actor.to_string(),
                    icn_governance::ActionItemTransition::Completed,
                    now,
                    capability_scope.to_string(),
                    icn_governance::ReceiptMandateAttestation::NoMandateRequired {
                        reason: icn_governance::NoMandateReason::MembershipStandingOnly,
                    },
                )
                .map_err(|e| {
                    anyhow::anyhow!("Invalid v2 action item completion receipt for {id}: {e}")
                })?;
                store
                    .put_action_item_completion_v2(&receipt_v2)
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to persist v2 action item completion receipt for {id}: {e}"
                        )
                    })?;
            }
        }

        item.status = status;
        item.updated_at = now;

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
        authority_scope: Vec<String>,
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
        let mut assignment = RoleAssignment::new(structure_id, person_did, role, now);
        assignment.authority_scope = authority_scope;
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
        self.create_activity_with_links(
            parent_entity_id,
            kind,
            name,
            description,
            start_date,
            end_date,
            Vec::new(),
            parent_program_id,
        )
    }

    /// Create a new activity with explicit linked structures.
    pub fn create_activity_with_links(
        &self,
        parent_entity_id: String,
        kind: ActivityKind,
        name: String,
        description: Option<String>,
        start_date: Option<u64>,
        end_date: Option<u64>,
        linked_structures: Vec<StructureId>,
        parent_program_id: Option<icn_governance::program::ProgramId>,
    ) -> Result<Activity> {
        // Validate date range when both are provided
        if let (Some(start), Some(end)) = (start_date, end_date) {
            if end < start {
                return Err(anyhow::anyhow!("Activity end_date must be >= start_date"));
            }
        }
        for structure_id in &linked_structures {
            if self
                .structure_store
                .get_structure(structure_id)
                .map_err(|e| anyhow::anyhow!("Failed to look up structure: {e}"))?
                .is_none()
            {
                return Err(anyhow::anyhow!(
                    "Linked structure not found: {}",
                    structure_id
                ));
            }
        }
        let now = icn_time::current_timestamp_secs();
        let id = ActivityId::generate();
        let mut a = Activity::new(id, parent_entity_id, kind, name, now);
        a.description = description;
        a.start_date = start_date;
        a.end_date = end_date;
        a.linked_structures = linked_structures;
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

    /// Mark an attendee's status on a meeting, emitting an
    /// [`icn_governance::MeetingAttendanceReceipt`] when the transition is
    /// receipt-bearing.
    ///
    /// Receipt-bearing transitions are exactly the attend-shaped ones:
    /// `Present` and `Remote`. `Absent` and `Invited` mutate state but
    /// emit no receipt — absence is not an attend event.
    ///
    /// Idempotence: re-marking an attendee with their current status is a
    /// no-op for the receipt seam (no fresh receipt is appended). Real
    /// transitions between distinct receipt-bearing states (e.g.
    /// `Present` → `Remote`) DO append a fresh receipt so the audit chain
    /// preserves the change.
    ///
    /// The receipt is persisted **before** the meeting state is committed
    /// — same fail-closed discipline as `update_action_item_status`. If
    /// the backend rejects the receipt, attendee state is not saved and
    /// the caller observes the error rather than a silent commit-without-
    /// receipt.
    ///
    /// `recorded_by` is the authenticated caller. It can differ from
    /// `attendee_did` (steward-recorded attendance) and is bound into the
    /// receipt's canonical hash.
    ///
    /// `capability_scope` is the capability scope that actually authorized the
    /// request (e.g. `"governance:meeting:write"`, or the legacy
    /// `"governance:write"` during the compatibility period). It is recorded
    /// verbatim into the emitted [`MeetingAttendanceReceiptV2`]'s
    /// `capability_scope_presented` field — it is evidence, so callers must
    /// pass the accepted scope, not a canonical preferred one.
    pub fn update_meeting_attendance(
        &self,
        meeting_id: &MeetingId,
        attendee_did: &str,
        status: AttendanceStatus,
        recorded_by: &Did,
        capability_scope: &str,
    ) -> Result<Meeting> {
        let mut m = self
            .meeting_store
            .get(meeting_id)
            .map_err(|e| anyhow::anyhow!("Failed to get meeting: {e}"))?
            .ok_or_else(|| anyhow::anyhow!("Meeting not found: {}", meeting_id.0))?;

        let attendee = m
            .attendees
            .iter_mut()
            .find(|a| a.did == attendee_did)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Attendee not found in meeting {}: {}",
                    meeting_id.0,
                    attendee_did
                )
            })?;

        let prior_status = attendee.status;
        let now = icn_time::current_timestamp_secs();

        // Receipt seam: emit only on a real transition into Present/Remote.
        // Same-status re-marks produce no fresh receipt; transitions
        // between Present and Remote DO produce a fresh receipt because
        // the documented attendance changed.
        let transition = match status {
            AttendanceStatus::Present if prior_status != AttendanceStatus::Present => {
                Some(MeetingAttendanceTransition::Present)
            }
            AttendanceStatus::Remote if prior_status != AttendanceStatus::Remote => {
                Some(MeetingAttendanceTransition::Remote)
            }
            _ => None,
        };

        if let Some(transition) = transition {
            if let Some(ref store) = self.receipt_store {
                let receipt = icn_governance::MeetingAttendanceReceipt::new(
                    m.id.0.clone(),
                    m.domain_id.clone(),
                    attendee_did.to_string(),
                    recorded_by.to_string(),
                    transition,
                    now,
                );
                store.put_meeting_attendance(&receipt).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to persist meeting attendance receipt for ({}, {}): {e}",
                        m.id.0,
                        attendee_did
                    )
                })?;

                // #1868: emit the v2 receipt alongside v1. Meeting attendance
                // is a membership-standing-only act (decomposition §6 / §10
                // step 9), so the attestation is the explicit
                // `NoMandateRequired { MembershipStandingOnly }` discriminator —
                // no MandateGate call, no grant. `capability_scope` is recorded
                // verbatim as the scope that authorized this request.
                let receipt_v2 = icn_governance::MeetingAttendanceReceiptV2::new(
                    m.id.0.clone(),
                    m.domain_id.clone(),
                    attendee_did.to_string(),
                    recorded_by.to_string(),
                    transition,
                    now,
                    capability_scope.to_string(),
                    icn_governance::ReceiptMandateAttestation::NoMandateRequired {
                        reason: icn_governance::NoMandateReason::MembershipStandingOnly,
                    },
                )
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Invalid v2 meeting attendance receipt for ({}, {}): {e}",
                        m.id.0,
                        attendee_did
                    )
                })?;
                store.put_meeting_attendance_v2(&receipt_v2).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to persist v2 meeting attendance receipt for ({}, {}): {e}",
                        m.id.0,
                        attendee_did
                    )
                })?;
            }
        }

        attendee.status = status;
        self.meeting_store
            .save(&m)
            .map_err(|e| anyhow::anyhow!("Failed to update meeting: {e}"))?;

        Ok(m)
    }

    /// Delete a meeting (hard delete).
    pub fn delete_meeting(&self, id: &MeetingId) -> Result<bool> {
        self.meeting_store
            .delete(id)
            .map_err(|e| anyhow::anyhow!("Failed to delete meeting: {e}"))
    }

    // ========================================================================
    // Process gate results (idea-0019 Institutional Process Substrate —
    // first runtime receipt-backed slice, ADR-0026 Layer 2)
    // ========================================================================

    /// Record the result of a single named process gate evaluation
    /// and emit a [`icn_governance::ProcessGateResultReceipt`].
    ///
    /// This is the first `ProcessTransitionReceipt` class the runtime
    /// emits for the `idea-0019` Institutional Process Substrate. The
    /// receipt is the institutional record of the gate result; the
    /// runtime does not (yet) model the surrounding `ProcessSession`
    /// as a stored object — `session_id` is treated opaquely so a
    /// process surface (read-model viewer, holder shell, future
    /// runtime session store) can reuse the same identifier.
    ///
    /// **Append-only.** A re-record of the same `(session_id,
    /// gate_kind)` at a strictly later `recorded_at` produces a
    /// distinct `record_hash` and a fresh receipt; prior receipts
    /// remain readable as the audit chain. A same-second re-record
    /// has the same `record_hash` and is therefore idempotent under
    /// the backend's append-only contract — the chain reads as a
    /// single receipt.
    ///
    /// **Persist-before-return.** The receipt is persisted to the
    /// backend before this method returns — same fail-closed
    /// discipline as `update_action_item_status` and
    /// `update_meeting_attendance`. If the backend rejects the
    /// receipt, this method returns the error and the caller observes
    /// the failure rather than a silent commit-without-receipt.
    /// When no receipt backend is configured, the receipt is still
    /// constructed and returned but **not persisted** — callers
    /// expecting durable provenance must wire a backend.
    ///
    /// **Domain scoping.** `domain_id` is bound into the receipt's
    /// canonical hash; a probe with a different `domain_id` produces
    /// a different `record_hash` even with identical other fields.
    /// The runtime does not perform an authorization check here —
    /// the caller is responsible for confirming that `recorded_by`
    /// is permitted to record gate results for `domain_id` /
    /// `session_id` per its institution charter. The receipt is the
    /// record of fact; charter enforcement is upstream.
    pub fn record_process_gate_result(
        &self,
        domain_id: &GovernanceDomainId,
        session_id: &str,
        gate_kind: icn_governance::ProcessGateKind,
        result: icn_governance::ProcessGateResult,
        recorded_by: &Did,
    ) -> Result<icn_governance::ProcessGateResultReceipt> {
        if session_id.is_empty() {
            return Err(anyhow::anyhow!(
                "record_process_gate_result: session_id must be non-empty"
            ));
        }
        let now = icn_time::current_timestamp_secs();
        let receipt = icn_governance::ProcessGateResultReceipt::new(
            session_id.to_string(),
            domain_id.0.clone(),
            gate_kind,
            result,
            recorded_by.to_string(),
            now,
        );

        if let Some(ref store) = self.receipt_store {
            store.put_process_gate_result(&receipt).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to persist process gate result receipt for session {session_id} kind {:?}: {e}",
                    gate_kind
                )
            })?;
        }

        Ok(receipt)
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

    /// Update a program's lifecycle status.
    ///
    /// Records a [`ProgramEvent`] when the status actually changes. Event-log
    /// failures are logged but do not fail the mutation — the program record is
    /// the source of truth.
    pub fn update_program_status(
        &self,
        id: &ProgramId,
        status: ProgramStatus,
        actor: &Did,
    ) -> Result<Program> {
        let mut p = self
            .program_store
            .get(id)
            .map_err(|e| anyhow::anyhow!("Failed to look up program: {e}"))?
            .ok_or_else(|| anyhow::anyhow!("Program not found: {id}"))?;

        let from_status = p.status;
        p.status = status;
        self.program_store
            .save(&p)
            .map_err(|e| anyhow::anyhow!("Failed to save program: {e}"))?;

        if from_status != status {
            if let Some(ref log) = self.program_event_log {
                let event = ProgramEvent {
                    program_id: id.clone(),
                    changed_at: icn_time::current_timestamp_secs(),
                    changed_by: actor.clone(),
                    from_status,
                    to_status: status,
                };
                if let Err(e) = log.append(&event) {
                    tracing::warn!(
                        program_id = %id,
                        error = %e,
                        "Failed to append program event (non-fatal)"
                    );
                }
            }
        }

        Ok(p)
    }

    /// Return all recorded status transitions for a program, oldest-to-newest.
    ///
    /// Returns an empty `Vec` when no event log is configured. Callers should
    /// treat an empty result as "no log available."
    pub fn list_program_events(&self, id: &ProgramId) -> Result<Vec<ProgramEvent>> {
        match &self.program_event_log {
            None => Ok(Vec::new()),
            Some(log) => log
                .list_by_program(id)
                .map_err(|e| anyhow::anyhow!("Failed to list program events: {e}")),
        }
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
    ///
    /// If a `milestone_event_log` is configured, a [`MilestoneEvent`] is
    /// appended after the successful save. Event-log failures are logged but
    /// do not cause the status update to fail — the milestone state is the
    /// source of truth; the log is a supplementary observability record.
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

        let from_status = m.status;

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

        // Append event-log entry when a log is configured.
        // Only append if the status actually changed to avoid spurious entries
        // (e.g. marking Completed again when already Completed).
        if from_status != status {
            if let Some(ref log) = self.milestone_event_log {
                let event = MilestoneEvent {
                    milestone_id: id.clone(),
                    changed_at: icn_time::current_timestamp_secs(),
                    changed_by: actor.clone(),
                    from_status,
                    to_status: status,
                };
                if let Err(e) = log.append(&event) {
                    tracing::warn!(
                        milestone_id = %id,
                        error = %e,
                        "Failed to append milestone event log entry (non-fatal)"
                    );
                }
            }
        }

        Ok(m)
    }

    /// List all recorded status-transition events for a milestone, oldest first.
    ///
    /// Returns an empty `Vec` when no event log is configured (in-memory /
    /// standalone mode without an explicit event log). Callers should treat an
    /// empty result as "no log available" and fall back to lifecycle bookmarks.
    pub fn list_milestone_events(&self, id: &MilestoneId) -> Result<Vec<MilestoneEvent>> {
        match &self.milestone_event_log {
            None => Ok(Vec::new()),
            Some(log) => log
                .list_by_milestone(id)
                .map_err(|e| anyhow::anyhow!("Failed to list milestone events: {e}")),
        }
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
        institutional_effects: std::sync::Mutex<Vec<InstitutionalEffectRecord>>,
        dispatch_evidence: std::sync::Mutex<Vec<EffectDispatchEvidence>>,
        mandates: std::sync::Mutex<Vec<icn_governance::Mandate>>,
        authority_grants: std::sync::Mutex<Vec<icn_governance::AuthorityGrant>>,
    }

    impl InMemoryReceiptBackend {
        fn new() -> Self {
            Self {
                governance: std::sync::Mutex::new(vec![]),
                allocations: std::sync::Mutex::new(vec![]),
                institutional_effects: std::sync::Mutex::new(vec![]),
                dispatch_evidence: std::sync::Mutex::new(vec![]),
                mandates: std::sync::Mutex::new(vec![]),
                authority_grants: std::sync::Mutex::new(vec![]),
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
        fn put_institutional_effect(
            &self,
            record: &InstitutionalEffectRecord,
        ) -> Result<(), String> {
            self.institutional_effects
                .lock()
                .unwrap()
                .push(record.clone());
            Ok(())
        }
        fn list_institutional_effects_by_proposal(
            &self,
            proposal_id: &str,
        ) -> Result<Vec<InstitutionalEffectRecord>, String> {
            let mut items: Vec<InstitutionalEffectRecord> = self
                .institutional_effects
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.proposal_id == proposal_id)
                .cloned()
                .collect();
            items.sort_by_key(|r| r.recorded_at);
            Ok(items)
        }
        fn put_effect_dispatch_evidence(
            &self,
            evidence: &EffectDispatchEvidence,
        ) -> Result<(), String> {
            self.dispatch_evidence
                .lock()
                .unwrap()
                .push(evidence.clone());
            Ok(())
        }
        fn list_effect_dispatch_evidence_by_record(
            &self,
            effect_record_id: &str,
        ) -> Result<Vec<EffectDispatchEvidence>, String> {
            let mut items: Vec<EffectDispatchEvidence> = self
                .dispatch_evidence
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.effect_record_id == effect_record_id)
                .cloned()
                .collect();
            items.sort_by_key(|e| e.recorded_at);
            Ok(items)
        }
        fn put_mandate(&self, mandate: &icn_governance::Mandate) -> Result<(), String> {
            self.mandates.lock().unwrap().push(mandate.clone());
            Ok(())
        }
        fn get_mandate_by_proposal(
            &self,
            proposal_id: &str,
        ) -> Result<Option<icn_governance::Mandate>, String> {
            Ok(self
                .mandates
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.decision.proposal_id == proposal_id)
                .cloned())
        }
        fn list_mandates_by_decision(
            &self,
            decision_hash: &icn_kernel_api::Hash,
        ) -> Result<Vec<icn_governance::Mandate>, String> {
            let mut items: Vec<icn_governance::Mandate> = self
                .mandates
                .lock()
                .unwrap()
                .iter()
                .filter(|m| m.decision.decision_hash == *decision_hash)
                .cloned()
                .collect();
            items.sort_by_key(|m| m.issued_at);
            Ok(items)
        }
        fn put_authority_grant(
            &self,
            grant: &icn_governance::AuthorityGrant,
        ) -> Result<(), String> {
            self.authority_grants.lock().unwrap().push(grant.clone());
            Ok(())
        }
        fn get_authority_grant(
            &self,
            grant_id: &icn_governance::AuthorityGrantId,
        ) -> Result<Option<icn_governance::AuthorityGrant>, String> {
            Ok(self
                .authority_grants
                .lock()
                .unwrap()
                .iter()
                .find(|g| g.id == *grant_id)
                .cloned())
        }
        fn list_authority_grants_by_decision(
            &self,
            decision_hash: &icn_kernel_api::Hash,
        ) -> Result<Vec<icn_governance::AuthorityGrant>, String> {
            let mut items: Vec<icn_governance::AuthorityGrant> = self
                .authority_grants
                .lock()
                .unwrap()
                .iter()
                .filter(|g| {
                    g.granted_by
                        .as_ref()
                        .is_some_and(|p| &p.decision_hash == decision_hash)
                })
                .cloned()
                .collect();
            items.sort_by_key(|g| g.valid_from);
            Ok(items)
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

    #[tokio::test]
    async fn declarative_text_proposal_can_close_without_receipt_store() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Adopt statement".to_string(),
                "Declarative text".to_string(),
                ProposalPayload::Text {
                    body: "We endorse this statement.".to_string(),
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

        let closed = mgr.get_proposal(&proposal_id).await.unwrap().unwrap();
        assert!(
            matches!(closed.state, ProposalState::Accepted { .. }),
            "declarative payload should remain closable without execution linkage backend"
        );
    }

    #[tokio::test]
    async fn execution_required_payload_rejects_accept_without_receipt_store() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let target = test_did(77);
        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Freeze member".to_string(),
                "Execution-required".to_string(),
                ProposalPayload::FreezeMember {
                    member: target,
                    reason: "policy breach".to_string(),
                    duration_seconds: Some(3600),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        mgr.open_proposal(proposal_id.clone(), 86400).await.unwrap();
        mgr.cast_vote(proposal_id.clone(), member_did, VoteChoice::For, None)
            .await
            .unwrap();

        let result = mgr.close_proposal(proposal_id.clone()).await;
        assert!(
            result
                .as_ref()
                .err()
                .is_some_and(|e| e.to_string().contains("requires execution closure")),
            "execution-required payload must not close Accepted without closure backend; got {result:?}"
        );
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
    // Deliberation trail tests (proposal → meetings reverse read-model)
    // ========================================================================

    /// Helper: add an agenda item linked to `proposal_id` onto `meeting_id`,
    /// with an optional outcome string. Goes through `update_meeting` so it
    /// exercises the same store path the HTTP handler uses.
    async fn push_linked_agenda_item(
        mgr: &GovernanceManager,
        meeting_id: &icn_governance::MeetingId,
        proposal_id: &ProposalId,
        title: &str,
        outcome: Option<&str>,
        notes: Option<&str>,
    ) {
        let mut m = mgr.get_meeting(meeting_id).unwrap().unwrap();
        let mut item = icn_governance::AgendaItem::new(title);
        item.linked_proposal = Some(proposal_id.clone());
        item.outcome = outcome.map(|s| s.to_string());
        item.discussion_notes = notes.map(|s| s.to_string());
        m.agenda.push(item);
        mgr.update_meeting(&m).unwrap();
    }

    #[tokio::test]
    async fn deliberation_none_when_proposal_missing() {
        let (mgr, _domain, _member) = make_manager_with_domain().await;
        let missing = ProposalId("prop-does-not-exist".to_string());
        let result = mgr.get_deliberation(&missing).await.unwrap();
        assert!(
            result.is_none(),
            "get_deliberation must return None for an unknown proposal_id"
        );
    }

    #[tokio::test]
    async fn deliberation_empty_when_no_meeting_references_proposal() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let prop_id = mgr
            .create_proposal(
                ProposalId("prop-no-meetings".to_string()),
                domain_id.clone(),
                member_did.clone(),
                "Standalone".to_string(),
                "No meeting ever touched this".to_string(),
                ProposalPayload::Text {
                    body: "body".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        // Create a meeting in the same domain whose agenda does NOT link this proposal.
        let m = mgr
            .create_meeting(
                domain_id.0.clone(),
                "Unrelated meeting".to_string(),
                None,
                None,
                member_did.to_string(),
            )
            .unwrap();
        // Add an agenda item linked to a different proposal id — should NOT match.
        let other = ProposalId("prop-other".to_string());
        push_linked_agenda_item(&mgr, &m.id, &other, "Unrelated item", None, None).await;

        let trail = mgr.get_deliberation(&prop_id).await.unwrap().unwrap();
        assert_eq!(trail.proposal_id, prop_id);
        assert_eq!(trail.domain_id, domain_id);
        assert!(trail.deliberations.is_empty());
        assert!(trail.governance_receipt.is_none());
        assert_eq!(trail.state_label, "draft");
        assert_eq!(trail.effect_kind, "unhandled"); // Text payload has no structured effect.
        assert!(trail.decided_at.is_none());
    }

    #[tokio::test]
    async fn deliberation_collects_linked_agenda_items_across_meetings() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let prop_id = mgr
            .create_proposal(
                ProposalId("prop-deliberated".to_string()),
                domain_id.clone(),
                member_did.clone(),
                "Multi-meeting".to_string(),
                "Discussed twice, decided once".to_string(),
                ProposalPayload::Text {
                    body: "body".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        // Meeting A scheduled earlier, tabled discussion.
        let mut m_a = mgr
            .create_meeting(
                domain_id.0.clone(),
                "March review".to_string(),
                None,
                Some(1_700_000_000),
                member_did.to_string(),
            )
            .unwrap();
        m_a.started_at = Some(1_700_000_100);
        mgr.update_meeting(&m_a).unwrap();
        push_linked_agenda_item(
            &mgr,
            &m_a.id,
            &prop_id,
            "Initial discussion",
            Some("tabled"),
            Some("Needs more research"),
        )
        .await;

        // Meeting B scheduled later, resolved.
        let mut m_b = mgr
            .create_meeting(
                domain_id.0.clone(),
                "April decision".to_string(),
                None,
                Some(1_700_500_000),
                member_did.to_string(),
            )
            .unwrap();
        m_b.started_at = Some(1_700_500_100);
        mgr.update_meeting(&m_b).unwrap();
        push_linked_agenda_item(&mgr, &m_b.id, &prop_id, "Vote", Some("resolved"), None).await;

        // A meeting in a different domain must not leak in.
        let other_domain = GovernanceDomainId::new("other-coop");
        mgr.create_domain(
            other_domain.clone(),
            "Other Coop".to_string(),
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
        let m_other = mgr
            .create_meeting(
                other_domain.0.clone(),
                "Other domain".to_string(),
                None,
                Some(1_700_250_000),
                member_did.to_string(),
            )
            .unwrap();
        push_linked_agenda_item(&mgr, &m_other.id, &prop_id, "Cross-domain", None, None).await;

        let trail = mgr.get_deliberation(&prop_id).await.unwrap().unwrap();

        assert_eq!(
            trail.deliberations.len(),
            2,
            "only same-domain meetings linking the proposal should appear",
        );
        // Chronological order by started_at (earliest first).
        assert_eq!(trail.deliberations[0].meeting_title, "March review");
        assert_eq!(trail.deliberations[0].outcome.as_deref(), Some("tabled"));
        assert_eq!(
            trail.deliberations[0].discussion_notes.as_deref(),
            Some("Needs more research"),
        );
        assert_eq!(trail.deliberations[1].meeting_title, "April decision");
        assert_eq!(trail.deliberations[1].outcome.as_deref(), Some("resolved"));
    }

    #[tokio::test]
    async fn deliberation_includes_decision_receipt_after_close() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend);

        let prop_id = mgr
            .create_proposal(
                ProposalId("prop-closed".to_string()),
                domain_id.clone(),
                member_did.clone(),
                "Decided text".to_string(),
                "Will be accepted".to_string(),
                ProposalPayload::Text {
                    body: "body".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        let m = mgr
            .create_meeting(
                domain_id.0.clone(),
                "Decision meeting".to_string(),
                None,
                Some(1_700_000_000),
                member_did.to_string(),
            )
            .unwrap();
        push_linked_agenda_item(&mgr, &m.id, &prop_id, "Vote", Some("resolved"), None).await;

        mgr.open_proposal(prop_id.clone(), 86400).await.unwrap();
        mgr.cast_vote(prop_id.clone(), member_did.clone(), VoteChoice::For, None)
            .await
            .unwrap();
        mgr.close_proposal(prop_id.clone()).await.unwrap();

        let trail = mgr.get_deliberation(&prop_id).await.unwrap().unwrap();
        assert_eq!(trail.state_label, "accepted");
        assert!(trail.decided_at.is_some());
        assert!(
            trail.governance_receipt.is_some(),
            "closed proposal must carry its decision receipt in the deliberation trail",
        );
        assert_eq!(trail.deliberations.len(), 1);
    }

    // ========================================================================
    // Institutional effect record persistence (acceptance-time artifact)
    // ========================================================================

    #[tokio::test]
    async fn institutional_effects_empty_without_receipt_store() {
        let (mgr, _domain, _member) = make_manager_with_domain().await;
        // No receipt store wired: list returns empty, write is a no-op Ok.
        let pid = ProposalId("prop-none".to_string());
        let list = mgr.list_institutional_effects(&pid).unwrap();
        assert!(list.is_empty());

        let rec = InstitutionalEffectRecord::new(
            "prop-none",
            "test-coop",
            None,
            "freeze_member",
            None,
            None,
            None,
            1,
            serde_json::json!({}),
        );
        assert!(mgr.record_institutional_effect(&rec).is_ok());
        // Still empty — no store to read from.
        let list = mgr.list_institutional_effects(&pid).unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn institutional_effects_roundtrip_and_ordering() {
        let (mgr, _domain, _member) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend);

        let pid = ProposalId("prop-rt".to_string());

        let older = InstitutionalEffectRecord::new(
            pid.0.clone(),
            "test-coop",
            None,
            "freeze_member",
            Some("did:icn:a".into()),
            None,
            Some("cause".into()),
            100,
            serde_json::json!({"n": 1}),
        );
        let newer = InstitutionalEffectRecord::new(
            pid.0.clone(),
            "test-coop",
            None,
            "unfreeze_member",
            Some("did:icn:a".into()),
            None,
            Some("resolved".into()),
            200,
            serde_json::json!({"n": 2}),
        );

        // Write out-of-order to prove ordering is by recorded_at, not insert order.
        mgr.record_institutional_effect(&newer).unwrap();
        mgr.record_institutional_effect(&older).unwrap();

        let list = mgr.list_institutional_effects(&pid).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].effect_kind, "freeze_member");
        assert_eq!(list[1].effect_kind, "unfreeze_member");
        assert!(list[0].recorded_at < list[1].recorded_at);

        // Unrelated proposal_id returns empty, not the above.
        let empty = mgr
            .list_institutional_effects(&ProposalId("prop-other".into()))
            .unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn deliberation_surfaces_emitted_effects() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend);

        let target = test_did(9);
        let prop_id = mgr
            .create_proposal(
                ProposalId("prop-freeze-surface".into()),
                domain_id.clone(),
                member_did.clone(),
                "Freeze".into(),
                "Reason".into(),
                ProposalPayload::FreezeMember {
                    member: target.clone(),
                    reason: "audit".into(),
                    duration_seconds: None,
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        // Simulate the HTTP handler's post-accept persistence by writing the
        // record through the manager directly (the HTTP handler drives the
        // same code path on real accept).
        let rec = crate::institutional_effect::record_from_accepted_payload(
            &prop_id.0,
            &domain_id.0,
            None,
            &ProposalPayload::FreezeMember {
                member: target.clone(),
                reason: "audit".into(),
                duration_seconds: None,
            },
            42,
        )
        .expect("FreezeMember must translate to a record");
        mgr.record_institutional_effect(&rec).unwrap();

        let trail = mgr.get_deliberation(&prop_id).await.unwrap().unwrap();
        assert_eq!(trail.emitted_effects.len(), 1);
        let surfaced = &trail.emitted_effects[0];
        assert_eq!(surfaced.record.effect_kind, "freeze_member");
        assert_eq!(
            surfaced.record.target_did.as_deref(),
            Some(target.to_string().as_str())
        );
        assert_eq!(surfaced.record.reason.as_deref(), Some("audit"));
        // No dispatch evidence wired for freeze_member → emitted_only.
        assert!(surfaced.dispatch_evidence.is_empty());
        assert_eq!(
            surfaced.reconciliation_status,
            ReconciliationStatus::EmittedOnly
        );
    }

    // ========================================================================
    // Dispatch evidence + reconciliation status (governance → execution bridge)
    // ========================================================================

    #[tokio::test]
    async fn dispatch_evidence_roundtrip_and_ordering_per_record() {
        let (mgr, _domain, _member) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend);

        let older = EffectDispatchEvidence::new(
            "rec-a",
            "prop-1",
            "sdis",
            Some("state-hash-1".into()),
            true,
            None,
            None,
            100,
        );
        let newer = EffectDispatchEvidence::new(
            "rec-a",
            "prop-1",
            "sdis",
            Some("state-hash-2".into()),
            true,
            None,
            None,
            200,
        );
        // Unrelated record — must not leak into rec-a's list.
        let other_record =
            EffectDispatchEvidence::new("rec-b", "prop-2", "sdis", None, true, None, None, 150);

        mgr.record_dispatch_evidence(&newer).unwrap();
        mgr.record_dispatch_evidence(&older).unwrap();
        mgr.record_dispatch_evidence(&other_record).unwrap();

        let list = mgr.list_dispatch_evidence("rec-a").unwrap();
        assert_eq!(list.len(), 2, "must scope by effect_record_id");
        assert_eq!(list[0].recorded_at, 100, "oldest first");
        assert_eq!(list[1].recorded_at, 200);
        assert_eq!(list[0].receipt_ref.as_deref(), Some("state-hash-1"));
    }

    #[tokio::test]
    async fn deliberation_surfaces_execution_evidenced_when_success_recorded() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend);

        let target = test_did(8);
        let prop_id = mgr
            .create_proposal(
                ProposalId("prop-evid".into()),
                domain_id.clone(),
                member_did.clone(),
                "Freeze".into(),
                "r".into(),
                ProposalPayload::FreezeMember {
                    member: target.clone(),
                    reason: "audit".into(),
                    duration_seconds: None,
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        let rec = crate::institutional_effect::record_from_accepted_payload(
            &prop_id.0,
            &domain_id.0,
            None,
            &ProposalPayload::FreezeMember {
                member: target.clone(),
                reason: "audit".into(),
                duration_seconds: None,
            },
            10,
        )
        .expect("must produce record");
        let rec_id = rec.record_id.clone();
        mgr.record_institutional_effect(&rec).unwrap();

        mgr.record_dispatch_evidence(&EffectDispatchEvidence::new(
            rec_id.clone(),
            prop_id.0.clone(),
            "commons",
            Some("commons-receipt-abc".into()),
            true,
            None,
            None,
            20,
        ))
        .unwrap();

        let trail = mgr.get_deliberation(&prop_id).await.unwrap().unwrap();
        assert_eq!(trail.emitted_effects.len(), 1);
        let e = &trail.emitted_effects[0];
        assert_eq!(e.dispatch_evidence.len(), 1);
        assert_eq!(e.dispatch_evidence[0].subsystem, "commons");
        assert_eq!(
            e.reconciliation_status,
            ReconciliationStatus::ExecutionEvidenced
        );
    }

    #[tokio::test]
    async fn deliberation_surfaces_execution_failed_with_subsystem_error() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend);

        let target = test_did(11);
        let prop_id = mgr
            .create_proposal(
                ProposalId("prop-evid-fail".into()),
                domain_id.clone(),
                member_did.clone(),
                "Freeze".into(),
                "r".into(),
                ProposalPayload::FreezeMember {
                    member: target.clone(),
                    reason: "audit".into(),
                    duration_seconds: None,
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        let rec = crate::institutional_effect::record_from_accepted_payload(
            &prop_id.0,
            &domain_id.0,
            None,
            &ProposalPayload::FreezeMember {
                member: target.clone(),
                reason: "audit".into(),
                duration_seconds: None,
            },
            10,
        )
        .unwrap();
        let rec_id = rec.record_id.clone();
        mgr.record_institutional_effect(&rec).unwrap();

        // Subsystem reports failure, then a later success. Audit-discipline:
        // failure should stick and the most recent failure message surfaces.
        mgr.record_dispatch_evidence(&EffectDispatchEvidence::new(
            rec_id.clone(),
            prop_id.0.clone(),
            "commons",
            None,
            false,
            Some("member not found".into()),
            None,
            20,
        ))
        .unwrap();
        mgr.record_dispatch_evidence(&EffectDispatchEvidence::new(
            rec_id.clone(),
            prop_id.0.clone(),
            "commons",
            Some("later-hash".into()),
            true,
            None,
            None,
            30,
        ))
        .unwrap();

        let trail = mgr.get_deliberation(&prop_id).await.unwrap().unwrap();
        let e = &trail.emitted_effects[0];
        assert_eq!(e.dispatch_evidence.len(), 2);
        match &e.reconciliation_status {
            ReconciliationStatus::ExecutionFailed { error } => {
                assert_eq!(error.as_deref(), Some("member not found"));
            }
            other => panic!("expected ExecutionFailed, got {other:?}"),
        }
    }

    // ========================================================================
    // Canonical acceptance-pipeline tests
    //
    // These prove that `apply_acceptance_effects` is the one authoritative
    // emission path: it is idempotent on (proposal_id, effect_kind), works
    // regardless of which call site invoked it, and is consistent with
    // direct writes through `record_institutional_effect`. The goal is
    // that force-accept (actor path) and normal-accept (HTTP path) produce
    // the same durable artifacts.
    // ========================================================================

    use crate::institutional_effect::AcceptanceEmissionOutcome;

    async fn make_freeze_proposal(
        mgr: &GovernanceManager,
        domain_id: &GovernanceDomainId,
        proposer: &Did,
        target: &Did,
        id: &str,
    ) -> icn_governance::Proposal {
        let pid = mgr
            .create_proposal(
                ProposalId(id.to_string()),
                domain_id.clone(),
                proposer.clone(),
                "Freeze".into(),
                "reason".into(),
                ProposalPayload::FreezeMember {
                    member: target.clone(),
                    reason: "audit".into(),
                    duration_seconds: None,
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();
        mgr.get_proposal(&pid).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn apply_acceptance_effects_noop_without_receipt_store() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let target = test_did(42);
        let proposal = make_freeze_proposal(&mgr, &domain_id, &member_did, &target, "p-noop").await;

        let outcome = mgr
            .apply_acceptance_effects(&proposal, None, 100)
            .expect("apply_acceptance_effects must not error without a store");
        assert_eq!(
            outcome,
            AcceptanceEmissionOutcome::NoEffect,
            "with no receipt store wired, emission is NoEffect (and a no-op)"
        );
    }

    #[tokio::test]
    async fn apply_acceptance_effects_emits_freeze_record_once() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend);

        let target = test_did(42);
        let proposal = make_freeze_proposal(&mgr, &domain_id, &member_did, &target, "p-once").await;

        let first = mgr
            .apply_acceptance_effects(&proposal, Some([9u8; 32]), 100)
            .unwrap();
        let first_id = match first {
            AcceptanceEmissionOutcome::Emitted { record_id } => record_id,
            other => panic!("expected Emitted, got {other:?}"),
        };

        // Idempotence: second call on the same (proposal_id, effect_kind) is
        // AlreadyEmitted with the same record_id and does not write again.
        let second = mgr
            .apply_acceptance_effects(&proposal, Some([9u8; 32]), 200)
            .unwrap();
        match second {
            AcceptanceEmissionOutcome::AlreadyEmitted { record_id } => {
                assert_eq!(
                    record_id, first_id,
                    "idempotent call must return same record_id"
                );
            }
            other => panic!("expected AlreadyEmitted, got {other:?}"),
        }

        // Exactly one record persisted.
        let records = mgr.list_institutional_effects(&proposal.id).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].effect_kind, "freeze_member");
        assert_eq!(records[0].record_id, first_id);
        // recorded_at is the first call's timestamp — second call did not overwrite.
        assert_eq!(records[0].recorded_at, 100);
    }

    #[tokio::test]
    async fn apply_acceptance_effects_returns_no_effect_for_unhandled_payload() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend);

        let pid = mgr
            .create_proposal(
                ProposalId("p-text".into()),
                domain_id.clone(),
                member_did.clone(),
                "Text".into(),
                "body".into(),
                ProposalPayload::Text { body: "hi".into() },
                ProposalScope::Local,
            )
            .await
            .unwrap();
        let proposal = mgr.get_proposal(&pid).await.unwrap().unwrap();

        let outcome = mgr.apply_acceptance_effects(&proposal, None, 100).unwrap();
        assert_eq!(outcome, AcceptanceEmissionOutcome::NoEffect);
        assert!(mgr.list_institutional_effects(&pid).unwrap().is_empty());
    }

    #[tokio::test]
    async fn apply_acceptance_effects_dedups_across_http_and_actor_callers() {
        // Simulates the HTTP+actor double-invocation pattern: an actor-backed
        // normal close emits first (actor call site), then the HTTP handler
        // returns and calls emission again (HTTP call site). The second call
        // MUST be AlreadyEmitted, not a second record. This is the property
        // that makes the unified canonical path safe to enable on both sites.
        let (mgr, domain_id, proposer) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend);
        let target = test_did(55);
        let p = make_freeze_proposal(&mgr, &domain_id, &proposer, &target, "p-dedup").await;

        // Actor call site: uses the gate-receipt decision hash.
        let first = mgr
            .apply_acceptance_effects(&p, Some([3u8; 32]), 1000)
            .unwrap();
        let first_id = first.record_id().unwrap().to_string();

        // HTTP call site: uses the governance_receipt.decision_hash (same
        // hash in production since both compute GovernanceDecisionReceipt
        // from the same inputs). The dedup key is (proposal_id, effect_kind)
        // so AlreadyEmitted even if decision_hash differed.
        let second = mgr
            .apply_acceptance_effects(&p, Some([3u8; 32]), 2000)
            .unwrap();
        assert!(
            matches!(second, AcceptanceEmissionOutcome::AlreadyEmitted { .. }),
            "HTTP-after-actor invocation must dedup to AlreadyEmitted, got {second:?}"
        );
        assert_eq!(second.record_id(), Some(first_id.as_str()));
        let records = mgr.list_institutional_effects(&p.id).unwrap();
        assert_eq!(records.len(), 1, "exactly one record after both callers");
    }

    #[tokio::test]
    async fn apply_acceptance_effects_emits_same_record_semantics_regardless_of_caller() {
        // Proves path-agnostic semantics: whether the caller is the HTTP
        // handler (normal accept) or the actor's force-close branch, the
        // resulting InstitutionalEffectRecord has identical effect_kind,
        // target_did, reason, and payload shape for the same payload.
        let (mgr, domain_id, proposer) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend);

        let target = test_did(77);
        let p_normal = make_freeze_proposal(&mgr, &domain_id, &proposer, &target, "p-normal").await;
        let p_forced = make_freeze_proposal(&mgr, &domain_id, &proposer, &target, "p-forced").await;

        // Simulate the HTTP normal-accept caller.
        mgr.apply_acceptance_effects(&p_normal, Some([1u8; 32]), 1000)
            .unwrap();
        // Simulate the actor force-accept caller (distinct decision_hash because
        // the force-close constructs VoteTally::empty()).
        mgr.apply_acceptance_effects(&p_forced, Some([2u8; 32]), 2000)
            .unwrap();

        let r_normal = &mgr.list_institutional_effects(&p_normal.id).unwrap()[0];
        let r_forced = &mgr.list_institutional_effects(&p_forced.id).unwrap()[0];

        assert_eq!(r_normal.effect_kind, r_forced.effect_kind);
        assert_eq!(r_normal.target_did, r_forced.target_did);
        assert_eq!(r_normal.reason, r_forced.reason);
        // Payload JSON shape must match exactly (duration_seconds, reason, member).
        assert_eq!(r_normal.payload, r_forced.payload);
        // decision_hash distinguishes the two receipts (expected and honest).
        assert_ne!(r_normal.decision_hash, r_forced.decision_hash);
    }

    /// Regression guard for the deliberation sort key.
    ///
    /// Prior sort key `(started_at.unwrap_or(MAX), scheduled_at.unwrap_or(MAX))`
    /// always pushed entries without `started_at` to the end, even when their
    /// `scheduled_at` placed them earlier in the timeline. The corrected key
    /// uses `started_at.or(scheduled_at)` for the primary slot so a meeting
    /// that was scheduled but never started is interleaved correctly with
    /// meetings that did start.
    #[test]
    fn deliberation_sort_interleaves_scheduled_only_entries() {
        use icn_governance::{AgendaItemId, MeetingId, MeetingStatus};

        fn entry(
            id: &str,
            started_at: Option<u64>,
            scheduled_at: Option<u64>,
        ) -> DeliberationMeetingEntry {
            DeliberationMeetingEntry {
                meeting_id: MeetingId(id.to_string()),
                meeting_title: id.to_string(),
                meeting_status: MeetingStatus::Scheduled,
                scheduled_at,
                started_at,
                ended_at: None,
                agenda_item_id: AgendaItemId(uuid::Uuid::nil()),
                agenda_item_title: String::new(),
                presenter: None,
                discussion_notes: None,
                outcome: None,
                generated_action_items: vec![],
            }
        }

        // Three meetings:
        //   A: started_at = 2000
        //   B: scheduled_at = 1000, no started_at
        //   C: started_at = 3000
        // By effective-timestamp ordering the sequence must be B (1000),
        // A (2000), C (3000). The old key would have produced A, C, B —
        // pushing B to the end because its `started_at` was None.
        let mut xs = [
            entry("A", Some(2000), None),
            entry("B", None, Some(1000)),
            entry("C", Some(3000), None),
        ];
        xs.sort_by_key(|e| {
            (
                e.started_at.or(e.scheduled_at).unwrap_or(u64::MAX),
                e.scheduled_at.unwrap_or(u64::MAX),
            )
        });
        let ids: Vec<&str> = xs.iter().map(|e| e.meeting_title.as_str()).collect();
        assert_eq!(ids, vec!["B", "A", "C"]);
    }

    #[tokio::test]
    async fn deliberation_effect_kind_labels_freeze_member() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let target = test_did(42);
        let prop_id = mgr
            .create_proposal(
                ProposalId("prop-freeze".to_string()),
                domain_id.clone(),
                member_did.clone(),
                "Freeze".to_string(),
                "Reason".to_string(),
                ProposalPayload::FreezeMember {
                    member: target,
                    reason: "cause".to_string(),
                    duration_seconds: None,
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        let trail = mgr.get_deliberation(&prop_id).await.unwrap().unwrap();
        assert_eq!(trail.effect_kind, "freeze_member");
        assert_eq!(trail.payload_type, "freeze_member");
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

    #[tokio::test]
    async fn create_activity_with_linked_structures_persists_links() {
        let (mgr, _domain_id, _member) = make_manager_with_domain().await;
        let structure = mgr
            .create_structure(
                "ent-1".to_string(),
                StructureKind::Committee,
                "Content".to_string(),
                None,
            )
            .unwrap();

        let act = mgr
            .create_activity_with_links(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "Linked Activity".to_string(),
                None,
                None,
                None,
                vec![structure.id.clone()],
                None,
            )
            .unwrap();

        assert_eq!(act.linked_structures, vec![structure.id]);
    }

    #[tokio::test]
    async fn create_activity_with_missing_linked_structure_fails() {
        let (mgr, _domain_id, _member) = make_manager_with_domain().await;
        let missing_structure = StructureId::generate();
        let err = mgr
            .create_activity_with_links(
                "ent-1".to_string(),
                icn_governance::ActivityKind::Event,
                "Broken Linked Activity".to_string(),
                None,
                None,
                None,
                vec![missing_structure.clone()],
                None,
            )
            .unwrap_err();
        assert!(err
            .to_string()
            .contains(&format!("Linked structure not found: {missing_structure}")));
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

    // ============================================================================
    // ADR-0014: Mandate recording at the decision-acceptance seam
    // ============================================================================

    /// Accepted proposal mints a pending-grants `Mandate` bound to the
    /// decision provenance and payload content, recorded distinctly from
    /// the governance receipt.
    #[tokio::test]
    async fn adr0014_mandate_minted_on_accepted_proposal() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend.clone());

        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Adopt policy".to_string(),
                "A non-economic policy decision".to_string(),
                ProposalPayload::Text {
                    body: "Adopt the house rules.".to_string(),
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

        // Governance receipt exists.
        let gov_receipt = backend
            .get_governance_by_proposal(&proposal_id.0)
            .unwrap()
            .expect("governance receipt must exist");
        assert_eq!(gov_receipt.outcome, ProofOutcome::Accepted);

        // Mandate exists and binds the same decision provenance.
        let mandate = backend
            .get_mandate_by_proposal(&proposal_id.0)
            .unwrap()
            .expect("mandate must be recorded on Accepted path");
        assert_eq!(mandate.decision.proposal_id, proposal_id.0);
        assert_eq!(mandate.decision.decision_hash, gov_receipt.decision_hash);
        // Bootstrap phase: no typed grants attached yet.
        assert!(
            mandate.has_no_grants(),
            "bootstrap-phase mandate carries no attached grants"
        );
        assert_eq!(mandate.status, icn_governance::MandateStatus::Pending);
        // payload_hash is bound to the payload content (non-zero).
        assert_ne!(
            mandate.payload_hash, [0u8; 32],
            "payload_hash must be bound to actual payload serialization"
        );

        // Indexing by decision_hash returns the same mandate.
        let by_decision = backend
            .list_mandates_by_decision(&gov_receipt.decision_hash)
            .unwrap();
        assert_eq!(by_decision.len(), 1);
        assert_eq!(by_decision[0].id, mandate.id);
    }

    /// Rejected proposal must not mint a mandate — a mandate is
    /// *authorization*, which only arises from acceptance.
    #[tokio::test]
    async fn adr0014_no_mandate_on_rejected_proposal() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend.clone());

        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Reject this".to_string(),
                "Expected rejection".to_string(),
                ProposalPayload::Text {
                    body: "A thing that will be voted down.".to_string(),
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        mgr.open_proposal(proposal_id.clone(), 86400).await.unwrap();
        mgr.cast_vote(
            proposal_id.clone(),
            member_did.clone(),
            VoteChoice::Against,
            None,
        )
        .await
        .unwrap();
        mgr.close_proposal(proposal_id.clone()).await.unwrap();

        let gov_receipt = backend
            .get_governance_by_proposal(&proposal_id.0)
            .unwrap()
            .expect("governance receipt must exist");
        assert_eq!(gov_receipt.outcome, ProofOutcome::Rejected);

        let mandate = backend.get_mandate_by_proposal(&proposal_id.0).unwrap();
        assert!(
            mandate.is_none(),
            "Rejected proposal must not mint a mandate — authorization requires acceptance"
        );
    }

    /// The Mandate record must remain upstream of evidence-side records.
    /// Acceptance of a non-effect-translating proposal (`Text`) still mints
    /// a Mandate (authorization provenance) but no `InstitutionalEffectRecord`
    /// (evidence of translation). This pins the Mandate/Evidence distinction.
    #[tokio::test]
    async fn adr0014_mandate_distinct_from_institutional_effect_record() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend.clone());

        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Text only".to_string(),
                "No effect translation".to_string(),
                ProposalPayload::Text {
                    body: "Declarative text; no kernel effect.".to_string(),
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

        // Mandate: recorded (authorization arose from the decision).
        let mandate = backend.get_mandate_by_proposal(&proposal_id.0).unwrap();
        assert!(
            mandate.is_some(),
            "Mandate must be recorded for any accepted decision"
        );

        // InstitutionalEffectRecord: NOT recorded for Text payloads, since
        // they do not translate to a structured GovernanceEffect. This is
        // the explicit distinctness: authorization is upstream of translation.
        let effects = backend
            .list_institutional_effects_by_proposal(&proposal_id.0)
            .unwrap();
        assert!(
            effects.is_empty(),
            "Text payload translates to no effect record; mandate remains upstream"
        );
    }

    /// Steward-appointment proposals mint one bounded `AuthorityGrant`
    /// and a mandate whose `grants` field references it. This is the
    /// first truthful binding of typed authority to a real decision.
    #[tokio::test]
    async fn adr0014_appoint_steward_mints_bounded_authority_grant() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend.clone());

        let candidate_kp = icn_identity::KeyPair::generate().unwrap();
        let candidate_did = candidate_kp.did().clone();
        let term_length_seconds: u64 = 365 * 24 * 60 * 60;

        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Appoint steward".to_string(),
                "Appoint a new regional steward for identity attestations".to_string(),
                ProposalPayload::Sdis {
                    proposal: icn_governance::sdis::SdisProposal::AppointSteward {
                        candidate: candidate_did.clone(),
                        sponsors: vec![member_did.clone()],
                        region: "nyc".into(),
                        bond_amount: 100,
                        term_length: term_length_seconds,
                    },
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
            .expect("governance receipt must exist");
        assert_eq!(gov_receipt.outcome, ProofOutcome::Accepted);

        // Grant is persisted and bound to the decision provenance.
        let grants = backend
            .list_authority_grants_by_decision(&gov_receipt.decision_hash)
            .unwrap();
        assert_eq!(
            grants.len(),
            1,
            "AppointSteward must mint exactly one grant"
        );
        let grant = &grants[0];
        assert_eq!(grant.class, icn_governance::AuthorityClass::Attestation);
        assert_eq!(
            grant.grantor,
            icn_governance::GrantorEntityId(domain_id.0.clone())
        );
        assert_eq!(
            grant.grantee,
            icn_governance::Grantee::Person(candidate_did.clone())
        );
        assert_eq!(grant.scope.domain.as_ref(), Some(&domain_id));
        assert_eq!(grant.scope.proposal_class, vec!["Sdis".to_string()]);
        assert!(
            !grant.scope.is_empty(),
            "scope must be bounded (not unbounded-on-everything)"
        );
        let prov = grant.granted_by.as_ref().expect("granted_by set");
        assert_eq!(prov.proposal_id, proposal_id.0);
        assert_eq!(prov.decision_hash, gov_receipt.decision_hash);
        assert_eq!(
            grant.valid_until,
            Some(grant.valid_from + term_length_seconds),
            "valid_until must match grant.valid_from + term_length"
        );
        assert!(grant.revoked_at.is_none());

        // Mandate references the grant and is constructed via the strict
        // `::new` path (no longer bootstrap-phase for this proposal class).
        let mandate = backend
            .get_mandate_by_proposal(&proposal_id.0)
            .unwrap()
            .expect("mandate must exist");
        assert!(
            !mandate.has_no_grants(),
            "AppointSteward mandate must reference attached grants"
        );
        assert_eq!(mandate.grants, vec![grant.id.clone()]);
        assert_eq!(mandate.decision.decision_hash, gov_receipt.decision_hash);
    }

    /// Accepted proposals whose payload class does not truthfully imply
    /// bounded authority (here: a `Text` proposal) must not mint grants,
    /// and the mandate must remain in the bootstrap-phase "no grants"
    /// state — this is the truthful default, not a failure.
    #[tokio::test]
    async fn adr0014_text_proposal_mints_zero_grants_and_unbound_mandate() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend.clone());

        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Text".to_string(),
                "No derivable authority".to_string(),
                ProposalPayload::Text {
                    body: "House rules update.".to_string(),
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
        let grants = backend
            .list_authority_grants_by_decision(&gov_receipt.decision_hash)
            .unwrap();
        assert!(
            grants.is_empty(),
            "Text payload must not mint grants; default is truthful restraint"
        );

        let mandate = backend
            .get_mandate_by_proposal(&proposal_id.0)
            .unwrap()
            .expect("mandate still recorded as authorization-provenance");
        assert!(
            mandate.has_no_grants(),
            "bootstrap-phase mandate stays unbound on authority when no grants derive"
        );
    }

    /// A rejected AppointSteward proposal must not mint a grant — grants
    /// descend from *accepted* decisions only.
    #[tokio::test]
    async fn adr0014_rejected_appoint_steward_mints_no_grant() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend.clone());

        let candidate_kp = icn_identity::KeyPair::generate().unwrap();

        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Appoint".to_string(),
                "Expected rejection".to_string(),
                ProposalPayload::Sdis {
                    proposal: icn_governance::sdis::SdisProposal::AppointSteward {
                        candidate: candidate_kp.did().clone(),
                        sponsors: vec![member_did.clone()],
                        region: "nyc".into(),
                        bond_amount: 100,
                        term_length: 1_000,
                    },
                },
                ProposalScope::Local,
            )
            .await
            .unwrap();

        mgr.open_proposal(proposal_id.clone(), 86400).await.unwrap();
        mgr.cast_vote(
            proposal_id.clone(),
            member_did.clone(),
            VoteChoice::Against,
            None,
        )
        .await
        .unwrap();
        mgr.close_proposal(proposal_id.clone()).await.unwrap();

        let gov_receipt = backend
            .get_governance_by_proposal(&proposal_id.0)
            .unwrap()
            .unwrap();
        assert_eq!(gov_receipt.outcome, ProofOutcome::Rejected);

        let grants = backend
            .list_authority_grants_by_decision(&gov_receipt.decision_hash)
            .unwrap();
        assert!(
            grants.is_empty(),
            "rejected proposal must not mint authority grants"
        );
        assert!(backend
            .get_mandate_by_proposal(&proposal_id.0)
            .unwrap()
            .is_none());
    }

    /// AuthorityGrant lives on the authorization side of the chain;
    /// `InstitutionalEffectRecord` lives on the evidence side. Accepting
    /// an AppointSteward proposal mints a grant but (given the current
    /// translator) no effect record — proving grant and evidence records
    /// are not collapsed at the seam.
    #[tokio::test]
    async fn adr0014_authority_grant_distinct_from_institutional_effect() {
        let (mgr, domain_id, member_did) = make_manager_with_domain().await;
        let backend = Arc::new(InMemoryReceiptBackend::new());
        let mgr = mgr.with_receipt_store(backend.clone());

        let candidate_kp = icn_identity::KeyPair::generate().unwrap();

        let proposal_id = mgr
            .create_proposal(
                ProposalId(format!("prop-{}", uuid::Uuid::new_v4())),
                domain_id.clone(),
                member_did.clone(),
                "Appoint steward".to_string(),
                "Distinctness test".to_string(),
                ProposalPayload::Sdis {
                    proposal: icn_governance::sdis::SdisProposal::AppointSteward {
                        candidate: candidate_kp.did().clone(),
                        sponsors: vec![member_did.clone()],
                        region: "nyc".into(),
                        bond_amount: 100,
                        term_length: 1_000,
                    },
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
        let grants = backend
            .list_authority_grants_by_decision(&gov_receipt.decision_hash)
            .unwrap();
        assert_eq!(grants.len(), 1, "grant recorded on authorization side");

        // Evidence side is governed by a separate translator; whether or
        // not it emits a record for this SDIS variant, the point of this
        // test is that grant recording does not imply effect recording
        // and vice versa. We assert the grant exists and is indexable by
        // decision_hash separately from any effect record.
        let grant_ids_via_decision: Vec<_> = grants.iter().map(|g| g.id.clone()).collect();
        let mandate = backend
            .get_mandate_by_proposal(&proposal_id.0)
            .unwrap()
            .unwrap();
        assert_eq!(mandate.grants, grant_ids_via_decision);
    }
}
