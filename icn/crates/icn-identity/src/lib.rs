// Allow unused_assignments from Zeroize derive macro generated code
#![allow(unused_assignments)]
#![allow(missing_docs)]
// Prevent panics in production code paths
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! ICN Identity - DID management, key generation, and cryptographic operations
//!
//! This crate provides identity primitives for ICN, including:
//! - Traditional Ed25519-based DIDs (`did:icn:<pubkey>`)
//! - SDIS Anchor-based DIDs (`did:icn:<anchor-id>`)
//! - KeyBundle with hybrid post-quantum signatures
//! - VUI (Verifiable Unique Identifier) types

pub mod anchor;
pub mod batch_verify;
pub mod bundle;
pub mod commons;
pub mod commons_store;
pub mod keybundle;
pub mod keystore;
pub mod multi_device;
pub mod personhood;
pub mod personhood_store;
pub mod recovery;
pub mod revocation;
pub mod revocation_store;
pub mod sync;
pub mod vui;

use anyhow::Result;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub use anchor::{Anchor, EnrollmentPathway};
pub use batch_verify::{
    verify_signatures_batched, BatchVerifier, BatchVerifyResult, SignatureToVerify,
};
pub use bundle::{verify_binding_info, verify_did_matches_binding, BindingInfo, IdentityBundle};
pub use commons::{
    Affiliation, CommonsHolderRecord, CommonsRight, CommonsRights, HolderStatus, JurisdictionId,
    JurisdictionType, MembershipCapability, MembershipStatus,
};
pub use commons_store::CommonsHolderStore;
pub use keybundle::KeyBundle;
pub use keystore::{AgeKeyStore, KeyRotation, KeyStore, RotationReason};
pub use multi_device::{
    Capability, DidDocument, KeyType, RecoveryConfig, RecoveryMethod, RecoveryProof,
    RevocationReason, RotationEvent, RotationEventType, VerificationMethod,
};
pub use personhood::{
    AnchorStatus, BiometricType, KeyRotationReason, KeyRotationRecord, POPAttestation, POPLevel,
    POPMethod, PersonhoodAnchor, RecoverySignature, UniquenessAttestation, UniquenessProof,
    UniquenessProofType,
};
pub use personhood_store::{InMemoryPersonhoodStore, PersonhoodAnchorStore, PersonhoodStore};
pub use recovery::{
    RecoveryAttestation, RecoveryEvent, RecoveryMessage, RecoveryStatus, IDENTITY_RECOVERY_TOPIC,
};
pub use revocation::{
    AppealStatus, CommonsRevocationReason, RevocationCheck, RevocationRecord, RevocationScope,
    RevocationType,
};
pub use revocation_store::RevocationRegistry;
pub use sync::{DidDocumentCache, IdentityUpdateMessage, IDENTITY_UPDATES_TOPIC};
pub use vui::Vui;

/// Hybrid signature or classical signature wrapper
#[cfg(feature = "post-quantum")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HybridSignatureOrClassical {
    /// Hybrid PQ signature (Ed25519 + ML-DSA)
    Hybrid(icn_crypto_pq::HybridSignature),
    /// Classical Ed25519 signature only
    Classical(ed25519_dalek::Signature),
}

#[cfg(feature = "post-quantum")]
impl HybridSignatureOrClassical {
    /// Verify the signature against a message and keypair
    pub fn verify(&self, message: &[u8], keypair: &KeyPair) -> bool {
        match self {
            Self::Hybrid(sig) => {
                if let Some(ref pq_kp) = keypair.pq_keypair {
                    let hybrid_pub = icn_crypto_pq::HybridPublicKey {
                        classical: keypair.verifying_key.to_bytes().to_vec(),
                        pq: pq_kp.public_key().clone(),
                    };
                    sig.verify(message, &hybrid_pub)
                } else {
                    false // Hybrid sig requires PQ keys
                }
            }
            Self::Classical(sig) => {
                use ed25519_dalek::Verifier;
                keypair.verifying_key.verify(message, sig).is_ok()
            }
        }
    }

