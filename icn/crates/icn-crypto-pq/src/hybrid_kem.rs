//! Hybrid classical/post-quantum key encapsulation
//!
//! Combines X25519 (classical ECDH) with ML-KEM (post-quantum KEM) to provide
//! quantum-resistant key exchange with classical security as a fallback.
//!
//! # Security Model
//!
//! The hybrid construction provides defense-in-depth:
//! - If X25519 is broken (e.g., by quantum computers), ML-KEM provides security
//! - If ML-KEM has an undiscovered weakness, X25519 provides security
//! - Final shared secret is derived via HKDF from BOTH shared secrets
//!
//! # Size Considerations
//!
//! | Component | Size |
//! |-----------|------|
//! | X25519 public key | 32 bytes |
//! | ML-KEM public key | 1184 bytes |
//! | **Hybrid public key** | **~1.2 KB** |
//! | X25519 ephemeral | 32 bytes |
//! | ML-KEM ciphertext | 1088 bytes |
//! | **Hybrid ciphertext** | **~1.1 KB** |

use serde::{Deserialize, Serialize};
use x25519_dalek::{EphemeralSecret, PublicKey as X25519PublicKey, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::kdf::derive_hybrid_key;
use crate::ml_kem::{MlKemCiphertext, MlKemKeypair, MlKemPublicKey};
use crate::{CryptoError, Result};

/// Hybrid public key combining X25519 and ML-KEM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridKemPublicKey {
    /// Classical X25519 public key (32 bytes)
    pub classical: [u8; 32],
    /// Post-quantum ML-KEM public key (1184 bytes)
    pub pq: MlKemPublicKey,
}

impl HybridKemPublicKey {
    /// X25519 public key size
    pub const CLASSICAL_SIZE: usize = 32;

    /// Create from component keys
    pub fn new(classical: X25519PublicKey, pq: MlKemPublicKey) -> Self {
        Self {
            classical: classical.to_bytes(),
            pq,
        }
    }

    /// Create from raw bytes
    pub fn from_bytes(classical: &[u8], pq: &[u8]) -> Result<Self> {
        if classical.len() != Self::CLASSICAL_SIZE {
            return Err(CryptoError::InvalidKey(format!(
                "X25519 public key must be {} bytes, got {}",
                Self::CLASSICAL_SIZE,
                classical.len()
            )));
        }

        let mut classical_bytes = [0u8; 32];
        classical_bytes.copy_from_slice(classical);

        Ok(Self {
            classical: classical_bytes,
            pq: MlKemPublicKey::from_bytes(pq)?,
        })
    }

    /// Get the X25519 public key
    pub fn classical_key(&self) -> X25519PublicKey {
        X25519PublicKey::from(self.classical)
    }

    /// Total size in bytes
    pub fn size(&self) -> usize {
        Self::CLASSICAL_SIZE + self.pq.as_bytes().len()
    }
}

/// Hybrid ciphertext combining X25519 ephemeral and ML-KEM ciphertext
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridKemCiphertext {
    /// Ephemeral X25519 public key (32 bytes)
    pub classical_ephemeral: [u8; 32],
    /// ML-KEM ciphertext (1088 bytes)
    pub pq_ciphertext: MlKemCiphertext,
}

impl HybridKemCiphertext {
    /// Total size in bytes
    pub fn size(&self) -> usize {
        32 + self.pq_ciphertext.as_bytes().len()
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.size());
        bytes.extend_from_slice(&self.classical_ephemeral);
        bytes.extend_from_slice(self.pq_ciphertext.as_bytes());
        bytes
    }
}

/// Hybrid KEM keypair for key encapsulation
///
/// Contains both classical (X25519) and post-quantum (ML-KEM) keypairs.
/// Secret keys are zeroized on drop for security.
#[derive(Zeroize, ZeroizeOnDrop)]
#[allow(unused_assignments)] // Zeroize derive generates code that triggers this warning
pub struct HybridKemKeypair {
    /// X25519 secret key (zeroized on drop)
    classical_secret: [u8; 32],
    /// ML-KEM keypair
    #[zeroize(skip)]
    pq_keypair: MlKemKeypair,
    /// Cached public key
    #[zeroize(skip)]
    public_key: HybridKemPublicKey,
}

