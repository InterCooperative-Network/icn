//! API request/response models

use serde::{Deserialize, Serialize};

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks: Option<std::collections::HashMap<String, ComponentHealth>>,
}

/// Health status of an individual component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub status: String, // "ok", "degraded", "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

// === Authentication ===

/// Request a challenge for DID-based authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeRequest {
    pub did: String,
}

/// Challenge response containing nonce to sign
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeResponse {
    pub nonce: String,
    pub expires_in: u64, // seconds
}

/// Verify signed challenge and get capability token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyRequest {
    pub did: String,
    pub signature: String, // Hex-encoded signature
    pub coop_id: String,
    pub scopes: Vec<String>,
}

/// Token response with JWT capability token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub token: String,
    pub expires_in: u64, // seconds
}

// === Cooperative Management ===

/// Create a new cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCoopRequest {
    pub id: String,
    pub name: String,
}

/// Add a member to a cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMemberRequest {
    pub did: String,
    /// Role for the new member: "steward", "facilitator", or "participant"
    /// Legacy names "owner", "admin", "member" are also accepted for backwards compatibility
    pub role: String,
}

/// Update member role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRoleRequest {
    /// New role: "steward", "facilitator", or "participant"
    /// Legacy names "owner", "admin", "member" are also accepted
    pub role: String,
}

/// Update cooperative settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSettingsRequest {
    pub governance_model: Option<String>,
    pub credit_policy: Option<String>,
    pub currency: Option<String>,
}

// === Ledger Operations ===

/// Create a payment/transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaymentRequest {
    pub from: String, // DID
    pub to: String,   // DID
    pub amount: i64,
    pub currency: String, // e.g., "hours", "USD"
    pub memo: Option<String>,
}

/// Balance response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceResponse {
    pub did: String,
    pub balances: std::collections::HashMap<String, i64>, // currency -> balance
}

/// Transaction history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionHistoryEntry {
    pub id: String, // Entry hash
    pub timestamp: u64,
    pub author: String, // DID
    pub accounts: Vec<AccountDeltaResponse>,
}

/// Account delta for transaction history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountDeltaResponse {
    pub account_id: String, // DID
    pub currency: String,
    pub debit: Option<i64>,
    pub credit: Option<i64>,
}

// === Governance Operations ===

/// Create a new governance domain
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProposalRequest {
    pub domain_id: String,   // Domain this proposal belongs to
    pub title: String,       // Short title
    pub description: String, // Full description/rationale
    pub payload: ProposalPayloadRequest,
}

/// Proposal payload types (matches icn_governance::ProposalPayload)
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenProposalRequest {
    pub voting_period_seconds: Option<u64>, // Optional override of domain default
}

/// Cast a vote on a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastVoteRequest {
    pub choice: String, // "for", "against", or "abstain"
    pub comment: Option<String>,
}

/// Vote choice response (simpler for API consumers)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoteChoiceResponse {
    For,
    Against,
    Abstain,
}
