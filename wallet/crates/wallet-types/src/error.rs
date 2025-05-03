use thiserror::Error;
use std::io;

/// Common error type used across wallet components
#[derive(Error, Debug)]
pub enum WalletError {
    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    
    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    /// DAG error
    #[error("DAG error: {0}")]
    DagError(String),
    
    /// Identity error
    #[error("Identity error: {0}")]
    IdentityError(String),
    
    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    
    /// Resource not found
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),
    
    /// Connection error
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    /// Timeout error
    #[error("Timeout error: {0}")]
    TimeoutError(String),
    
    /// Storage error
    #[error("Storage error: {0}")]
    StorageError(String),
    
    /// Runtime error
    #[error("Runtime error: {0}")]
    RuntimeError(String),
    
    /// Generic error
    #[error("{0}")]
    GenericError(String),
}

impl From<serde_json::Error> for WalletError {
    fn from(err: serde_json::Error) -> Self {
        WalletError::SerializationError(err.to_string())
    }
}

#[cfg(feature = "runtime-compat")]
impl From<icn_dag::DagError> for WalletError {
    fn from(err: icn_dag::DagError) -> Self {
        WalletError::DagError(err.to_string())
    }
}

#[cfg(feature = "runtime-compat")]
impl From<icn_storage::StorageError> for WalletError {
    fn from(err: icn_storage::StorageError) -> Self {
        WalletError::StorageError(err.to_string())
    }
}

/// Conversion from Runtime errors to Wallet errors
/// This trait is implemented when the runtime-compat feature is enabled
#[cfg(feature = "runtime-compat")]
pub trait FromRuntimeError<T> {
    fn convert_runtime_error(self) -> WalletResult<T>;
}

/// Implement the conversion for any Result with E: Display
#[cfg(feature = "runtime-compat")]
impl<T, E: std::fmt::Display> FromRuntimeError<T> for Result<T, E> {
    fn convert_runtime_error(self) -> WalletResult<T> {
        self.map_err(|e| WalletError::RuntimeError(e.to_string()))
    }
}

/// Result type alias using WalletError
pub type WalletResult<T> = Result<T, WalletError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let wallet_err: WalletError = json_err.into();
        
        match wallet_err {
            WalletError::SerializationError(msg) => {
                assert!(msg.contains("expected value"), "Error message should contain the original error");
            },
            _ => panic!("Expected SerializationError variant"),
        }
    }
    
    #[test]
    fn test_error_display() {
        let err = WalletError::DagError("Invalid DAG node".to_string());
        assert_eq!(format!("{}", err), "DAG error: Invalid DAG node");
        
        let err = WalletError::ValidationError("Signature verification failed".to_string());
        assert_eq!(format!("{}", err), "Validation error: Signature verification failed");
    }

    #[test]
    #[cfg(feature = "runtime-compat")]
    fn test_runtime_error_conversion() {
        // Test the FromRuntimeError trait with a simple error
        let test_result: Result<(), &str> = Err("Test runtime error");
        let wallet_result = test_result.convert_runtime_error();
        
        assert!(wallet_result.is_err());
        if let Err(err) = wallet_result {
            match err {
                WalletError::RuntimeError(msg) => {
                    assert_eq!(msg, "Test runtime error");
                },
                _ => panic!("Expected RuntimeError variant"),
            }
        }
    }
} 