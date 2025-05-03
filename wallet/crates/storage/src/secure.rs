use crate::error::{StorageError, StorageResult};
use crate::traits::{SecureStorage, ensure_directory};
use std::path::{Path, PathBuf};
use async_trait::async_trait;
use tokio::fs;
use serde::{Serialize, de::DeserializeOwned};
use tracing::debug;
use rand::{rngs::OsRng, RngCore};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce
};
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::sync::Arc;

/// Simple secure storage implementation
/// 
/// Note: This is a basic implementation for development.
/// In a production environment, this should use platform-specific
/// secure storage APIs like Keychain on iOS/macOS or KeyStore on Android.
pub struct SimpleSecureStorage {
    /// Directory where encrypted data is stored
    secure_dir: PathBuf,
    
    /// Encryption key
    encryption_key: [u8; 32],
    
    /// In-memory cache of decrypted values
    cache: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl SimpleSecureStorage {
    /// Create a new secure storage with an auto-generated key
    pub async fn new(base_dir: impl AsRef<Path>) -> StorageResult<Self> {
        let secure_dir = base_dir.as_ref().join("secure");
        ensure_directory(&secure_dir).await?;
        
        // Generate a random encryption key
        let mut encryption_key = [0u8; 32];
        OsRng.fill_bytes(&mut encryption_key);
        
        Ok(Self {
            secure_dir,
            encryption_key,
            cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    /// Create a secure storage with a specific encryption key
    pub async fn with_key(base_dir: impl AsRef<Path>, key: [u8; 32]) -> StorageResult<Self> {
        let secure_dir = base_dir.as_ref().join("secure");
        ensure_directory(&secure_dir).await?;
        
        Ok(Self {
            secure_dir,
            encryption_key: key,
            cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    
    /// Get the path for a secure storage file
    fn secure_path(&self, key: &str) -> PathBuf {
        self.secure_dir.join(format!("{}.enc", key))
    }
    
    /// Encrypt data
    fn encrypt(&self, data: &[u8]) -> StorageResult<Vec<u8>> {
        // Create cipher
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| StorageError::EncryptionError(format!("Failed to create cipher: {}", e)))?;
            
        // Generate a random nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        // Encrypt the data
        let ciphertext = cipher.encrypt(nonce, data)
            .map_err(|e| StorageError::EncryptionError(format!("Encryption failed: {}", e)))?;
            
        // Combine nonce and ciphertext for storage
        let mut result = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        
        Ok(result)
    }
    
    /// Decrypt data
    fn decrypt(&self, encrypted_data: &[u8]) -> StorageResult<Vec<u8>> {
        if encrypted_data.len() < 12 {
            return Err(StorageError::DataCorruption("Encrypted data too small".to_string()));
        }
        
        // Extract nonce and ciphertext
        let nonce = Nonce::from_slice(&encrypted_data[..12]);
        let ciphertext = &encrypted_data[12..];
        
        // Create cipher
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| StorageError::EncryptionError(format!("Failed to create cipher: {}", e)))?;
            
        // Decrypt the data
        let plaintext = cipher.decrypt(nonce, ciphertext)
            .map_err(|e| StorageError::EncryptionError(format!("Decryption failed: {}", e)))?;
            
        Ok(plaintext)
    }
}

#[async_trait]
impl SecureStorage for SimpleSecureStorage {
    async fn store_secret<V: Serialize + Send + Sync>(&self, key: &str, value: &V) -> StorageResult<()> {
        // Serialize the value
        let serialized = serde_json::to_vec(value)
            .map_err(|e| StorageError::SerializationError(format!("Failed to serialize secret: {}", e)))?;
            
        // Encrypt the serialized data
        let encrypted = self.encrypt(&serialized)?;
        
        // Write to disk
        let path = self.secure_path(key);
        fs::write(&path, &encrypted).await?;
        
        // Update cache
        let mut cache = self.cache.lock().await;
        cache.insert(key.to_string(), serialized);
        
        debug!("Stored secret for key: {}", key);
        
        Ok(())
    }
    
    async fn get_secret<V: DeserializeOwned + Send + Sync>(&self, key: &str) -> StorageResult<V> {
        // Check if we have it in cache
        let mut cache = self.cache.lock().await;
        if let Some(cached) = cache.get(key) {
            // Deserialize from cache
            return serde_json::from_slice(cached)
                .map_err(|e| StorageError::SerializationError(format!("Failed to deserialize cached secret: {}", e)));
        }
        
        // Not in cache, read from disk
        let path = self.secure_path(key);
        
        if !path.exists() {
            return Err(StorageError::NotFound(format!("Secret not found: {}", key)));
        }
        
        // Read encrypted data
        let encrypted = fs::read(&path).await?;
        
        // Decrypt
        let decrypted = self.decrypt(&encrypted)?;
        
        // Update cache
        cache.insert(key.to_string(), decrypted.clone());
        
        // Deserialize
        let value = serde_json::from_slice(&decrypted)
            .map_err(|e| StorageError::SerializationError(format!("Failed to deserialize secret: {}", e)))?;
            
        Ok(value)
    }
    
    async fn delete_secret(&self, key: &str) -> StorageResult<()> {
        let path = self.secure_path(key);
        
        if path.exists() {
            fs::remove_file(&path).await?;
            
            // Remove from cache
            let mut cache = self.cache.lock().await;
            cache.remove(key);
            
            debug!("Deleted secret: {}", key);
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_secure_storage() -> StorageResult<()> {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        
        // Create secure storage
        let storage = SimpleSecureStorage::new(temp_dir.path()).await?;
        
        // Test data
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct TestSecret {
            username: String,
            password: String,
        }
        
        let secret = TestSecret {
            username: "test_user".to_string(),
            password: "test_password".to_string(),
        };
        
        // Store the secret
        storage.store_secret("test_creds", &secret).await?;
        
        // Retrieve the secret
        let retrieved: TestSecret = storage.get_secret("test_creds").await?;
        
        // Verify it matches
        assert_eq!(secret, retrieved);
        
        // Delete the secret
        storage.delete_secret("test_creds").await?;
        
        // Verify it's gone
        let result = storage.get_secret::<TestSecret>("test_creds").await;
        assert!(result.is_err());
        
        Ok(())
    }
} 