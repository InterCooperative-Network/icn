//! API request/response models

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks: Option<std::collections::HashMap<String, ComponentHealth>>,
}

/// Health status of an individual component
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ComponentHealth {
    pub status: String, // "ok", "degraded", "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Structured health status enum for typed responses
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Ok => write!(f, "ok"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

/// Detailed component health with latency information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DetailedComponentHealth {
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

/// Detailed health response with component-level status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DetailedHealthResponse {
    pub status: HealthStatus,
    pub version: String,
    pub uptime_seconds: u64,
    pub components: std::collections::HashMap<String, DetailedComponentHealth>,
}

// === Authentication ===

/// Request a challenge for DID-based authentication
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChallengeRequest {
    pub did: String,
}

/// Challenge response containing nonce to sign
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChallengeResponse {
    pub nonce: String,
    pub expires_in: u64, // seconds
}

/// Verify signed challenge and get capability token
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerifyRequest {
    pub did: String,
    pub signature: String, // Hex-encoded signature
    pub coop_id: String,
    pub scopes: Vec<String>,
}

/// Token response with JWT capability token
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenResponse {
    pub token: String,
    pub expires_in: u64, // seconds
}

// === Cooperative Management ===

/// Create a new cooperative
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateCoopRequest {
    pub id: String,
    pub name: String,
}

/// Add a member to a cooperative
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddMemberRequest {
    pub did: String,
    /// Role for the new member: "steward", "facilitator", or "participant"
    /// Legacy names "owner", "admin", "member" are also accepted for backwards compatibility
    pub role: String,
}

/// Update member role
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateRoleRequest {
    /// New role: "steward", "facilitator", or "participant"
    /// Legacy names "owner", "admin", "member" are also accepted
    pub role: String,
}

/// Update cooperative settings
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateSettingsRequest {
    pub governance_model: Option<String>,
    pub credit_policy: Option<String>,
    pub currency: Option<String>,
}

// === Ledger Operations ===

/// Create a payment/transaction
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatePaymentRequest {
    pub from: String, // DID
    pub to: String,   // DID
    pub amount: i64,
    pub currency: String, // e.g., "hours", "USD"
    pub memo: Option<String>,
}

/// Balance response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BalanceResponse {
    pub did: String,
    pub balances: std::collections::HashMap<String, i64>, // currency -> balance
}

/// Transaction history entry
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransactionHistoryEntry {
    pub id: String, // Entry hash
    pub timestamp: u64,
    pub author: String, // DID
    pub accounts: Vec<AccountDeltaResponse>,
    /// Decision receipt ID (node-local, links to governance decision)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_receipt_id: Option<String>,
    /// Decision canonical hash (cross-node anchor)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_hash: Option<String>,
}

/// Account delta for transaction history
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AccountDeltaResponse {
    pub account_id: String, // DID
    pub currency: String,
    pub debit: Option<i64>,
    pub credit: Option<i64>,
}

/// Paginated transaction history response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TransactionHistoryResponse {
    /// The transactions on this page
    pub transactions: Vec<TransactionHistoryEntry>,

    /// Pagination metadata
    pub pagination: PaginationInfo,
}

/// Generic pagination metadata for list responses
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PaginationInfo {
    /// Total number of items (if known/available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<usize>,

    /// Cursor for the next page (None if no more items)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,

    /// Cursor for the previous page (None if at beginning)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_cursor: Option<String>,

    /// Number of items in current page
    pub count: usize,

    /// Whether there are more items after this page
    pub has_more: bool,

    /// Current offset (for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,

    /// Limit used for this query
    pub limit: usize,
}

impl PaginationInfo {
    /// Create pagination info for cursor-based response
    pub fn cursor_based(
        next_cursor: Option<String>,
        prev_cursor: Option<String>,
        count: usize,
        has_more: bool,
        limit: usize,
    ) -> Self {
        Self {
            total: None,
            next_cursor,
            prev_cursor,
            count,
            has_more,
            offset: None,
            limit,
        }
    }

    /// Create pagination info for offset-based response
    pub fn offset_based(total: Option<usize>, offset: usize, limit: usize, count: usize) -> Self {
        let has_more = total.map(|t| offset + count < t).unwrap_or(count >= limit);
        Self {
            total,
            next_cursor: None,
            prev_cursor: None,
            count,
            has_more,
            offset: Some(offset),
            limit,
        }
    }
}

