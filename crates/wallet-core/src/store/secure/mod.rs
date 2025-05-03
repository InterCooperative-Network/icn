// Platform-specific secure storage backends
// Each backend implements the SecureStorageProvider trait

use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use crate::error::{WalletResult, WalletError};

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

#[cfg(target_os = "android")]
mod android_keystore;
#[cfg(target_os = "android")]
pub use android_keystore::AndroidKeystoreProvider;

#[cfg(target_os = "ios")]
mod ios_keychain;
#[cfg(target_os = "ios")]
pub use ios_keychain::IosKeychainProvider;

// Mock provider for testing and non-mobile platforms
mod mock;
pub use mock::MockSecureProvider;

// Platform-specific provider selection
#[cfg(target_os = "android")]
pub fn get_platform_provider() -> impl SecureStorageProvider {
    AndroidKeystoreProvider::new()
}

#[cfg(target_os = "ios")]
pub fn get_platform_provider() -> impl SecureStorageProvider {
    IosKeychainProvider::new()
}

// For non-mobile platforms, use the mock provider
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn get_platform_provider() -> impl SecureStorageProvider {
    MockSecureProvider::new(SecurePlatform::Generic)
} 