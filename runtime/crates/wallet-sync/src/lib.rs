/*!
 * ICN Wallet Sync
 *
 * Synchronization and communication between wallet and ICN nodes.
 */

pub mod credentials;
pub mod federation;

pub use credentials::{CredentialStore, CredentialManager};
pub use federation::{
    FederationSyncClient, FederationSyncClientConfig, FederationEndpoint,
    CredentialStore as FederationCredentialStore, CredentialNotifier,
    MemoryCredentialStore, SyncCredentialType
}; 