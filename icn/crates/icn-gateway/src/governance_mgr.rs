//! Governance Manager for Gateway API
//!
//! Provides governance operations for the gateway API.
//! This is a simplified interface that can be backed by:
//! 1. In-memory storage (for standalone gateway)
//! 2. GovernanceActor handle (when integrated with daemon)
//!
//! ## Actor-Backed Mode
//!
//! When created with `with_handle()`, the GovernanceManager delegates all operations
//! to the daemon's GovernanceActor, ensuring:
//! - Single source of truth for governance data
//! - Persistence across restarts
//! - Gossip synchronization of proposals and votes
//!
//! When created with `new()`, it uses in-memory storage (suitable for testing only).

use anyhow::Result;
use icn_governance::{
    scopes_overlap, ActionItem, ActionItemFilter, ActionItemId, ActionItemPriority,
    ActionItemStatus, ActionItemStoreBackend, Comment, CommentId, Delegation, DelegationId,
    DelegationScope, Discussion, DiscussionStore, GovernanceConfig, GovernanceDomain,
    GovernanceDomainId, GovernanceError, GovernanceOps, GovernanceParams, GovernanceProfileId,
    InMemoryActionItemStore, InMemoryDiscussionStore, MembershipConfig, MembershipSource,
    PaginatedResult, Proposal, ProposalDomainLookup, ProposalId, ProposalPayload, ProposalState,
    Timestamp, Vote, VoteChoice, VoteTally, DEFAULT_MAX_DELEGATION_DEPTH,
};
use icn_identity::Did;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tracing::debug;

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
/// indexes for common filter fields:
/// - `action_item_idx:status:{domain}:{status}:{id}` for status filtering
/// - `action_item_idx:assignee:{domain}:{did}:{id}` for assignee filtering
/// - `action_item_idx:due:{domain}:{timestamp}:{id}` for due date queries
///
/// Index maintenance would need to be transactional with the primary item storage.
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
        }
    }

    /// Set a custom action item store backend
    ///
    /// Use this to configure Sled-backed persistent storage for action items.
    /// Must be called before any action items are created.
    pub fn set_action_item_store(&mut self, store: Box<dyn ActionItemStoreBackend>) {
        self.action_items = store;
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
        // Create profile ID based on the profile string
        // - "contract:did:..." -> Contract-based profile
        // - Anything else -> Built-in profile name
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

        // Check for duplicate domain ID
        if domains.contains_key(&domain_id) {
            anyhow::bail!(
                "Domain '{}' already exists. Use a unique domain ID or update the existing domain.",
                domain_id.0
            );
        }

        domains.insert(domain_id, domain);

        Ok(())
    }

    /// Get a governance domain
    pub async fn get_domain(
        &self,
        domain_id: &GovernanceDomainId,
    ) -> Result<Option<GovernanceDomain>> {
        if let Some(ref handle) = self.governance_handle {
            // Actor-backed mode: delegate to GovernanceActor
            return handle.get_domain(domain_id).await;
        }

        // Standalone mode: in-memory storage
        let domains = self.domains.read().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;
        Ok(domains.get(domain_id).cloned())
    }

    /// List all governance domains
    pub async fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        if let Some(ref handle) = self.governance_handle {
            // Actor-backed mode: delegate to GovernanceActor
            return handle.list_domains().await;
        }

        // Standalone mode: in-memory storage
        let domains = self.domains.read().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;
        Ok(domains.values().cloned().collect())
    }

    /// List governance domains with pagination
    ///
    /// Returns a page of domains. Use this for large datasets to avoid
    /// loading all domains into memory at once.
    ///
    /// # Arguments
    /// * `cursor` - Optional cursor from a previous page
    /// * `limit` - Maximum number of items to return
    pub async fn list_domains_paginated(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<PaginatedResult<GovernanceDomain>> {
        if let Some(ref handle) = self.governance_handle {
            // Actor-backed mode: delegate to GovernanceActor
            return handle.list_domains_paginated(cursor, limit).await;
        }

        // Standalone mode: in-memory storage with offset-based pagination
        let domains = self.domains.read().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;

        // Parse cursor as offset (format: "offset:<number>")
        let offset: usize = cursor
            .and_then(|c| c.strip_prefix("offset:"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // Get sorted keys for deterministic ordering
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
    ///
    /// Returns the `ProposalId` of the created proposal. In actor-backed mode,
    /// the actor generates the ID (the `proposal_id` parameter is ignored).
    /// In standalone mode, the provided `proposal_id` is used.
    ///
    /// In actor-backed mode, the `proposer` parameter is ignored - the actor
    /// uses its authenticated identity as the proposer.
    pub async fn create_proposal(
        &self,
        proposal_id: ProposalId,
        domain_id: GovernanceDomainId,
        proposer: Did,
        title: String,
        description: String,
        payload: ProposalPayload,
    ) -> Result<ProposalId> {
        if let Some(ref handle) = self.governance_handle {
            // Actor-backed mode: delegate to GovernanceActor
            // Note: proposal_id and proposer are ignored - actor generates ID and uses own DID
            let generated_id = handle
                .create_proposal(domain_id, title, description, payload)
                .await?;
            return Ok(generated_id);
        }

        // Standalone mode: in-memory storage
        // Validate domain exists
        let domains = self.domains.read().map_err(|e| {
            anyhow::anyhow!("Domains storage lock poisoned (concurrent panic?): {e}")
        })?;
        if !domains.contains_key(&domain_id) {
            anyhow::bail!(
                "Domain '{}' not found. Create the domain first using create_domain().",
                domain_id.0
            );
        }
        drop(domains); // Release read lock before acquiring write lock

        let mut proposal = Proposal::new(domain_id, proposer, title, description, payload);
        // Override the generated ID with the one provided
        proposal.id = proposal_id.clone();

        let mut proposals = self.proposals.write().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;

        // Check for duplicate proposal ID
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
            // Actor-backed mode: delegate to GovernanceActor
            return handle.get_proposal(proposal_id).await;
        }

        // Standalone mode: in-memory storage
        let proposals = self.proposals.read().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;
        Ok(proposals.get(proposal_id).cloned())
    }

    /// Insert a proposal with any state (test helper)
    ///
    /// This method is intended for testing only. It allows inserting a proposal
    /// with any arbitrary state, bypassing the normal proposal lifecycle.
    ///
    /// # Panics
    /// Panics if used in actor-backed mode (only works in standalone mode).
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
            // Actor-backed mode: delegate to GovernanceActor
            return handle.list_proposals().await;
        }

        // Standalone mode: in-memory storage
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
            // Actor-backed mode: delegate to GovernanceActor
            return handle
                .open_proposal(proposal_id, voting_period_seconds)
                .await;
        }

        // Standalone mode: in-memory storage
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
            // Actor-backed mode: delegate to GovernanceActor
            return handle.close_proposal(proposal_id).await;
        }

        // Standalone mode: in-memory storage
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
            // Validate proposal is in Open state
            if !proposal.state.is_open() {
                anyhow::bail!(
                    "Proposal '{}' cannot be closed: not open for voting (current state: {:?}). \
                     Only proposals in 'Open' state can be closed.",
                    proposal_id.0,
                    proposal.state
                );
            }

            // Get domain to access governance params
            let domain = domains.get(&proposal.domain_id).ok_or_else(|| {
                anyhow::anyhow!(
                    "Domain '{}' not found for proposal '{}'. Domain may have been deleted.",
                    proposal.domain_id.0,
                    proposal_id.0
                )
            })?;

            // Calculate vote tally using proper vote tally system
            let proposal_votes = votes.get(&proposal_id).cloned().unwrap_or_default();
            let tally = VoteTally::from(proposal_votes);

            // Get total eligible voters from domain membership
            let total_members = match &domain.config.membership.source {
                MembershipSource::StaticList(members) => members.len(),
                MembershipSource::TrustThreshold(_) => {
                    // For trust-based membership, we don't know the exact count
                    // Use total votes as a proxy (conservative approach)
                    tally.total_votes().max(1) // Ensure at least 1 to avoid division by zero
                }
            };

            // Calculate quorum: percentage of eligible voters who participated
            let quorum_percentage = if total_members > 0 {
                // Use checked_mul to prevent overflow
                let total_votes = tally.total_votes();
                let percentage = total_votes
                    .checked_mul(100)
                    .and_then(|v| v.checked_div(total_members));

                match percentage {
                    Some(p) => p.min(100) as u8, // Clamp to 100% max
                    None => {
                        // Overflow in quorum calculation is a critical error - don't silently fail
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

            // Evaluate outcome based on governance params
            let final_state = if quorum_percentage < domain.config.params.quorum_percentage {
                // Quorum not met
                ProposalState::NoQuorum { closed_at: now }
            } else if tally.approval_percentage()
                >= domain.config.params.approval_threshold_percentage
            {
                // Quorum met and approval threshold reached
                ProposalState::Accepted { closed_at: now }
            } else {
                // Quorum met but approval threshold not reached
                ProposalState::Rejected { closed_at: now }
            };

            proposal.close(final_state)?;
            Ok(())
        } else {
            anyhow::bail!(
                "Proposal '{}' not found. It may not exist or was already deleted.",
                proposal_id.0
            )
        }
    }

    /// Get vote tally for a proposal
    pub async fn get_vote_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally> {
        if let Some(ref handle) = self.governance_handle {
            // Actor-backed mode: delegate to GovernanceActor via GovernanceOps
            return handle.get_vote_tally(proposal_id).await;
        }

        // Standalone mode: in-memory storage
        let votes = self
            .votes
            .read()
            .map_err(|e| anyhow::anyhow!("Votes storage lock poisoned (concurrent panic?): {e}"))?;
        let proposal_votes = votes.get(proposal_id).cloned().unwrap_or_default();
        Ok(VoteTally::from(proposal_votes))
    }

    /// Get list of voter DIDs for a proposal (for notifications)
    pub async fn get_voter_dids(&self, proposal_id: &ProposalId) -> Result<Vec<Did>> {
        if let Some(ref handle) = self.governance_handle {
            // Actor-backed mode: delegate to GovernanceActor via GovernanceOps
            return handle.get_voter_dids(proposal_id).await;
        }

        // Standalone mode: in-memory storage
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

    /// Cast a vote on a proposal
    ///
    /// In actor-backed mode, the `voter` parameter is ignored - the actor
    /// uses its authenticated identity as the voter. The caller should use
    /// the authenticated user's DID for event logging purposes.
    ///
    /// In standalone mode, the `voter` parameter is used to record the vote
    /// and validate domain membership.
    pub async fn cast_vote(
        &self,
        proposal_id: ProposalId,
        voter: Did,
        choice: VoteChoice,
        comment: Option<String>,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            // Actor-backed mode: delegate to GovernanceActor
            // Note: voter is ignored - actor uses its own DID
            return handle.cast_vote(proposal_id, choice, comment).await;
        }

        // Standalone mode: in-memory storage
        // Validate proposal exists and is open for voting
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

        // Validate voter is a member of the domain
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
            MembershipSource::TrustThreshold(_) => {
                // For trust-based membership, we'd need trust graph integration
                // For now, allow all (will be enforced by daemon integration)
                true
            }
        };

        if !is_member {
            anyhow::bail!(
                "Voter {} is not a member of domain {}",
                voter,
                proposal.domain_id.0
            );
        }

        // Drop locks before acquiring votes write lock to avoid holding multiple locks
        drop(domains);
        drop(proposals);

        // Acquire votes write lock
        let mut votes = self
            .votes
            .write()
            .map_err(|e| anyhow::anyhow!("Votes storage lock poisoned (concurrent panic?): {e}"))?;

        // CRITICAL: Re-check proposal state after acquiring votes lock to prevent TOCTOU
        // Another thread could have closed the proposal between our initial check and now
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

        // Check for duplicate vote
        let proposal_votes = votes.entry(proposal_id.clone()).or_insert_with(Vec::new);

        if proposal_votes.iter().any(|v| v.voter == voter) {
            anyhow::bail!(
                "Voter {} has already voted on proposal {}",
                voter,
                proposal_id.0
            );
        }

        // Create and store vote record
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
    ///
    /// Validates that:
    /// - Delegator is not the same as delegate (no self-delegation)
    /// - No cycles would be created
    /// - Max delegation depth not exceeded
    /// - No duplicate delegation for same scope
    pub async fn create_delegation(&self, delegation: Delegation) -> Result<()> {
        // Actor-backed mode: delegate to GovernanceActor
        if let Some(ref handle) = self.governance_handle {
            return handle.create_delegation(delegation).await;
        }

        // Standalone mode: use local storage
        // Validate no self-delegation
        if delegation.delegator == delegation.delegate {
            anyhow::bail!(
                "Cannot delegate to yourself. Delegator and delegate must be different DIDs."
            );
        }

        let mut delegations = self.delegations.write().map_err(|e| {
            anyhow::anyhow!("Delegations storage lock poisoned (concurrent panic?): {e}")
        })?;

        // Get proposals for precise scope overlap checking
        let proposals = self.proposals.read().map_err(|e| {
            anyhow::anyhow!("Proposals storage lock poisoned (concurrent panic?): {e}")
        })?;

        // Check for duplicate delegation ID
        if delegations.contains_key(&delegation.id) {
            anyhow::bail!(
                "Delegation '{}' already exists. Use a unique delegation ID.",
                delegation.id.0
            );
        }

        // Check for duplicate scope (same delegator + scope)
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

        // Full transitive cycle detection: walk the delegation chain from delegate
        // and check if it eventually leads back to delegator (A→B→C→A detection)
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

        // Check max delegation depth: count how many hops lead TO the delegator
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

        // Release proposals lock before inserting
        drop(proposals);

        delegations.insert(delegation.id.clone(), delegation);
        Ok(())
    }

    /// Get a delegation by ID
    pub async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        // Actor-backed mode: delegate to GovernanceActor
        if let Some(ref handle) = self.governance_handle {
            return handle.get_delegation(id).await;
        }

        // Standalone mode: use local storage
        let delegations = self.delegations.read().map_err(|e| {
            anyhow::anyhow!("Delegations storage lock poisoned (concurrent panic?): {e}")
        })?;

        Ok(delegations.get(id).cloned())
    }

    /// Get all delegations given by a specific DID
    pub async fn get_delegations_from(&self, delegator: &Did) -> Result<Vec<Delegation>> {
        // Actor-backed mode: delegate to GovernanceActor
        if let Some(ref handle) = self.governance_handle {
            return handle.get_delegations_from(delegator).await;
        }

        // Standalone mode: use local storage
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
        // Actor-backed mode: delegate to GovernanceActor
        if let Some(ref handle) = self.governance_handle {
            return handle.get_delegations_to(delegate).await;
        }

        // Standalone mode: use local storage
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
        // Actor-backed mode: delegate to GovernanceActor
        if let Some(ref handle) = self.governance_handle {
            return handle.revoke_delegation(id, revoked_at).await;
        }

        // Standalone mode: use local storage
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
    ///
    /// Transitions the proposal from Draft to Deliberation state.
    /// The deliberation period allows structured discussion before voting.
    pub async fn start_deliberation(
        &self,
        proposal_id: &ProposalId,
        deliberation_period_seconds: u64,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            // Actor-backed mode: delegate to GovernanceActor
            return handle
                .start_deliberation(proposal_id.clone(), deliberation_period_seconds)
                .await;
        }

        // Standalone mode: in-memory storage
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
    ///
    /// Transitions the proposal from Deliberation to Open state.
    pub async fn end_deliberation_and_open(
        &self,
        proposal_id: &ProposalId,
        voting_period_seconds: u64,
    ) -> Result<()> {
        if let Some(ref handle) = self.governance_handle {
            // Actor-backed mode: delegate to GovernanceActor
            return handle
                .end_deliberation_and_open(proposal_id.clone(), voting_period_seconds)
                .await;
        }

        // Standalone mode: in-memory storage
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
        // Validate proposal exists and allows comments
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
}

impl Default for GovernanceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Delegation Cycle Detection Helpers
// ============================================================================

/// Wrapper to implement `ProposalDomainLookup` for HashMap<ProposalId, Proposal>.
///
/// Used to pass proposal storage to the shared `scopes_overlap` function.
struct ProposalMapLookup<'a>(&'a HashMap<ProposalId, Proposal>);

