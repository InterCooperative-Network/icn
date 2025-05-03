/*! 
# ICN Wallet

Mobile-first agent for identity, credentials, and DAG participation.

This is the top-level crate that composes the wallet functionality from its component crates:
- wallet-identity: For DID and VC support
- wallet-storage: For secure credential and key storage
- wallet-sync: For DAG synchronization with the runtime mesh
- wallet-actions: For DAG operations and proposal management
- wallet-api: For application-facing interfaces

*/

use wallet_sync as sync;
use wallet_identity as identity;
use wallet_storage as storage;
use wallet_actions as actions;
use wallet_api as api;

pub mod error {
    //! Error types for the wallet

    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum WalletError {
        #[error("Synchronization error: {0}")]
        Sync(#[from] wallet_sync::error::SyncError),

        #[error("Identity error: {0}")]
        Identity(String),

        #[error("Storage error: {0}")]
        Storage(String),

        #[error("Internal error: {0}")]
        Internal(String),
    }
}

pub use error::WalletError;

/// The main wallet struct that provides access to all wallet functionality
pub struct Wallet {
    // These will be added as the crates are implemented
}

impl Wallet {
    /// Create a new wallet instance
    pub fn new() -> Result<Self, WalletError> {
        Ok(Wallet {})
    }
    
    /// Get the sync module
    pub fn sync(&self) -> &sync::SyncManager {
        unimplemented!("Sync manager not yet initialized")
    }
    
    /// Get the identity module
    pub fn identity(&self) -> &identity::IdentityManager {
        unimplemented!("Identity manager not yet initialized")
    }
} 