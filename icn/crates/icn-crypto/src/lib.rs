//! ICN Crypto - Unified cryptography layer
//!
//! This crate provides a unified interface to ICN's cryptography layers.
//! Currently re-exports `icn-crypto-pq` for post-quantum hybrid cryptography.
//!
//! # Post-Quantum Cryptography
//!
//! ICN uses hybrid classical/PQ constructions for long-term security:
//! - Ed25519 + ML-DSA (Dilithium) for signatures
//! - X25519 + ML-KEM (Kyber) for key exchange
//!
//! # Example
//!
//! ```rust,ignore
//! use icn_crypto::HybridKeypair;
//!
//! let keypair = HybridKeypair::generate()?;
//! let signature = keypair.sign(b"message")?;
//! ```

#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// Post-quantum cryptography re-exports
pub use icn_crypto_pq::*;

#[cfg(test)]
mod tests {
    //! Smoke tests verifying facade re-exports work correctly.

    #[test]
    fn facade_exports_hybrid_keypair() {
        // Verify HybridKeypair is accessible via facade
        let _: fn() -> &'static str = || std::any::type_name::<crate::HybridKeypair>();
    }

    #[test]
    fn facade_exports_signature() {
        // Verify HybridSignature is accessible via facade
        let _: fn() -> &'static str = || std::any::type_name::<crate::HybridSignature>();
    }
}
