//! Governance Manager for Gateway API
//!
//! Provides governance operations for the gateway API.
//! This is a simplified interface that can be backed by:
//! 1. In-memory storage (for standalone gateway)
//! 2. GovernanceActor handle (when integrated with daemon)

use anyhow::Result;
use icn_governance::{
    GovernanceConfig, GovernanceDomain, GovernanceDomainId, GovernanceParams, GovernanceProfileId,
    MembershipConfig, Proposal, ProposalId, ProposalPayload, ProposalState, Vote, VoteChoice,
};
use icn_identity::Did;
use std::collections::HashMap;
use std::sync::RwLock;

/// Governance manager for gateway API
pub struct GovernanceManager {
    domains: RwLock<HashMap<GovernanceDomainId, GovernanceDomain>>,
    proposals: RwLock<HashMap<ProposalId, Proposal>>,
    votes: RwLock<HashMap<ProposalId, Vec<Vote>>>,
}

impl GovernanceManager {
    /// Create a new governance manager with in-memory storage
    pub fn new() -> Self {
        GovernanceManager {
            domains: RwLock::new(HashMap::new()),
            proposals: RwLock::new(HashMap::new()),
            votes: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new governance domain
    pub async fn create_domain(
        &self,
        domain_id: GovernanceDomainId,
        name: String,
        _profile: String, // TODO: Use profile to configure params
        params: GovernanceParams,
        membership: MembershipConfig,
    ) -> Result<()> {
        let config = GovernanceConfig::new(
            GovernanceProfileId::builtin("cooperative"),
            membership,
            params,
        );

        let domain = GovernanceDomain::new(name, config);

        let mut domains = self.domains.write().unwrap();
        domains.insert(domain_id, domain);

        Ok(())
    }

    /// Get a governance domain
    pub async fn get_domain(&self, domain_id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        let domains = self.domains.read().unwrap();
        Ok(domains.get(domain_id).cloned())
    }

    /// List all governance domains
    pub async fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        let domains = self.domains.read().unwrap();
        Ok(domains.values().cloned().collect())
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
    ) -> Result<()> {
        let proposal = Proposal::new(domain_id, proposer, title, description, payload);

        let mut proposals = self.proposals.write().unwrap();
        proposals.insert(proposal_id, proposal);

        Ok(())
    }

    /// Get a specific proposal
    pub async fn get_proposal(&self, proposal_id: &ProposalId) -> Result<Option<Proposal>> {
        let proposals = self.proposals.read().unwrap();
        Ok(proposals.get(proposal_id).cloned())
    }

    /// List all proposals
    pub async fn list_proposals(&self) -> Result<Vec<Proposal>> {
        let proposals = self.proposals.read().unwrap();
        Ok(proposals.values().cloned().collect())
    }

    /// Open a proposal for voting
    pub async fn open_proposal(&self, proposal_id: ProposalId, voting_period_seconds: u64) -> Result<()> {
        let mut proposals = self.proposals.write().unwrap();

        if let Some(proposal) = proposals.get_mut(&proposal_id) {
            proposal.open(voting_period_seconds)?;
        }

        Ok(())
    }

    /// Close a proposal and finalize voting
    pub async fn close_proposal(&self, proposal_id: ProposalId) -> Result<()> {
        let mut proposals = self.proposals.write().unwrap();
        let votes = self.votes.read().unwrap();

        if let Some(proposal) = proposals.get_mut(&proposal_id) {
            // Calculate vote tally
            let proposal_votes = votes.get(&proposal_id).cloned().unwrap_or_default();
            let for_votes = proposal_votes.iter().filter(|v| matches!(v.choice, VoteChoice::For)).count();
            let against_votes = proposal_votes.iter().filter(|v| matches!(v.choice, VoteChoice::Against)).count();
            let _abstain_votes = proposal_votes.iter().filter(|v| matches!(v.choice, VoteChoice::Abstain)).count();

            // Simple majority logic (for now)
            // TODO: Use actual governance profile evaluation
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();

            let final_state = if for_votes > against_votes {
                ProposalState::Accepted { closed_at: now }
            } else {
                ProposalState::Rejected { closed_at: now }
            };

            proposal.close(final_state)?;
        }

        Ok(())
    }

    /// Cast a vote on a proposal
    pub async fn cast_vote(
        &self,
        proposal_id: ProposalId,
        voter: Did,
        choice: VoteChoice,
        comment: Option<String>,
    ) -> Result<()> {
        // Create vote record using Vote::new()
        let mut vote = Vote::new(proposal_id.clone(), voter.clone(), choice);

        // Add comment if provided
        if let Some(c) = comment {
            vote = vote.with_comment(c);
        }

        // Store vote
        let mut votes = self.votes.write().unwrap();
        votes.entry(proposal_id).or_insert_with(Vec::new).push(vote);

        Ok(())
    }
}

impl Default for GovernanceManager {
    fn default() -> Self {
        Self::new()
    }
}
