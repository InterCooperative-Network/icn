//! Governance storage layer

use crate::{
    Delegation, DelegationId, DelegationScope, GovernanceDomain, GovernanceDomainId, Proposal,
    ProposalId, Timestamp, Vote, VoteTally,
};
use anyhow::Result;
use icn_identity::Did;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

/// Result of a paginated query
#[derive(Debug, Clone)]
pub struct PaginatedResult<T> {
    /// Items in the current page
    pub items: Vec<T>,
    /// Cursor for the next page, if any
    pub next_cursor: Option<String>,
    /// Total count of items (if available)
    pub total: Option<usize>,
}

impl<T> PaginatedResult<T> {
    /// Create a new paginated result
    pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self {
            items,
            next_cursor,
            total: None,
        }
    }

    /// Create a paginated result with total count
    pub fn with_total(mut self, total: usize) -> Self {
        self.total = Some(total);
        self
    }

    /// Check if there are more pages
    pub fn has_more(&self) -> bool {
        self.next_cursor.is_some()
    }
}

/// Trait for governance storage operations
pub trait GovernanceStore: Send + Sync {
    /// Store a governance domain
    fn store_domain(&self, domain: &GovernanceDomain) -> Result<()>;

    /// Retrieve a governance domain
    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>>;

    /// List all domains (deprecated - use list_domains_paginated for large datasets)
    fn list_domains(&self) -> Result<Vec<GovernanceDomain>>;

