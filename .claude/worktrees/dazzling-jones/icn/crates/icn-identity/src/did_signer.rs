//! DID signing abstraction for software and hardware backends
//!
//! This module provides types that enable hardware-backed signing without
//! requiring private key material in memory.

use crate::Did;
use anyhow::Result;
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use zeroize::Zeroizing;

/// Trait for DID signing operations
///
/// This trait abstracts over software and hardware signing backends,
/// enabling HSM/TPM integration without exposing private keys.
pub trait DidSigner: Send + Sync {
    /// Get the DID for this signer
    fn did(&self) -> &Did;

    /// Get the verifying (public) key
    fn verifying_key(&self) -> &VerifyingKey;

    /// Sign a message
    ///
    /// For software signers, this uses the in-memory private key.
    /// For hardware signers, this delegates to HSM/TPM.
    fn sign(&self, message: &[u8]) -> Result<Signature>;

    /// Check if this is a hardware-backed signer
    fn is_hardware_backed(&self) -> bool {
        false // Default to software
    }

    /// Get backend type identifier
    fn backend_type(&self) -> &str {
        "software"
    }
}

/// DID signing key - either software or hardware
///
/// This enum enables honest type representation:
/// - Software keys have private key material
/// - Hardware keys delegate to secure element
///
/// This prevents the "dummy KeyPair" footgun where we pretend
/// to have a private key that doesn't actually exist in memory.
#[derive(Clone)]
pub enum DidKey {
    /// Software key with private key material in memory
    Software {
        /// Secret key bytes (zeroized on drop)
        secret_bytes: Zeroizing<[u8; 32]>,
        /// Public key
        verifying_key: VerifyingKey,
        /// DID derived from public key
        did: Did,
    },

    /// Hardware-backed key (HSM/TPM)
    ///
    /// Private key never exists in application memory.
    /// Signing operations are delegated to hardware.
    Hardware {
        /// DID for this key
        did: Did,
        /// Public key (can be in memory)
        verifying_key: VerifyingKey,
        /// Backend type identifier (e.g., "pkcs11", "tpm")
        backend_type: String,
        /// Hardware-specific identifier (slot/handle/etc)
        hardware_id: String,
    },
}

impl DidKey {
    /// Create a new software key from bytes
    pub fn from_software_bytes(secret_bytes: [u8; 32], public_bytes: [u8; 32]) -> Result<Self> {
        let verifying_key = VerifyingKey::from_bytes(&public_bytes)?;
        let did = Did::from_public_key(&verifying_key);

        // Verify the keys match
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        if signing_key.verifying_key() != verifying_key {
            anyhow::bail!("Public key does not match secret key");
        }

        Ok(Self::Software {
            secret_bytes: Zeroizing::new(secret_bytes),
            verifying_key,
            did,
        })
    }

    /// Create a new hardware-backed key
    pub fn from_hardware(
        verifying_key: VerifyingKey,
        backend_type: String,
        hardware_id: String,
    ) -> Self {
        let did = Did::from_public_key(&verifying_key);

        Self::Hardware {
            did,
            verifying_key,
            backend_type,
            hardware_id,
        }
    }

    /// Get the DID
    pub fn did(&self) -> &Did {
        match self {
            Self::Software { did, .. } => did,
            Self::Hardware { did, .. } => did,
        }
    }

    /// Get the verifying key
    pub fn verifying_key(&self) -> &VerifyingKey {
        match self {
            Self::Software { verifying_key, .. } => verifying_key,
            Self::Hardware { verifying_key, .. } => verifying_key,
        }
    }

    /// Check if this is a hardware-backed key
    pub fn is_hardware_backed(&self) -> bool {
        matches!(self, Self::Hardware { .. })
    }

    /// Get backend type
    pub fn backend_type(&self) -> &str {
        match self {
            Self::Software { .. } => "software",
            Self::Hardware { backend_type, .. } => backend_type,
        }
    }

    /// Get hardware ID (slot/handle/etc) if hardware-backed
    pub fn hardware_id(&self) -> Option<&str> {
        match self {
            Self::Software { .. } => None,
            Self::Hardware { hardware_id, .. } => Some(hardware_id),
        }
    }