impl ProposalDomainLookup for ProposalMapLookup<'_> {
    fn lookup_proposal_domain(&self, proposal_id: &ProposalId) -> Option<GovernanceDomainId> {
        self.0.get(proposal_id).map(|p| p.domain_id.clone())
    }
}

/// Check if two delegation scopes overlap (for cycle detection)
///
/// Returns true if delegations with these scopes could conflict.
/// Delegates to the shared `scopes_overlap` function from icn-governance.
///
/// # Eventual Consistency
///
/// If proposal is not found, assumes NO overlap (proposal-specific delegations
/// are narrower than domain delegations). This allows delegations to proceed
/// when proposal info hasn't propagated via gossip yet.
fn gateway_scopes_overlap(
    a: &DelegationScope,
    b: &DelegationScope,
    proposals: &HashMap<ProposalId, Proposal>,
) -> bool {
    let lookup = ProposalMapLookup(proposals);
    // default_on_unknown=false: assume no overlap when proposal is unknown
    // (permissive behavior allows delegations to proceed during gossip propagation)
    scopes_overlap(a, b, &lookup, false)
}

/// Find a delegation cycle if adding delegator→delegate would create one
///
/// Returns Some(cycle_path) if a cycle would be created, None otherwise.
/// The path includes the full cycle from delegator back to delegator.
///
/// # Algorithm Limitations
///
/// This function uses a single-path traversal starting from the delegate, following
/// one outgoing delegation at each step. For each node, it picks the first active
/// delegation that matches the scope, which means it may not explore all possible paths
/// in a branching delegation graph.
///
/// In practice, this is acceptable because:
/// 1. Most delegation graphs are linear chains, not DAGs
/// 2. The MAX_DELEGATION_DEPTH limit (3) bounds the blast radius
/// 3. Diamond patterns (A→B, A→C, B→D, C→D) don't create cycles by themselves
///
/// A more comprehensive BFS/DFS approach could be implemented if needed, but would
/// add complexity without significant benefit for the expected use cases.
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

    // Use inclusive range to detect cycles at the depth boundary
    // With MAX_DELEGATION_DEPTH=3: 0..=3 checks 4 positions (delegate + 3 hops)
    for _ in 0..=DEFAULT_MAX_DELEGATION_DEPTH {
        if visited.contains(&current) {
            // Found a cycle - current is already in our path
            return Some(path);
        }
        visited.insert(current.clone());

        // Find any active delegation from current that matches scope
        let next = delegations
            .values()
            .find(|d| {
                d.delegator == current
                    && d.is_active(now)
                    && gateway_scopes_overlap(&d.scope, scope, proposals)
            })
            .map(|d| d.delegate.clone());

        match next {
            Some(d) => {
                if d == *delegator {
                    // Completes the cycle back to delegator
                    path.push(d);
                    return Some(path);
                }
                path.push(d.clone());
                current = d;
            }
            None => return None, // No more delegations, no cycle
        }
    }

    None
}

