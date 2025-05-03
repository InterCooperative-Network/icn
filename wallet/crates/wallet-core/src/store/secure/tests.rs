#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::crypto::KeyPair;
    use crate::error::WalletResult;
    use async_trait::async_trait;
    
    // Mock KeyPair for testing
    fn create_test_keypair() -> KeyPair {
        KeyPair {
            public_key: vec![1, 2, 3, 4],
            private_key: Some(vec![5, 6, 7, 8]),
            key_type: "ed25519".to_string(),
        }
    }
    
    #[tokio::test]
    async fn test_mock_secure_provider() {
        // Create a mock provider
        let provider = mock::MockSecureProvider::new(SecurePlatform::Generic);
        
        // Test storing and retrieving data
        let key = "test_key";
        let data = b"test_data";
        
        // Store data
        provider.store(key, data).await.expect("Failed to store data");
        
        // Verify data exists
        let exists = provider.exists(key).await.expect("Failed to check if key exists");
        assert!(exists, "Key should exist");
        
        // Retrieve data
        let retrieved = provider.retrieve(key).await.expect("Failed to retrieve data");
        assert_eq!(retrieved, data, "Retrieved data should match stored data");
        
        // List keys
        let keys = provider.list_keys().await.expect("Failed to list keys");
        assert_eq!(keys.len(), 1, "Should have one key");
        assert_eq!(keys[0], key, "Key should match");
        
        // Delete data
        provider.delete(key).await.expect("Failed to delete data");
        
        // Verify data no longer exists
        let exists = provider.exists(key).await.expect("Failed to check if key exists");
        assert!(!exists, "Key should not exist after deletion");
        
        // Retrieving deleted data should fail
        let result = provider.retrieve(key).await;
        assert!(result.is_err(), "Retrieving deleted data should fail");
    }
    
    #[tokio::test]
    async fn test_secure_store() {
        // Create a temporary directory for testing
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let file_base_path = temp_dir.path().to_str().unwrap();
        
        // Create a mock provider
        let provider = mock::MockSecureProvider::new(SecurePlatform::Generic);
        
        // Create a file store
        let file_store = FileStore::new(file_base_path);
        
        // Create a secure store
        let secure_store = SecureStore::new(provider, file_store, "test");
        
        // Initialize the store
        secure_store.init().await.expect("Failed to initialize store");
        
        // Test storing and retrieving keypairs
        let id = "test_keypair";
        let keypair = create_test_keypair();
        
        // Store the keypair
        secure_store.store_keypair(id, &keypair).await.expect("Failed to store keypair");
        
        // Verify keypair exists
        let has_keypair = secure_store.has_keypair(id).await.expect("Failed to check if keypair exists");
        assert!(has_keypair, "Keypair should exist");
        
        // Retrieve the keypair
        let retrieved = secure_store.load_keypair(id).await.expect("Failed to load keypair");
        assert_eq!(retrieved.public_key, keypair.public_key, "Public key should match");
        assert_eq!(retrieved.private_key, keypair.private_key, "Private key should match");
        assert_eq!(retrieved.key_type, keypair.key_type, "Key type should match");
        
        // List keypairs
        let keypairs = secure_store.list_keypairs().await.expect("Failed to list keypairs");
        assert_eq!(keypairs.len(), 1, "Should have one keypair");
        assert_eq!(keypairs[0], id, "Keypair ID should match");
        
        // Delete the keypair
        secure_store.delete_keypair(id).await.expect("Failed to delete keypair");
        
        // Verify keypair no longer exists
        let has_keypair = secure_store.has_keypair(id).await.expect("Failed to check if keypair exists");
        assert!(!has_keypair, "Keypair should not exist after deletion");
        
        // Retrieving deleted keypair should fail
        let result = secure_store.load_keypair(id).await;
        assert!(result.is_err(), "Loading deleted keypair should fail");
    }
    
    #[cfg(target_os = "android")]
    #[tokio::test]
    async fn test_android_keystore_provider() {
        // This test only runs on Android
        // It requires a valid Android environment with JNI access
        
        // Create an Android provider
        let provider = android_keystore::AndroidKeystoreProvider::new();
        
        // Run basic tests
        run_provider_tests(provider).await;
    }
    
    #[cfg(target_os = "ios")]
    #[tokio::test]
    async fn test_ios_keychain_provider() {
        // This test only runs on iOS
        // It requires a valid iOS environment
        
        // Create an iOS provider
        let provider = ios_keychain::IosKeychainProvider::new();
        
        // Run basic tests
        run_provider_tests(provider).await;
    }
    
    // Generic helper to test any provider implementation
    async fn run_provider_tests<P>(provider: P) 
    where 
        P: SecureStorageProvider
    {
        // Test storing and retrieving data
        let key = "test_key";
        let data = b"test_data";
        
        // Store data
        provider.store(key, data).await.expect("Failed to store data");
        
        // Verify data exists
        let exists = provider.exists(key).await.expect("Failed to check if key exists");
        assert!(exists, "Key should exist");
        
        // Retrieve data
        let retrieved = provider.retrieve(key).await.expect("Failed to retrieve data");
        assert_eq!(retrieved, data, "Retrieved data should match stored data");
        
        // Delete data
        provider.delete(key).await.expect("Failed to delete data");
        
        // Verify data no longer exists
        let exists = provider.exists(key).await.expect("Failed to check if key exists");
        assert!(!exists, "Key should not exist after deletion");
    }
} 