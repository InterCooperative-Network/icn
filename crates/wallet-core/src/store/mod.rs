pub mod file;
pub mod secure;

pub use file::FileStore;
pub use secure::{
    SecureStorageProvider,
    SecurePlatform,
    MockSecureProvider,
    get_platform_provider,
};

use async_trait::async_trait;
use crate::error::{WalletResult, WalletError};
use crate::identity::IdentityWallet;
use crate::vc::VerifiableCredential;
use crate::dag::{DagNode, DagThread, CachedDagThreadInfo};
use crate::crypto::KeyPair;

/// Trait defining storage operations for wallet data
#[async_trait]
pub trait LocalWalletStore: Send + Sync + Clone {
    /// Initialize the store
    async fn init(&self) -> WalletResult<()>;
    
    /// Save an identity to the store
    async fn save_identity(&self, identity: &IdentityWallet) -> WalletResult<()>;
    
    /// Load an identity from the store by DID
    async fn load_identity(&self, did: &str) -> WalletResult<IdentityWallet>;
    
    /// List all stored identities
    async fn list_identities(&self) -> WalletResult<Vec<String>>;
    
    /// Save a credential to the store
    async fn save_credential(&self, credential: &VerifiableCredential, id: &str) -> WalletResult<()>;
    
    /// Load a credential from the store by ID
    async fn load_credential(&self, id: &str) -> WalletResult<VerifiableCredential>;
    
    /// List all stored credentials
    async fn list_credentials(&self) -> WalletResult<Vec<String>>;
    
    /// Save a DAG node to the store
    async fn save_dag_node(&self, cid: &str, node: &DagNode) -> WalletResult<()>;
    
    /// Load a DAG node from the store by CID
    async fn load_dag_node(&self, cid: &str) -> WalletResult<DagNode>;
    
    /// Save a DAG thread to the store
    async fn save_dag_thread(&self, thread_id: &str, thread: &DagThread) -> WalletResult<()>;
    
    /// Load a DAG thread from the store by ID
    async fn load_dag_thread(&self, thread_id: &str) -> WalletResult<DagThread>;
    
    /// List all stored DAG threads
    async fn list_dag_threads(&self) -> WalletResult<Vec<String>>;

    /// Store a keypair securely, identified by a unique ID
    async fn store_keypair(&self, id: &str, keypair: &KeyPair) -> WalletResult<()>;
    
    /// Load a keypair by its ID
    async fn load_keypair(&self, id: &str) -> WalletResult<KeyPair>;
    
    /// Delete a keypair by its ID
    async fn delete_keypair(&self, id: &str) -> WalletResult<()>;
    
    /// Check if a keypair exists
    async fn has_keypair(&self, id: &str) -> WalletResult<bool>;
    
    /// List all stored keypair IDs
    async fn list_keypairs(&self) -> WalletResult<Vec<String>>;

    /// Save DAG thread cache information
    async fn save_dag_thread_cache(&self, thread_id: &str, cache: &CachedDagThreadInfo) -> WalletResult<()>;
    
    /// Load DAG thread cache information
    async fn load_dag_thread_cache(&self, thread_id: &str) -> WalletResult<CachedDagThreadInfo>;
    
    /// List all cached DAG thread IDs
    async fn list_dag_thread_caches(&self) -> WalletResult<Vec<String>>;
} 