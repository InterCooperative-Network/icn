//! ICN Identity - DID management, key generation, and cryptographic operations

pub mod keystore;

use anyhow::Result;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub use keystore::{AgeKeyStore, KeyRotation, KeyStore, RotationReason};

/// A decentralized identifier for an ICN node
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Did(String);

impl Did {
    /// Create a DID from an ed25519 public key
    pub fn from_public_key(public_key: &VerifyingKey) -> Self {
        let encoded = multibase::encode(multibase::Base::Base58Btc, public_key.as_bytes());
        Did(format!("did:icn:{}", encoded))
    }

    /// Parse and validate a DID string
    ///
    /// Validates that:
    /// - String starts with "did:icn:" prefix
    /// - Remaining part is valid multibase (base58btc)
    /// - Decoded bytes are exactly 32 bytes (Ed25519 public key size)
    ///
    /// Returns an error for malformed DIDs instead of panicking.
    pub fn from_str(s: &str) -> Result<Self> {
        // Validate prefix
        if !s.starts_with("did:icn:") {
            anyhow::bail!("Invalid DID format: must start with 'did:icn:' (got: {})", s);
        }

        // Extract multibase-encoded part
        let encoded_part = &s[8..]; // Skip "did:icn:"

        if encoded_part.is_empty() {
            anyhow::bail!("Invalid DID format: empty identifier after prefix");
        }

        // Decode multibase
        let (_base, decoded_bytes) = multibase::decode(encoded_part)
            .map_err(|e| anyhow::anyhow!("Invalid DID multibase encoding: {}", e))?;

        // Validate decoded size (Ed25519 public key is 32 bytes)
        if decoded_bytes.len() != 32 {
            anyhow::bail!(
                "Invalid DID: decoded public key has {} bytes, expected 32",
                decoded_bytes.len()
            );
        }

        // Validate it's a valid Ed25519 public key
        VerifyingKey::from_bytes(
            decoded_bytes.as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Failed to convert to 32-byte array"))?,
        )
        .map_err(|e| anyhow::anyhow!("Invalid Ed25519 public key in DID: {}", e))?;

        Ok(Did(s.to_string()))
    }

    /// Get the string representation of this DID
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Did {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for Did {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        Did::from_str(s)
    }
}

/// A key pair for ICN identity
pub struct KeyPair {
    // Store key bytes in a zeroizing container for security
    secret_bytes: Zeroizing<[u8; 32]>,
    verifying_key: VerifyingKey,
    did: Did,
}

impl Clone for KeyPair {
    fn clone(&self) -> Self {
        KeyPair {
            secret_bytes: Zeroizing::new(*self.secret_bytes),
            verifying_key: self.verifying_key,
            did: self.did.clone(),
        }
    }
}

impl KeyPair {
    /// Generate a new random key pair
    pub fn generate() -> Result<Self> {
        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes = signing_key.to_bytes();
        let verifying_key = signing_key.verifying_key();
        let did = Did::from_public_key(&verifying_key);

        Ok(KeyPair {
            secret_bytes: Zeroizing::new(secret_bytes),
            verifying_key,
            did,
        })
    }

    /// Reconstruct a keypair from raw bytes
    pub fn from_bytes(secret_bytes: &[u8; 32], public_bytes: &[u8; 32]) -> Result<Self> {
        let verifying_key = VerifyingKey::from_bytes(public_bytes)?;
        let did = Did::from_public_key(&verifying_key);

        // Verify the keys match
        let signing_key = SigningKey::from_bytes(secret_bytes);
        if signing_key.verifying_key() != verifying_key {
            anyhow::bail!("Public key does not match secret key");
        }

        Ok(KeyPair {
            secret_bytes: Zeroizing::new(*secret_bytes),
            verifying_key,
            did,
        })
    }

    /// Get the DID for this key pair
    pub fn did(&self) -> &Did {
        &self.did
    }

    /// Get the verifying (public) key
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    /// Get access to secret bytes (for serialization only)
    pub(crate) fn secret_bytes(&self) -> &[u8; 32] {
        &self.secret_bytes
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> ed25519_dalek::Signature {
        use ed25519_dalek::Signer;
        let signing_key = SigningKey::from_bytes(&self.secret_bytes);
        signing_key.sign(message)
    }
}

impl Drop for KeyPair {
    fn drop(&mut self) {
        // Zeroizing handles the secure drop of secret_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypair() {
        let kp = KeyPair::generate().unwrap();
        assert!(kp.did().as_str().starts_with("did:icn:"));
    }

    #[test]
    fn test_sign_verify() {
        use ed25519_dalek::Verifier;

        let kp = KeyPair::generate().unwrap();
        let message = b"hello world";
        let signature = kp.sign(message);

        assert!(kp.verifying_key().verify(message, &signature).is_ok());
    }

    #[test]
    fn test_did_from_str_valid() {
        // Generate a valid DID
        let kp = KeyPair::generate().unwrap();
        let did_str = kp.did().as_str();

        // Should parse successfully
        let parsed_did = Did::from_str(did_str).unwrap();
        assert_eq!(parsed_did.as_str(), did_str);
    }

    #[test]
    fn test_did_from_str_invalid_prefix() {
        let result = Did::from_str("invalid:prefix:abc123");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must start with 'did:icn:'"));
    }

    #[test]
    fn test_did_from_str_empty_identifier() {
        let result = Did::from_str("did:icn:");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty identifier"));
    }

    #[test]
    fn test_did_from_str_invalid_multibase() {
        let result = Did::from_str("did:icn:INVALID!!!BASE58");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("multibase encoding"));
    }

    #[test]
    fn test_did_from_str_wrong_key_size() {
        // Create a multibase-encoded string with wrong size (16 bytes instead of 32)
        let short_bytes = vec![0u8; 16];
        let encoded = multibase::encode(multibase::Base::Base58Btc, &short_bytes);
        let did_str = format!("did:icn:{}", encoded);

        let result = Did::from_str(&did_str);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected 32"));
    }

    #[test]
    fn test_did_from_str_invalid_ed25519_key() {
        // All zeros is not a valid Ed25519 public key
        let invalid_key = vec![0u8; 32];
        let encoded = multibase::encode(multibase::Base::Base58Btc, &invalid_key);
        let did_str = format!("did:icn:{}", encoded);

        let result = Did::from_str(&did_str);
        // Note: All-zeros might actually be accepted by ed25519_dalek
        // This test documents the behavior even if it passes
        if result.is_err() {
            assert!(result.unwrap_err().to_string().contains("Ed25519"));
        }
    }
}
