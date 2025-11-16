//! Governance handle trait for RPC integration without circular dependencies

use anyhow::Result;
use async_trait::async_trait;

use crate::{GovernanceDomain, GovernanceDomainId, Proposal, ProposalId};

/// Trait for governance operations exposed to RPC layer
///
/// This trait allows icn-rpc to interact with governance without depending on icn-core,
/// breaking the circular dependency: icn-core → icn-gateway → icn-rpc → icn-core
#[async_trait]
pub trait GovernanceOps: Send + Sync {
    /// List all governance domains
    async fn list_domains(&self) -> Result<Vec<GovernanceDomain>>;

    /// Get a specific domain by ID
    async fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>>;

    /// List all proposals
    async fn list_proposals(&self) -> Result<Vec<Proposal>>;

    /// Get a specific proposal by ID
    async fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>>;
}