    /// Sign a message using software key
    ///
    /// This only works for software keys. Hardware keys must use
    /// a DidSigner implementation that delegates to the hardware.
    ///
    /// Returns an error for hardware keys since we don't have
    /// the private key material.
    pub fn sign_software(&self, message: &[u8]) -> Result<Signature> {
        match self {
            Self::Software { secret_bytes, .. } => {
                use ed25519_dalek::Signer;
                let signing_key = SigningKey::from_bytes(secret_bytes);
                Ok(signing_key.sign(message))
            }
            Self::Hardware { backend_type, .. } => {
                anyhow::bail!(
                    "Cannot sign with hardware key using software method. \
                     Use a DidSigner implementation for backend: {backend_type}"
                )
            }
        }
    }

    /// Get secret bytes (only for software keys)
    ///
    /// This is only available for software keys and should be used
    /// sparingly (e.g., for keystore serialization).
    #[allow(dead_code)]
    pub(crate) fn secret_bytes(&self) -> Result<&[u8; 32]> {
        match self {
            Self::Software { secret_bytes, .. } => Ok(secret_bytes),
            Self::Hardware { .. } => {
                anyhow::bail!("Hardware-backed keys do not expose secret bytes")
            }
        }
    }
}

/// Software DID signer implementation
pub struct SoftwareSigner {
    did_key: DidKey,
}

impl SoftwareSigner {
    /// Create a new software signer
    pub fn new(did_key: DidKey) -> Result<Self> {
        if did_key.is_hardware_backed() {
            anyhow::bail!("SoftwareSigner requires a software DidKey");
        }
        Ok(Self { did_key })
    }
}

impl DidSigner for SoftwareSigner {
    fn did(&self) -> &Did {
        self.did_key.did()
    }

    fn verifying_key(&self) -> &VerifyingKey {
        self.did_key.verifying_key()
    }

    fn sign(&self, message: &[u8]) -> Result<Signature> {
        self.did_key.sign_software(message)
    }

    fn is_hardware_backed(&self) -> bool {
        false
    }

    fn backend_type(&self) -> &str {
        "software"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    #[test]
    fn test_software_key_creation() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let public_bytes = signing_key.verifying_key().to_bytes();

        let did_key = DidKey::from_software_bytes(secret_bytes, public_bytes).unwrap();

        assert!(!did_key.is_hardware_backed());
        assert_eq!(did_key.backend_type(), "software");
        assert!(did_key.hardware_id().is_none());
    }

    #[test]
    fn test_software_signing() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let public_bytes = signing_key.verifying_key().to_bytes();

        let did_key = DidKey::from_software_bytes(secret_bytes, public_bytes).unwrap();

        let message = b"test message";
        let signature = did_key.sign_software(message).unwrap();

        // Verify signature
        use ed25519_dalek::Verifier;
        assert!(did_key.verifying_key().verify(message, &signature).is_ok());
    }

    #[test]
    fn test_hardware_key_creation() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let did_key = DidKey::from_hardware(
            verifying_key,
            "pkcs11".to_string(),
            "slot=0,label=test".to_string(),
        );

        assert!(did_key.is_hardware_backed());
        assert_eq!(did_key.backend_type(), "pkcs11");
        assert_eq!(did_key.hardware_id(), Some("slot=0,label=test"));
    }

    #[test]
    fn test_hardware_key_cannot_sign_software() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let did_key =
            DidKey::from_hardware(verifying_key, "pkcs11".to_string(), "slot=0".to_string());

        let message = b"test message";
        let result = did_key.sign_software(message);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot sign with hardware key"));
    }

    #[test]
    fn test_hardware_key_no_secret_bytes() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let did_key = DidKey::from_hardware(
            verifying_key,
            "tpm".to_string(),
            "handle=0x81000001".to_string(),
        );

        let result = did_key.secret_bytes();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("do not expose secret bytes"));
    }

    #[test]
    fn test_software_signer() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let public_bytes = signing_key.verifying_key().to_bytes();

        let did_key = DidKey::from_software_bytes(secret_bytes, public_bytes).unwrap();
        let signer = SoftwareSigner::new(did_key).unwrap();

        let message = b"test message";
        let signature = signer.sign(message).unwrap();

        // Verify signature
        use ed25519_dalek::Verifier;
        assert!(signer.verifying_key().verify(message, &signature).is_ok());
    }
}
