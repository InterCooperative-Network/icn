//! Backend trait abstraction for keystore implementations
//!
//! This module defines the `KeyStoreBackend` trait that enables pluggable
//! storage backends for ICN identities. Supported backends include:
//!
//! - Age-encrypted files (software, default)
//! - PKCS#11 HSM (hardware security modules)
//! - TPM 2.0 (trusted platform module)

use crate::{Did, IdentityBundle};
use anyhow::Result;
use std::path::Path;

/// Backend-specific signing operation
///
/// Some backends (HSM, TPM) perform signing operations internally
/// without exposing the private key. This trait enables backends
/// to implement custom signing logic.
pub trait SigningBackend: Send + Sync {
    /// Sign a message using the backend's signing key
    ///
    /// For hardware backends, this delegates to the secure element.
    /// For software backends, this uses the in-memory keypair.
    fn sign(&self, message: &[u8]) -> Result<ed25519_dalek::Signature>;

    /// Get the public key (DID) associated with this backend
    fn did(&self) -> &Did;

    /// Get the verifying key for signature verification
    fn verifying_key(&self) -> &ed25519_dalek::VerifyingKey;
}

/// Trait for keystore backend implementations
///
/// This trait abstracts over different storage backends:
/// - Software: Age-encrypted files
/// - Hardware: PKCS#11 HSM, TPM 2.0
///
/// All backends must support:
/// - Unlock/lock operations
/// - Key retrieval (or signing delegation)
/// - Identity bundle access
/// - Secure key destruction
pub trait KeyStoreBackend: Send + Sync {
    /// Unlock the keystore with authentication credentials
    ///
    /// For software backends, this is a passphrase.
    /// For hardware backends, this might be PIN + slot ID.
    fn unlock(&mut self, credentials: &[u8]) -> Result<()>;

    /// Lock the keystore (clear in-memory keys)
    fn lock(&mut self);

    /// Check if the keystore is currently locked
    fn is_locked(&self) -> bool;

    /// Get the identity bundle (fails if locked)
    ///
    /// Note: For hardware backends, the keypair in the bundle
    /// may have limited functionality (no secret key export).
    /// Some backends may not support constructing bundles until
    /// signer integration is implemented.
    fn get_identity_bundle(&self) -> Result<&IdentityBundle>;

    /// Get the storage path or identifier
    ///
    /// For software backends, this is a file path.
    /// For hardware backends, this is a slot identifier or URI.
    fn path(&self) -> &Path;

    /// Get a signing backend for this keystore
    ///
    /// This enables hardware backends to perform signing operations
    /// without exposing the private key to the application.
    fn signing_backend(&self) -> Result<Box<dyn SigningBackend>>;

    /// Check if this backend supports hardware-backed keys
    fn is_hardware_backed(&self) -> bool {
        false // Default to software
    }

    /// Get backend type identifier
    fn backend_type(&self) -> &str;
}

/// Configuration for PKCS#11 HSM backend
#[cfg(feature = "hsm")]
#[derive(Debug, Clone)]
pub struct Pkcs11Config {
    /// Path to PKCS#11 library (e.g., /usr/lib/softhsm/libsofthsm2.so)
    pub library_path: String,
    /// HSM slot ID
    pub slot_id: u64,
    /// Label for the key pair
    pub key_label: String,
    /// Optional token label
    pub token_label: Option<String>,
}

/// Configuration for TPM 2.0 backend
#[cfg(feature = "tpm-experimental")]
#[derive(Debug, Clone)]
pub struct TpmConfig {
    /// TPM device path (e.g., /dev/tpmrm0)
    pub device_path: String,
    /// Persistent handle for the key
    pub key_handle: u32,
    /// Whether to use platform binding
    pub platform_binding: bool,
    /// Whether to enable attestation
    pub attestation: bool,
    /// Optional directory for sealed blob storage.
    /// If None, defaults to XDG_DATA_HOME/icn/tpm/ (~/.local/share/icn/tpm/ on Linux).
    /// Using /tmp is insecure as it's world-readable and cleared on reboot.
    pub sealed_blob_dir: Option<std::path::PathBuf>,
}

/// Backend configuration enum
#[derive(Debug, Clone)]
pub enum BackendConfig {
    /// Age-encrypted file backend
    Age {
        /// Path to keystore file
        path: String,
    },
    /// PKCS#11 HSM backend
    #[cfg(feature = "hsm")]
    Pkcs11(Pkcs11Config),
    /// TPM 2.0 backend
    #[cfg(feature = "tpm-experimental")]
    Tpm(TpmConfig),
}

impl BackendConfig {
    /// Get a human-readable backend type name
    pub fn backend_type(&self) -> &str {
        match self {
            BackendConfig::Age { .. } => "age",
            #[cfg(feature = "hsm")]
            BackendConfig::Pkcs11(_) => "pkcs11",
            #[cfg(feature = "tpm-experimental")]
            BackendConfig::Tpm(_) => "tpm",
        }
    }

    /// Check if this is a hardware backend
    pub fn is_hardware(&self) -> bool {
        match self {
            BackendConfig::Age { .. } => false,
            #[cfg(feature = "hsm")]
            BackendConfig::Pkcs11(_) => true,
            #[cfg(feature = "tpm-experimental")]
            BackendConfig::Tpm(_) => true,
        }
    }
}
