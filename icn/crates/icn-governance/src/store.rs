//! Governance storage layer

use crate::{GovernanceDomain, GovernanceDomainId, Proposal, ProposalId, Vote, VoteTally};
use anyhow::Result;
use icn_identity::Did;
use std::collections::HashMap;
use std::sync::Arc;

/// Trait for governance storage operations
pub trait GovernanceStore: Send + Sync {
    /// Store a governance domain
    fn store_domain(&self, domain: &GovernanceDomain) -> Result<()>;

    /// Retrieve a governance domain
    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>>;

    /// List all domains
    fn list_domains(&self) -> Result<Vec<GovernanceDomain>>;

    /// Store a proposal
    fn store_proposal(&self, proposal: &Proposal) -> Result<()>;

    /// Retrieve a proposal
    fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>>;

    /// List proposals for a domain
    fn list_proposals(&self, domain_id: &GovernanceDomainId) -> Result<Vec<Proposal>>;

    /// Store a vote
    fn store_vote(&self, vote: &Vote) -> Result<()>;

    /// Get a specific vote
    fn get_vote(&self, proposal_id: &ProposalId, voter: &Did) -> Result<Option<Vote>>;

    /// List all votes for a proposal
    fn list_votes(&self, proposal_id: &ProposalId) -> Result<Vec<Vote>>;

    /// Compute vote tally for a proposal
    fn compute_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally>;
}

/// In-memory governance store implementation
///
/// This is used for testing and as a reference implementation.
/// Production deployments should use a persistent store (Sled).
#[derive(Clone)]
pub struct InMemoryGovernanceStore {
    domains: Arc<std::sync::RwLock<HashMap<String, GovernanceDomain>>>,
    proposals: Arc<std::sync::RwLock<HashMap<String, Proposal>>>,
    votes: Arc<std::sync::RwLock<HashMap<String, Vec<Vote>>>>,
}

impl InMemoryGovernanceStore {
    /// Create a new in-memory store
    pub fn new() -> Self {
        Self {
            domains: Arc::new(std::sync::RwLock::new(HashMap::new())),
            proposals: Arc::new(std::sync::RwLock::new(HashMap::new())),
            votes: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryGovernanceStore {
    fn default() -> Self {
        Self::new()
    }
}

impl GovernanceStore for InMemoryGovernanceStore {
    fn store_domain(&self, domain: &GovernanceDomain) -> Result<()> {
        let mut domains = self.domains.write().unwrap();
        domains.insert(domain.id.0.clone(), domain.clone());
        Ok(())
    }

    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        let domains = self.domains.read().unwrap();
        Ok(domains.get(&id.0).cloned())
    }

    fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        let domains = self.domains.read().unwrap();
        Ok(domains.values().cloned().collect())
    }

    fn store_proposal(&self, proposal: &Proposal) -> Result<()> {
        let mut proposals = self.proposals.write().unwrap();
        proposals.insert(proposal.id.0.clone(), proposal.clone());
        Ok(())
    }

    fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>> {
        let proposals = self.proposals.read().unwrap();
        Ok(proposals.get(&id.0).cloned())
    }

    fn list_proposals(&self, domain_id: &GovernanceDomainId) -> Result<Vec<Proposal>> {
        let proposals = self.proposals.read().unwrap();
        Ok(proposals
            .values()
            .filter(|p| p.domain_id == *domain_id)
            .cloned()
            .collect())
    }

    fn store_vote(&self, vote: &Vote) -> Result<()> {
        let mut votes = self.votes.write().unwrap();
        let proposal_votes = votes.entry(vote.proposal_id.0.clone()).or_default();

        // Replace existing vote from same voter (allow vote changes)
        proposal_votes.retain(|v| v.voter != vote.voter);
        proposal_votes.push(vote.clone());

        Ok(())
    }

    fn get_vote(&self, proposal_id: &ProposalId, voter: &Did) -> Result<Option<Vote>> {
        let votes = self.votes.read().unwrap();
        Ok(votes
            .get(&proposal_id.0)
            .and_then(|v| v.iter().find(|vote| vote.voter == *voter).cloned()))
    }

    fn list_votes(&self, proposal_id: &ProposalId) -> Result<Vec<Vote>> {
        let votes = self.votes.read().unwrap();
        Ok(votes.get(&proposal_id.0).cloned().unwrap_or_default())
    }

    fn compute_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally> {
        let votes = self.list_votes(proposal_id)?;
        Ok(VoteTally::from(votes))
    }
}

/// Persistent governance store using Sled (reserved for future use)
#[cfg(feature = "governance_sled")]
#[allow(dead_code)]
pub struct SledGovernanceStore {
    db: sled::Db,
}

#[cfg(feature = "governance_sled")]
impl SledGovernanceStore {
    /// Create a new Sled-based governance store
    pub fn new(db: sled::Db) -> Self {
        Self { db }
    }

