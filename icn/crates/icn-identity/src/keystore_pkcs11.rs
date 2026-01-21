//! PKCS#11 HSM backend for hardware key storage
//!
//! **⚠️ SCAFFOLDING ONLY - NOT PRODUCTION READY ⚠️**
//!
//! This backend is placeholder scaffolding for future HSM support.
//! All operations return errors because:
//! 1. IdentityBundle requires KeyPair refactoring for hardware keys
//! 2. The cryptoki library's internal APIs are not publicly accessible
//!
//! This module requires the `hsm` feature flag.

use crate::keystore_backend::{KeyStoreBackend, Pkcs11Config, SigningBackend};
use crate::{Did, IdentityBundle};
use anyhow::Result;
use ed25519_dalek::VerifyingKey;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tracing::info;

/// PKCS#11 HSM backend (scaffolding only)
///
/// This backend is scaffolding for future HSM support. All operations
/// currently return errors.
pub struct Pkcs11Backend {
    /// Key label for finding the keypair (reserved for future use)
    #[allow(dead_code)]
    key_label: String,
    /// Token label (reserved for future use)
    #[allow(dead_code)]
    token_label: Option<String>,
    /// Locked state (for Sync compatibility)
    locked: Mutex<bool>,
    /// Storage identifier (for path() trait method)
    storage_id: PathBuf,
}

impl Pkcs11Backend {
    /// Create a new PKCS#11 backend (scaffolding)
    ///
    /// Note: This is scaffolding only. The backend cannot perform real
    /// HSM operations until IdentityBundle is refactored for hardware keys.
    pub fn new(config: Pkcs11Config) -> Result<Self> {
        info!(
            "Initializing PKCS#11 backend scaffolding: library={}, slot={}",
            config.library_path, config.slot_id
        );

        let storage_id = PathBuf::from(format!("pkcs11://slot={}", config.slot_id));

        Ok(Self {
            key_label: config.key_label,
            token_label: config.token_label,
            locked: Mutex::new(true),
            storage_id,
        })
    }

    /// Initialize a new keypair in the HSM
    ///
    /// **NOT IMPLEMENTED**: Returns error.
    pub fn init(&mut self, _pin: &[u8]) -> Result<IdentityBundle> {
        anyhow::bail!(
            "PKCS#11 backend init is not yet implemented: IdentityBundle \
             currently requires KeyPair with secret material, but HSM keys \
             never expose secrets. IdentityBundle must be refactored to use \
             DidKey::Hardware before this backend can be used."
        )
    }
}

impl KeyStoreBackend for Pkcs11Backend {
    fn unlock(&mut self, _credentials: &[u8]) -> Result<()> {
        anyhow::bail!(
            "PKCS#11 backend unlock is not yet implemented: \
             Hardware-backed keys require IdentityBundle refactoring. \
             Use software keys for now."
        )
    }

    fn lock(&mut self) {
        if let Ok(mut guard) = self.locked.lock() {
            *guard = true;
        }
        info!("Locked PKCS#11 backend (scaffolding)");
    }

    fn is_locked(&self) -> bool {
        self.locked.lock().map(|g| *g).unwrap_or(true)
    }

    fn get_identity_bundle(&self) -> Result<&IdentityBundle> {
        anyhow::bail!("PKCS#11 backend is scaffolding only - no identity bundle available")
    }

    fn path(&self) -> &Path {
        &self.storage_id
    }

    fn signing_backend(&self) -> Result<Box<dyn SigningBackend>> {
        anyhow::bail!(
            "PKCS#11 signing backend is not yet implemented: \
             Requires IdentityBundle refactoring for hardware keys."
        )
    }

    fn is_hardware_backed(&self) -> bool {
        true
    }

    fn backend_type(&self) -> &str {
        "pkcs11"
    }
}

impl Drop for Pkcs11Backend {
    fn drop(&mut self) {
        self.lock();
    }
}

/// Placeholder signing backend for PKCS#11
///
/// This is scaffolding - all operations return errors.
#[allow(dead_code)]
pub struct Pkcs11SigningBackend {
    key_label: String,
    did: Did,
    verifying_key: VerifyingKey,
}

impl SigningBackend for Pkcs11SigningBackend {
    fn sign(&self, _message: &[u8]) -> Result<ed25519_dalek::Signature> {
        anyhow::bail!("PKCS#11 signing is not yet implemented")
    }

    fn did(&self) -> &Did {
        &self.did
    }

    fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkcs11_backend_scaffolding() {
        // Test that scaffolding compiles and returns appropriate errors
        let config = Pkcs11Config {
            library_path: "/usr/lib/softhsm/libsofthsm2.so".to_string(),
            slot_id: 0,
            key_label: "test-key".to_string(),
            token_label: Some("test-token".to_string()),
        };

        let mut backend = Pkcs11Backend::new(config).unwrap();
        assert!(backend.is_locked());

        // All operations should return errors
        assert!(backend.unlock(b"pin").is_err());
        assert!(backend.init(b"pin").is_err());
        assert!(backend.signing_backend().is_err());
        assert!(backend.get_identity_bundle().is_err());

        // Lock should work (no-op for scaffolding)
        backend.lock();
        assert!(backend.is_locked());
    }
}