// === Governance Operations ===

/// Create a new governance domain
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateDomainRequest {
    pub id: String,              // Domain ID (e.g., "coop:food-coop")
    pub name: String,            // Human-readable name
    pub profile: String,         // Governance profile (e.g., "cooperative_default")
    pub quorum_percent: u8,      // Quorum percentage (0-100)
    pub approval_percent: u8,    // Approval percentage (0-100)
    pub voting_period_days: u64, // Default voting period in days
    pub members: Vec<String>,    // List of member DIDs
}

/// Add a member to a governance domain
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddDomainMemberRequest {
    pub did: String,
    /// Voting weight (reserved for future use, currently ignored)
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}

/// Request to connect to a federation peer
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FederationConnectRequest {
    /// Address of the peer to connect to (e.g., "node-b.local:9000")
    pub address: String,
    /// DID of the remote cooperative (required for identity verification)
    pub peer_did: Option<String>,
    /// Optional cooperative ID to register the peer as
    pub coop_id: Option<String>,
    /// Optional human-readable name for the peer
    pub name: Option<String>,
}

/// Create a new proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateProposalRequest {
    pub domain_id: String,   // Domain this proposal belongs to
    pub title: String,       // Short title
    pub description: String, // Full description/rationale
    pub payload: ProposalPayloadRequest,
}

/// Proposal payload types (matches icn_governance::ProposalPayload)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProposalPayloadRequest {
    Text {
        body: String,
    },
    Budget {
        amount: i64,
        recipient: String, // DID
        currency: String,
        purpose: String,
    },
    Membership {
        action: String, // "add" or "remove"
        did: String,
    },
    ConfigChange {
        key: String,
        value: String,
    },
}

/// Open a proposal for voting
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OpenProposalRequest {
    pub voting_period_seconds: Option<u64>, // Optional override of domain default
}

/// Cast a vote on a proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CastVoteRequest {
    pub choice: String, // "for", "against", or "abstain"
    pub comment: Option<String>,
}

/// Vote choice response (simpler for API consumers)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VoteChoiceResponse {
    For,
    Against,
    Abstain,
}

// === Invite System ===

/// Create an invite for someone to join the coop
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateInviteRequest {
    pub coop_id: String,
    /// Role for the invitee: "member", "admin", "participant", "facilitator"
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_seconds: Option<u64>,
}

/// Join via invite code - request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JoinRequest {
    pub invite_code: String,
    pub did: String, // Client provides their own DID
}

/// Invite response with generated code
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InviteResponse {
    pub code: String,
    pub coop_id: String,
    pub coop_name: String,
    pub role: String,
    pub expires_at: u64,
    pub invite_url: String,
}

/// List of invites
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InviteListResponse {
    pub invites: Vec<InviteInfo>,
}

/// Individual invite info (for listing)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InviteInfo {
    pub code: String,
    pub role: String,
    pub created_by: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub used: bool,
}

/// Join via invite - creates identity and returns credentials
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JoinResponse {
    pub did: String,
    pub token: String,
    pub token_expires_in: u64,
    pub coop_id: String,
    pub role: String,
    pub private_key: String,
}

// === QR Login Sessions ===

/// Create a new login session for QR-based authentication
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateSessionRequest {
    pub coop_id: String,
}

/// Data to encode in the QR code (shown to mobile wallet)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionQrData {
    pub session_id: String,
    pub gateway_url: String,
    pub coop_id: String,
    pub expires_at: u64,
}

/// Session creation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub expires_at: u64,
    pub qr_data: SessionQrData,
}

/// Session status check response (polling endpoint)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SessionStatusResponse {
    pub session_id: String,
    pub status: String, // "pending", "approved", "expired", "consumed"
    pub expires_at: u64,
    /// Token for the web session (only present when status is "approved")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Token expiry in seconds (only present when status is "approved")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_expires_in: Option<u64>,
    /// DID that approved the session (only present when status is "approved")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub did: Option<String>,
    /// Scopes granted (only present when status is "approved")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
}

