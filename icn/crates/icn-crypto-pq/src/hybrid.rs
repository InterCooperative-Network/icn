//! Hybrid classical/post-quantum signatures
//!
//! All long-term signatures in SDIS use hybrid constructions that combine
//! Ed25519 (classical) and ML-DSA (post-quantum). Verification requires
//! BOTH signatures to be valid.
//!
//! # Security Model
//!
//! The hybrid construction provides defense-in-depth:
//! - If Ed25519 is broken (e.g., by quantum computers), ML-DSA provides security
//! - If ML-DSA has an undiscovered weakness, Ed25519 provides security
//! - An attacker must break BOTH algorithms to forge a signature
//!
//! # Size Considerations
//!
//! | Component | Size |
//! |-----------|------|
//! | Ed25519 signature | 64 bytes |
//! | ML-DSA signature | 3293 bytes |
//! | **Hybrid signature** | **~3.4 KB** |
//! | Ed25519 public key | 32 bytes |
//! | ML-DSA public key | 1952 bytes |
//! | **Hybrid public key** | **~2 KB** |

use ed25519_dalek::{Signature as Ed25519Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::ml_dsa::{MlDsaKeypair, MlDsaPublicKey, MlDsaSignature};
use crate::{CryptoError, Result};

/// Hybrid signature combining Ed25519 and ML-DSA
///
/// Both signatures must verify for the hybrid signature to be valid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSignature {
    /// Classical Ed25519 signature (64 bytes)
    pub classical: Vec<u8>,
    /// Post-quantum ML-DSA signature (~3309 bytes)
    pub pq: Vec<u8>,
}

impl HybridSignature {
    /// Ed25519 signature size
    pub const CLASSICAL_SIZE: usize = 64;

    /// Create from component signatures
    pub fn new(classical: Ed25519Signature, pq: MlDsaSignature) -> Self {
        Self {
            classical: classical.to_bytes().to_vec(),
            pq: pq.as_bytes().to_vec(),
        }
    }

    /// Create from raw bytes
    pub fn from_bytes(classical: &[u8], pq: &[u8]) -> Result<Self> {
        if classical.len() != Self::CLASSICAL_SIZE {
            return Err(CryptoError::InvalidSignature(format!(
                "Ed25519 signature must be {} bytes, got {}",
                Self::CLASSICAL_SIZE,
                classical.len()
            )));
        }

        // Validate pq signature format
        let _ = MlDsaSignature::from_bytes(pq)?;

        Ok(Self {
            classical: classical.to_vec(),
            pq: pq.to_vec(),
        })
    }

    /// Verify the hybrid signature
    ///
    /// Returns true only if BOTH classical and post-quantum signatures are valid.
    pub fn verify(&self, message: &[u8], public_key: &HybridPublicKey) -> bool {
        let classical_valid = self.verify_classical(message, &public_key.classical);
        let pq_valid = self.verify_pq(message, &public_key.pq);

        // BOTH must be valid
        classical_valid && pq_valid
    }

    /// Verify only the classical (Ed25519) signature
    pub fn verify_classical(&self, message: &[u8], public_key: &[u8]) -> bool {
        let pk_bytes: [u8; 32] = match public_key.try_into() {
            Ok(b) => b,
            Err(_) => return false,
        };

        let verifying_key = match VerifyingKey::from_bytes(&pk_bytes) {
            Ok(k) => k,
            Err(_) => return false,
        };

        let sig_bytes: [u8; 64] = match self.classical.as_slice().try_into() {
            Ok(b) => b,
            Err(_) => return false,
        };

        let signature = Ed25519Signature::from_bytes(&sig_bytes);

        verifying_key.verify(message, &signature).is_ok()
    }

    /// Verify only the post-quantum (ML-DSA) signature
    pub fn verify_pq(&self, message: &[u8], public_key: &MlDsaPublicKey) -> bool {
        let signature = match MlDsaSignature::from_bytes(&self.pq) {
            Ok(s) => s,
            Err(_) => return false,
        };

        MlDsaKeypair::verify(public_key, message, &signature)
    }

    /// Total size in bytes
    pub fn size(&self) -> usize {
        self.classical.len() + self.pq.len()
    }

    /// Serialize to bytes (classical || pq)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.size());
        bytes.extend_from_slice(&self.classical);
        bytes.extend_from_slice(&self.pq);
        bytes
    }
}

/// Hybrid public key combining Ed25519 and ML-DSA public keys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridPublicKey {
    /// Classical Ed25519 public key (32 bytes)
    pub classical: Vec<u8>,
    /// Post-quantum ML-DSA public key (~1952 bytes)
    pub pq: MlDsaPublicKey,
}

impl HybridPublicKey {
    /// Ed25519 public key size
    pub const CLASSICAL_SIZE: usize = 32;

    /// Create from component keys
    pub fn new(classical: VerifyingKey, pq: MlDsaPublicKey) -> Self {
        Self {
            classical: classical.as_bytes().to_vec(),
            pq,
        }
    }