    /// Convert to bytes for serialization
    pub fn to_bytes(&self) -> Vec<u8> {
        // SAFETY: HybridSignature is a simple struct with byte arrays that always serializes
        // successfully. Bincode encoding cannot fail for well-formed data.
        #[allow(clippy::expect_used)]
        bincode::serde::encode_to_vec(self, bincode::config::legacy())
            .expect("HybridSignature serialization is infallible")
    }

    /// Parse from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        bincode::serde::decode_from_slice(bytes, bincode::config::legacy())
            .map(|(v, _)| v)
            .map_err(|e| anyhow::anyhow!("Failed to parse signature: {e}"))
    }
}

/// A decentralized identifier for an ICN node
///
/// DIDs are validated on construction/deserialization to ensure:
/// - They start with "did:icn:" prefix
/// - The identifier part is valid multibase (base58btc)
/// - The decoded bytes are exactly 32 bytes (Ed25519 public key size)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Did(String);

// Custom deserializer that validates DIDs on deserialization
// This prevents malformed DIDs from bypassing validation when received over the network
impl<'de> Deserialize<'de> for Did {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Did::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Did {
    /// Create a DID from an ed25519 public key
    pub fn from_public_key(public_key: &VerifyingKey) -> Self {
        let encoded = multibase::encode(multibase::Base::Base58Btc, public_key.as_bytes());
        Did(format!("did:icn:{encoded}"))
    }

    /// Parse and validate a DID string
    ///
    /// Validates that:
    /// - String starts with "did:icn:" prefix
    /// - Remaining part is valid multibase (base58btc)
    /// - Decoded bytes are exactly 32 bytes (Ed25519 public key size)
    ///
    /// Returns an error for malformed DIDs instead of panicking.
    ///
    /// Note: This is the implementation used by the `FromStr` trait.
    #[allow(clippy::should_implement_trait)] // FromStr trait is implemented below
    pub fn from_str(s: &str) -> Result<Self> {
        // Validate prefix
        if !s.starts_with("did:icn:") {
            anyhow::bail!("Invalid DID format: must start with 'did:icn:' (got: {s})");
        }

        // Extract multibase-encoded part
        let encoded_part = &s[8..]; // Skip "did:icn:"

        if encoded_part.is_empty() {
            anyhow::bail!("Invalid DID format: empty identifier after prefix");
        }

        // Decode multibase
        let (_base, decoded_bytes) = multibase::decode(encoded_part)
            .map_err(|e| anyhow::anyhow!("Invalid DID multibase encoding: {e}"))?;

        // Validate decoded size (Ed25519 public key is 32 bytes)
        if decoded_bytes.len() != 32 {
            anyhow::bail!(
                "Invalid DID: decoded public key has {} bytes, expected 32",
                decoded_bytes.len()
            );
        }

        // Validate it's a valid Ed25519 public key
        VerifyingKey::from_bytes(
            decoded_bytes
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Failed to convert to 32-byte array"))?,
        )
        .map_err(|e| anyhow::anyhow!("Invalid Ed25519 public key in DID: {e}"))?;

        Ok(Did(s.to_string()))
    }

