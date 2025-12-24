//! Governance handle trait for RPC integration without circular dependencies

use anyhow::Result;
use async_trait::async_trait;
use icn_identity::Did;

use crate::{
    Delegation, DelegationId, GovernanceDomain, GovernanceDomainId, GovernanceParams,
    MembershipConfig, ParameterChange, Proposal, ProposalId, ProposalPayload, ProtocolParameter,
    Timestamp, VoteChoice, VoteTally,
};
use icn_entity::EntityId;

/// Trait for governance operations exposed to RPC layer
///
/// This trait allows icn-rpc to interact with governance without depending on icn-core,
/// breaking the circular dependency: icn-core → icn-gateway → icn-rpc → icn-core
#[async_trait]
pub trait GovernanceOps: Send + Sync {
    // Read operations

    /// List all governance domains
    async fn list_domains(&self) -> Result<Vec<GovernanceDomain>>;

    /// Get a specific domain by ID
    async fn get_domain(&self, id: &GovernanceDomainId) -> Result<Option<GovernanceDomain>>;

    /// List all proposals
    async fn list_proposals(&self) -> Result<Vec<Proposal>>;

    /// Get a specific proposal by ID
    async fn get_proposal(&self, id: &ProposalId) -> Result<Option<Proposal>>;

    // Write operations

    /// Create a new governance domain
    async fn create_domain(
        &self,
        domain_id: GovernanceDomainId,
        name: String,
        profile: String,
        params: GovernanceParams,
        membership: MembershipConfig,
    ) -> Result<()>;

    /// Create a new proposal in a domain
    async fn create_proposal(
        &self,
        domain_id: GovernanceDomainId,
        title: String,
        description: String,
        payload: ProposalPayload,
    ) -> Result<ProposalId>;

    /// Open a proposal for voting
    async fn open_proposal(
        &self,
        proposal_id: ProposalId,
        voting_period_seconds: u64,
    ) -> Result<()>;

    /// Cast a vote on a proposal
    async fn cast_vote(
        &self,
        proposal_id: ProposalId,
        choice: VoteChoice,
        comment: Option<String>,
    ) -> Result<()>;

    /// Close a proposal and evaluate the outcome
    async fn close_proposal(&self, proposal_id: ProposalId) -> Result<()>;

    // Delegation operations

    /// Create a new vote delegation
    async fn create_delegation(&self, delegation: Delegation) -> Result<()>;

    /// Get a delegation by ID
    async fn get_delegation(&self, id: &DelegationId) -> Result<Option<Delegation>>;

    /// Get all delegations given by a specific DID
    async fn get_delegations_from(&self, delegator: &Did) -> Result<Vec<Delegation>>;

    /// Get all delegations received by a specific DID
    async fn get_delegations_to(&self, delegate: &Did) -> Result<Vec<Delegation>>;

    /// Revoke a delegation
    async fn revoke_delegation(&self, id: &DelegationId, revoked_at: Timestamp) -> Result<()>;

    // Vote tracking operations

    /// Get the vote tally for a proposal
    ///
    /// Returns the current vote counts (for, against, abstain) for a proposal.
    async fn get_vote_tally(&self, proposal_id: &ProposalId) -> Result<VoteTally>;

    /// Get the list of DIDs who voted on a proposal
    ///
    /// Returns all DIDs that have cast votes on the proposal.
    /// Useful for notifications and audit purposes.
    async fn get_voter_dids(&self, proposal_id: &ProposalId) -> Result<Vec<Did>>;

    // Protocol parameter operations (Phase 20)

    /// List all protocol parameters
    ///
    /// Returns all defined protocol parameters with their current values.
    async fn list_protocol_parameters(&self) -> Result<Vec<ProtocolParameter>>;

    /// Get a specific protocol parameter by ID
    ///
    /// Returns the parameter if it exists.
    async fn get_protocol_parameter(&self, id: &str) -> Result<Option<ProtocolParameter>>;

    /// Get the effective value of a protocol parameter with scope resolution
    ///
    /// Scope resolution order: Cooperative > Federation > Global
    /// If coop_id is provided and has an override, that value is returned.
    /// Otherwise, if fed_id is provided and has an override, that value is returned.
    /// Otherwise, the global value is returned.
    async fn get_effective_protocol_parameter(
        &self,
        id: &str,
        coop_id: Option<&EntityId>,
        fed_id: Option<&EntityId>,
    ) -> Result<Option<ProtocolParameter>>;

    /// Get the change history for a protocol parameter
    ///
    /// Returns all historical changes to the parameter, ordered by timestamp.
    async fn get_protocol_parameter_history(&self, id: &str) -> Result<Vec<ParameterChange>>;
}
