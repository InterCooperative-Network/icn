mod resource_view;
mod compute_view;

pub use resource_view::ResourceView;
pub use compute_view::ComputeTokenView;

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