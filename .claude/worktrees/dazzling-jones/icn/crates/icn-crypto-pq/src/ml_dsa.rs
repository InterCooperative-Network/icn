//! ML-DSA (Module-Lattice Digital Signature Algorithm) wrapper
//!
//! This module wraps the pqcrypto-mldsa crate to provide a clean API
//! for ML-DSA signatures (NIST FIPS 204).
//!
//! We use ML-DSA-65 which provides:
//! - Security level: NIST Level 3 (~128-bit post-quantum security)
//! - Public key: 1952 bytes
//! - Signature: 3309 bytes
//! - Secret key: 4032 bytes
//!
//! ## Deterministic Key Generation
//!
//! This module supports deterministic key generation from a seed using HKDF
//! and ML-DSA's `from_seed` construction. This enables key recovery from a master seed.
//!
//! ```rust,ignore
//! use icn_crypto_pq::ml_dsa::MlDsaKeypair;
//!
//! let seed = [0u8; 32];
//! let keypair1 = MlDsaKeypair::from_seed(&seed)?;
//! let keypair2 = MlDsaKeypair::from_seed(&seed)?;
//!
//! // Same seed always produces identical keypairs
//! assert_eq!(keypair1.public_key().as_bytes(), keypair2.public_key().as_bytes());
//! ```

use hkdf::Hkdf;
use ml_dsa::{KeyGen, MlDsa65, Seed};
use pqcrypto_mldsa::mldsa65::{
    detached_sign, keypair, verify_detached_signature, DetachedSignature, PublicKey, SecretKey,
};
use pqcrypto_traits::sign::{DetachedSignature as _, PublicKey as _, SecretKey as _};
use serde::{Deserialize, Serialize};
use sha3::Sha3_256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{CryptoError, Result};

/// Domain separator for ML-DSA key derivation
const ML_DSA_KEY_DERIVATION_DOMAIN: &[u8] = b"icn-ml-dsa-key-v1";

/// ML-DSA public key (ML-DSA-65)
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
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid ML-DSA public key: {e:?}")))
    }
}

/// ML-DSA signature (ML-DSA-65)
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
        DetachedSignature::from_bytes(&self.bytes)
            .map_err(|e| CryptoError::InvalidSignature(format!("Invalid ML-DSA signature: {e:?}")))
    }
}

/// ML-DSA keypair (ML-DSA-65)
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MlDsaKeypair {
    #[zeroize(skip)]
    public_key: MlDsaPublicKey,
    secret_key_bytes: Vec<u8>,
}

impl MlDsaKeypair {
    /// Secret key size in bytes
    pub const SECRET_KEY_SIZE: usize = 4032;

    /// Generate a new keypair with random keys
    pub fn generate() -> Result<Self> {
        let (pk, sk) = keypair();

        Ok(Self {
            public_key: MlDsaPublicKey {
                bytes: pk.as_bytes().to_vec(),
            },
            secret_key_bytes: sk.as_bytes().to_vec(),
        })
    }

