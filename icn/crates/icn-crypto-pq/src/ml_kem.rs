//! ML-KEM (Module-Lattice Key Encapsulation Mechanism) wrapper
//!
//! This module wraps the pqcrypto-mlkem crate to provide a clean API
//! for ML-KEM key encapsulation (NIST FIPS 203).
//!
//! We use ML-KEM-768 which provides:
//! - Security level: NIST Level 3 (~128-bit post-quantum security)
//! - Public key: 1184 bytes
//! - Secret key: 2400 bytes
//! - Ciphertext: 1088 bytes
//! - Shared secret: 32 bytes

use pqcrypto_mlkem::mlkem768::{
    decapsulate, encapsulate, keypair, Ciphertext, PublicKey, SecretKey,
};
use pqcrypto_traits::kem::{Ciphertext as _, PublicKey as _, SecretKey as _, SharedSecret as _};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{CryptoError, Result};

/// ML-KEM public key (ML-KEM-768)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlKemPublicKey {
    bytes: Vec<u8>,
}

impl MlKemPublicKey {
    /// Public key size in bytes
    pub const SIZE: usize = 1184;

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != Self::SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "ML-KEM public key must be {} bytes, got {}",
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
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid ML-KEM public key: {e:?}")))
    }
}

/// ML-KEM ciphertext (encapsulated key)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlKemCiphertext {
    bytes: Vec<u8>,
}

impl MlKemCiphertext {
    /// Ciphertext size in bytes
    pub const SIZE: usize = 1088;

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != Self::SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "ML-KEM ciphertext must be {} bytes, got {}",
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

    /// Convert to pqcrypto Ciphertext
    fn to_pqcrypto(&self) -> Result<Ciphertext> {
        Ciphertext::from_bytes(&self.bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid ML-KEM ciphertext: {e:?}")))
    }
}

/// ML-KEM keypair (ML-KEM-768)
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MlKemKeypair {
    #[zeroize(skip)]
    public_key: MlKemPublicKey,
    secret_key_bytes: Vec<u8>,
}

impl MlKemKeypair {
    /// Secret key size in bytes
    pub const SECRET_KEY_SIZE: usize = 2400;

    /// Shared secret size in bytes
    pub const SHARED_SECRET_SIZE: usize = 32;

    /// Generate a new keypair
    pub fn generate() -> Result<Self> {
        let (pk, sk) = keypair();

        Ok(Self {
            public_key: MlKemPublicKey {
                bytes: pk.as_bytes().to_vec(),
            },
            secret_key_bytes: sk.as_bytes().to_vec(),
        })
    }

    /// Create from existing key bytes
    pub fn from_bytes(secret_key: &[u8], public_key: &[u8]) -> Result<Self> {
        if secret_key.len() != Self::SECRET_KEY_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "ML-KEM secret key must be {} bytes, got {}",
                Self::SECRET_KEY_SIZE,
                secret_key.len()
            )));
        }

        Ok(Self {
            public_key: MlKemPublicKey::from_bytes(public_key)?,
            secret_key_bytes: secret_key.to_vec(),
        })
    }

    /// Get public key
    pub fn public_key(&self) -> &MlKemPublicKey {
        &self.public_key
    }

    /// Get secret key bytes (use with caution)
    pub fn secret_key_bytes(&self) -> &[u8] {
        &self.secret_key_bytes
    }

    /// Encapsulate: generate a shared secret and ciphertext for a recipient's public key
    ///
    /// Returns (shared_secret, ciphertext) where:
    /// - shared_secret is the 32-byte key material
    /// - ciphertext should be sent to the recipient
    pub fn encapsulate(recipient_pk: &MlKemPublicKey) -> Result<([u8; 32], MlKemCiphertext)> {
        let pk = recipient_pk.to_pqcrypto()?;
        let (shared_secret, ciphertext) = encapsulate(&pk);

        let mut ss = [0u8; 32];
        ss.copy_from_slice(shared_secret.as_bytes());

        Ok((
            ss,
            MlKemCiphertext {
                bytes: ciphertext.as_bytes().to_vec(),
            },
        ))
    }

    /// Decapsulate: recover the shared secret from a ciphertext
    pub fn decapsulate(&self, ciphertext: &MlKemCiphertext) -> Result<[u8; 32]> {
        let sk = SecretKey::from_bytes(&self.secret_key_bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid secret key: {e:?}")))?;

        let ct = ciphertext.to_pqcrypto()?;
        let shared_secret = decapsulate(&ct, &sk);

        let mut ss = [0u8; 32];
        ss.copy_from_slice(shared_secret.as_bytes());

        Ok(ss)
    }
}