// === Vote Delegation ===

/// Create a new vote delegation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateDelegationRequest {
    /// DID of the delegate (who receives voting power)
    pub delegate: String,
    /// Scope of delegation: "blanket", "domain:<id>", or "proposal:<id>"
    pub scope: String,
    /// Optional expiry timestamp (Unix seconds)
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
    /// Delegations given by the caller
    pub given: Vec<DelegationResponse>,
    /// Delegations received by the caller
    pub received: Vec<DelegationResponse>,
}

// === Cross-Currency Payments ===

/// Create a cross-currency payment
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateCrossPaymentRequest {
    /// Sender DID
    pub from: String,
    /// Recipient DID
    pub to: String,
    /// Amount to send in source currency
    pub amount: i64,
    /// Source currency (what sender pays)
    pub from_currency: String,
    /// Target currency (what recipient receives)
    pub to_currency: String,
    /// Optional slippage protection: max target amount to receive
    /// If the converted amount exceeds this, the transfer is rejected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_target_amount: Option<i64>,
    /// Optional memo/note for the payment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

/// Response for a cross-currency payment
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CrossPaymentResponse {
    /// Hash of the journal entry
    pub hash: String,
    /// Sender DID
    pub from: String,
    /// Recipient DID
    pub to: String,
    /// Amount debited from sender
    pub source_amount: i64,
    /// Source currency
    pub from_currency: String,
    /// Gross amount before fees
    pub gross_target_amount: i64,
    /// Fee amount (sent to treasury)
    pub fee_amount: i64,
    /// Net amount credited to recipient
    pub net_target_amount: i64,
    /// Target currency
    pub to_currency: String,
    /// Exchange rate used
    pub rate_used: f64,
    /// When the rate was fetched (Unix seconds)
    pub rate_timestamp: u64,
    /// Sources that provided the rate
    pub rate_sources: Vec<String>,
}

/// Request for a cross-currency payment quote
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CrossPaymentQuoteRequest {
    /// Amount to send in source currency
    pub amount: i64,
    /// Source currency (what sender pays)
    pub from_currency: String,
    /// Target currency (what recipient receives)
    pub to_currency: String,
}

/// Quote for a cross-currency payment (preview without execution)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CrossPaymentQuote {
    /// Amount that would be debited from sender
    pub source_amount: i64,
    /// Source currency
    pub from_currency: String,
    /// Gross amount before fees
    pub gross_target_amount: i64,
    /// Fee amount that would be deducted
    pub fee_amount: i64,
    /// Net amount that would be credited to recipient
    pub net_target_amount: i64,
    /// Target currency
    pub to_currency: String,
    /// Exchange rate
    pub rate: f64,
    /// When the rate was fetched (Unix seconds)
    pub rate_timestamp: u64,
    /// Sources that provided the rate
    pub rate_sources: Vec<String>,
    /// When this quote expires (Unix seconds)
    pub valid_until: u64,
    /// Whether the rate is considered stale
    pub is_stale: bool,
}

// === Federation Proposal Requests (Issue #518) ===

/// Common fields for federation proposal requests
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FederationProposalCommon {
    /// Governance domain to create proposal in
    pub domain_id: String,
    /// Short title for the proposal
    pub title: String,
    /// Rationale/explanation for the proposal
    pub description: String,
}

/// Federation terms for join proposals
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FederationTermsRequest {
    /// Minimum trust score required for federation members (0.0-1.0)
    pub min_trust_threshold: f64,
    /// Whether federation governance decisions are binding
    pub governance_binding: bool,
    /// Data sharing level: "none", "metadata_only", "full"
    pub data_sharing_level: String,
    /// Dispute resolution: "federation_mediation", "federation_vote", or "arbitrator:<coop_id>"
    pub dispute_resolution: String,
}

/// Request to create a "join federation" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JoinFederationProposalRequest {
    /// Governance domain to create proposal in
    pub domain_id: String,
    /// Short title for the proposal
    pub title: String,
    /// Rationale/explanation for the proposal
    pub description: String,
    /// ID of the federation to join
    pub federation_id: String,
    /// Terms for joining the federation
    pub terms: FederationTermsRequest,
    /// Optional sponsor cooperative ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sponsor_coop_id: Option<String>,
}

