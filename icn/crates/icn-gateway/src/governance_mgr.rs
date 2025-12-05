//! Governance Manager for Gateway API
//!
//! Provides governance operations for the gateway API.
//! This is a simplified interface that can be backed by:
//! 1. In-memory storage (for standalone gateway)
//! 2. GovernanceActor handle (when integrated with daemon)

use anyhow::Result;
use icn_governance::{
    GovernanceConfig, GovernanceDomain, GovernanceDomainId, GovernanceParams, GovernanceProfileId,
    MembershipConfig, MembershipSource, Proposal, ProposalId, ProposalPayload, ProposalState, Vote,
    VoteChoice, VoteTally,
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
        profile: String,
        params: GovernanceParams,
        membership: MembershipConfig,
    ) -> Result<()> {
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

        let mut domains = self
            .domains
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        // Check for duplicate domain ID
        if domains.contains_key(&domain_id) {
            anyhow::bail!("Domain already exists: {}", domain_id.0);
        }

        domains.insert(domain_id, domain);

        Ok(())
    }

    /// Get a governance domain
    pub async fn get_domain(
        &self,
        domain_id: &GovernanceDomainId,
    ) -> Result<Option<GovernanceDomain>> {
        let domains = self
            .domains
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(domains.get(domain_id).cloned())
    }

    /// List all governance domains
    pub async fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        let domains = self
            .domains
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
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
        // Validate domain exists
        let domains = self
            .domains
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        if !domains.contains_key(&domain_id) {
            anyhow::bail!("Domain not found: {}", domain_id.0);
        }
        drop(domains); // Release read lock before acquiring write lock

        let mut proposal = Proposal::new(domain_id, proposer, title, description, payload);
        // Override the generated ID with the one provided
        proposal.id = proposal_id.clone();

        let mut proposals = self
            .proposals
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        // Check for duplicate proposal ID
        if proposals.contains_key(&proposal_id) {
            anyhow::bail!("Proposal already exists: {}", proposal_id.0);
        }

        proposals.insert(proposal_id, proposal);

        Ok(())
    }

    /// Get a specific proposal
    pub async fn get_proposal(&self, proposal_id: &ProposalId) -> Result<Option<Proposal>> {
        let proposals = self
            .proposals
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(proposals.get(proposal_id).cloned())
    }

    /// List all proposals
    pub async fn list_proposals(&self) -> Result<Vec<Proposal>> {
        let proposals = self
            .proposals
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(proposals.values().cloned().collect())
    }

    /// Open a proposal for voting
    pub async fn open_proposal(
        &self,
        proposal_id: ProposalId,
        voting_period_seconds: u64,
    ) -> Result<()> {
        let mut proposals = self
            .proposals
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(proposal) = proposals.get_mut(&proposal_id) {
            proposal.open(voting_period_seconds)?;
            Ok(())
        } else {
            anyhow::bail!("Proposal not found: {}", proposal_id.0)
        }
    }

    /// Close a proposal and finalize voting
    pub async fn close_proposal(&self, proposal_id: ProposalId) -> Result<()> {
        let mut proposals = self
            .proposals
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let votes = self
            .votes
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let domains = self
            .domains
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(proposal) = proposals.get_mut(&proposal_id) {
            // Validate proposal is in Open state
            if !proposal.state.is_open() {
                anyhow::bail!(
                    "Proposal is not open for voting (current state: {:?})",
                    proposal.state
                );
            }

            // Get domain to access governance params
            let domain = domains
                .get(&proposal.domain_id)
                .ok_or_else(|| anyhow::anyhow!("Domain not found: {}", proposal.domain_id.0))?;

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
                // Use checked_mul to prevent overflow, then clamp to u8 range
                let total_votes = tally.total_votes();
                let percentage = total_votes
                    .checked_mul(100)
                    .and_then(|v| v.checked_div(total_members))
                    .unwrap_or(0); // Overflow = 0% (conservative)
                percentage.min(100) as u8 // Clamp to 100% max
            } else {
                0
            };

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();

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
            anyhow::bail!("Proposal not found: {}", proposal_id.0)
        }
    }

    /// Get vote tally for a proposal
    pub async fn get_vote_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally> {
        let votes = self
            .votes
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let proposal_votes = votes.get(proposal_id).cloned().unwrap_or_default();
        Ok(VoteTally::from(proposal_votes))
    }

    /// Cast a vote on a proposal
    pub async fn cast_vote(
        &self,
        proposal_id: ProposalId,
        voter: Did,
        choice: VoteChoice,
        comment: Option<String>,
    ) -> Result<()> {
        // Validate proposal exists and is open for voting
        let proposals = self
            .proposals
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let proposal = proposals
            .get(&proposal_id)
            .ok_or_else(|| anyhow::anyhow!("Proposal not found: {}", proposal_id.0))?;

        if !proposal.state.is_open() {
            anyhow::bail!(
                "Proposal is not open for voting (current state: {:?})",
                proposal.state
            );
        }

        // Validate voter is a member of the domain
        let domains = self
            .domains
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let domain = domains
            .get(&proposal.domain_id)
            .ok_or_else(|| anyhow::anyhow!("Domain not found: {}", proposal.domain_id.0))?;

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
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        // CRITICAL: Re-check proposal state after acquiring votes lock to prevent TOCTOU
        // Another thread could have closed the proposal between our initial check and now
        let proposals_recheck = self
            .proposals
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let proposal_recheck = proposals_recheck
            .get(&proposal_id)
            .ok_or_else(|| anyhow::anyhow!("Proposal not found: {}", proposal_id.0))?;

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
