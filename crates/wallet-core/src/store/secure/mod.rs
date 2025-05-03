// Platform-specific secure storage backends
// Each backend implements the SecureStorageProvider trait

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
pub fn get_platform_provider() -> impl crate::store::secure::SecureStorageProvider {
    AndroidKeystoreProvider::new()
}

#[cfg(target_os = "ios")]
pub fn get_platform_provider() -> impl crate::store::secure::SecureStorageProvider {
    IosKeychainProvider::new()
}

// For non-mobile platforms, use the mock provider
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn get_platform_provider() -> impl crate::store::secure::SecureStorageProvider {
    MockSecureProvider::new(crate::store::secure::SecurePlatform::Generic)
} 