use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents a record of a token being consumed/burned
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBurn {
    /// Unique identifier for this burn record
    pub id: String,
    /// ID of the token that was burned
    pub token_id: String,
    /// Amount of token that was consumed
    pub amount: f64,
    /// Type of the token (e.g., "compute", "storage", etc.)
    pub token_type: String,
    /// Scope of the federation this burn applies to
    pub federation_scope: String,
    /// DID of the token owner
    pub owner_did: String,
    /// Timestamp when the burn occurred
    pub timestamp: DateTime<Utc>,
    /// Job ID associated with this burn (if applicable)
    pub job_id: Option<String>,
    /// Type of job that consumed the token (if applicable)
    pub job_type: Option<String>,
    /// Proposal ID associated with this burn (if applicable)
    pub proposal_id: Option<String>,
    /// Receipt ID to verify this burn operation
    pub receipt_id: Option<String>,
    /// Human-readable reason for the burn
    pub reason: String,
}

impl TokenBurn {
    /// Create a new token burn record
    pub fn new(
        token_id: String,
        amount: f64,
        token_type: String,
        federation_scope: String,
        owner_did: String,
        job_id: Option<String>,
        job_type: Option<String>,
        proposal_id: Option<String>,
        receipt_id: Option<String>,
        reason: String,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            token_id,
            amount,
            token_type,
            federation_scope,
            owner_did,
            timestamp: Utc::now(),
            job_id,
            job_type,
            proposal_id,
            receipt_id,
            reason,
        }
    }
    
    /// Returns a human-readable description of this burn
    pub fn description(&self) -> String {
        let job_info = if let Some(job_id) = &self.job_id {
            let job_type_info = if let Some(job_type) = &self.job_type {
                format!(" ({} job)", job_type)
            } else {
                String::new()
            };
            
            format!(" for job {}{}", job_id, job_type_info)
        } else {
            String::new()
        };
        
        let proposal_info = if let Some(proposal_id) = &self.proposal_id {
            format!(" for proposal {}", proposal_id)
        } else {
            String::new()
        };
        
        format!(
            "Burned {:.2} {} tokens{}{}{}",
            self.amount, 
            self.token_type,
            job_info,
            proposal_info,
            if self.reason.is_empty() { "" } else { format!(": {}", self.reason).as_str() }
        )
    }
} 