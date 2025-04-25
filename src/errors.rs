use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Token with ID '{token_id}' not found")]
    TokenNotFound { token_id: String },
    
    #[error("Resource token with ID '{token_id}' not found")]
    ResourceTokenNotFound { token_id: String },
    
    #[error("Insufficient balance for token '{token_id}': requested {requested}, available {available}")]
    InsufficientBalance { 
        token_id: String, 
        requested: f64, 
        available: f64 
    },
    
    #[error("Token '{token_id}' does not belong to '{owner}'")]
    Unauthorized { token_id: String, owner: String },
    
    #[error("Token burn with ID '{burn_id}' not found")]
    TokenBurnNotFound { burn_id: String },
    
    #[error("Token '{token_id}' has expired")]
    TokenExpired { token_id: String },
    
    #[error("Token '{token_id}' has been revoked")]
    TokenRevoked { token_id: String },
    
    #[error("Invalid token type: {0}")]
    InvalidTokenType(String),
    
    #[error("Command error: {0}")]
    Command(String),
} 