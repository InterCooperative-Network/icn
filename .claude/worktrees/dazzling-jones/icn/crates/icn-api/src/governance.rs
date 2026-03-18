//! Governance service for shared read operations.

use std::sync::Arc;

use icn_governance::{GovernanceDomain, GovernanceDomainId, GovernanceOps, Proposal, ProposalId};

use crate::error::ApiError;

/// Shared governance service used by both RPC and gateway layers.
pub struct GovernanceService {
    governance: Arc<dyn GovernanceOps + Send + Sync>,
}

impl GovernanceService {
    /// Create a governance service backed by a GovernanceOps handle.
    pub fn new(governance: Arc<dyn GovernanceOps + Send + Sync>) -> Self {
        Self { governance }
    }

    /// List all governance domains.
    pub async fn list_domains(&self) -> Result<Vec<GovernanceDomain>, ApiError> {
        self.governance
            .list_domains()
            .await
            .map_err(|e| ApiError::GovernanceError(e.to_string()))
    }

    /// Get a governance domain by id.
    pub async fn get_domain(&self, domain_id: &str) -> Result<Option<GovernanceDomain>, ApiError> {
        let domain_id = GovernanceDomainId(domain_id.to_string());
        self.governance
            .get_domain(&domain_id)
            .await
            .map_err(|e| ApiError::GovernanceError(e.to_string()))
    }

    /// List all proposals.
    pub async fn list_proposals(&self) -> Result<Vec<Proposal>, ApiError> {
        self.governance
            .list_proposals()
            .await
            .map_err(|e| ApiError::GovernanceError(e.to_string()))
    }

    /// Get proposal by id.
    pub async fn get_proposal(&self, proposal_id: &str) -> Result<Option<Proposal>, ApiError> {
        let proposal_id = ProposalId(proposal_id.to_string());
        self.governance
            .get_proposal(&proposal_id)
            .await
            .map_err(|e| ApiError::GovernanceError(e.to_string()))
    }
}
