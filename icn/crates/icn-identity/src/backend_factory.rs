//! Backend factory for creating keystore instances
//!
//! This module provides a factory pattern for creating keystore backends
//! based on configuration. It supports:
//! - Age (software, default)
//! - PKCS#11 HSM (feature-gated)
//! - TPM 2.0 (feature-gated, experimental)

use crate::keystore_backend::BackendConfig;
use crate::{AgeKeyStore, KeyStore};
use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

#[cfg(feature = "hsm")]
use crate::keystore_backend::Pkcs11Config;
#[cfg(feature = "hsm")]
use crate::keystore_pkcs11::Pkcs11Backend;

#[cfg(feature = "tpm-experimental")]
use crate::keystore_backend::TpmConfig;
#[cfg(feature = "tpm-experimental")]
use crate::keystore_tpm::TpmBackend;

/// Open a keystore backend based on configuration
///
/// This factory function creates the appropriate backend instance based
/// on the provided configuration. The backend will be locked initially
/// and must be unlocked with credentials before use.
///
/// # Arguments
/// * `config` - Backend configuration (Age, PKCS#11, or TPM)
///
/// # Returns
/// A boxed keystore trait object
///
/// # Errors
/// Returns an error if:
/// - The backend type is not supported (feature flag not enabled)
/// - The backend initialization fails
/// - Configuration is invalid
///
/// # Example
/// ```no_run
/// use icn_identity::backend_factory::open_keystore;
/// use icn_identity::keystore_backend::BackendConfig;
///
/// let config = BackendConfig::Age {
///     path: "/home/user/.icn/identity.age".to_string(),
/// };
///
/// let backend = open_keystore(config)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn open_keystore(config: BackendConfig) -> Result<Box<dyn KeyStore>> {
    match config {
        BackendConfig::Age { path } => {
            info!("Opening Age keystore backend: {}", path);
            let backend = AgeKeyStore::open(Path::new(&path))
                .context("Failed to open Age keystore")?;
            Ok(Box::new(backend))
        }

        #[cfg(feature = "hsm")]
        BackendConfig::Pkcs11(pkcs11_config) => {
            info!(
                "Opening PKCS#11 keystore backend: library={}, slot={}",
                pkcs11_config.library_path, pkcs11_config.slot_id
            );
            // Note: PKCS#11 backend is scaffolding only - not yet functional
            anyhow::bail!(
                "PKCS#11 backend is scaffolding only and not yet functional. \
                 See issue #744 for implementation status."
            );
        }

        #[cfg(feature = "tpm-experimental")]
        BackendConfig::Tpm(tpm_config) => {
            info!(
                "Opening TPM 2.0 keystore backend: device={}, handle={:#x}",
                tpm_config.device_path, tpm_config.key_handle
            );
            // Note: TPM backend is scaffolding only - not yet functional
            anyhow::bail!(
                "TPM backend is scaffolding only and not yet functional. \
                 See issue #745 for implementation status."
            );
        }
    }
}

/// Initialize a new keystore with the specified backend
///
/// Creates a new keystore file/slot and generates initial credentials.
/// This is used during identity initialization.
///
/// # Arguments
/// * `config` - Backend configuration
/// * `credentials` - Initial credentials (passphrase, PIN, etc.)
///
/// # Returns
/// A boxed keystore trait object
///
/// # Errors
/// Returns an error if:
/// - The backend type is not supported
/// - Initialization fails
/// - The keystore already exists (for file-based backends)
pub fn init_keystore(
    config: BackendConfig,
    credentials: &[u8],
) -> Result<Box<dyn KeyStore>> {
    match config {
        BackendConfig::Age { path } => {
            info!("Initializing Age keystore: {}", path);
            let backend = AgeKeyStore::init(Path::new(&path), credentials)
                .context("Failed to initialize Age keystore")?;
            Ok(Box::new(backend))
        }

        #[cfg(feature = "hsm")]
        BackendConfig::Pkcs11(_pkcs11_config) => {
            anyhow::bail!(
                "PKCS#11 backend is scaffolding only and not yet functional. \
                 See issue #744 for implementation status."
            );
        }

        #[cfg(feature = "tpm-experimental")]
        BackendConfig::Tpm(_tpm_config) => {
            anyhow::bail!(
                "TPM backend is scaffolding only and not yet functional. \
                 See issue #745 for implementation status."
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_age_backend_config() {
        let config = BackendConfig::Age {
            path: "/tmp/test.age".to_string(),
        };

        assert_eq!(config.backend_type(), "age");
        assert!(!config.is_hardware());
    }

    #[cfg(feature = "hsm")]
    #[test]
    fn test_pkcs11_backend_config() {
        let config = BackendConfig::Pkcs11(Pkcs11Config {
            library_path: "/usr/lib/libsofthsm2.so".to_string(),
            slot_id: 0,
            key_label: "test-key".to_string(),
            token_label: Some("test-token".to_string()),
        });

        assert_eq!(config.backend_type(), "pkcs11");
        assert!(config.is_hardware());
    }

    #[cfg(feature = "tpm-experimental")]
    #[test]
    fn test_tpm_backend_config() {
        let config = BackendConfig::Tpm(TpmConfig {
            device_path: "/dev/tpmrm0".to_string(),
            key_handle: 0x8100_0000,
            platform_binding: true,
            attestation: false,
        });

        assert_eq!(config.backend_type(), "tpm");
        assert!(config.is_hardware());
    }

    #[test]
    fn test_open_age_keystore_nonexistent() {
        let config = BackendConfig::Age {
            path: "/tmp/nonexistent-test-keystore.age".to_string(),
        };

        let result = open_keystore(config);
        // Should fail because keystore doesn't exist
        assert!(result.is_err());
    }
}
