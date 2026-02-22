//! GovernanceStateStore — typed storage abstraction for the GovernanceActor.
//!
//! This module decouples the actor from raw `icn_store::Store` byte operations.
//! The actor works entirely in domain terms; this module owns all key encoding
//! and serialization details.

use anyhow::Result;
use std::sync::Arc;

use icn_governance::{
    Delegation, DelegationId, GovernanceDomain, GovernanceDomainId, Proposal, ProposalId,
    Timestamp, Vote,
};
use icn_identity::Did;
use icn_store::Store;

// ---- Trait ----

/// State storage abstraction for the GovernanceActor.
///
/// The actor works in domain terms; the implementor handles serialization
/// and key encoding. This keeps raw byte-level concerns out of actor.rs.
pub trait GovernanceStateStore: Send + Sync {
    // --- Domains ---

    /// Load a domain by ID.
    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>>;

    /// Persist a domain (insert or overwrite).
    fn save_domain(&self, domain: &GovernanceDomain) -> Result<()>;

    /// Return all domains in the store.
    fn list_domains(&self) -> Result<Vec<GovernanceDomain>>;

    // --- Proposals ---

    /// Load a proposal by ID.
    fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>>;

    /// Persist a proposal (insert or overwrite).
    fn save_proposal(&self, proposal: &Proposal) -> Result<()>;

    /// Return all proposals in the store.
    fn list_proposals(&self) -> Result<Vec<Proposal>>;

    // --- Votes ---

    /// Load a single vote cast by `voter` on `proposal_id`.
    fn get_vote(&self, proposal_id: &ProposalId, voter: &Did) -> Result<Option<Vote>>;

    /// Persist a vote (insert or overwrite).
    fn save_vote(&self, proposal_id: &ProposalId, vote: &Vote) -> Result<()>;

    /// Return all votes for a given proposal.
    fn list_votes(&self, proposal_id: &ProposalId) -> Result<Vec<Vote>>;

    // --- Delegations ---

    /// Load a delegation by ID.
    fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>>;

    /// Persist a delegation (insert or overwrite).
    fn save_delegation(&self, delegation: &Delegation) -> Result<()>;

    /// Return all delegations in the store (unfiltered).
    fn list_all_delegations(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    /// Persist a delegation with an updated `revoked_at` field.
    fn save_revoked_delegation(&self, delegation: &Delegation, revoked_at: Timestamp)
        -> Result<()>;

    // --- Governance proofs ---

    /// Load the raw proof bytes for a closed proposal.
    fn get_proof_bytes(&self, proposal_id: &ProposalId) -> Result<Option<Vec<u8>>>;

    /// Persist raw proof bytes for a proposal.
    fn save_proof_bytes(&self, proposal_id: &ProposalId, proof: &[u8]) -> Result<()>;
}

// ---- Key helpers ----

fn domain_key(id: &GovernanceDomainId) -> Vec<u8> {
    format!("gov:domain:{}", id.0).into_bytes()
}

fn domain_key_prefix() -> &'static [u8] {
    b"gov:domain:"
}

fn proposal_key(id: &ProposalId) -> Vec<u8> {
    format!("gov:proposal:{}", id.0).into_bytes()
}

fn proposal_key_prefix() -> &'static [u8] {
    b"gov:proposal:"
}

fn vote_key(proposal_id: &ProposalId, voter: &Did) -> Vec<u8> {
    format!("gov:vote:{}:{}", proposal_id.0, voter).into_bytes()
}

fn vote_key_prefix(proposal_id: &ProposalId) -> Vec<u8> {
    format!("gov:vote:{}:", proposal_id.0).into_bytes()
}

fn delegation_key(id: &DelegationId) -> Vec<u8> {
    format!("gov:delegation:{}", id.0).into_bytes()
}

fn delegation_key_prefix() -> &'static [u8] {
    b"gov:delegation:"
}

fn proof_key(proposal_id: &ProposalId) -> Vec<u8> {
    format!("governance:proof:{}", proposal_id.0).into_bytes()
}

// ---- Utility ----

