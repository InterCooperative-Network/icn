/*!
 * Credential management for wallet synchronization
 *
 * This module provides interfaces for storing and retrieving credentials
 * synchronized from federation nodes.
 */

use thiserror::Error;
use async_trait::async_trait;
use std::sync::Arc;
use anyhow::Result;

/// Error types for credential operations
#[derive(Error, Debug)]
pub enum CredentialError {
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Invalid credential: {0}")]
    InvalidCredential(String),
    
    #[error("Verification error: {0}")]
    VerificationError(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
}

/// Result type for credential operations
pub type CredentialResult<T> = std::result::Result<T, CredentialError>;

/// Interface for credential storage
#[async_trait]
pub trait CredentialStore {
    /// Store a credential
    async fn store_credential(&self, credential_id: &str, credential: &str) -> CredentialResult<()>;
    
    /// Get a credential by ID
    async fn get_credential(&self, credential_id: &str) -> CredentialResult<Option<String>>;
    
    /// List all credentials
    async fn list_credentials(&self) -> CredentialResult<Vec<String>>;
}

/// Credential manager for handling credential operations
pub struct CredentialManager {
    store: Arc<dyn CredentialStore>,
}

impl CredentialManager {
    /// Create a new credential manager
    pub fn new(store: Arc<dyn CredentialStore>) -> Self {
        Self { store }
    }
    
    /// Store a credential
    pub async fn store_credential(&self, credential_id: &str, credential: &str) -> Result<()> {
        self.store.store_credential(credential_id, credential)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to store credential: {}", e))
    }
    
    /// Get a credential by ID
    pub async fn get_credential(&self, credential_id: &str) -> Result<Option<String>> {
        self.store.get_credential(credential_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get credential: {}", e))
    }
    
    /// List all credentials
    pub async fn list_credentials(&self) -> Result<Vec<String>> {
        self.store.list_credentials()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list credentials: {}", e))
    }
} 