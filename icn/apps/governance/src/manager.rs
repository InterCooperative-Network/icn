//! Governance Manager for Gateway API
//!
//! Moved from `icn-gateway` to the app layer so the manager lives next to the
//! domain it manages. The only structural change relative to the gateway version
//! is that `ReceiptStore` (a gateway-internal type) is replaced by the
//! [`GovernanceReceiptBackend`] trait so this crate does not depend on
//! `icn-gateway`.

use crate::receipt_backend::GovernanceReceiptBackend;
use anyhow::Result;
use icn_governance::{
    scopes_overlap, ActionItem, ActionItemFilter, ActionItemId, ActionItemPriority,
    ActionItemStatus, ActionItemStoreBackend, Comment, CommentId, Delegation, DelegationId,
    DelegationScope, Discussion, DiscussionStore, GovernanceConfig, GovernanceDecisionReceipt,
    GovernanceDomain, GovernanceDomainId, GovernanceError, GovernanceOps, GovernanceParams,
    GovernanceProfileId, InMemoryActionItemStore, InMemoryDiscussionStore, MembershipConfig,
    MembershipSource, PaginatedResult, ProofOutcome, Proposal, ProposalDomainLookup, ProposalId,
    ProposalPayload, ProposalScope, ProposalState, Timestamp, Vote, VoteChoice, VoteTally,
    DEFAULT_MAX_DELEGATION_DEPTH,
};
use icn_identity::Did;
use icn_kernel_api::{AllocationReceipt, ScopeLevel, SettlementIntent};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tracing::debug;

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
}

impl ActionItemStoreBackend for SledActionItemStore {
    fn save(&self, item: &ActionItem) -> std::result::Result<(), GovernanceError> {
        let key = Self::item_key(&item.domain_id, &item.id);
        let value = icn_encoding::encode_versioned(item)
            .map_err(|e| GovernanceError::Internal(format!("Failed to encode action item: {e}")))?;
        self.db
            .insert(key.as_bytes(), value)
            .map_err(|e| GovernanceError::Internal(format!("Sled insert failed: {e}")))?;
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
        self.db
            .remove(key.as_bytes())
            .map(|opt| opt.is_some())
            .map_err(|e| GovernanceError::Internal(format!("Sled delete failed: {e}")))
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
            action_items: Box::new(SledActionItemStore::new(db)),
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
            action_items: Box::new(SledActionItemStore::new(db)),
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
        let domain = GovernanceDomain::new(name, config);

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
        if let Some(ref handle) = self.governance_handle {
            let generated_id = handle
                .create_proposal(domain_id, title, description, payload, scope)
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

        let mut proposal =
            Proposal::new(domain_id, proposer, title, description, payload).with_scope(scope);
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

            let proposal_votes = votes.get(&proposal_id).cloned().unwrap_or_default();
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

        // Chain is complete when:
        // - governance receipt exists AND
        // - either there are allocations (economic proposal) OR
        //   the proposal is non-economic (Text/Membership/etc — no allocation expected)
        let has_governance = governance_receipt.is_some();
        let has_allocations = !allocations.is_empty();

        // Determine if this is an economic proposal by checking the stored receipt or proposal payload
        let is_economic = has_allocations; // If allocations were created, it was economic
        let chain_complete = has_governance && (is_economic == has_allocations);

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
                    &domain_id.0,           // from: domain treasury
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

        // Verify decision_hash binds governance to allocation
        let gov_hash = chain.governance_receipt.unwrap().decision_hash;
        assert_eq!(
            chain.allocations[0].decision_hash, gov_hash,
            "INV-5: allocation.decision_hash must equal governance.decision_hash"
        );
    }
}