pub(crate) fn load_json<T: for<'a> serde::Deserialize<'a>>(
    store: &dyn Store,
    key: &[u8],
) -> Result<Option<T>> {
    match store.get(key)? {
        Some(v) => Ok(Some(serde_json::from_slice::<T>(&v)?)),
        None => Ok(None),
    }
}

// ---- SledGovernanceStateStore ----

/// `icn_store::Store`-backed implementation of [`GovernanceStateStore`].
///
/// All key encoding is centralised here; the actor never constructs raw keys.
pub struct SledGovernanceStateStore {
    store: Arc<dyn Store>,
}

impl SledGovernanceStateStore {
    /// Wrap an existing `Arc<dyn Store>`.
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    /// Expose the inner store for callers that still need raw KV access
    /// (e.g. `handle_incoming` which operates on a `&dyn Store`).
    pub fn as_store(&self) -> &dyn Store {
        self.store.as_ref()
    }
}

impl GovernanceStateStore for SledGovernanceStateStore {
    // --- Domains ---

    fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>> {
        load_json(self.store.as_ref(), &domain_key(id))
    }

    fn save_domain(&self, domain: &GovernanceDomain) -> Result<()> {
        let id = GovernanceDomainId(domain.id.0.clone());
        self.store
            .put(&domain_key(&id), &serde_json::to_vec(domain)?)
    }

    fn list_domains(&self) -> Result<Vec<GovernanceDomain>> {
        let rows = self.store.scan(domain_key_prefix())?;
        rows.into_iter()
            .map(|(_k, v)| Ok(serde_json::from_slice::<GovernanceDomain>(&v)?))
            .collect()
    }

    // --- Proposals ---

    fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>> {
        load_json(self.store.as_ref(), &proposal_key(id))
    }

    fn save_proposal(&self, proposal: &Proposal) -> Result<()> {
        self.store
            .put(&proposal_key(&proposal.id), &serde_json::to_vec(proposal)?)
    }

    fn list_proposals(&self) -> Result<Vec<Proposal>> {
        let rows = self.store.scan(proposal_key_prefix())?;
        rows.into_iter()
            .map(|(_k, v)| Ok(serde_json::from_slice::<Proposal>(&v)?))
            .collect()
    }

    // --- Votes ---

    fn get_vote(&self, proposal_id: &ProposalId, voter: &Did) -> Result<Option<Vote>> {
        load_json(self.store.as_ref(), &vote_key(proposal_id, voter))
    }

    fn save_vote(&self, proposal_id: &ProposalId, vote: &Vote) -> Result<()> {
        self.store.put(
            &vote_key(proposal_id, &vote.voter),
            &serde_json::to_vec(vote)?,
        )
    }

    fn list_votes(&self, proposal_id: &ProposalId) -> Result<Vec<Vote>> {
        let prefix = vote_key_prefix(proposal_id);
        let rows = self.store.scan(&prefix)?;
        rows.into_iter()
            .map(|(_k, v)| Ok(serde_json::from_slice::<Vote>(&v)?))
            .collect()
    }

    // --- Delegations ---

    fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>> {
        load_json(self.store.as_ref(), &delegation_key(id))
    }

    fn save_delegation(&self, delegation: &Delegation) -> Result<()> {
        self.store.put(
            &delegation_key(&delegation.id),
            &serde_json::to_vec(delegation)?,
        )
    }

    fn list_all_delegations(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.store.scan(delegation_key_prefix())
    }

    fn save_revoked_delegation(
        &self,
        delegation: &Delegation,
        revoked_at: Timestamp,
    ) -> Result<()> {
        let mut d = delegation.clone();
        d.revoked_at = Some(revoked_at);
        self.store
            .put(&delegation_key(&d.id), &serde_json::to_vec(&d)?)
    }

    // --- Proofs ---

    fn get_proof_bytes(&self, proposal_id: &ProposalId) -> Result<Option<Vec<u8>>> {
        self.store.get(&proof_key(proposal_id))
    }

    fn save_proof_bytes(&self, proposal_id: &ProposalId, proof: &[u8]) -> Result<()> {
        self.store.put(&proof_key(proposal_id), proof)
    }
}
