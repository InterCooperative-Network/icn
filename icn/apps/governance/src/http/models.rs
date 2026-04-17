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

    /// Decision mode: `"majority"` (default) or `"consent"`.
    ///
    /// In consent mode, proposals pass unless objections exceed `max_objections`.
    #[serde(default)]
    pub decision_mode: Option<String>,

    /// Maximum number of objections (against-votes) allowed in consent mode.
    /// Default `0` means any objection blocks the proposal (strict consensus).
    /// Only meaningful when `decision_mode` is `"consent"`.
    #[serde(default)]
    pub max_objections: Option<u8>,
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
    /// Action items to create when this proposal is accepted.
    /// Each spec becomes a linked ActionItem with provenance to this proposal.
    #[serde(default)]
    pub action_items_on_accept: Vec<ActionItemSpecRequest>,
}

/// Template for creating action items from accepted proposals (API model).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionItemSpecRequest {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Assignee DID (optional)
    #[serde(default)]
    pub assignee: Option<String>,
    /// Seconds after acceptance before this item is due
    #[serde(default)]
    pub due_offset_seconds: Option<u64>,
    /// Priority: low, medium (default), high, critical
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
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

/// Query parameters for `GET /gov/me/work`.
///
/// A subset of [`ActionItemFilterParams`] — the `assignee` field is omitted
/// because `me/work` implicitly filters by the authenticated caller's DID.
/// All fields default to `None` (no filtering applied).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct MyWorkFilterParams {
    // Per-field `serde(default)` is required for serde_urlencoded (used by
    // actix-web `web::Query`) to fill absent query params with `None`. Struct-level
    // `#[serde(default)]` is not sufficient for key-value formats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overdue: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

// ============================================================================
// Institutional Structure (committees, working groups, teams)
// ============================================================================

/// Request to create an internal structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateStructureRequest {
    /// Kind: "committee", "working_group", "team", "office"
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Structure response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StructureResponse {
    pub id: String,
    pub entity_id: String,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    pub created_at: u64,
}

/// Request to assign a role in a structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AssignRoleRequest {
    /// DID of the person receiving the role
    pub did: String,
    /// Role name: "coordinator", "member", "facilitator", "note_taker", etc.
    pub role: String,
}

/// Role assignment response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RoleAssignmentResponse {
    pub id: String,
    pub structure_id: String,
    pub person_did: String,
    pub role: String,
    pub start_date: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<u64>,
}

// ============================================================================
// Activity (events, programs, projects, initiatives)
// ============================================================================

/// Request to create an activity
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateActivityRequest {
    /// Kind: "event", "program", "project", "initiative"
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<u64>,
    /// Optional parent program ID (e.g., "annual-summit-cycle")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_program_id: Option<String>,
}

/// Activity response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActivityResponse {
    pub id: String,
    pub entity_id: String,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<u64>,
    pub linked_structures: Vec<String>,
    /// Parent program ID if this activity executes within a program cycle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_program_id: Option<String>,
    pub created_at: u64,
}

// ============================================================================
// Meeting (deliberation trace objects)
// ============================================================================

/// Request to create a meeting
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateMeetingRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Scheduled start time as Unix timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<u64>,
}

/// Request to add an attendee to a meeting
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddAttendeeRequest {
    pub did: String,
    /// Coordination role: "facilitator", "note_taker", "participant", "observer"
    #[serde(default = "default_meeting_role")]
    pub meeting_role: String,
}

fn default_meeting_role() -> String {
    "participant".to_string()
}

/// Request to mark attendance for a participant
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MarkAttendanceRequest {
    pub did: String,
    /// Status: "present", "absent", "remote"
    pub status: String,
}

/// Request to add an agenda item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddAgendaItemRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presenter: Option<String>,
    /// Proposal ID to discuss during this agenda item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_proposal: Option<String>,
}

/// Request to update an agenda item outcome
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateAgendaItemRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discussion_notes: Option<String>,
    /// Outcome: "resolved", "tabled", "referred", "no_action"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// Attendee in a meeting response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MeetingAttendeeResponse {
    pub did: String,
    pub status: String,
    pub meeting_role: String,
}

/// Agenda item in a meeting response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AgendaItemResponse {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presenter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_proposal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discussion_notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    pub generated_action_items: Vec<String>,
}

/// Meeting response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MeetingResponse {
    pub id: String,
    pub domain_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<u64>,
    pub attendees: Vec<MeetingAttendeeResponse>,
    pub agenda: Vec<AgendaItemResponse>,
    pub linked_structures: Vec<String>,
    pub linked_activities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_doc_id: Option<String>,
    pub created_by: String,
    pub created_at: u64,
    pub present_count: usize,
}

