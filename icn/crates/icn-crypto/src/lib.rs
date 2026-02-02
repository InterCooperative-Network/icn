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
//! use icn_crypto::KeyPair;
//! 
//! let keypair = KeyPair::generate_hybrid()?;
//! let signature = keypair.sign(b"message")?;
//! ```

#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// Post-quantum cryptography re-exports
pub use icn_crypto_pq::*;