    /// List domains with pagination
    ///
    /// Returns a page of domains starting from the cursor position.
    /// If cursor is None, starts from the beginning.
    ///
    /// # Arguments
    /// * `cursor` - Optional cursor from previous page
    /// * `limit` - Maximum number of items to return
    fn list_domains_paginated(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<PaginatedResult<GovernanceDomain>>;

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

    // === Delegation Methods ===

    /// Store a delegation
    fn store_delegation(&self, delegation: &Delegation) -> Result<()>;

    /// Get a delegation by ID
    fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>>;

    /// Get all delegations from a delegator
    fn get_delegations_from(&self, delegator: &Did) -> Result<Vec<Delegation>>;

    /// Get all delegations to a delegate
    fn get_delegations_to(&self, delegate: &Did) -> Result<Vec<Delegation>>;

    /// Get active delegation for a delegator and scope
    fn get_active_delegation(
        &self,
        delegator: &Did,
        scope: &DelegationScope,
    ) -> Result<Option<Delegation>>;

    /// Revoke a delegation
    fn revoke_delegation(&self, id: &DelegationId, revoked_at: Timestamp) -> Result<()>;

    /// List all active delegations
    fn list_active_delegations(&self) -> Result<Vec<Delegation>>;
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
    delegations: Arc<std::sync::RwLock<HashMap<String, Delegation>>>,
}

impl InMemoryGovernanceStore {
    /// Create a new in-memory store
    pub fn new() -> Self {
        Self {
            domains: Arc::new(std::sync::RwLock::new(HashMap::new())),
            proposals: Arc::new(std::sync::RwLock::new(HashMap::new())),
            votes: Arc::new(std::sync::RwLock::new(HashMap::new())),
            delegations: Arc::new(std::sync::RwLock::new(HashMap::new())),
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
        let mut domains = self.domains.write().unwrap_or_else(|poisoned| {
            warn!("Domains lock poisoned, recovering");
            poisoned.into_inner()
        });
        domains.insert(domain.id.0.clone(), domain.clone());
        Ok(())
    }

    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        let domains = self.domains.read().unwrap_or_else(|poisoned| {
            warn!("Domains lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(domains.get(&id.0).cloned())
    }

    fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        let domains = self.domains.read().unwrap_or_else(|poisoned| {
            warn!("Domains lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(domains.values().cloned().collect())
    }

    fn list_domains_paginated(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<PaginatedResult<GovernanceDomain>> {
        let domains = self.domains.read().unwrap_or_else(|poisoned| {
            warn!("Domains lock poisoned, recovering");
            poisoned.into_inner()
        });

        // Parse cursor as offset (format: "offset:<number>")
        // Log invalid cursors for debugging but default to 0 for resilience
        let offset: usize = match cursor {
            Some(c) => match c.strip_prefix("offset:") {
                Some(s) => match s.parse::<usize>() {
                    Ok(n) => n,
                    Err(e) => {
                        warn!(cursor = %c, error = %e, "Invalid cursor format - starting from beginning");
                        0
                    }
                },
                None => {
                    warn!(cursor = %c, "Cursor missing 'offset:' prefix - starting from beginning");
                    0
                }
            },
            None => 0,
        };

        // Get sorted keys for deterministic ordering
        let mut keys: Vec<_> = domains.keys().collect();
        keys.sort();

        let total = keys.len();
        // Clamp offset to valid range to prevent out-of-bounds access
        let offset = offset.min(total);
        let items: Vec<GovernanceDomain> = keys
            .into_iter()
            .skip(offset)
            .take(limit)
            .filter_map(|k| domains.get(k).cloned())
            .collect();

        let next_offset = offset + items.len();
        let next_cursor = if next_offset < total {
            Some(format!("offset:{next_offset}"))
        } else {
            None
        };

        Ok(PaginatedResult::new(items, next_cursor).with_total(total))
    }

    fn store_proposal(&self, proposal: &Proposal) -> Result<()> {
        let mut proposals = self.proposals.write().unwrap_or_else(|poisoned| {
            warn!("Proposals lock poisoned, recovering");
            poisoned.into_inner()
        });
        proposals.insert(proposal.id.0.clone(), proposal.clone());
        Ok(())
    }

    fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>> {
        let proposals = self.proposals.read().unwrap_or_else(|poisoned| {
            warn!("Proposals lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(proposals.get(&id.0).cloned())
    }

    fn list_proposals(&self, domain_id: &GovernanceDomainId) -> Result<Vec<Proposal>> {
        let proposals = self.proposals.read().unwrap_or_else(|poisoned| {
            warn!("Proposals lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(proposals
            .values()
            .filter(|p| p.domain_id == *domain_id)
            .cloned()
            .collect())
    }

    fn store_vote(&self, vote: &Vote) -> Result<()> {
        let mut votes = self.votes.write().unwrap_or_else(|poisoned| {
            warn!("Votes lock poisoned, recovering");
            poisoned.into_inner()
        });
        let proposal_votes = votes.entry(vote.proposal_id.0.clone()).or_default();

        // Replace existing vote from same voter (allow vote changes)
        proposal_votes.retain(|v| v.voter != vote.voter);
        proposal_votes.push(vote.clone());

        Ok(())
    }

    fn get_vote(&self, proposal_id: &ProposalId, voter: &Did) -> Result<Option<Vote>> {
        let votes = self.votes.read().unwrap_or_else(|poisoned| {
            warn!("Votes lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(votes
            .get(&proposal_id.0)
            .and_then(|v| v.iter().find(|vote| vote.voter == *voter).cloned()))
    }

    fn list_votes(&self, proposal_id: &ProposalId) -> Result<Vec<Vote>> {
        let votes = self.votes.read().unwrap_or_else(|poisoned| {
            warn!("Votes lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(votes.get(&proposal_id.0).cloned().unwrap_or_default())
    }

    fn compute_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally> {
        let votes = self.list_votes(proposal_id)?;
        Ok(VoteTally::from(votes))
    }

    // === Delegation Methods ===

    fn store_delegation(&self, delegation: &Delegation) -> Result<()> {
        let mut delegations = self.delegations.write().unwrap_or_else(|poisoned| {
            warn!("Delegations lock poisoned, recovering");
            poisoned.into_inner()
        });
        delegations.insert(delegation.id.0.clone(), delegation.clone());
        Ok(())
    }

    fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        let delegations = self.delegations.read().unwrap_or_else(|poisoned| {
            warn!("Delegations lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(delegations.get(&id.0).cloned())
    }

    fn get_delegations_from(&self, delegator: &Did) -> Result<Vec<Delegation>> {
        let delegations = self.delegations.read().unwrap_or_else(|poisoned| {
            warn!("Delegations lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(delegations
            .values()
            .filter(|d| d.delegator == *delegator)
            .cloned()
            .collect())
    }

    fn get_delegations_to(&self, delegate: &Did) -> Result<Vec<Delegation>> {
        let delegations = self.delegations.read().unwrap_or_else(|poisoned| {
            warn!("Delegations lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(delegations
            .values()
            .filter(|d| d.delegate == *delegate)
            .cloned()
            .collect())
    }

    fn get_active_delegation(
        &self,
        delegator: &Did,
        scope: &DelegationScope,
    ) -> Result<Option<Delegation>> {
        let now = icn_time::current_timestamp_secs();
        let delegations = self.delegations.read().unwrap_or_else(|poisoned| {
            warn!("Delegations lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(delegations
            .values()
            .find(|d| d.delegator == *delegator && d.scope == *scope && d.is_active(now))
            .cloned())
    }

    fn revoke_delegation(&self, id: &DelegationId, revoked_at: Timestamp) -> Result<()> {
        let mut delegations = self.delegations.write().unwrap_or_else(|poisoned| {
            warn!("Delegations lock poisoned, recovering");
            poisoned.into_inner()
        });
        if let Some(delegation) = delegations.get_mut(&id.0) {
            delegation.revoked_at = Some(revoked_at);
        }
        Ok(())
    }

    fn list_active_delegations(&self) -> Result<Vec<Delegation>> {
        let now = icn_time::current_timestamp_secs();
        let delegations = self.delegations.read().unwrap_or_else(|poisoned| {
            warn!("Delegations lock poisoned, recovering");
            poisoned.into_inner()
        });
        Ok(delegations
            .values()
            .filter(|d| d.is_active(now))
            .cloned()
            .collect())
    }
}

/// Persistent governance store using Sled (reserved for future use)
#[allow(dead_code)]
pub struct SledGovernanceStore {
    db: sled::Db,
}

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

    fn delegation_key(id: &DelegationId) -> Vec<u8> {
        format!("delegation:{}", id.0).into_bytes()
    }

    fn delegation_index_key() -> Vec<u8> {
        b"index:delegations".to_vec()
    }

    fn delegation_from_index_key(delegator: &Did) -> Vec<u8> {
        format!("index:delegations:from:{delegator}").into_bytes()
    }

    fn delegation_to_index_key(delegate: &Did) -> Vec<u8> {
        format!("index:delegations:to:{delegate}").into_bytes()
    }
}

#[allow(dead_code)]
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

    fn list_domains_paginated(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<PaginatedResult<GovernanceDomain>> {
        let index_key = Self::domain_index_key();
        let domain_ids: Vec<String> = self
            .db
            .get(&index_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        // Parse cursor as offset (format: "offset:<number>")
        // Log invalid cursors for debugging but default to 0 for resilience
        let offset: usize = match cursor {
            Some(c) => match c.strip_prefix("offset:") {
                Some(s) => match s.parse::<usize>() {
                    Ok(n) => n,
                    Err(e) => {
                        warn!(cursor = %c, error = %e, "Invalid cursor format - starting from beginning");
                        0
                    }
                },
                None => {
                    warn!(cursor = %c, "Cursor missing 'offset:' prefix - starting from beginning");
                    0
                }
            },
            None => 0,
        };

        let total = domain_ids.len();
        // Clamp offset to valid range to prevent out-of-bounds access
        let offset = offset.min(total);

        // Only fetch the domains we need for this page
        let page_ids: Vec<_> = domain_ids.into_iter().skip(offset).take(limit).collect();

        let mut domains = Vec::with_capacity(page_ids.len());
        for id in page_ids {
            if let Some(domain) = self.get_domain(&GovernanceDomainId(id))? {
                domains.push(domain);
            }
        }

        let next_offset = offset + domains.len();
        let next_cursor = if next_offset < total {
            Some(format!("offset:{next_offset}"))
        } else {
            None
        };

        Ok(PaginatedResult::new(domains, next_cursor).with_total(total))
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

    // === Delegation Methods ===

    fn store_delegation(&self, delegation: &Delegation) -> Result<()> {
        let key = Self::delegation_key(&delegation.id);
        let value = serde_json::to_vec(delegation)?;
        self.db.insert(&key, value)?;

        // Update main index
        let index_key = Self::delegation_index_key();
        let mut delegation_ids: Vec<String> = self
            .db
            .get(&index_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        if !delegation_ids.contains(&delegation.id.0) {
            delegation_ids.push(delegation.id.0.clone());
            self.db
                .insert(&index_key, serde_json::to_vec(&delegation_ids)?)?;
        }

        // Update delegator index
        let from_key = Self::delegation_from_index_key(&delegation.delegator);
        let mut from_ids: Vec<String> = self
            .db
            .get(&from_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        if !from_ids.contains(&delegation.id.0) {
            from_ids.push(delegation.id.0.clone());
            self.db.insert(&from_key, serde_json::to_vec(&from_ids)?)?;
        }

        // Update delegate index
        let to_key = Self::delegation_to_index_key(&delegation.delegate);
        let mut to_ids: Vec<String> = self
            .db
            .get(&to_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        if !to_ids.contains(&delegation.id.0) {
            to_ids.push(delegation.id.0.clone());
            self.db.insert(&to_key, serde_json::to_vec(&to_ids)?)?;
        }

        Ok(())
    }

    fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        let key = Self::delegation_key(id);
        match self.db.get(&key)? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    fn get_delegations_from(&self, delegator: &Did) -> Result<Vec<Delegation>> {
        let from_key = Self::delegation_from_index_key(delegator);
        let delegation_ids: Vec<String> = self
            .db
            .get(&from_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        let mut delegations = Vec::new();
        for id in delegation_ids {
            if let Some(delegation) = self.get_delegation(&DelegationId(id))? {
                delegations.push(delegation);
            }
        }

        Ok(delegations)
    }

    fn get_delegations_to(&self, delegate: &Did) -> Result<Vec<Delegation>> {
        let to_key = Self::delegation_to_index_key(delegate);
        let delegation_ids: Vec<String> = self
            .db
            .get(&to_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        let mut delegations = Vec::new();
        for id in delegation_ids {
            if let Some(delegation) = self.get_delegation(&DelegationId(id))? {
                delegations.push(delegation);
            }
        }

        Ok(delegations)
    }

    fn get_active_delegation(
        &self,
        delegator: &Did,
        scope: &DelegationScope,
    ) -> Result<Option<Delegation>> {
        let now = icn_time::current_timestamp_secs();
        let delegations = self.get_delegations_from(delegator)?;
        Ok(delegations
            .into_iter()
            .find(|d| d.scope == *scope && d.is_active(now)))
    }

    fn revoke_delegation(&self, id: &DelegationId, revoked_at: Timestamp) -> Result<()> {
        if let Some(mut delegation) = self.get_delegation(id)? {
            delegation.revoked_at = Some(revoked_at);
            let key = Self::delegation_key(id);
            let value = serde_json::to_vec(&delegation)?;
            self.db.insert(&key, value)?;
        }
        Ok(())
    }

    fn list_active_delegations(&self) -> Result<Vec<Delegation>> {
        let now = icn_time::current_timestamp_secs();
        let index_key = Self::delegation_index_key();
        let delegation_ids: Vec<String> = self
            .db
            .get(&index_key)?
            .map(|v| serde_json::from_slice(&v).unwrap_or_default())
            .unwrap_or_default();

        let mut delegations = Vec::new();
        for id in delegation_ids {
            if let Some(delegation) = self.get_delegation(&DelegationId(id))? {
                if delegation.is_active(now) {
                    delegations.push(delegation);
                }
            }
        }

        Ok(delegations)
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

    #[test]
    fn test_store_delegation() {
        let store = InMemoryGovernanceStore::new();
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let delegator = kp1.did().clone();
        let delegate = kp2.did().clone();

        let delegation = Delegation::new(
            delegator.clone(),
            delegate.clone(),
            DelegationScope::Blanket,
        );
        let id = delegation.id.clone();

        store.store_delegation(&delegation).unwrap();

        let retrieved = store.get_delegation(&id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().delegator, delegator);
    }

    #[test]
    fn test_get_delegations_from() {
        let store = InMemoryGovernanceStore::new();
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let kp3 = KeyPair::generate().unwrap();
        let alice = kp1.did().clone();
        let bob = kp2.did().clone();
        let charlie = kp3.did().clone();

        // Alice delegates to both Bob and Charlie (different scopes)
        let d1 = Delegation::new(alice.clone(), bob.clone(), DelegationScope::Blanket);
        let d2 = Delegation::new(
            alice.clone(),
            charlie.clone(),
            DelegationScope::Domain(GovernanceDomainId::new("test")),
        );

        store.store_delegation(&d1).unwrap();
        store.store_delegation(&d2).unwrap();

        let delegations = store.get_delegations_from(&alice).unwrap();
        assert_eq!(delegations.len(), 2);
    }

    #[test]
    fn test_get_delegations_to() {
        let store = InMemoryGovernanceStore::new();
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let kp3 = KeyPair::generate().unwrap();
        let alice = kp1.did().clone();
        let bob = kp2.did().clone();
        let charlie = kp3.did().clone();

        // Alice and Bob both delegate to Charlie
        let d1 = Delegation::new(alice.clone(), charlie.clone(), DelegationScope::Blanket);
        let d2 = Delegation::new(bob.clone(), charlie.clone(), DelegationScope::Blanket);

        store.store_delegation(&d1).unwrap();
        store.store_delegation(&d2).unwrap();

        let delegations = store.get_delegations_to(&charlie).unwrap();
        assert_eq!(delegations.len(), 2);
    }

    #[test]
    fn test_revoke_delegation() {
        let store = InMemoryGovernanceStore::new();
        let kp1 = KeyPair::generate().unwrap();
        let kp2 = KeyPair::generate().unwrap();
        let delegator = kp1.did().clone();
        let delegate = kp2.did().clone();

        let delegation = Delegation::new(
            delegator.clone(),
            delegate.clone(),
            DelegationScope::Blanket,
        );
        let id = delegation.id.clone();

        store.store_delegation(&delegation).unwrap();

        // Initially should be in active list
        let active = store.list_active_delegations().unwrap();
        assert_eq!(active.len(), 1);

        // Revoke
        let now = icn_time::current_timestamp_secs();
        store.revoke_delegation(&id, now).unwrap();

        // Should no longer be in active list
        let active = store.list_active_delegations().unwrap();
        assert_eq!(active.len(), 0);
    }
}
