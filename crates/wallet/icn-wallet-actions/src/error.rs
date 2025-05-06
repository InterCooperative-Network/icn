use thiserror::Error;

/// Errors that can occur during wallet actions
#[derive(Error, Debug)]
pub enum ActionError {
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Action not found: {0}")]
    NotFound(String),
    
    #[error("Invalid action state: {0}")]
    InvalidState(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Data error: {0}")]
    DataError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for wallet actions
pub type ActionResult<T> = Result<T, ActionError>; 