    /// Generate a keypair deterministically from a seed
    ///
    /// This uses HKDF-SHA3-256 to derive a 32-byte seed, then uses the
    /// pure-Rust ml-dsa crate for deterministic key generation.
    /// The same seed will always produce the same keypair.
    ///
    /// # Arguments
    ///
    /// * `seed` - At least 32 bytes of entropy (typically from a master seed)
    ///
    /// # Security
    ///
    /// The seed should be cryptographically random and kept secret.
    /// Recovery of the seed allows regeneration of the keypair.
    pub fn from_seed(seed: &[u8]) -> Result<Self> {
        if seed.len() < 32 {
            return Err(CryptoError::InvalidKey(
                "ML-DSA seed must be at least 32 bytes".to_string(),
            ));
        }

        // Use HKDF to derive a 32-byte seed for the RNG
        let hk = Hkdf::<Sha3_256>::new(Some(ML_DSA_KEY_DERIVATION_DOMAIN), seed);
        let mut rng_seed = [0u8; 32];
        hk.expand(b"ml-dsa-65-keygen", &mut rng_seed)
            .map_err(|_| CryptoError::KeyDerivation("HKDF expansion failed".to_string()))?;

        let seed = Seed::from(rng_seed);

        // Generate keypair using the pure-Rust ml-dsa crate
        let kp = MlDsa65::from_seed(&seed);

        // Extract key bytes using encode() methods
        let signing_key = kp.signing_key();
        let verifying_key = kp.verifying_key();

        #[allow(deprecated)]
        let sk_encoded = signing_key.to_expanded();
        let pk_encoded = verifying_key.encode();

        Ok(Self {
            public_key: MlDsaPublicKey {
                bytes: pk_encoded.as_slice().to_vec(),
            },
            secret_key_bytes: sk_encoded.as_slice().to_vec(),
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
            .map_err(|e| CryptoError::Signing(format!("Invalid secret key: {e:?}")))?;

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

    #[test]
    fn test_ml_dsa_from_seed_deterministic() {
        let seed = [42u8; 32];

        // Generate two keypairs from the same seed
        let keypair1 = MlDsaKeypair::from_seed(&seed).unwrap();
        let keypair2 = MlDsaKeypair::from_seed(&seed).unwrap();

        // Public keys must be identical
        assert_eq!(
            keypair1.public_key().as_bytes(),
            keypair2.public_key().as_bytes(),
            "Same seed should produce identical public keys"
        );

        // Secret keys must be identical
        assert_eq!(
            keypair1.secret_key_bytes(),
            keypair2.secret_key_bytes(),
            "Same seed should produce identical secret keys"
        );
    }

    #[test]
    fn test_ml_dsa_from_seed_different_seeds() {
        let seed1 = [1u8; 32];
        let seed2 = [2u8; 32];

        let keypair1 = MlDsaKeypair::from_seed(&seed1).unwrap();
        let keypair2 = MlDsaKeypair::from_seed(&seed2).unwrap();

        // Different seeds should produce different keys
        assert_ne!(
            keypair1.public_key().as_bytes(),
            keypair2.public_key().as_bytes(),
            "Different seeds should produce different keys"
        );
    }

    #[test]
    fn test_ml_dsa_from_seed_key_sizes() {
        let seed = [0u8; 32];
        let keypair = MlDsaKeypair::from_seed(&seed).unwrap();

        // Verify key sizes match expected FIPS 204 ML-DSA-65 sizes
        assert_eq!(
            keypair.public_key().as_bytes().len(),
            MlDsaPublicKey::SIZE,
            "Public key size should match ML-DSA-65 spec"
        );
        assert_eq!(
            keypair.secret_key_bytes().len(),
            MlDsaKeypair::SECRET_KEY_SIZE,
            "Secret key size should match ML-DSA-65 spec"
        );
    }

    #[test]
    fn test_ml_dsa_from_seed_sign_verify() {
        let seed = [99u8; 32];
        let keypair = MlDsaKeypair::from_seed(&seed).unwrap();
        let message = b"test message for seed-derived keypair";

        // Sign with the seed-derived keypair
        let signature = keypair.sign(message).unwrap();

        // Verify the signature
        assert!(
            MlDsaKeypair::verify(keypair.public_key(), message, &signature),
            "Signature from seed-derived keypair should verify"
        );
    }

    #[test]
    fn test_ml_dsa_seed_recovery() {
        // Simulate key recovery from seed
        let seed = [123u8; 32];

        // Original keypair
        let original = MlDsaKeypair::from_seed(&seed).unwrap();
        let message = b"important document";
        let original_signature = original.sign(message).unwrap();

        // Later, recover the keypair from the same seed
        let recovered = MlDsaKeypair::from_seed(&seed).unwrap();

        // The recovered keypair should be able to:
        // 1. Verify signatures made by the original
        assert!(
            MlDsaKeypair::verify(recovered.public_key(), message, &original_signature),
            "Recovered keypair should verify original signatures"
        );

        // 2. Create signatures that verify with the original public key
        let new_signature = recovered.sign(b"new message").unwrap();
        assert!(
            MlDsaKeypair::verify(original.public_key(), b"new message", &new_signature),
            "Original public key should verify recovered keypair signatures"
        );
    }

    #[test]
    fn test_ml_dsa_seed_too_short() {
        let short_seed = [0u8; 16]; // Only 16 bytes, need at least 32
        let result = MlDsaKeypair::from_seed(&short_seed);

        assert!(result.is_err(), "Should fail with seed < 32 bytes");
    }
}
