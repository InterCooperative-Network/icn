//! ML-DSA (Module-Lattice Digital Signature Algorithm) wrapper
//!
//! This module wraps the pqcrypto-dilithium crate to provide a clean API
//! for ML-DSA signatures (NIST FIPS 204).
//!
//! We use Dilithium3 (ML-DSA-65) which provides:
//! - Security level: NIST Level 3 (~128-bit post-quantum security)
//! - Public key: 1952 bytes
//! - Signature: 3309 bytes
//! - Secret key: 4032 bytes

use pqcrypto_dilithium::dilithium3::{
    detached_sign, keypair, verify_detached_signature, DetachedSignature, PublicKey, SecretKey,
};
use pqcrypto_traits::sign::{DetachedSignature as _, PublicKey as _, SecretKey as _};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{CryptoError, Result};

/// ML-DSA public key (Dilithium3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlDsaPublicKey {
    bytes: Vec<u8>,
}

impl MlDsaPublicKey {
    /// Public key size in bytes
    pub const SIZE: usize = 1952;

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != Self::SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "ML-DSA public key must be {} bytes, got {}",
                Self::SIZE,
                bytes.len()
            )));
        }
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Convert to pqcrypto PublicKey
    fn to_pqcrypto(&self) -> Result<PublicKey> {
        PublicKey::from_bytes(&self.bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid ML-DSA public key: {:?}", e)))
    }
}

/// ML-DSA signature (Dilithium3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlDsaSignature {
    bytes: Vec<u8>,
}

impl MlDsaSignature {
    /// Maximum signature size in bytes
    pub const SIZE: usize = 3309;

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != Self::SIZE {
            return Err(CryptoError::InvalidSignature(format!(
                "ML-DSA signature must be {} bytes, got {}",
                Self::SIZE,
                bytes.len()
            )));
        }
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Convert to pqcrypto DetachedSignature
    fn to_pqcrypto(&self) -> Result<DetachedSignature> {
        DetachedSignature::from_bytes(&self.bytes).map_err(|e| {
            CryptoError::InvalidSignature(format!("Invalid ML-DSA signature: {:?}", e))
        })
    }
}

/// ML-DSA keypair (Dilithium3)
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MlDsaKeypair {
    #[zeroize(skip)]
    public_key: MlDsaPublicKey,
    secret_key_bytes: Vec<u8>,
}

impl MlDsaKeypair {
    /// Secret key size in bytes
    pub const SECRET_KEY_SIZE: usize = 4032;

    /// Generate a new keypair
    pub fn generate() -> Result<Self> {
        let (pk, sk) = keypair();

        Ok(Self {
            public_key: MlDsaPublicKey {
                bytes: pk.as_bytes().to_vec(),
            },
            secret_key_bytes: sk.as_bytes().to_vec(),
        })
    }

    /// Create from existing key bytes
    pub fn from_bytes(secret_key: &[u8], public_key: &[u8]) -> Result<Self> {
        if secret_key.len() != Self::SECRET_KEY_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "ML-DSA secret key must be {} bytes, got {}",
                Self::SECRET_KEY_SIZE,
                secret_key.len()
            )));
        }

        Ok(Self {
            public_key: MlDsaPublicKey::from_bytes(public_key)?,
            secret_key_bytes: secret_key.to_vec(),
        })
    }

    /// Get public key
    pub fn public_key(&self) -> &MlDsaPublicKey {
        &self.public_key
    }

    /// Get secret key bytes (use with caution)
    pub fn secret_key_bytes(&self) -> &[u8] {
        &self.secret_key_bytes
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> Result<MlDsaSignature> {
        let sk = SecretKey::from_bytes(&self.secret_key_bytes)
            .map_err(|e| CryptoError::Signing(format!("Invalid secret key: {:?}", e)))?;

        let sig = detached_sign(message, &sk);

        Ok(MlDsaSignature {
            bytes: sig.as_bytes().to_vec(),
        })
    }

    /// Verify a signature
    pub fn verify(public_key: &MlDsaPublicKey, message: &[u8], signature: &MlDsaSignature) -> bool {
        let pk = match public_key.to_pqcrypto() {
            Ok(pk) => pk,
            Err(_) => return false,
        };

        let sig = match signature.to_pqcrypto() {
            Ok(sig) => sig,
            Err(_) => return false,
        };

        verify_detached_signature(&sig, message, &pk).is_ok()
    }
}

impl Clone for MlDsaKeypair {
    fn clone(&self) -> Self {
        Self {
            public_key: self.public_key.clone(),
            secret_key_bytes: self.secret_key_bytes.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ml_dsa_keygen() {
        let keypair = MlDsaKeypair::generate().unwrap();
        assert_eq!(keypair.public_key().as_bytes().len(), MlDsaPublicKey::SIZE);
        assert_eq!(
            keypair.secret_key_bytes().len(),
            MlDsaKeypair::SECRET_KEY_SIZE
        );
    }

    #[test]
    fn test_ml_dsa_sign_verify() {
        let keypair = MlDsaKeypair::generate().unwrap();
        let message = b"test message for ML-DSA signing";

        let signature = keypair.sign(message).unwrap();
        assert_eq!(signature.as_bytes().len(), MlDsaSignature::SIZE);

        assert!(MlDsaKeypair::verify(
            keypair.public_key(),
            message,
            &signature
        ));
    }

    #[test]
    fn test_ml_dsa_tampered_message() {
        let keypair = MlDsaKeypair::generate().unwrap();
        let message = b"original message";

        let signature = keypair.sign(message).unwrap();

        assert!(!MlDsaKeypair::verify(
            keypair.public_key(),
            b"tampered message",
            &signature
        ));
    }

    #[test]
    fn test_ml_dsa_wrong_key() {
        let keypair1 = MlDsaKeypair::generate().unwrap();
        let keypair2 = MlDsaKeypair::generate().unwrap();
        let message = b"test message";

        let signature = keypair1.sign(message).unwrap();

        assert!(!MlDsaKeypair::verify(
            keypair2.public_key(),
            message,
            &signature
        ));
    }

    #[test]
    fn test_ml_dsa_serialization() {
        let keypair = MlDsaKeypair::generate().unwrap();
        let message = b"test message";
        let signature = keypair.sign(message).unwrap();

        // Serialize and deserialize public key
        let pk_json = serde_json::to_string(keypair.public_key()).unwrap();
        let pk_restored: MlDsaPublicKey = serde_json::from_str(&pk_json).unwrap();

        // Serialize and deserialize signature
        let sig_json = serde_json::to_string(&signature).unwrap();
        let sig_restored: MlDsaSignature = serde_json::from_str(&sig_json).unwrap();

        // Should still verify
        assert!(MlDsaKeypair::verify(&pk_restored, message, &sig_restored));
    }
}
