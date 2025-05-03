use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use crate::error::{WalletResult, WalletError};
use crate::store::secure::SecureStorageProvider;
use super::super::SecurePlatform;

/// Mock secure storage provider, simulating secure enclave behavior
/// but storing in memory. Useful for testing.
#[derive(Clone)]
pub struct MockSecureProvider {
    /// In-memory store
    store: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    /// The simulated platform, affects behavior
    platform: SecurePlatform,
}

impl MockSecureProvider {
    pub fn new(platform: SecurePlatform) -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
            platform,
        }
    }
}

#[async_trait]
impl SecureStorageProvider for MockSecureProvider {
    async fn store(&self, key: &str, data: &[u8]) -> WalletResult<()> {
        let mut store = self.store.lock().unwrap();
        store.insert(key.to_string(), data.to_vec());
        Ok(())
    }
    
    async fn retrieve(&self, key: &str) -> WalletResult<Vec<u8>> {
        let store = self.store.lock().unwrap();
        store.get(key)
            .cloned()
            .ok_or_else(|| WalletError::NotFound(format!("Key not found in secure storage: {}", key)))
    }
    
    async fn delete(&self, key: &str) -> WalletResult<()> {
        let mut store = self.store.lock().unwrap();
        if store.remove(key).is_none() {
            return Err(WalletError::NotFound(format!("Key not found in secure storage: {}", key)));
        }
        Ok(())
    }
    
    async fn exists(&self, key: &str) -> WalletResult<bool> {
        let store = self.store.lock().unwrap();
        Ok(store.contains_key(key))
    }
    
    async fn list_keys(&self) -> WalletResult<Vec<String>> {
        let store = self.store.lock().unwrap();
        Ok(store.keys().cloned().collect())
    }
} 