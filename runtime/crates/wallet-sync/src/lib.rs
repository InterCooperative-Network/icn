/*!
 * ICN Wallet Sync
 *
 * Synchronization and communication between wallet and ICN nodes.
 */

pub mod compat;
pub mod federation;
pub mod credentials;
pub mod export;

pub use credentials::{CredentialStore as InternalCredentialStore, CredentialManager, CredentialError, CredentialResult};
pub use federation::{
    FederationSyncClient, FederationSyncClientConfig, FederationEndpoint,
    CredentialStore as FederationCredentialStore, CredentialNotifier,
    MemoryCredentialStore, SyncCredentialType, FederationSyncError,
    VerifiableCredential, ExportFormat, verify_execution_receipt
};
pub use export::{
    export_receipts_to_file, import_receipts_from_file, ExportError
};
pub use compat::{
    WalletDagNode, WalletDagNodeMetadata, CompatError, CompatResult,
    runtime_to_wallet, wallet_to_runtime,
    legacy_to_wallet, wallet_to_legacy,
    system_time_to_datetime, datetime_to_system_time
};

use anyhow::Result;
use icn_dag::DagManager;
use icn_identity::IdentityId;
use icn_storage::Storage;
use std::sync::{Arc, Mutex};

/// The main wallet sync manager that orchestrates synchronization between 
/// wallets and ICN nodes.
pub struct WalletSync {
    storage: Arc<Mutex<dyn Storage>>,
    dag_manager: Arc<DagManager>,
}

impl WalletSync {
    /// Create a new wallet synchronization manager
    pub fn new(storage: Arc<Mutex<dyn Storage>>) -> Self {
        let dag_manager = Arc::new(DagManager::new(storage.clone()));
        Self {
            storage,
            dag_manager,
        }
    }

    /// Synchronize a wallet with the latest state
    pub async fn sync_wallet(&self, _wallet_id: &IdentityId) -> Result<()> {
        // Placeholder for wallet sync functionality
        tracing::info!("Wallet sync initiated");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
} 