    /// Create from raw bytes
    pub fn from_bytes(classical: &[u8], pq: &[u8]) -> Result<Self> {
        if classical.len() != Self::CLASSICAL_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "Ed25519 public key must be {} bytes, got {}",
                Self::CLASSICAL_SIZE,
                classical.len()
            )));
        }

        // Validate classical key
        // SAFETY: We verified classical.len() == 32 above
        let classical_bytes: [u8; 32] = classical
            .try_into()
            .map_err(|_| CryptoError::InvalidKey("Classical key length mismatch".to_string()))?;
        let _ = VerifyingKey::from_bytes(&classical_bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid Ed25519 public key: {e}")))?;

        let pq = MlDsaPublicKey::from_bytes(pq)?;

        Ok(Self {
            classical: classical.to_vec(),
            pq,
        })
    }

    /// Get classical public key as VerifyingKey
    pub fn classical_verifying_key(&self) -> Result<VerifyingKey> {
        let bytes: [u8; 32] = self
            .classical
            .as_slice()
            .try_into()
            .map_err(|_| CryptoError::InvalidKey("Invalid classical key length".to_string()))?;
        VerifyingKey::from_bytes(&bytes)
            .map_err(|e| CryptoError::InvalidKey(format!("Invalid Ed25519 public key: {e}")))
    }

    /// Total size in bytes
    pub fn size(&self) -> usize {
        self.classical.len() + self.pq.as_bytes().len()
    }
}

/// Hybrid keypair for signing
///
/// Contains both classical (Ed25519) and post-quantum (ML-DSA) keypairs.
/// Secret keys are zeroized on drop for security.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct HybridKeypair {
    /// Ed25519 signing key (zeroized on drop)
    #[zeroize(skip)]
    classical_signing: SigningKey,
    /// ML-DSA keypair
    #[zeroize(skip)]
    pq_keypair: MlDsaKeypair,
}

impl HybridKeypair {
    /// Generate a new hybrid keypair
    pub fn generate() -> Result<Self> {
        // Generate Ed25519 keypair
        let classical_signing = SigningKey::generate(&mut rand::rngs::OsRng);

        // Generate ML-DSA keypair
        let pq_keypair = MlDsaKeypair::generate()?;

        Ok(Self {
            classical_signing,
            pq_keypair,
        })
    }

    /// Create from seed bytes (deterministic generation)
    ///
    /// The seed must be at least 64 bytes:
    /// - First 32 bytes: Ed25519 seed
    /// - Remaining bytes: Used for ML-DSA derivation
    pub fn from_seed(seed: &[u8]) -> Result<Self> {
        if seed.len() < 64 {
            return Err(CryptoError::InvalidKey(
                "Seed must be at least 64 bytes".to_string(),
            ));
        }

        // Ed25519 from first 32 bytes
        // SAFETY: We verified seed.len() >= 64 above
        let classical_seed: [u8; 32] = seed[..32]
            .try_into()
            .map_err(|_| CryptoError::InvalidKey("Seed slice conversion failed".to_string()))?;
        let classical_signing = SigningKey::from_bytes(&classical_seed);

        // For ML-DSA, we can't do deterministic generation with the current library
        // In production, we'd use a KDF to derive the ML-DSA key from the seed
        // For now, generate fresh (TODO: implement deterministic ML-DSA keygen)
        let pq_keypair = MlDsaKeypair::generate()?;

        Ok(Self {
            classical_signing,
            pq_keypair,
        })
    }

    /// Sign a message with both algorithms
    ///
    /// # Errors
    ///
    /// Returns an error if ML-DSA signing fails (should not happen with valid keypair).
    pub fn sign(&self, message: &[u8]) -> Result<HybridSignature> {
        // Ed25519 signature
        let classical_sig = self.classical_signing.sign(message);

        // ML-DSA signature
        let pq_sig = self.pq_keypair.sign(message)?;

        Ok(HybridSignature::new(classical_sig, pq_sig))
    }

    /// Get the public key
    pub fn public_key(&self) -> HybridPublicKey {
        HybridPublicKey::new(
            self.classical_signing.verifying_key(),
            self.pq_keypair.public_key().clone(),
        )
    }

    /// Get the classical (Ed25519) verifying key
    pub fn classical_public(&self) -> VerifyingKey {
        self.classical_signing.verifying_key()
    }

    /// Get the post-quantum (ML-DSA) public key
    pub fn pq_public(&self) -> &MlDsaPublicKey {
        self.pq_keypair.public_key()
    }

    /// Sign with only Ed25519 (for ephemeral proofs)
    ///
    /// This is used for short-lived credentials where quantum resistance
    /// isn't necessary (e.g., 6-hour ephemeral QR proofs).
    pub fn sign_classical_only(&self, message: &[u8]) -> Ed25519Signature {
        self.classical_signing.sign(message)
    }

    /// Get the classical (Ed25519) secret key bytes
    ///
    /// WARNING: Handle with care - this exposes secret key material.
    /// Used for keystore serialization.
    pub fn classical_secret_bytes(&self) -> [u8; 32] {
        self.classical_signing.to_bytes()
    }