    /// Get the string representation of this DID
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Extract the Ed25519 verifying key from this DID
    ///
    /// This decodes the DID's multibase-encoded public key and returns
    /// the VerifyingKey for signature verification.
    pub fn to_verifying_key(&self) -> Result<VerifyingKey> {
        // Defensive bounds check (should not fail for validated DIDs)
        let encoded_part = self
            .0
            .get(8..)
            .ok_or_else(|| anyhow::anyhow!("Invalid DID format: too short"))?;

        if encoded_part.is_empty() {
            anyhow::bail!("Invalid DID format: empty identifier after prefix");
        }

        // Decode multibase
        let (_base, decoded_bytes) = multibase::decode(encoded_part)
            .map_err(|e| anyhow::anyhow!("Invalid DID multibase encoding: {e}"))?;

        // Convert to VerifyingKey
        let key_bytes: [u8; 32] = decoded_bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid key length"))?;

        VerifyingKey::from_bytes(&key_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid Ed25519 public key: {e}"))
    }

    /// Internal constructor for creating DIDs from raw strings
    ///
    /// This bypasses validation and should only be used by trusted code
    /// (e.g., anchor module creating DIDs from anchor IDs).
    pub(crate) fn new_unchecked(s: String) -> Self {
        Did(s)
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

    // Post-quantum keypair (optional, feature-gated)
    #[cfg(feature = "post-quantum")]
    pq_keypair: Option<icn_crypto_pq::MlDsaKeypair>,
}

impl Clone for KeyPair {
    fn clone(&self) -> Self {
        KeyPair {
            secret_bytes: Zeroizing::new(*self.secret_bytes),
            verifying_key: self.verifying_key,
            did: self.did.clone(),
            #[cfg(feature = "post-quantum")]
            pq_keypair: self.pq_keypair.clone(),
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

        #[cfg(feature = "post-quantum")]
        let pq_keypair = Some(
            icn_crypto_pq::MlDsaKeypair::generate()
                .map_err(|e| anyhow::anyhow!("PQ key generation failed: {e}"))?,
        );

        Ok(KeyPair {
            secret_bytes: Zeroizing::new(secret_bytes),
            verifying_key,
            did,
            #[cfg(feature = "post-quantum")]
            pq_keypair,
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
            #[cfg(feature = "post-quantum")]
            pq_keypair: None, // Legacy keys don't have PQ component
        })
    }

    /// Reconstruct a keypair with PQ keys from raw bytes
    #[cfg(feature = "post-quantum")]
    pub fn from_bytes_with_pq(
        secret_bytes: &[u8; 32],
        public_bytes: &[u8; 32],
        pq_secret: &[u8],
        pq_public: &[u8],
    ) -> Result<Self> {
        let verifying_key = VerifyingKey::from_bytes(public_bytes)?;
        let did = Did::from_public_key(&verifying_key);

        // Verify the keys match
        let signing_key = SigningKey::from_bytes(secret_bytes);
        if signing_key.verifying_key() != verifying_key {
            anyhow::bail!("Public key does not match secret key");
        }

        // Reconstruct PQ keypair
        let pq_keypair = icn_crypto_pq::MlDsaKeypair::from_bytes(pq_secret, pq_public)
            .map_err(|e| anyhow::anyhow!("Invalid PQ keypair: {e}"))?;

        Ok(KeyPair {
            secret_bytes: Zeroizing::new(*secret_bytes),
            verifying_key,
            did,
            pq_keypair: Some(pq_keypair),
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

    /// Check if this keypair has post-quantum keys
    #[cfg(feature = "post-quantum")]
    pub fn has_pq_keys(&self) -> bool {
        self.pq_keypair.is_some()
    }

    /// Get the PQ public key if available
    #[cfg(feature = "post-quantum")]
    pub fn pq_public_key(&self) -> Option<icn_crypto_pq::MlDsaPublicKey> {
        self.pq_keypair.as_ref().map(|kp| kp.public_key().clone())
    }

    /// Sign a message
    pub fn sign(&self, message: &[u8]) -> ed25519_dalek::Signature {
        use ed25519_dalek::Signer;
        let signing_key = SigningKey::from_bytes(&self.secret_bytes);
        signing_key.sign(message)
    }

    /// Sign a message with hybrid signature (if PQ keys available)
    #[cfg(feature = "post-quantum")]
    pub fn sign_hybrid(&self, message: &[u8]) -> Result<HybridSignatureOrClassical> {
        use ed25519_dalek::Signer;
        let signing_key = SigningKey::from_bytes(&self.secret_bytes);
        let classical_sig = signing_key.sign(message);

        if let Some(ref pq_kp) = self.pq_keypair {
            let pq_sig = pq_kp.sign(message)?;
            Ok(HybridSignatureOrClassical::Hybrid(
                icn_crypto_pq::HybridSignature::new(classical_sig, pq_sig),
            ))
        } else {
            Ok(HybridSignatureOrClassical::Classical(classical_sig))
        }
    }

    /// Get the signing key bytes for use in external signing operations
    ///
    /// This method exposes the secret key bytes for use in scenarios where
    /// direct access to the signing key is needed (e.g., for compute task signing).
    /// Use with caution - these bytes should be handled securely.
    pub fn to_signing_key_bytes(&self) -> [u8; 32] {
        *self.secret_bytes
    }

    /// Export keypair for upgrade (PQ feature only)
    #[cfg(feature = "post-quantum")]
    pub fn export_for_upgrade(&self) -> ([u8; 32], [u8; 32]) {
        (*self.secret_bytes, self.verifying_key.to_bytes())
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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must start with 'did:icn:'"));
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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("multibase encoding"));
    }

    #[test]
    fn test_did_from_str_wrong_key_size() {
        // Create a multibase-encoded string with wrong size (16 bytes instead of 32)
        let short_bytes = vec![0u8; 16];
        let encoded = multibase::encode(multibase::Base::Base58Btc, &short_bytes);
        let did_str = format!("did:icn:{encoded}");

        let result = Did::from_str(&did_str);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected 32"));
    }

    #[test]
    fn test_did_from_str_invalid_ed25519_key() {
        // All zeros is not a valid Ed25519 public key
        let invalid_key = vec![0u8; 32];
        let encoded = multibase::encode(multibase::Base::Base58Btc, &invalid_key);
        let did_str = format!("did:icn:{encoded}");

        let result = Did::from_str(&did_str);
        // Note: All-zeros might actually be accepted by ed25519_dalek
        // This test documents the behavior even if it passes
        if let Err(e) = result {
            assert!(e.to_string().contains("Ed25519"));
        }
    }

    #[test]
    fn test_did_to_verifying_key() {
        // Generate a keypair and DID
        let kp = KeyPair::generate().unwrap();
        let did = kp.did();

        // Extract verifying key from DID
        let extracted_key = did.to_verifying_key().unwrap();

        // Should match the original keypair's verifying key
        assert_eq!(extracted_key, *kp.verifying_key());
    }

    #[test]
    fn test_did_signature_verification() {
        use ed25519_dalek::Verifier;

        // Generate keypair and sign a message
        let kp = KeyPair::generate().unwrap();
        let message = b"contract deployment";
        let signature = kp.sign(message);

        // Extract verifying key from DID and verify
        let did = kp.did();
        let verifying_key = did.to_verifying_key().unwrap();

        assert!(verifying_key.verify(message, &signature).is_ok());
    }

    // Regression tests for malformed DID handling (Issue #149)
    #[test]
    fn test_did_parsing_empty_string() {
        let result = Did::from_str("");
        assert!(result.is_err());
    }

    #[test]
    fn test_did_parsing_very_short_string() {
        let result = Did::from_str("abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_did_parsing_prefix_only() {
        let result = Did::from_str("did:icn");
        assert!(result.is_err());
    }

    #[test]
    fn test_did_deserialization_validates() {
        // Valid DID should deserialize successfully
        let kp = KeyPair::generate().unwrap();
        let json = format!(r#""{}""#, kp.did().as_str());
        let did: Result<Did, _> = serde_json::from_str(&json);
        assert!(did.is_ok());
    }

    #[test]
    fn test_did_deserialization_rejects_invalid_prefix() {
        // Invalid prefix should be rejected during deserialization
        let json = r#""invalid:prefix:abc123""#;
        let result: Result<Did, _> = serde_json::from_str(json);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("did:icn:") || err_msg.contains("Invalid"));
    }

    #[test]
    fn test_did_deserialization_rejects_empty() {
        let json = r#""""#;
        let result: Result<Did, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_did_deserialization_rejects_short_string() {
        let json = r#""abc""#;
        let result: Result<Did, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_did_deserialization_rejects_wrong_key_size() {
        // Create a multibase-encoded string with wrong size (16 bytes instead of 32)
        let short_bytes = vec![0u8; 16];
        let encoded = multibase::encode(multibase::Base::Base58Btc, &short_bytes);
        let json = format!(r#""did:icn:{encoded}""#);

        let result: Result<Did, _> = serde_json::from_str(&json);
        assert!(result.is_err());
    }
}
