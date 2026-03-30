//! HTTP request/response models for governance endpoints.
//!
//! These were previously in `icn-gateway/src/models.rs`. Moving them here
//! ensures governance domain logic (including its wire format) lives entirely
//! in the app layer.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// ============================================================================
// Domain
// ============================================================================

/// Create a new governance domain
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateDomainRequest {
    pub id: String,
    pub name: String,
    pub profile: String,
    pub quorum_percent: u8,
    pub approval_percent: u8,
    pub voting_period_days: u64,
    pub members: Vec<String>,
}

/// Add a member to a governance domain
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddDomainMemberRequest {
    pub did: String,
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}

// ============================================================================
// Proposals
// ============================================================================

/// Scope for a proposal (local or federation-wide)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProposalScopeRequest {
    Local,
    Federation { federation_id: String },
}

impl Default for ProposalScopeRequest {
    fn default() -> Self {
        Self::Local
    }
}

/// Create a new proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub payload: ProposalPayloadRequest,
    #[serde(default)]
    pub scope: Option<ProposalScopeRequest>,
}

/// Proposal payload types
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProposalPayloadRequest {
    Text {
        body: String,
    },
    Budget {
        amount: i64,
        recipient: String,
        currency: String,
        purpose: String,
    },
    Membership {
        action: String,
        did: String,
    },
    ConfigChange {
        key: String,
        value: String,
    },
    /// Charter ratification — members vote to adopt a CCL charter document.
    Charter {
        /// Stable charter identifier (cooperative DID or human-readable name).
        charter_id: String,
        /// Complete YAML charter document (CCL schema_version: v0).
        charter_yaml: String,
    },
}

/// Request body for `POST /proposals/sdis/appoint-steward`.
///
/// Proposer must be an active steward (checked via `GovernanceContext::steward_checker`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AppointStewardProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    /// DID of the steward candidate.
    pub candidate: String,
    /// Geographic or operational region the steward will serve.
    pub region: String,
    /// Bond amount (in commons credits) the steward must post.
    pub bond_amount: i64,
    /// Proposed term length in seconds.
    pub term_length_seconds: u64,
    /// DIDs of stewards sponsoring this candidate. May be empty.
    #[serde(default)]
    pub sponsors: Vec<String>,
}

/// Request body for `POST /proposals/sdis/remove-steward`.
///
/// Proposer must be an active steward (checked via `GovernanceContext::steward_checker`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemoveStewardProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    /// DID of the steward to remove.
    pub steward: String,
    /// Reason for removal.
    pub reason: String,
    /// Whether the steward's bond should be returned on removal.
    #[serde(default)]
    pub return_bond: bool,
}

/// Open a proposal for voting
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OpenProposalRequest {
    pub voting_period_seconds: Option<u64>,
}

/// Cast a vote on a proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CastVoteRequest {
    pub choice: String,
    pub comment: Option<String>,
}

/// Vote choice response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VoteChoiceResponse {
    For,
    Against,
    Abstain,
}

/// Gateway response DTO for governance proposals
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProposalResponse {
    pub id: String,
    pub domain_id: String,
    pub proposer: String,
    pub title: String,
    pub description: String,
    pub state: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_ref: Option<String>,
}

// ============================================================================
// Vote Delegation
// ============================================================================

/// Create a new vote delegation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateDelegationRequest {
    pub delegate: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

/// Delegation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DelegationResponse {
    pub id: String,
    pub delegator: String,
    pub delegate: String,
    pub scope: String,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<u64>,
    pub is_active: bool,
}

/// List of delegations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DelegationListResponse {
    pub given: Vec<DelegationResponse>,
    pub received: Vec<DelegationResponse>,
}

// ============================================================================
// Federation Proposals
// ============================================================================

/// Common fields for federation proposal requests
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FederationProposalCommon {
    pub domain_id: String,
    pub title: String,
    pub description: String,
}

/// Federation terms for join proposals
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FederationTermsRequest {
    pub min_trust_threshold: f64,
    pub governance_binding: bool,
    pub data_sharing_level: String,
    pub dispute_resolution: String,
}

/// Request to create a "join federation" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JoinFederationProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub federation_id: String,
    pub terms: FederationTermsRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sponsor_coop_id: Option<String>,
}

/// Request to create a "leave federation" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LeaveFederationProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub federation_id: String,
    pub reason: String,
    pub grace_period_days: u32,
}

/// Request to create an "establish clearing" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EstablishClearingProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub partner_coop_id: String,
    pub partner_coop_did: String,
    pub max_imbalance: i64,
    pub settlement_interval: String,
    pub currency: String,
}

/// Request to create a "terminate clearing" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TerminateClearingProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub partner_coop_id: String,
    pub reason: String,
}

/// Request to create a "vouch for cooperative" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VouchProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub target_coop_id: String,
    pub target_coop_did: String,
    pub trust_score: f64,
    pub context: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// Request to create a "revoke vouch" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RevokeVouchProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    pub target_coop_id: String,
    pub reason: String,
}

/// Request to create an "update federation policy" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateFederationPolicyProposalRequest {
    pub domain_id: String,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_accept_vouch_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_decay_factor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attestations_per_minute: Option<u32>,
}

// ============================================================================
// Action Items
// ============================================================================

/// Request to create a new action item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateActionItemRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<u64>,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_proposal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_context: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_priority() -> String {
    "medium".to_string()
}

/// Request to update an action item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateActionItemRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Request to add a note to an action item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddActionItemNoteRequest {
    pub content: String,
}

/// Action item response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionItemResponse {
    pub id: String,
    pub domain_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<u64>,
    pub status: String,
    pub priority: String,
    pub created_by: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_proposal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_context: Option<String>,
    pub tags: Vec<String>,
    pub notes: Vec<ActionItemNoteResponse>,
    pub is_overdue: bool,
}

/// Action item note response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionItemNoteResponse {
    pub id: String,
    pub author: String,
    pub content: String,
    pub created_at: u64,
}

/// Query parameters for listing action items
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionItemFilterParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overdue: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

// ============================================================================
// Discussion
// ============================================================================

/// Request to add a comment to a proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddCommentRequest {
    pub content: String,
    pub parent_id: Option<String>,
}

/// Request to edit an existing comment
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EditCommentRequest {
    pub content: String,
}

/// Request to add an emoji reaction to a comment
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddReactionRequest {
    pub emoji: String,
}

/// Request to remove an emoji reaction from a comment (DELETE with body)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemoveReactionRequest {
    pub emoji: String,
}

/// Query parameters for listing comments
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListCommentsQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Single comment response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CommentResponse {
    pub id: String,
    pub proposal_id: String,
    pub author: String,
    pub content: String,
    pub parent_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub reactions: std::collections::HashMap<String, usize>,
    pub is_edited: bool,
    pub is_deleted: bool,
}

/// List of comments with pagination metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListCommentsResponse {
    pub comments: Vec<CommentResponse>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
}

/// Full discussion for a proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscussionResponse {
    pub proposal_id: String,
    pub comments: Vec<CommentResponse>,
    pub participant_count: usize,
    pub last_activity_at: u64,
}

// ============================================================================
// Delegation helpers
// ============================================================================

/// Query parameters for listing delegations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListDelegationsQuery {
    #[serde(default)]
    pub include_revoked: bool,
}

// ============================================================================
// Action item helpers
// ============================================================================

/// Request to update only the status of an action item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StatusUpdateRequest {
    pub status: String,
}

/// Request to remove a domain member (DELETE with body)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemoveDomainMemberRequest {
    pub did: String,
}