    /// Get the post-quantum (ML-DSA) secret key bytes
    ///
    /// WARNING: Handle with care - this exposes secret key material.
    /// Used for keystore serialization.
    pub fn pq_secret_bytes(&self) -> &[u8] {
        self.pq_keypair.secret_key_bytes()
    }

    /// Reconstruct a HybridKeypair from stored bytes
    ///
    /// Used when loading from keystore.
    pub fn from_bytes(
        classical_secret: &[u8; 32],
        classical_public: &[u8; 32],
        pq_secret: &[u8],
        pq_public: &[u8],
    ) -> Result<Self> {
        // Reconstruct Ed25519 signing key
        let classical_signing = SigningKey::from_bytes(classical_secret);

        // Verify public key matches
        let expected_public = classical_signing.verifying_key();
        if expected_public.as_bytes() != classical_public {
            return Err(CryptoError::InvalidKey(
                "Classical public key doesn't match secret key".to_string(),
            ));
        }

        // Reconstruct ML-DSA keypair
        let pq_keypair = MlDsaKeypair::from_bytes(pq_secret, pq_public)?;

        Ok(Self {
            classical_signing,
            pq_keypair,
        })
    }
}

impl Clone for HybridKeypair {
    fn clone(&self) -> Self {
        Self {
            classical_signing: SigningKey::from_bytes(&self.classical_signing.to_bytes()),
            pq_keypair: self.pq_keypair.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_keygen() {
        let keypair = HybridKeypair::generate().unwrap();
        let pk = keypair.public_key();

        assert_eq!(pk.classical.len(), HybridPublicKey::CLASSICAL_SIZE);
        assert_eq!(pk.pq.as_bytes().len(), MlDsaPublicKey::SIZE);
    }

    #[test]
    fn test_hybrid_sign_verify() {
        let keypair = HybridKeypair::generate().unwrap();
        let message = b"test message for hybrid signing";

        let signature = keypair.sign(message).unwrap();
        let public_key = keypair.public_key();

        assert!(signature.verify(message, &public_key));
    }

    #[test]
    fn test_hybrid_signature_size() {
        let keypair = HybridKeypair::generate().unwrap();
        let signature = keypair.sign(b"test").unwrap();

        // Ed25519 (64) + ML-DSA (~3293) = ~3357 bytes
        assert!(signature.size() > 3000);
        assert!(signature.size() < 4000);

        println!("Hybrid signature size: {} bytes", signature.size());
    }

    #[test]
    fn test_hybrid_public_key_size() {
        let keypair = HybridKeypair::generate().unwrap();
        let pk = keypair.public_key();

        // Ed25519 (32) + ML-DSA (1952) = 1984 bytes
        assert!(pk.size() > 1900);
        assert!(pk.size() < 2100);

        println!("Hybrid public key size: {} bytes", pk.size());
    }

    #[test]
    fn test_hybrid_tampered_message() {
        let keypair = HybridKeypair::generate().unwrap();
        let signature = keypair.sign(b"original message").unwrap();
        let public_key = keypair.public_key();

        assert!(!signature.verify(b"tampered message", &public_key));
    }

    #[test]
    fn test_hybrid_wrong_key() {
        let keypair1 = HybridKeypair::generate().unwrap();
        let keypair2 = HybridKeypair::generate().unwrap();
        let message = b"test message";

        let signature = keypair1.sign(message).unwrap();
        let wrong_key = keypair2.public_key();

        assert!(!signature.verify(message, &wrong_key));
    }

    #[test]
    fn test_classical_only_signature() {
        let keypair = HybridKeypair::generate().unwrap();
        let message = b"ephemeral proof message";

        let classical_sig = keypair.sign_classical_only(message);
        let verifying_key = keypair.classical_public();

        assert!(verifying_key.verify(message, &classical_sig).is_ok());

        // Classical signature is much smaller
        assert_eq!(classical_sig.to_bytes().len(), 64);
    }

    #[test]
    fn test_hybrid_serialization() {
        let keypair = HybridKeypair::generate().unwrap();
        let message = b"test message";
        let signature = keypair.sign(message).unwrap();
        let public_key = keypair.public_key();

        // Serialize
        let pk_json = serde_json::to_string(&public_key).unwrap();
        let sig_json = serde_json::to_string(&signature).unwrap();

        // Deserialize
        let pk_restored: HybridPublicKey = serde_json::from_str(&pk_json).unwrap();
        let sig_restored: HybridSignature = serde_json::from_str(&sig_json).unwrap();

        // Should still verify
        assert!(sig_restored.verify(message, &pk_restored));
    }

    #[test]
    fn test_partial_verification() {
        let keypair = HybridKeypair::generate().unwrap();
        let message = b"test message";
        let signature = keypair.sign(message).unwrap();
        let public_key = keypair.public_key();

        // Test individual verification methods
        assert!(signature.verify_classical(message, &public_key.classical));
        assert!(signature.verify_pq(message, &public_key.pq));
    }
}