/// Compute incoming delegation chain depth (how many hops lead TO this delegate)
///
/// For example, if Alice→Bob→Charlie, then Charlie has incoming depth 2.
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
    // Prevent infinite recursion
    if visited.contains(delegate) {
        return 0;
    }
    visited.insert(delegate.clone());

    // Safety limit
    if visited.len() > DEFAULT_MAX_DELEGATION_DEPTH + 10 {
        return 0;
    }

    // Find all delegators who have delegated to this delegate with overlapping scope
    let delegators: Vec<Did> = delegations
        .values()
        .filter(|d| {
            d.delegate == *delegate
                && d.is_active(now)
                && gateway_scopes_overlap(&d.scope, scope, proposals)
        })
        .map(|d| d.delegator.clone())
        .collect();

    if delegators.is_empty() {
        return 0;
    }

    // Recursively find the maximum depth
    delegators
        .into_iter()
        .map(|d| {
            1 + compute_incoming_depth_recursive(&d, scope, delegations, proposals, now, visited)
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
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

        // Verify domain was created with correct profile
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

        let result = mgr
            .create_domain(
                domain_id.clone(),
                "Contract Coop".to_string(),
                "contract:did:icn:abc123".to_string(),
                params,
                membership,
            )
            .await;

        assert!(result.is_ok());

        // Verify domain was created with contract-based profile
        let domain = mgr.get_domain(&domain_id).await.unwrap().unwrap();
        assert_eq!(domain.config.profile.0, "contract:did:icn:abc123");
        assert!(domain.config.profile.is_contract());
    }

    // ============================================================================
    // Delegation Cycle Detection Tests
    // ============================================================================

    fn test_did(seed: u8) -> Did {
        Did::from_anchor_id(&[seed; 32])
    }

    #[tokio::test]
    async fn test_delegation_direct_cycle_rejected() {
        let mgr = GovernanceManager::new();
        let alice = test_did(1);
        let bob = test_did(2);

        // Alice -> Bob (allowed)
        let d1 = Delegation::new(alice.clone(), bob.clone(), DelegationScope::Blanket);
        mgr.create_delegation(d1).await.unwrap();

        // Bob -> Alice (should fail - direct cycle)
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

        // Alice -> Bob (allowed)
        let d1 = Delegation::new(alice.clone(), bob.clone(), DelegationScope::Blanket);
        mgr.create_delegation(d1).await.unwrap();

        // Bob -> Charlie (allowed)
        let d2 = Delegation::new(bob.clone(), charlie.clone(), DelegationScope::Blanket);
        mgr.create_delegation(d2).await.unwrap();

        // Charlie -> Alice (should fail - transitive cycle A->B->C->A)
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

        // Create a chain: Alice -> Bob -> Charlie -> Dave (depth 3)
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

        // Dave -> Eve would exceed max depth (default is 3)
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

        // Alice -> Bob for domain1 (allowed)
        let d1 = Delegation::new(
            alice.clone(),
            bob.clone(),
            DelegationScope::Domain(domain1.clone()),
        );
        mgr.create_delegation(d1).await.unwrap();

        // Bob -> Alice for domain2 (allowed - different domain, no cycle)
        let d2 = Delegation::new(
            bob.clone(),
            alice.clone(),
            DelegationScope::Domain(domain2.clone()),
        );
        let result = mgr.create_delegation(d2).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delegation_blanket_causes_cycle_with_domain() {
        let mgr = GovernanceManager::new();
        let alice = test_did(1);
        let bob = test_did(2);
        let domain = GovernanceDomainId::new("test");

        // Alice -> Bob (blanket)
        let d1 = Delegation::new(alice.clone(), bob.clone(), DelegationScope::Blanket);
        mgr.create_delegation(d1).await.unwrap();

        // Bob -> Alice for domain (should fail - blanket overlaps with all domains)
        let d2 = Delegation::new(
            bob.clone(),
            alice.clone(),
            DelegationScope::Domain(domain.clone()),
        );
        let result = mgr.create_delegation(d2).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[tokio::test]
    async fn test_precise_scope_overlap_no_cycle_different_domains() {
        let mgr = GovernanceManager::new();
        let alice = test_did(1);
        let bob = test_did(2);

        let domain_a = GovernanceDomainId::new("domain-a");
        let domain_b = GovernanceDomainId::new("domain-b");
        let membership = MembershipConfig {
            source: MembershipSource::StaticList(vec![alice.clone(), bob.clone()]),
        };
        let params = GovernanceParams::new(50, 50, 86400);

        // Create domain B
        mgr.create_domain(
            domain_b.clone(),
            "Domain B".to_string(),
            "cooperative_default".to_string(),
            params.clone(),
            membership.clone(),
        )
        .await
        .unwrap();

        // Create a proposal in domain B
        let proposal_id = ProposalId::generate();
        mgr.create_proposal(
            proposal_id.clone(),
            domain_b.clone(),
            alice.clone(),
            "Test Proposal".to_string(),
            "Description".to_string(),
            ProposalPayload::Text {
                body: "test".to_string(),
            },
        )
        .await
        .unwrap();

        // Alice delegates domain A to Bob
        mgr.create_delegation(Delegation::new(
            alice.clone(),
            bob.clone(),
            DelegationScope::Domain(domain_a.clone()),
        ))
        .await
        .unwrap();

        // Bob delegates proposal (in domain B) to Alice
        // This should SUCCEED because domain A != domain B (no cycle with precise checking)
        let result = mgr
            .create_delegation(Delegation::new(
                bob.clone(),
                alice.clone(),
                DelegationScope::Proposal(proposal_id),
            ))
            .await;
        assert!(result.is_ok(), "Expected no cycle but got: {result:?}");
    }

    #[tokio::test]
    async fn test_precise_scope_overlap_cycle_same_domain() {
        let mgr = GovernanceManager::new();
        let alice = test_did(1);
        let bob = test_did(2);

        let domain_a = GovernanceDomainId::new("domain-a");
        let membership = MembershipConfig {
            source: MembershipSource::StaticList(vec![alice.clone(), bob.clone()]),
        };
        let params = GovernanceParams::new(50, 50, 86400);

        // Create domain A
        mgr.create_domain(
            domain_a.clone(),
            "Domain A".to_string(),
            "cooperative_default".to_string(),
            params.clone(),
            membership.clone(),
        )
        .await
        .unwrap();

        // Create a proposal in domain A
        let proposal_id = ProposalId::generate();
        mgr.create_proposal(
            proposal_id.clone(),
            domain_a.clone(),
            alice.clone(),
            "Test Proposal".to_string(),
            "Description".to_string(),
            ProposalPayload::Text {
                body: "test".to_string(),
            },
        )
        .await
        .unwrap();

        // Alice delegates domain A to Bob
        mgr.create_delegation(Delegation::new(
            alice.clone(),
            bob.clone(),
            DelegationScope::Domain(domain_a.clone()),
        ))
        .await
        .unwrap();

        // Bob delegates proposal (in domain A) to Alice
        // This should FAIL because proposal is in domain A (cycle detected)
        let result = mgr
            .create_delegation(Delegation::new(
                bob.clone(),
                alice.clone(),
                DelegationScope::Proposal(proposal_id),
            ))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }
}