/// Request to create a "leave federation" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LeaveFederationProposalRequest {
    /// Governance domain to create proposal in
    pub domain_id: String,
    /// Short title for the proposal
    pub title: String,
    /// Rationale/explanation for the proposal
    pub description: String,
    /// ID of the federation to leave
    pub federation_id: String,
    /// Reason for leaving
    pub reason: String,
    /// Grace period in days (1-365)
    pub grace_period_days: u32,
}

/// Request to create an "establish clearing" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EstablishClearingProposalRequest {
    /// Governance domain to create proposal in
    pub domain_id: String,
    /// Short title for the proposal
    pub title: String,
    /// Rationale/explanation for the proposal
    pub description: String,
    /// Partner cooperative ID
    pub partner_coop_id: String,
    /// Partner cooperative DID
    pub partner_coop_did: String,
    /// Maximum allowed imbalance (must be positive)
    pub max_imbalance: i64,
    /// Settlement interval: "daily", "weekly", "monthly", "manual"
    pub settlement_interval: String,
    /// Currency for the clearing agreement
    pub currency: String,
}

/// Request to create a "terminate clearing" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TerminateClearingProposalRequest {
    /// Governance domain to create proposal in
    pub domain_id: String,
    /// Short title for the proposal
    pub title: String,
    /// Rationale/explanation for the proposal
    pub description: String,
    /// Partner cooperative ID
    pub partner_coop_id: String,
    /// Reason for termination
    pub reason: String,
}

/// Request to create a "vouch for cooperative" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VouchProposalRequest {
    /// Governance domain to create proposal in
    pub domain_id: String,
    /// Short title for the proposal
    pub title: String,
    /// Rationale/explanation for the proposal
    pub description: String,
    /// Target cooperative ID to vouch for
    pub target_coop_id: String,
    /// Target cooperative DID
    pub target_coop_did: String,
    /// Trust score (0.0-1.0)
    pub trust_score: f64,
    /// Context for the vouch (e.g., "trade", "governance", "technical")
    pub context: String,
    /// Optional evidence/documentation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// Request to create a "revoke vouch" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RevokeVouchProposalRequest {
    /// Governance domain to create proposal in
    pub domain_id: String,
    /// Short title for the proposal
    pub title: String,
    /// Rationale/explanation for the proposal
    pub description: String,
    /// Target cooperative ID to revoke vouch from
    pub target_coop_id: String,
    /// Reason for revoking the vouch
    pub reason: String,
}

/// Request to create an "update federation policy" proposal
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateFederationPolicyProposalRequest {
    /// Governance domain to create proposal in
    pub domain_id: String,
    /// Short title for the proposal
    pub title: String,
    /// Rationale/explanation for the proposal
    pub description: String,
    /// Auto-accept vouch threshold.
    ///
    /// Uses `f64` instead of `TrustScore` because this field supports a sentinel
    /// value of `-1.0` to disable auto-accept functionality. Valid values are:
    /// - `-1.0`: Disable auto-accept (vouches require manual review)
    /// - `0.0` to `1.0`: Trust threshold for automatic vouch acceptance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_accept_vouch_threshold: Option<f64>,
    /// Trust decay factor (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_decay_factor: Option<f64>,
    /// Maximum attestations per minute (must be > 0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attestations_per_minute: Option<u32>,
}

// === Action Items ===

/// Request to create a new action item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateActionItemRequest {
    /// Title of the action item
    pub title: String,
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional assignee DID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// Optional due date (Unix timestamp in seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<u64>,
    /// Priority level: "low", "medium", "high", "critical"
    #[serde(default = "default_priority")]
    pub priority: String,
    /// Optional linked proposal ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_proposal: Option<String>,
    /// Optional meeting context (e.g., "2026-01-17 board meeting")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_context: Option<String>,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_priority() -> String {
    "medium".to_string()
}

/// Request to update an action item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateActionItemRequest {
    /// Updated title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Updated description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Updated assignee DID (use empty string to unassign)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// Updated due date (use 0 to remove)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<u64>,
    /// Updated priority
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// Updated status: "pending", "in_progress", "completed", "deferred"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Updated tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Request to add a note to an action item
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddActionItemNoteRequest {
    /// Note content
    pub content: String,
}

