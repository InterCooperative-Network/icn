use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use crate::error::{WalletResult, WalletError};
use crate::identity::IdentityWallet;
use crate::vc::VerifiableCredential;
use crate::dag::{DagNode, DagThread};
use crate::crypto::KeyPair;
use super::LocalWalletStore;
use super::file::FileStore;
use super::secure::SecureStorageProvider;
use super::secure::get_platform_provider;

/// Platform types for secure storage
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecurePlatform {
    /// Android platform (using KeyStore)
    Android,
    /// iOS platform (using Keychain)
    Ios,
    /// Desktop platform (using system keyring)
    Desktop,
    /// Generic platform (using file-based storage with encryption)
    Generic,
}

/// Secure storage provider trait that platform-specific implementations will use
#[async_trait]
pub trait SecureStorageProvider: Send + Sync + Clone {
    /// Store data securely
    async fn store(&self, key: &str, data: &[u8]) -> WalletResult<()>;
    
    /// Retrieve data from secure storage
    async fn retrieve(&self, key: &str) -> WalletResult<Vec<u8>>;
    
    /// Delete data from secure storage
    async fn delete(&self, key: &str) -> WalletResult<()>;
    
    /// Check if data exists
    async fn exists(&self, key: &str) -> WalletResult<bool>;
    
    /// List all keys in secure storage
    async fn list_keys(&self) -> WalletResult<Vec<String>>;
}

/// A secure storage implementation that uses a secure provider for key data
/// but delegates non-sensitive data to a file store
#[derive(Clone)]
pub struct SecureStore<P: SecureStorageProvider> {
    /// The secure provider for sensitive data like keys
    secure_provider: P,
    /// File store for non-sensitive data
    file_store: FileStore,
    /// Key prefix for secure storage
    key_prefix: String,
}

impl<P: SecureStorageProvider> SecureStore<P> {
    pub fn new(secure_provider: P, file_store: FileStore, key_prefix: &str) -> Self {
        Self {
            secure_provider,
            file_store,
            key_prefix: key_prefix.to_string(),
        }
    }
    
    /// Get the secure key with prefix
    fn get_secure_key(&self, id: &str) -> String {
        format!("{}.{}", self.key_prefix, id)
    }
}

#[async_trait]
impl<P: SecureStorageProvider + 'static> LocalWalletStore for SecureStore<P> {
    async fn init(&self) -> WalletResult<()> {
        // Initialize the file store component
        self.file_store.init().await
    }
    
    // --- Identity operations ---
    
    async fn save_identity(&self, identity: &IdentityWallet) -> WalletResult<()> {
        self.file_store.save_identity(identity).await
    }
    
    async fn load_identity(&self, did: &str) -> WalletResult<IdentityWallet> {
        self.file_store.load_identity(did).await
    }
    
    async fn list_identities(&self) -> WalletResult<Vec<String>> {
        self.file_store.list_identities().await
    }
    
    // --- Credential operations ---
    
    async fn save_credential(&self, credential: &VerifiableCredential, id: &str) -> WalletResult<()> {
        self.file_store.save_credential(credential, id).await
    }
    
    async fn load_credential(&self, id: &str) -> WalletResult<VerifiableCredential> {
        self.file_store.load_credential(id).await
    }
    
    async fn list_credentials(&self) -> WalletResult<Vec<String>> {
        self.file_store.list_credentials().await
    }
    
    // --- DAG operations ---
    
    async fn save_dag_node(&self, cid: &str, node: &DagNode) -> WalletResult<()> {
        self.file_store.save_dag_node(cid, node).await
    }
    
    async fn load_dag_node(&self, cid: &str) -> WalletResult<DagNode> {
        self.file_store.load_dag_node(cid).await
    }
    
    async fn save_dag_thread(&self, thread_id: &str, thread: &DagThread) -> WalletResult<()> {
        self.file_store.save_dag_thread(thread_id, thread).await
    }
    
    async fn load_dag_thread(&self, thread_id: &str) -> WalletResult<DagThread> {
        self.file_store.load_dag_thread(thread_id).await
    }
    
    async fn list_dag_threads(&self) -> WalletResult<Vec<String>> {
        self.file_store.list_dag_threads().await
    }
    
    // --- Secure Keypair operations ---
    
    async fn store_keypair(&self, id: &str, keypair: &KeyPair) -> WalletResult<()> {
        // Serialize the keypair to bytes
        let keypair_data = serde_json::to_vec(keypair)
            .map_err(|e| WalletError::SerializationError(format!("Failed to serialize keypair: {}", e)))?;
        
        // Store in the secure provider with prefix
        let secure_key = self.get_secure_key(id);
        self.secure_provider.store(&secure_key, &keypair_data).await
    }
    
    async fn load_keypair(&self, id: &str) -> WalletResult<KeyPair> {
        // Retrieve from the secure provider with prefix
        let secure_key = self.get_secure_key(id);
        let keypair_data = self.secure_provider.retrieve(&secure_key).await?;
        
        // Deserialize the keypair
        serde_json::from_slice(&keypair_data)
            .map_err(|e| WalletError::SerializationError(format!("Failed to deserialize keypair: {}", e)))
    }
    
    async fn delete_keypair(&self, id: &str) -> WalletResult<()> {
        // Delete from the secure provider with prefix
        let secure_key = self.get_secure_key(id);
        self.secure_provider.delete(&secure_key).await
    }
    
    async fn has_keypair(&self, id: &str) -> WalletResult<bool> {
        // Check if exists in the secure provider with prefix
        let secure_key = self.get_secure_key(id);
        self.secure_provider.exists(&secure_key).await
    }
    
    async fn list_keypairs(&self) -> WalletResult<Vec<String>> {
        // List from the secure provider, and strip the prefix
        let keys = self.secure_provider.list_keys().await?;
        let prefix = format!("{}.", self.key_prefix);
        
        Ok(keys.into_iter()
            .filter_map(|key| key.strip_prefix(&prefix).map(|s| s.to_string()))
            .collect())
    }
}

/// Create a secure store appropriate for the current platform
pub fn create_platform_secure_store(file_base_path: &str) -> impl LocalWalletStore {
    let platform_provider = get_platform_provider();
    let file_store = FileStore::new(file_base_path);
    SecureStore::new(platform_provider, file_store, "icn.wallet")
} 