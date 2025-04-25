mod resource_view;
mod compute_view;
pub mod token_burn;

use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

pub use resource_view::ResourceView;
pub use compute_view::ComputeTokenView;
pub use token_burn::TokenBurn;

/// Federation resource usage report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationResourceReport {
    /// Federation ID
    pub federation_id: String,
    
    /// When this report was generated
    pub report_generated_at: DateTime<Utc>,
    
    /// Number of days included in this report
    pub period_days: i64,
    
    /// Total tokens burned for this federation
    pub total_tokens_burned: f64,
    
    /// Average daily token burn
    pub avg_daily_burn: f64,
    
    /// Peak daily token burn
    pub peak_daily_burn: f64,
    
    /// Date of peak usage
    pub peak_date: Option<DateTime<Utc>>,
    
    /// Total quota for this federation
    pub quota_total: f64,
    
    /// Remaining quota
    pub quota_remaining: f64,
    
    /// Remaining quota as percentage
    pub quota_remaining_percent: Option<f64>,
    
    /// Projected days until quota exhaustion
    pub projected_exhaustion_days: Option<i64>,
    
    /// Projected date of quota exhaustion
    pub projected_exhaustion_date: Option<DateTime<Utc>>,
}

/// Represents a resource token in the wallet
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceToken {
    /// Unique identifier for the token
    pub token_id: String,
    
    /// Type of resource token
    pub token_type: String,
    
    /// Amount of resources the token represents
    pub amount: f64,
    
    /// Owner's DID
    pub owner_did: String,
    
    /// Federation or scope where token is valid
    pub federation_scope: String,
    
    /// Optional expiration timestamp
    pub expires_at: Option<u64>,
    
    /// Whether token has been revoked
    pub revoked: bool,
}

/// Represents a record of a token burn operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenBurn {
    /// Unique identifier for the burn record
    pub burn_id: String,
    
    /// ID of the token that was burned
    pub token_id: String,
    
    /// Amount of token that was burned
    pub amount: f64,
    
    /// Federation or scope where token was burned
    pub federation_scope: String,
    
    /// Timestamp when the burn occurred
    pub burn_timestamp: u64,
    
    /// Owner's DID
    pub owner_did: String,
    
    /// Optional job ID that consumed the tokens
    pub job_id: Option<String>,
    
    /// Optional proposal ID that authorized the burn
    pub proposal_id: Option<String>,
    
    /// Optional DAG transaction ID for verification
    pub dag_tx_id: Option<String>,
} 