impl Clone for MlKemKeypair {
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
    fn test_ml_kem_keygen() {
        let keypair = MlKemKeypair::generate().unwrap();
        assert_eq!(keypair.public_key().as_bytes().len(), MlKemPublicKey::SIZE);
        assert_eq!(
            keypair.secret_key_bytes().len(),
            MlKemKeypair::SECRET_KEY_SIZE
        );
    }

    #[test]
    fn test_ml_kem_encapsulate_decapsulate() {
        let keypair = MlKemKeypair::generate().unwrap();

        // Encapsulate to the public key
        let (sender_shared, ciphertext) = MlKemKeypair::encapsulate(keypair.public_key()).unwrap();
        assert_eq!(ciphertext.as_bytes().len(), MlKemCiphertext::SIZE);

        // Decapsulate with the secret key
        let recipient_shared = keypair.decapsulate(&ciphertext).unwrap();

        // Both parties should derive the same shared secret
        assert_eq!(sender_shared, recipient_shared);
    }

    #[test]
    fn test_ml_kem_different_keys() {
        let keypair1 = MlKemKeypair::generate().unwrap();
        let keypair2 = MlKemKeypair::generate().unwrap();

        // Encapsulate to keypair1's public key
        let (sender_shared, ciphertext) = MlKemKeypair::encapsulate(keypair1.public_key()).unwrap();

        // Try to decapsulate with keypair2's secret key (wrong key)
        let wrong_shared = keypair2.decapsulate(&ciphertext).unwrap();

        // Shared secrets should NOT match (implicit rejection)
        assert_ne!(sender_shared, wrong_shared);
    }

    #[test]
    fn test_ml_kem_serialization() {
        let keypair = MlKemKeypair::generate().unwrap();

        // Encapsulate
        let (original_shared, ciphertext) =
            MlKemKeypair::encapsulate(keypair.public_key()).unwrap();

        // Serialize and deserialize public key
        let pk_json = serde_json::to_string(keypair.public_key()).unwrap();
        let pk_restored: MlKemPublicKey = serde_json::from_str(&pk_json).unwrap();

        // Serialize and deserialize ciphertext
        let ct_json = serde_json::to_string(&ciphertext).unwrap();
        let ct_restored: MlKemCiphertext = serde_json::from_str(&ct_json).unwrap();

        // Should still work
        let restored_shared = keypair.decapsulate(&ct_restored).unwrap();
        assert_eq!(original_shared, restored_shared);

        // Encapsulating to restored public key should also work
        let (new_shared, new_ct) = MlKemKeypair::encapsulate(&pk_restored).unwrap();
        let decap_shared = keypair.decapsulate(&new_ct).unwrap();
        assert_eq!(new_shared, decap_shared);
    }

    #[test]
    fn test_ml_kem_key_sizes() {
        let keypair = MlKemKeypair::generate().unwrap();
        let (_, ciphertext) = MlKemKeypair::encapsulate(keypair.public_key()).unwrap();

        println!(
            "ML-KEM public key size: {} bytes",
            keypair.public_key().as_bytes().len()
        );
        println!(
            "ML-KEM secret key size: {} bytes",
            keypair.secret_key_bytes().len()
        );
        println!(
            "ML-KEM ciphertext size: {} bytes",
            ciphertext.as_bytes().len()
        );

        // Verify documented sizes
        assert_eq!(keypair.public_key().as_bytes().len(), 1184);
        assert_eq!(keypair.secret_key_bytes().len(), 2400);
        assert_eq!(ciphertext.as_bytes().len(), 1088);
    }
}
