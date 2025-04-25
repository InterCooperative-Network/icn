use uuid::Uuid;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

/// TokenBurn represents a record of a resource token being consumed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBurn {
    /// Unique identifier for this burn record
    pub id: String,
    /// ID of the token that was burned
    pub token_id: String,
    /// Amount of the token that was burned
    pub amount: f64,
    /// Type of token (e.g., "icn:resource/compute")
    pub token_type: String,
    /// Federation scope this token belongs to
    pub federation_scope: String,
    /// DID of the token owner
    pub owner_did: String,
    /// When the token was burned
    pub timestamp: i64,
    /// Optional job ID if token was burned for a job execution
    pub job_id: Option<String>,
    /// Optional receipt ID linking to an execution receipt
    pub receipt_id: Option<String>,
    /// Optional reason for the burn
    pub reason: Option<String>,
}

impl TokenBurn {
    /// Create a new TokenBurn with the current timestamp
    pub fn new(
        token_id: String,
        amount: f64,
        token_type: String,
        federation_scope: String,
        owner_did: String,
        job_id: Option<String>,
        receipt_id: Option<String>,
        reason: Option<String>,
    ) -> Self {
        let timestamp = chrono::Utc::now().timestamp();
        Self {
            id: Uuid::new_v4().to_string(),
            token_id,
            amount,
            token_type,
            federation_scope,
            owner_did,
            timestamp,
            job_id,
            receipt_id,
            reason,
        }
    }
    
    /// Format the timestamp as a human-readable date/time
    pub fn formatted_timestamp(&self) -> String {
        let dt = DateTime::<Utc>::from_timestamp(self.timestamp, 0)
            .unwrap_or_default();
        dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    }
} 