#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBurn {
    /// Unique identifier for the token burn record
    pub id: String,
    /// Reference to the token that was burned
    pub token_id: String,
    /// Amount of token that was burned
    pub amount: u64,
    /// Type of token (e.g. "icn:resource/compute")
    pub token_type: String,
    /// Federation scope of the token
    pub federation_scope: String,
    /// DID of the token owner
    pub owner_did: String,
    /// Timestamp when the token was burned
    pub timestamp: String,
    /// Optional job ID that consumed the token
    pub job_id: Option<String>,
    /// Optional execution receipt ID for verification
    pub receipt_id: Option<String>,
    /// Optional reason for the token burn
    pub reason: Option<String>,
} 