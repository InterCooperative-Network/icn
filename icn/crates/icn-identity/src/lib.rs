//! ICN Identity - DID management, key generation, and cryptographic operations

use anyhow::Result;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

/// A decentralized identifier for an ICN node
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Did(String);

impl Did {
    /// Create a DID from an ed25519 public key
    pub fn from_public_key(public_key: &VerifyingKey) -> Self {
        let encoded = multibase::encode(multibase::Base::Base58Btc, public_key.as_bytes());
        Did(format!("did:icn:{}", encoded))
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

/// A key pair for ICN identity
pub struct KeyPair {
    // Store key bytes in a zeroizing container for security
    secret_bytes: Zeroizing<[u8; 32]>,
    verifying_key: VerifyingKey,
    did: Did,
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

    /// Get the DID for this key pair
    pub fn did(&self) -> &Did {
        &self.did
    }

    /// Get the verifying (public) key
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
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
}
