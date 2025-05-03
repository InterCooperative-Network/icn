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

#[cfg(feature = "runtime-compat")]
impl From<icn_identity::IdentityError> for WalletError {
    fn from(err: icn_identity::IdentityError) -> Self {
        match err {
            icn_identity::IdentityError::VerificationFailed(msg) => WalletError::ValidationError(msg),
            icn_identity::IdentityError::InvalidDid(msg) => WalletError::IdentityError(format!("Invalid DID: {}", msg)),
            icn_identity::IdentityError::InvalidScope(msg) => WalletError::IdentityError(format!("Invalid scope: {}", msg)),
            icn_identity::IdentityError::NotFound(msg) => WalletError::ResourceNotFound(format!("Identity not found: {}", msg)),
            icn_identity::IdentityError::CryptoError(msg) => WalletError::IdentityError(format!("Crypto error: {}", msg)),
            icn_identity::IdentityError::KeyError(msg) => WalletError::IdentityError(format!("Key error: {}", msg)),
            icn_identity::IdentityError::CredentialError(msg) => WalletError::IdentityError(format!("Credential error: {}", msg)),
            icn_identity::IdentityError::PermissionDenied(msg) => WalletError::AuthenticationError(format!("Permission denied: {}", msg)),
            _ => WalletError::IdentityError(err.to_string()),
        }
    }
}

#[cfg(feature = "runtime-compat")]
impl From<icn_federation::FederationError> for WalletError {
    fn from(err: icn_federation::FederationError) -> Self {
        match err {
            icn_federation::FederationError::NotFound(msg) => WalletError::ResourceNotFound(msg),
            icn_federation::FederationError::ValidationFailed(msg) => WalletError::ValidationError(msg),
            icn_federation::FederationError::AuthenticationFailed(msg) => WalletError::AuthenticationError(msg),
            _ => WalletError::RuntimeError(format!("Federation error: {}", err)),
        }
    }
}

#[cfg(feature = "runtime-compat")]
impl From<icn_governance_kernel::GovernanceError> for WalletError {
    fn from(err: icn_governance_kernel::GovernanceError) -> Self {
        WalletError::RuntimeError(format!("Governance error: {}", err))
    }
}

/// Trait for converting runtime errors to wallet errors
/// This trait allows for a consistent way to handle errors across the runtime-wallet boundary
#[cfg(feature = "runtime-compat")]
pub trait FromRuntimeError<T> {
    /// Convert a runtime error to a wallet error
    fn convert_runtime_error(self) -> WalletResult<T>;
}

/// Implement the conversion for any Result with E: Display
#[cfg(feature = "runtime-compat")]
impl<T, E: std::fmt::Display> FromRuntimeError<T> for Result<T, E> {
    fn convert_runtime_error(self) -> WalletResult<T> {
        self.map_err(|e| WalletError::RuntimeError(e.to_string()))
    }
}

// Specialized implementations for specific runtime error types for optimal error mapping
#[cfg(feature = "runtime-compat")]
impl<T> FromRuntimeError<T> for Result<T, icn_dag::DagError> {
    fn convert_runtime_error(self) -> WalletResult<T> {
        self.map_err(|e| e.into())
    }
}

#[cfg(feature = "runtime-compat")]
impl<T> FromRuntimeError<T> for Result<T, icn_storage::StorageError> {
    fn convert_runtime_error(self) -> WalletResult<T> {
        self.map_err(|e| e.into())
    }
}

#[cfg(feature = "runtime-compat")]
impl<T> FromRuntimeError<T> for Result<T, icn_identity::IdentityError> {
    fn convert_runtime_error(self) -> WalletResult<T> {
        self.map_err(|e| e.into())
    }
}

#[cfg(feature = "runtime-compat")]
impl<T> FromRuntimeError<T> for Result<T, icn_federation::FederationError> {
    fn convert_runtime_error(self) -> WalletResult<T> {
        self.map_err(|e| e.into())
    }
}

#[cfg(feature = "runtime-compat")]
impl<T> FromRuntimeError<T> for Result<T, icn_governance_kernel::GovernanceError> {
    fn convert_runtime_error(self) -> WalletResult<T> {
        self.map_err(|e| e.into())
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

    #[test]
    #[cfg(feature = "runtime-compat")]
    fn test_error_type_mapping() {
        use std::convert::From;
        
        // Test mapping from one error to another
        let storage_err = icn_storage::StorageError::NotFound("test resource".to_string());
        let wallet_err = WalletError::from(storage_err);
        
        match wallet_err {
            WalletError::StorageError(msg) => {
                assert!(msg.contains("test resource"), "Error message should be preserved");
            },
            _ => panic!("Expected StorageError variant"),
        }
    }
    
    #[test]
    #[cfg(feature = "runtime-compat")]
    fn test_error_propagation_chain() {
        // This test simulates error propagation through multiple boundaries
        
        // Create a chain of results to simulate passing through multiple layers
        fn level3_function() -> Result<(), icn_dag::DagError> {
            Err(icn_dag::DagError::InvalidCid("Test DAG error".to_string()))
        }
        
        fn level2_function() -> Result<(), WalletError> {
            level3_function().convert_runtime_error()
        }
        
        fn level1_function() -> Result<(), WalletError> {
            level2_function()?;
            Ok(())
        }
        
        // Test error propagation
        let result = level1_function();
        assert!(result.is_err());
        
        if let Err(err) = result {
            match err {
                WalletError::DagError(msg) => {
                    assert!(msg.contains("Invalid"), "Error should be properly mapped through the chain");
                },
                _ => panic!("Expected DagError variant after propagation"),
            }
        }
    }
    
    #[test]
    #[cfg(feature = "runtime-compat")]
    fn test_identity_error_mapping() {
        // Test that IdentityError variants are properly mapped to WalletError variants
        
        let verification_err = icn_identity::IdentityError::VerificationFailed(
            "Signature verification failed".to_string()
        );
        let wallet_err = WalletError::from(verification_err);
        
        match wallet_err {
            WalletError::ValidationError(msg) => {
                assert!(msg.contains("verification failed"), 
                       "Verification error should map to ValidationError");
            },
            _ => panic!("Expected ValidationError variant"),
        }
        
        let not_found_err = icn_identity::IdentityError::NotFound(
            "Identity did:icn:test not found".to_string()
        );
        let wallet_err = WalletError::from(not_found_err);
        
        match wallet_err {
            WalletError::ResourceNotFound(msg) => {
                assert!(msg.contains("Identity not found"), 
                       "NotFound error should map to ResourceNotFound");
            },
            _ => panic!("Expected ResourceNotFound variant"),
        }
    }
} 