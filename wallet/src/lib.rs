/*! 
# ICN Wallet

Mobile-first agent for identity, credentials, and DAG participation.

This is the top-level crate that composes the wallet functionality from its component crates:
- wallet-identity: For DID and VC support
- wallet-storage: For secure credential and key storage
- icn-wallet-sync: For DAG synchronization with the runtime mesh
- wallet-actions: For DAG operations and proposal management
- wallet-api: For application-facing interfaces

*/

// use icn_wallet_sync as sync; // Temporarily disabled
use wallet_identity as identity;
use wallet_storage as storage;
use wallet_actions as actions;
use wallet_api as api;

pub mod error {
    //! Error types for the wallet

    use thiserror::Error;
    // use super::sync; // Temporarily disabled

    #[derive(Error, Debug)]
    pub enum WalletError {
        // #[error("Synchronization error: {0}")]
        // Sync(#[from] sync::federation::FederationSyncError), // Temporarily disabled

        #[error("Identity error: {0}")]
        Identity(String),

        #[error("Storage error: {0}")]
        Storage(String),

        #[error("Internal error: {0}")]
        Internal(String),

        // #[error("Compatibility error: {0}")]
        // Compatibility(#[from] sync::compat::CompatError), // Temporarily disabled
    }
}

pub use error::WalletError;
// use sync::WalletSync; // Temporarily disabled

/// The main wallet struct that provides access to all wallet functionality
pub struct Wallet {
    // sync_manager: WalletSync, // Temporarily disabled
}

impl Wallet {
    /// Create a new wallet instance
    pub fn new(storage_manager: storage::StorageManager) -> Result<Self, WalletError> {
        // Assuming file_storage() provides the necessary interface
        let storage = storage_manager.file_storage();
        // let sync_manager = WalletSync::new(storage.clone()); // Temporarily disabled

        Ok(Wallet {
            // sync_manager, // Temporarily disabled
        })
    }

    /// Get the sync module
    // pub fn sync(&self) -> &WalletSync { // Temporarily disabled
    //     &self.sync_manager
    // }

    /// Get the identity module
    pub fn identity(&self) -> &identity::IdentityManager {
        unimplemented!("Identity manager not yet initialized")
    }
} 