impl HybridKemKeypair {
    /// Generate a new hybrid KEM keypair
    pub fn generate() -> Result<Self> {
        // Generate X25519 keypair
        let classical_secret = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let classical_public = X25519PublicKey::from(&classical_secret);

        // Generate ML-KEM keypair
        let pq_keypair = MlKemKeypair::generate()?;

        let public_key = HybridKemPublicKey::new(classical_public, pq_keypair.public_key().clone());

        Ok(Self {
            classical_secret: classical_secret.to_bytes(),
            pq_keypair,
            public_key,
        })
    }

    /// Get the public key
    pub fn public_key(&self) -> &HybridKemPublicKey {
        &self.public_key
    }

    /// Encapsulate: generate a shared secret for a recipient
    ///
    /// This performs ECIES-style encapsulation:
    /// 1. Generate ephemeral X25519 keypair, compute ECDH
    /// 2. Encapsulate to ML-KEM public key
    /// 3. Combine both shared secrets via HKDF
    ///
    /// Returns (shared_secret, ciphertext) where:
    /// - shared_secret is the 32-byte derived key
    /// - ciphertext should be sent to the recipient
    pub fn encapsulate(
        recipient_pk: &HybridKemPublicKey,
        context: &[u8],
    ) -> Result<([u8; 32], HybridKemCiphertext)> {
        // X25519: ephemeral ECDH
        let ephemeral_secret = EphemeralSecret::random_from_rng(rand::rngs::OsRng);
        let ephemeral_public = X25519PublicKey::from(&ephemeral_secret);
        let classical_shared = ephemeral_secret.diffie_hellman(&recipient_pk.classical_key());

        // ML-KEM: encapsulate
        let (pq_shared, pq_ciphertext) = MlKemKeypair::encapsulate(&recipient_pk.pq)?;

        // Combine via HKDF
        let shared_secret = derive_hybrid_key(classical_shared.as_bytes(), &pq_shared, context, 32);

        let mut result = [0u8; 32];
        result.copy_from_slice(&shared_secret);

        Ok((
            result,
            HybridKemCiphertext {
                classical_ephemeral: ephemeral_public.to_bytes(),
                pq_ciphertext,
            },
        ))
    }

    /// Decapsulate: recover the shared secret from a ciphertext
    ///
    /// This reverses the encapsulation:
    /// 1. Compute ECDH with the ephemeral X25519 key
    /// 2. Decapsulate the ML-KEM ciphertext
    /// 3. Combine both shared secrets via HKDF
    pub fn decapsulate(
        &self,
        ciphertext: &HybridKemCiphertext,
        context: &[u8],
    ) -> Result<[u8; 32]> {
        // X25519: ECDH with ephemeral
        let ephemeral_public = X25519PublicKey::from(ciphertext.classical_ephemeral);
        let classical_secret = StaticSecret::from(self.classical_secret);
        let classical_shared = classical_secret.diffie_hellman(&ephemeral_public);

        // ML-KEM: decapsulate
        let pq_shared = self.pq_keypair.decapsulate(&ciphertext.pq_ciphertext)?;

        // Combine via HKDF
        let shared_secret = derive_hybrid_key(classical_shared.as_bytes(), &pq_shared, context, 32);

        let mut result = [0u8; 32];
        result.copy_from_slice(&shared_secret);

        Ok(result)
    }

    /// Get the X25519 public key only (for protocols that need it separately)
    pub fn classical_public(&self) -> X25519PublicKey {
        self.public_key.classical_key()
    }

    /// Get the ML-KEM public key only
    pub fn pq_public(&self) -> &MlKemPublicKey {
        &self.public_key.pq
    }
}

