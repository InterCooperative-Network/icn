//! API request/response models

use serde::{Deserialize, Serialize};

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
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
    pub role: String, // "owner", "admin", or "member"
}

/// Update member role
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRoleRequest {
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
    pub from: String,      // DID
    pub to: String,        // DID
    pub amount: i64,
    pub currency: String,  // e.g., "hours", "USD"
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
    pub id: String,           // Entry hash
    pub timestamp: u64,
    pub author: String,       // DID
    pub accounts: Vec<AccountDeltaResponse>,
}

/// Account delta for transaction history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountDeltaResponse {
    pub account_id: String,  // DID
    pub currency: String,
    pub debit: Option<i64>,
    pub credit: Option<i64>,
}
