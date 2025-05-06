//! Common cryptographic types for the ICN project
//!
//! This module provides shared cryptographic definitions used across components.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Hash algorithm types supported in the ICN system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HashAlgorithm {
    /// SHA-256 hash algorithm
    Sha256,
    /// SHA-512 hash algorithm
    Sha512,
    /// Blake2b hash algorithm
    Blake2b,
    /// Blake3 hash algorithm
    Blake3,
}

/// Digital signature algorithm types supported in the ICN system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignatureAlgorithm {
    /// Ed25519 signature algorithm
    Ed25519,
    /// ECDSA with secp256k1 curve
    EcdsaSecp256k1,
    /// RSA signature algorithm
    Rsa,
}

/// Cryptographic hash with algorithm information
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash {
    /// The hash algorithm used
    pub algorithm: HashAlgorithm,
    /// The raw hash bytes
    pub bytes: Vec<u8>,
}

/// Digital signature with algorithm information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// The signature algorithm used
    pub algorithm: SignatureAlgorithm,
    /// The raw signature bytes
    pub bytes: Vec<u8>,
    /// Optional signer information
    pub signer: Option<String>,
}

impl fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HashAlgorithm::Sha256 => write!(f, "sha256"),
            HashAlgorithm::Sha512 => write!(f, "sha512"),
            HashAlgorithm::Blake2b => write!(f, "blake2b"),
            HashAlgorithm::Blake3 => write!(f, "blake3"),
        }
    }
}

impl fmt::Display for SignatureAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignatureAlgorithm::Ed25519 => write!(f, "ed25519"),
            SignatureAlgorithm::EcdsaSecp256k1 => write!(f, "ecdsa-secp256k1"),
            SignatureAlgorithm::Rsa => write!(f, "rsa"),
        }
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}",
            self.algorithm,
            hex::encode(&self.bytes)
        )
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(signer) = &self.signer {
            write!(
                f,
                "{}:{}:{}",
                self.algorithm,
                signer,
                hex::encode(&self.bytes)
            )
        } else {
            write!(
                f,
                "{}:{}",
                self.algorithm,
                hex::encode(&self.bytes)
            )
        }
    }
}

impl Hash {
    /// Create a new hash with the given algorithm and bytes
    pub fn new(algorithm: HashAlgorithm, bytes: Vec<u8>) -> Self {
        Self { algorithm, bytes }
    }

    /// Create a new SHA-256 hash
    pub fn sha256(bytes: Vec<u8>) -> Self {
        Self::new(HashAlgorithm::Sha256, bytes)
    }

    /// Create a new SHA-512 hash
    pub fn sha512(bytes: Vec<u8>) -> Self {
        Self::new(HashAlgorithm::Sha512, bytes)
    }

    /// Create a new Blake2b hash
    pub fn blake2b(bytes: Vec<u8>) -> Self {
        Self::new(HashAlgorithm::Blake2b, bytes)
    }

    /// Create a new Blake3 hash
    pub fn blake3(bytes: Vec<u8>) -> Self {
        Self::new(HashAlgorithm::Blake3, bytes)
    }
}

impl Signature {
    /// Create a new signature with the given algorithm and bytes
    pub fn new(
        algorithm: SignatureAlgorithm,
        bytes: Vec<u8>,
        signer: Option<String>,
    ) -> Self {
        Self {
            algorithm,
            bytes,
            signer,
        }
    }

    /// Create a new Ed25519 signature
    pub fn ed25519(bytes: Vec<u8>, signer: Option<String>) -> Self {
        Self::new(SignatureAlgorithm::Ed25519, bytes, signer)
    }

    /// Create a new ECDSA signature using secp256k1 curve
    pub fn ecdsa_secp256k1(bytes: Vec<u8>, signer: Option<String>) -> Self {
        Self::new(SignatureAlgorithm::EcdsaSecp256k1, bytes, signer)
    }

    /// Create a new RSA signature
    pub fn rsa(bytes: Vec<u8>, signer: Option<String>) -> Self {
        Self::new(SignatureAlgorithm::Rsa, bytes, signer)
    }
} 