    fn domain_key(id: &GovernanceDomainId) -> Vec<u8> {
        format!("domain:{}", id.0).into_bytes()
    }

    fn proposal_key(id: &ProposalId) -> Vec<u8> {
        format!("proposal:{}", id.0).into_bytes()
    }

    fn vote_key(proposal_id: &ProposalId, voter: &Did) -> Vec<u8> {
        format!("vote:{}:{}", proposal_id.0, voter).into_bytes()
    }

    fn domain_index_key() -> Vec<u8> {
        b"index:domains".to_vec()
    }

    fn proposal_index_key(domain_id: &GovernanceDomainId) -> Vec<u8> {
        format!("index:proposals:{}", domain_id.0).into_bytes()
    }

    fn vote_index_key(proposal_id: &ProposalId) -> Vec<u8> {
        format!("index:votes:{}", proposal_id.0).into_bytes()
    }
}

#[cfg(feature = "governance_sled")]
impl GovernanceStore for SledGovernanceStore {
    fn store_domain(&self, domain: &GovernanceDomain) -> Result<()> {
        let key = Self::domain_key(&domain.id);
        let value = serde_json::to_vec(domain)?;
        self.db.insert(&key, value)?;

        // Update index
        let index_key = Self::domain_index_key();
        let mut domain_ids: Vec<String> = self
            .db
            .get(&index_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        if !domain_ids.contains(&domain.id.0) {
            domain_ids.push(domain.id.0.clone());
            self.db
                .insert(&index_key, serde_json::to_vec(&domain_ids)?)?;
        }

        Ok(())
    }

    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        let key = Self::domain_key(id);
        match self.db.get(&key)? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        let index_key = Self::domain_index_key();
        let domain_ids: Vec<String> = self
            .db
            .get(&index_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        let mut domains = Vec::new();
        for id in domain_ids {
            if let Some(domain) = self.get_domain(&GovernanceDomainId(id))? {
                domains.push(domain);
            }
        }

        Ok(domains)
    }

    fn store_proposal(&self, proposal: &Proposal) -> Result<()> {
        let key = Self::proposal_key(&proposal.id);
        let value = serde_json::to_vec(proposal)?;
        self.db.insert(&key, value)?;

        // Update index
        let index_key = Self::proposal_index_key(&proposal.domain_id);
        let mut proposal_ids: Vec<String> = self
            .db
            .get(&index_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        if !proposal_ids.contains(&proposal.id.0) {
            proposal_ids.push(proposal.id.0.clone());
            self.db
                .insert(&index_key, serde_json::to_vec(&proposal_ids)?)?;
        }

        Ok(())
    }

    fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>> {
        let key = Self::proposal_key(id);
        match self.db.get(&key)? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    fn list_proposals(&self, domain_id: &GovernanceDomainId) -> Result<Vec<Proposal>> {
        let index_key = Self::proposal_index_key(domain_id);
        let proposal_ids: Vec<String> = self
            .db
            .get(&index_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        let mut proposals = Vec::new();
        for id in proposal_ids {
            if let Some(proposal) = self.get_proposal(&ProposalId(id))? {
                proposals.push(proposal);
            }
        }

        Ok(proposals)
    }

    fn store_vote(&self, vote: &Vote) -> Result<()> {
        let key = Self::vote_key(&vote.proposal_id, &vote.voter);
        let value = serde_json::to_vec(vote)?;
        self.db.insert(&key, value)?;

        // Update index
        let index_key = Self::vote_index_key(&vote.proposal_id);
        let mut voter_dids: Vec<String> = self
            .db
            .get(&index_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        let voter_str = vote.voter.to_string();
        if !voter_dids.contains(&voter_str) {
            voter_dids.push(voter_str);
            self.db
                .insert(&index_key, serde_json::to_vec(&voter_dids)?)?;
        }

        Ok(())
    }

    fn get_vote(&self, proposal_id: &ProposalId, voter: &Did) -> Result<Option<Vote>> {
        let key = Self::vote_key(proposal_id, voter);
        match self.db.get(&key)? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    fn list_votes(&self, proposal_id: &ProposalId) -> Result<Vec<Vote>> {
        let index_key = Self::vote_index_key(proposal_id);
        let voter_dids: Vec<String> = self
            .db
            .get(&index_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        let mut votes = Vec::new();
        for voter_str in voter_dids {
            if let Ok(voter_did) = voter_str.parse() {
                if let Some(vote) = self.get_vote(proposal_id, &voter_did)? {
                    votes.push(vote);
                }
            }
        }

        Ok(votes)
    }

    fn compute_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally> {
        let votes = self.list_votes(proposal_id)?;
        Ok(VoteTally::from(votes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GovernanceConfig, ProposalPayload, VoteChoice};
    use icn_identity::KeyPair;

    #[test]
    fn test_in_memory_store_domain() {
        let store = InMemoryGovernanceStore::new();
        let config = GovernanceConfig::cooperative_default();
        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        store.store_domain(&domain).unwrap();

        let retrieved = store.get_domain(&domain.id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Coop");

        let domains = store.list_domains().unwrap();
        assert_eq!(domains.len(), 1);
    }

    #[test]
    fn test_in_memory_store_proposal() {
        let store = InMemoryGovernanceStore::new();
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test");

        let proposal = Proposal::new(
            domain_id.clone(),
            did,
            "Test Proposal".to_string(),
            "Description".to_string(),
            ProposalPayload::Text {
                body: "Should we do this?".to_string(),
            },
        );

        store.store_proposal(&proposal).unwrap();

        let retrieved = store.get_proposal(&proposal.id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test Proposal");

        let proposals = store.list_proposals(&domain_id).unwrap();
        assert_eq!(proposals.len(), 1);
    }

    #[test]
    fn test_in_memory_store_vote() {
        let store = InMemoryGovernanceStore::new();
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let proposal_id = ProposalId::generate();

        let vote = Vote::new(proposal_id.clone(), did.clone(), VoteChoice::For);

        store.store_vote(&vote).unwrap();

        let retrieved = store.get_vote(&proposal_id, &did).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().choice, VoteChoice::For);

        let votes = store.list_votes(&proposal_id).unwrap();
        assert_eq!(votes.len(), 1);
    }

    #[test]
    fn test_vote_replacement() {
        let store = InMemoryGovernanceStore::new();
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let proposal_id = ProposalId::generate();

        // Vote "For"
        let vote1 = Vote::new(proposal_id.clone(), did.clone(), VoteChoice::For);
        store.store_vote(&vote1).unwrap();

        // Change vote to "Against"
        let vote2 = Vote::new(proposal_id.clone(), did.clone(), VoteChoice::Against);
        store.store_vote(&vote2).unwrap();

        // Should only have one vote, and it should be "Against"
        let votes = store.list_votes(&proposal_id).unwrap();
        assert_eq!(votes.len(), 1);
        assert_eq!(votes[0].choice, VoteChoice::Against);
    }

    #[test]
    fn test_compute_tally() {
        let store = InMemoryGovernanceStore::new();
        let proposal_id = ProposalId::generate();

        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let kp3 = KeyPair::generate().unwrap();

        store
            .store_vote(&Vote::new(
                proposal_id.clone(),
                kp1.did().clone(),
                VoteChoice::For,
            ))
            .unwrap();

        store
            .store_vote(&Vote::new(
                proposal_id.clone(),
                kp2.did().clone(),
                VoteChoice::For,
            ))
            .unwrap();

        store
            .store_vote(&Vote::new(
                proposal_id.clone(),
                kp3.did().clone(),
                VoteChoice::Against,
            ))
            .unwrap();

        let tally = store.compute_tally(&proposal_id).unwrap();
        assert_eq!(tally.for_votes, 2);
        assert_eq!(tally.against_votes, 1);
        assert_eq!(tally.abstain_votes, 0);
    }
}
