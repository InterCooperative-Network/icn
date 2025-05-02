use thiserror::Error;

#[derive(Error, Debug)]
pub enum WalletError {
    #[error("Identity error: {0}")]
    IdentityError(String),
    
    #[error("Cryptography error: {0}")]
    CryptoError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Invalid DID format: {0}")]
    InvalidDidFormat(String),
    
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
}

pub type WalletResult<T> = Result<T, WalletError>; 