// ============================================================================
// Program (multi-phase institutional endeavors)
// ============================================================================

/// Request to create a program
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateProgramRequest {
    /// Entity ID of the owning entity (cooperative, community, federation)
    pub parent_entity_id: String,
    /// Kind: "cycle", "campaign", "initiative", "series", or a custom string
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional start time as Unix timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<u64>,
    /// Optional end time as Unix timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_at: Option<u64>,
    /// Proposal ID that authorized this program (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_decision: Option<String>,
}

/// Program response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProgramResponse {
    pub id: String,
    pub domain_id: String,
    pub parent_entity_id: String,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_at: Option<u64>,
    pub milestones: Vec<String>,
    pub activities: Vec<String>,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_decision: Option<String>,
}

// ============================================================================
// Milestone (stage-gates within programs)
// ============================================================================

/// Request to create a milestone
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateMilestoneRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Ordinal position among the program's milestones (0-based)
    #[serde(default)]
    pub phase_index: u32,
    /// Optional target date as Unix timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<u64>,
    /// Free-form checklist of completion criteria
    #[serde(default)]
    pub completion_criteria: Vec<String>,
    /// Optional CCL completion gate expression (JSON-encoded `icn_ccl::Expr`).
    /// When present the gate is validated immediately; a malformed expression
    /// causes the whole create request to be rejected with 400.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub completion_gate: Option<serde_json::Value>,
}

/// Request to update a milestone's status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateMilestoneStatusRequest {
    /// Status: "pending", "in_progress", "completed", "blocked", "skipped"
    pub status: String,
}

/// Request to set or clear the CCL completion gate on a milestone.
///
/// `PUT /gov/milestones/{milestone_id}/gate`
///
/// Set `completion_gate` to a JSON-encoded `icn_ccl::Expr` object to install a
/// gate.  Set it to `null` (or omit the field) to remove an existing gate.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SetMilestoneGateRequest {
    /// The CCL gate expression as a JSON object matching `icn_ccl::Expr`, or
    /// `null` to remove the gate.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub completion_gate: Option<serde_json::Value>,
}

/// Milestone response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MilestoneResponse {
    pub id: String,
    pub program_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub phase_index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<u64>,
    pub status: String,
    pub completion_criteria: Vec<String>,
    /// The active CCL completion gate expression, if any.
    /// Returned as a JSON object (the `icn_ccl::Expr` tree) so clients can
    /// inspect or display it without additional decoding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_gate: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_by: Option<String>,
    pub created_at: u64,
}

// ============================================================================
// Program dashboard (composite read surface)
// ============================================================================

/// Compact milestone row inside the program dashboard.
///
/// Uses the full `MilestoneResponse` field set minus `completion_criteria` and
/// `created_at`, which are detail-level and inflate the composite response
/// without helping a dashboard consumer understand program progress.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardMilestoneSummary {
    pub id: String,
    pub name: String,
    pub phase_index: u32,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
}

/// Milestone status counts for the program dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct DashboardMilestoneCounts {
    pub total: usize,
    pub completed: usize,
    pub in_progress: usize,
    pub blocked: usize,
    pub pending: usize,
    pub skipped: usize,
}

/// Compact activity row inside the program dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DashboardActivitySummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub status: String,
}

/// Action item status counts for the program dashboard.
///
/// Counts only items whose `parent` is an `Activity` listed in the program's
/// `activities` vec. Domain items with no activity parent or a parent that
/// does not belong to this program are excluded.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct DashboardActionItemCounts {
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
    pub deferred: usize,
    pub cancelled: usize,
    pub total: usize,
}

/// Composite program dashboard response.
///
/// Returned by `GET /gov/programs/{program_id}/dashboard`. Combines the program
/// record, its ordered milestones (with status counts), its linked activities,
/// and action item counts scoped to those activities.
///
/// Meetings are intentionally absent from v1: the `Meeting` type links to
/// activities via `linked_activities: Vec<ActivityId>`, but there is no
/// activity-keyed index on the meeting store. Resolving meetings would require
/// a full domain scan. Deferred until a `list_by_activity` index exists.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProgramDashboardResponse {
    pub program_id: String,
    pub domain_id: String,
    pub parent_entity_id: String,
    pub name: String,
    pub kind: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_at: Option<u64>,
    pub milestones: Vec<DashboardMilestoneSummary>,
    pub milestone_counts: DashboardMilestoneCounts,
    pub activities: Vec<DashboardActivitySummary>,
    pub action_item_counts: DashboardActionItemCounts,
}