impl Clone for HybridKemKeypair {
    fn clone(&self) -> Self {
        Self {
            classical_secret: self.classical_secret,
            pq_keypair: self.pq_keypair.clone(),
            public_key: self.public_key.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_kem_keygen() {
        let keypair = HybridKemKeypair::generate().unwrap();
        let pk = keypair.public_key();

        assert_eq!(pk.classical.len(), HybridKemPublicKey::CLASSICAL_SIZE);
        assert_eq!(pk.pq.as_bytes().len(), MlKemPublicKey::SIZE);
    }

    #[test]
    fn test_hybrid_kem_encapsulate_decapsulate() {
        let keypair = HybridKemKeypair::generate().unwrap();
        let context = b"test-context";

        // Encapsulate
        let (sender_shared, ciphertext) =
            HybridKemKeypair::encapsulate(keypair.public_key(), context).unwrap();

        // Decapsulate
        let recipient_shared = keypair.decapsulate(&ciphertext, context).unwrap();

        // Both parties should derive the same shared secret
        assert_eq!(sender_shared, recipient_shared);
    }

    #[test]
    fn test_hybrid_kem_different_context() {
        let keypair = HybridKemKeypair::generate().unwrap();

        // Encapsulate with one context
        let (sender_shared, ciphertext) =
            HybridKemKeypair::encapsulate(keypair.public_key(), b"context-1").unwrap();

        // Decapsulate with different context
        let recipient_shared = keypair.decapsulate(&ciphertext, b"context-2").unwrap();

        // Shared secrets should NOT match (context binding)
        assert_ne!(sender_shared, recipient_shared);
    }

    #[test]
    fn test_hybrid_kem_wrong_key() {
        let keypair1 = HybridKemKeypair::generate().unwrap();
        let keypair2 = HybridKemKeypair::generate().unwrap();
        let context = b"test-context";

        // Encapsulate to keypair1's public key
        let (sender_shared, ciphertext) =
            HybridKemKeypair::encapsulate(keypair1.public_key(), context).unwrap();

        // Try to decapsulate with keypair2's secret key
        let wrong_shared = keypair2.decapsulate(&ciphertext, context).unwrap();

        // Shared secrets should NOT match
        assert_ne!(sender_shared, wrong_shared);
    }

    #[test]
    fn test_hybrid_kem_sizes() {
        let keypair = HybridKemKeypair::generate().unwrap();
        let pk = keypair.public_key();
        let (_, ciphertext) = HybridKemKeypair::encapsulate(pk, b"test").unwrap();

        println!("Hybrid KEM public key size: {} bytes", pk.size());
        println!("Hybrid KEM ciphertext size: {} bytes", ciphertext.size());

        // Public key: 32 (X25519) + 1184 (ML-KEM) = 1216
        assert_eq!(pk.size(), 32 + 1184);

        // Ciphertext: 32 (ephemeral) + 1088 (ML-KEM) = 1120
        assert_eq!(ciphertext.size(), 32 + 1088);
    }

    #[test]
    fn test_hybrid_kem_serialization() {
        let keypair = HybridKemKeypair::generate().unwrap();
        let context = b"test-context";

        // Encapsulate
        let (original_shared, ciphertext) =
            HybridKemKeypair::encapsulate(keypair.public_key(), context).unwrap();

        // Serialize and deserialize public key
        let pk_json = serde_json::to_string(keypair.public_key()).unwrap();
        let pk_restored: HybridKemPublicKey = serde_json::from_str(&pk_json).unwrap();

        // Serialize and deserialize ciphertext
        let ct_json = serde_json::to_string(&ciphertext).unwrap();
        let ct_restored: HybridKemCiphertext = serde_json::from_str(&ct_json).unwrap();

        // Decapsulate with restored ciphertext should still work
        let restored_shared = keypair.decapsulate(&ct_restored, context).unwrap();
        assert_eq!(original_shared, restored_shared);

        // Encapsulating to restored public key should also work
        let (new_shared, new_ct) = HybridKemKeypair::encapsulate(&pk_restored, context).unwrap();
        let decap_shared = keypair.decapsulate(&new_ct, context).unwrap();
        assert_eq!(new_shared, decap_shared);
    }

    #[test]
    fn test_hybrid_kem_determinism() {
        let keypair = HybridKemKeypair::generate().unwrap();
        let context = b"test-context";

        // Two encapsulations should produce different shared secrets
        // (due to ephemeral randomness)
        let (shared1, _) = HybridKemKeypair::encapsulate(keypair.public_key(), context).unwrap();
        let (shared2, _) = HybridKemKeypair::encapsulate(keypair.public_key(), context).unwrap();

        assert_ne!(shared1, shared2);
    }
}
