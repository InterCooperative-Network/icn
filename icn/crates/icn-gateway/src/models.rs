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
