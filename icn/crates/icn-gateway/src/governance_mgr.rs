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
    Delegation, DelegationId, GovernanceConfig, GovernanceDomain, GovernanceDomainId,
    GovernanceOps, GovernanceParams, GovernanceProfileId, MembershipConfig, MembershipSource,
    Proposal, ProposalId, ProposalPayload, ProposalState, Timestamp, Vote, VoteChoice, VoteTally,
};
use icn_identity::Did;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::debug;

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
pub struct GovernanceManager {
    /// In-memory storage for domains (standalone mode only)
    domains: RwLock<HashMap<GovernanceDomainId, GovernanceDomain>>,
    /// In-memory storage for proposals (standalone mode only)
    proposals: RwLock<HashMap<ProposalId, Proposal>>,
    /// In-memory storage for votes (standalone mode only)
    votes: RwLock<HashMap<ProposalId, Vec<Vote>>>,
    /// In-memory storage for delegations (standalone mode only)
    delegations: RwLock<HashMap<DelegationId, Delegation>>,
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
    pub fn with_handle(handle: GovernanceHandle) -> Self {
        debug!("GovernanceManager created with daemon GovernanceActor handle");
        GovernanceManager {
            // These fields are unused in actor-backed mode - all operations
            // delegate to the GovernanceActor via governance_handle
            domains: RwLock::new(HashMap::new()),
            proposals: RwLock::new(HashMap::new()),
            votes: RwLock::new(HashMap::new()),
            delegations: RwLock::new(HashMap::new()),
            governance_handle: Some(handle),
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
    ///
    /// Note: Returns error in actor-backed mode (not exposed via GovernanceOps).
    /// TODO(#273): Add get_vote_tally to GovernanceOps trait.
    pub async fn get_vote_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally> {
        if self.governance_handle.is_some() {
            // Actor-backed mode: vote tally not exposed via GovernanceOps
            // Return explicit error rather than silent empty data
            anyhow::bail!(
                "Vote tally not available in actor-backed mode for proposal '{}'. \
                 Use proposal state to get final outcome, or add get_vote_tally to GovernanceOps trait.",
                proposal_id.0
            );
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
    ///
    /// Note: Returns error in actor-backed mode (not exposed via GovernanceOps).
    /// TODO(#273): Add get_voter_dids to GovernanceOps trait.
    pub async fn get_voter_dids(&self, proposal_id: &ProposalId) -> Result<Vec<Did>> {
        if self.governance_handle.is_some() {
            // Actor-backed mode: voter DIDs not exposed via GovernanceOps
            // Return explicit error rather than silent empty data
            anyhow::bail!(
                "Voter list not available in actor-backed mode for proposal '{}'. \
                 Add get_voter_dids to GovernanceOps trait to expose this data.",
                proposal_id.0
            );
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

        // Simple cycle check: would this create a direct cycle?
        // (Full transitive cycle detection would require building a DelegationManager)
        let would_cycle = delegations.values().any(|d| {
            d.delegator == delegation.delegate
                && d.delegate == delegation.delegator
                && d.is_active(now)
        });
        if would_cycle {
            anyhow::bail!(
                "Delegation would create a cycle: {} <-> {}. The delegate already delegates to this delegator.",
                delegation.delegator,
                delegation.delegate
            );
        }

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
}

impl Default for GovernanceManager {
    fn default() -> Self {
        Self::new()
    }
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
}