/// Action item response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionItemResponse {
    /// Unique action item ID
    pub id: String,
    /// Domain ID
    pub domain_id: String,
    /// Title
    pub title: String,
    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Assignee DID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// Due date (Unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<u64>,
    /// Status
    pub status: String,
    /// Priority
    pub priority: String,
    /// Creator DID
    pub created_by: String,
    /// Creation timestamp
    pub created_at: u64,
    /// Last update timestamp
    pub updated_at: u64,
    /// Linked proposal ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_proposal: Option<String>,
    /// Meeting context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_context: Option<String>,
    /// Tags
    pub tags: Vec<String>,
    /// Notes
    pub notes: Vec<ActionItemNoteResponse>,
    /// Whether the item is overdue
    pub is_overdue: bool,
}

/// Action item note response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionItemNoteResponse {
    /// Note ID
    pub id: String,
    /// Author DID
    pub author: String,
    /// Note content
    pub content: String,
    /// Timestamp
    pub created_at: u64,
}

/// Query parameters for listing action items
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionItemFilterParams {
    /// Filter by status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Filter by assignee DID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// Filter by priority
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// Filter by overdue status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overdue: Option<bool>,
    /// Filter by tag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

// === Listings / Exchange ===

/// Request to create a new listing
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateListingRequest {
    /// Type: "offer" or "want"
    pub listing_type: String,
    /// Title of the listing
    pub title: String,
    /// Detailed description
    pub description: String,
    /// Category: "equipment", "services", "materials", "space", "other"
    pub category: String,
    /// What the poster is seeking in exchange
    pub seeking: String,
    /// Visibility: "coop", "federation", "public"
    #[serde(default = "default_visibility")]
    pub visibility: String,
    /// Optional photo URLs (IPFS hashes or URLs)
    #[serde(default)]
    pub photos: Vec<String>,
    /// Optional expiry date (Unix timestamp)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    /// Optional tags for discoverability
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_visibility() -> String {
    "federation".to_string()
}

/// Request to update a listing
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateListingRequest {
    /// Updated title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Updated description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Updated category
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Updated seeking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seeking: Option<String>,
    /// Updated visibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    /// Updated photos
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photos: Option<Vec<String>>,
    /// Updated expiry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    /// Updated tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Request to express interest in a listing
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExpressInterestRequest {
    /// Message to the listing owner
    pub message: String,
    /// Optional offer details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offer: Option<String>,
}

/// Listing response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListingResponse {
    /// Unique listing ID
    pub id: String,
    /// Type: "offer" or "want"
    pub listing_type: String,
    /// Title
    pub title: String,
    /// Description
    pub description: String,
    /// Category
    pub category: String,
    /// Photo URLs
    pub photos: Vec<String>,
    /// Creator DID
    pub offered_by: String,
    /// Creator's cooperative ID
    pub coop_id: String,
    /// What they're seeking
    pub seeking: String,
    /// Visibility level
    pub visibility: String,
    /// Status: "active", "matched", "completed", "expired", "cancelled"
    pub status: String,
    /// Creation timestamp
    pub created_at: u64,
    /// Last update timestamp
    pub updated_at: u64,
    /// Expiry timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    /// Tags
    pub tags: Vec<String>,
    /// Number of interests expressed
    pub interest_count: usize,
}

/// Interest response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListingInterestResponse {
    /// Interest ID
    pub id: String,
    /// Listing ID
    pub listing_id: String,
    /// Interested party's DID
    pub from_did: String,
    /// Interested party's cooperative
    pub from_coop: String,
    /// Message
    pub message: String,
    /// Offer details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offer: Option<String>,
    /// Timestamp
    pub created_at: u64,
}

/// Query parameters for listing search
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListingFilterParams {
    /// Filter by type: "offer" or "want"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listing_type: Option<String>,
    /// Filter by category
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Filter by visibility level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    /// Filter by status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Filter by coop
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coop_id: Option<String>,
    /// Filter by creator DID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offered_by: Option<String>,
    /// Search in title and description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    /// Filter by tag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// Maximum number of results (default: 20, max: 100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Number of results to skip for pagination (default: 0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}
