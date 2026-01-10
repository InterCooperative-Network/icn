// Allow unused_assignments from Zeroize derive macro generated code
#![allow(unused_assignments)]
// Allow unwrap in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! ICN Post-Quantum Cryptography
//!
//! This crate provides hybrid classical/post-quantum cryptography for the
//! SDIS (Sovereign Digital Identity System) integration with ICN.
//!
//! # Design Rationale
//!
//! All long-term cryptographic operations use hybrid constructions that combine:
//! - **Classical**: Ed25519 (signatures), X25519 (key exchange)
//! - **Post-Quantum**: ML-DSA/Dilithium (signatures), ML-KEM/Kyber (key encapsulation)
//!
//! This provides defense-in-depth: an attacker must break BOTH algorithms.
//!
//! # Quick Start
//!
//! ```rust
//! use icn_crypto_pq::{HybridKeypair, HybridSignature};
//!
//! // Generate hybrid keypair
//! let keypair = HybridKeypair::generate().unwrap();
//!
//! // Sign a message (creates both Ed25519 and ML-DSA signatures)
//! let message = b"important document";
//! let signature = keypair.sign(message).unwrap();
//!
//! // Verify (both signatures must be valid)
//! let public_key = keypair.public_key();
//! assert!(signature.verify(message, &public_key));
//! ```
//!
//! # Size Budget
//!
//! | Component | Classical | Hybrid |
//! |-----------|-----------|--------|
//! | Signature | 64 bytes | ~2.5 KB |
//! | Public Key | 32 bytes | ~2 KB |
//! | Secret Key | 32 bytes | ~4 KB |

pub mod blind;
pub mod hybrid;
pub mod hybrid_kem;
pub mod kdf;
pub mod ml_dsa;
pub mod ml_kem;
pub mod shamir;
pub mod threshold;

pub use blind::{
    BlindSignature, BlindedMessage, BlindingFactor, EnrollmentToken, UnblindedSignature,
};
pub use hybrid::{HybridKeypair, HybridPublicKey, HybridSignature};
pub use hybrid_kem::{HybridKemCiphertext, HybridKemKeypair, HybridKemPublicKey};
pub use kdf::{derive_hybrid_key, derive_keybundle_keys};
pub use ml_dsa::{MlDsaKeypair, MlDsaPublicKey, MlDsaSignature};
pub use ml_kem::{MlKemCiphertext, MlKemKeypair, MlKemPublicKey};
pub use shamir::{ShamirSecretSharing, ShamirShare, ShamirSplitResult};
pub use threshold::{
    combine_prf_partials, AdditiveSecretSharing, PepperShare, PrfPartial, ShareCommitment,
};

/// Errors from cryptographic operations
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("Key generation failed: {0}")]
    KeyGeneration(String),

    #[error("Key derivation failed: {0}")]
    KeyDerivation(String),

    #[error("Signing failed: {0}")]
    Signing(String),

    #[error("Verification failed: {0}")]
    Verification(String),

    #[error("Invalid key format: {0}")]
    InvalidKey(String),

    #[error("Invalid signature format: {0}")]
    InvalidSignature(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

pub type Result<T> = std::result::Result<T, CryptoError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_roundtrip() {
        let keypair = HybridKeypair::generate().unwrap();
        let message = b"test message for signing";

        let signature = keypair.sign(message).unwrap();
        let public_key = keypair.public_key();

        assert!(signature.verify(message, &public_key));
    }

    #[test]
    fn test_hybrid_tampered_message() {
        let keypair = HybridKeypair::generate().unwrap();
        let message = b"original message";

        let signature = keypair.sign(message).unwrap();
        let public_key = keypair.public_key();

        // Tampered message should fail
        assert!(!signature.verify(b"tampered message", &public_key));
    }

    #[test]
    fn test_hybrid_wrong_key() {
        let keypair1 = HybridKeypair::generate().unwrap();
        let keypair2 = HybridKeypair::generate().unwrap();
        let message = b"test message";

        let signature = keypair1.sign(message).unwrap();
        let wrong_key = keypair2.public_key();

        // Wrong key should fail
        assert!(!signature.verify(message, &wrong_key));
    }
}
