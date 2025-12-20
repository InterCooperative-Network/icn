//! SDIS KeyBundle - Rotatable cryptographic key bundle
//!
//! A KeyBundle contains the cryptographic keys bound to an Anchor.
//! Unlike the Anchor which is permanent, KeyBundles can be rotated
//! while maintaining the same identity.
//!
//! # Key Components
//!
//! - **Hybrid Signature Keys**: Ed25519 + ML-DSA for quantum-resistant signatures
//! - **Encryption Key**: X25519 for key exchange
//! - **Version**: Monotonically increasing to track rotations
//!
//! # Rotation Protocol
//!
//! 1. Generate new KeyBundle with incremented version
//! 2. Sign rotation request with old KeyBundle
//! 3. Publish rotation event to gossip network
//! 4. Stewards verify and attest to the rotation
//! 5. Old KeyBundle enters revocation period
//! 6. New KeyBundle becomes active

use icn_crypto_pq::{HybridKeypair, HybridPublicKey, HybridSignature};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519Secret};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::anchor::Anchor;
use crate::Did;

/// Rotatable key bundle bound to an Anchor
///
/// Contains all cryptographic keys needed for SDIS operations:
/// - Hybrid signatures (Ed25519 + ML-DSA)
/// - Encryption (X25519)
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct KeyBundle {
    /// The anchor this bundle is bound to
    #[zeroize(skip)]
    pub anchor: Anchor,
    /// Version number (1, 2, 3, ...)
    #[zeroize(skip)]
    pub version: u32,
    /// Hybrid signing keypair (Ed25519 + ML-DSA)
    #[zeroize(skip)]
    hybrid_keypair: HybridKeypair,
    /// X25519 secret key for encryption
    x25519_secret: [u8; 32],
    /// X25519 public key
    #[zeroize(skip)]
    x25519_public: [u8; 32],
    /// When this bundle was issued
    #[zeroize(skip)]
    pub issued_at: u64,
    /// Optional expiration time
    #[zeroize(skip)]
    pub expires_at: Option<u64>,
}

/// Public portion of a KeyBundle for verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBundlePublic {
    /// Anchor this bundle belongs to
    pub anchor_id: [u8; 32],
    /// Bundle version
    pub version: u32,
    /// Hybrid public key (Ed25519 + ML-DSA)
    pub hybrid_public: HybridPublicKey,
    /// X25519 public key for encryption
    pub x25519_public: [u8; 32],
    /// When issued
    pub issued_at: u64,
    /// Optional expiration
    pub expires_at: Option<u64>,
}

/// Rotation request to transition to a new KeyBundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationRequest {
    /// Anchor ID being rotated
    pub anchor_id: [u8; 32],
    /// Old version number
    pub old_version: u32,
    /// New version number (must be old + 1)
    pub new_version: u32,
    /// Public keys of the new bundle
    pub new_public: KeyBundlePublic,
    /// Signature from OLD bundle authorizing this rotation
    pub old_signature: HybridSignature,
    /// Optional reason for rotation
    pub reason: Option<RotationReason>,
    /// Timestamp of request
    pub requested_at: u64,
}

/// Reason for key rotation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationReason {
    /// Scheduled rotation (e.g., annual)
    Scheduled,
    /// Key compromise suspected
    Compromise,
    /// Device lost
    DeviceLoss,
    /// Recovery procedure
    Recovery,
    /// Administrative/policy change
    Administrative,
}

impl KeyBundle {
    /// Generate a new KeyBundle for an anchor
    ///
    /// Creates fresh random keys. For deterministic key generation
    /// from a seed, use `from_seed`.
    pub fn generate(anchor: Anchor, version: u32) -> anyhow::Result<Self> {
        let hybrid_keypair = HybridKeypair::generate()
            .map_err(|e| anyhow::anyhow!("Failed to generate hybrid keypair: {e}"))?;

        let x25519_secret = X25519Secret::random_from_rng(rand::rngs::OsRng);
        let x25519_public = X25519PublicKey::from(&x25519_secret);

        Ok(Self {
            anchor,
            version,
            hybrid_keypair,
            x25519_secret: x25519_secret.to_bytes(),
            x25519_public: x25519_public.to_bytes(),
            issued_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            expires_at: None,
        })
    }

    /// Generate a KeyBundle from a seed (deterministic)
    ///
    /// The seed must be at least 64 bytes. Key derivation is version-aware,
    /// so the same seed with different versions produces different keys.
    pub fn from_seed(anchor: Anchor, version: u32, seed: &[u8; 64]) -> anyhow::Result<Self> {
        let hybrid_keypair = HybridKeypair::from_seed(seed)
            .map_err(|e| anyhow::anyhow!("Failed to generate hybrid keypair from seed: {e}"))?;

        // Derive X25519 key from seed
        let (_, _, x25519_seed) = icn_crypto_pq::derive_keybundle_keys(seed, version);
        let x25519_secret = X25519Secret::from(x25519_seed);
        let x25519_public = X25519PublicKey::from(&x25519_secret);

        Ok(Self {
            anchor,
            version,
            hybrid_keypair,
            x25519_secret: x25519_secret.to_bytes(),
            x25519_public: x25519_public.to_bytes(),
            issued_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            expires_at: None,
        })
    }

    /// Sign a message with hybrid signatures (Ed25519 + ML-DSA)
    ///
    /// Both signatures must verify for the message to be considered valid.
    pub fn sign(&self, message: &[u8]) -> HybridSignature {
        self.hybrid_keypair.sign(message)
    }

    /// Sign a message with classical (Ed25519) only
    ///
    /// Use this for ephemeral proofs where post-quantum security isn't needed
    /// (e.g., short-lived QR codes that expire in hours).
    pub fn sign_classical(&self, message: &[u8]) -> ed25519_dalek::Signature {
        self.hybrid_keypair.sign_classical_only(message)
    }

    /// Get the DID for this KeyBundle
    ///
    /// Returns the anchor's DID, not a key-based DID.
    pub fn did(&self) -> Did {
        self.anchor.to_did()
    }

    /// Get the public portion of this bundle
    pub fn public_bundle(&self) -> KeyBundlePublic {
        KeyBundlePublic {
            anchor_id: self.anchor.id,
            version: self.version,
            hybrid_public: self.hybrid_keypair.public_key(),
            x25519_public: self.x25519_public,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
        }
    }

    /// Get the hybrid public key
    pub fn hybrid_public(&self) -> HybridPublicKey {
        self.hybrid_keypair.public_key()
    }

    /// Get the X25519 public key
    pub fn x25519_public(&self) -> [u8; 32] {
        self.x25519_public
    }

    /// Create a rotation request to a new bundle
    ///
    /// The returned request is signed by this (old) bundle to authorize
    /// the transition to the new bundle.
    pub fn create_rotation_request(
        &self,
        new_bundle: &KeyBundle,
        reason: Option<RotationReason>,
    ) -> anyhow::Result<RotationRequest> {
        if new_bundle.version != self.version + 1 {
            anyhow::bail!(
                "New bundle version {} must be exactly one more than old version {}",
                new_bundle.version,
                self.version
            );
        }

        if new_bundle.anchor.id != self.anchor.id {
            anyhow::bail!("New bundle must be for the same anchor");
        }

        // Create the rotation message to sign
        let rotation_message = RotationMessage {
            anchor_id: self.anchor.id,
            old_version: self.version,
            new_version: new_bundle.version,
            new_hybrid_public: new_bundle.hybrid_keypair.public_key(),
            new_x25519_public: new_bundle.x25519_public,
        };

        let message_bytes =
            bincode::serde::encode_to_vec(&rotation_message, bincode::config::legacy())?;
        let signature = self.sign(&message_bytes);

        Ok(RotationRequest {
            anchor_id: self.anchor.id,
            old_version: self.version,
            new_version: new_bundle.version,
            new_public: new_bundle.public_bundle(),
            old_signature: signature,
            reason,
            requested_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }

    /// Set expiration time for this bundle
    pub fn set_expires_at(&mut self, expires_at: u64) {
        self.expires_at = Some(expires_at);
    }

    /// Check if this bundle has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now > expires
        } else {
            false
        }
    }

    // === Secret key extraction (for keystore serialization) ===

    /// Get the classical (Ed25519) secret key bytes
    ///
    /// WARNING: Handle with care - this exposes secret key material.
    pub fn classical_secret_bytes(&self) -> [u8; 32] {
        self.hybrid_keypair.classical_secret_bytes()
    }

    /// Get the classical (Ed25519) public key bytes
    pub fn classical_public_bytes(&self) -> Vec<u8> {
        self.hybrid_keypair.public_key().classical.clone()
    }

    /// Get the post-quantum (ML-DSA) secret key bytes
    ///
    /// WARNING: Handle with care - this exposes secret key material.
    pub fn pq_secret_bytes(&self) -> &[u8] {
        self.hybrid_keypair.pq_secret_bytes()
    }

    /// Get the post-quantum (ML-DSA) public key bytes
    pub fn pq_public_bytes(&self) -> Vec<u8> {
        self.hybrid_keypair.pq_public().as_bytes().to_vec()
    }

    /// Get the X25519 secret key bytes
    ///
    /// WARNING: Handle with care - this exposes secret key material.
    pub fn x25519_secret_bytes(&self) -> [u8; 32] {
        self.x25519_secret
    }

    /// Reconstruct a KeyBundle from stored bytes
    ///
    /// Used when loading from keystore.
    pub fn from_stored(
        anchor: Anchor,
        version: u32,
        classical_secret: &[u8; 32],
        classical_public: &[u8; 32],
        pq_secret: &[u8],
        pq_public: &[u8],
        x25519_secret: [u8; 32],
        x25519_public: [u8; 32],
        issued_at: u64,
        expires_at: Option<u64>,
    ) -> anyhow::Result<Self> {
        let hybrid_keypair =
            HybridKeypair::from_bytes(classical_secret, classical_public, pq_secret, pq_public)
                .map_err(|e| anyhow::anyhow!("Failed to reconstruct hybrid keypair: {e}"))?;

        Ok(Self {
            anchor,
            version,
            hybrid_keypair,
            x25519_secret,
            x25519_public,
            issued_at,
            expires_at,
        })
    }
}

/// Internal rotation message for signing
#[derive(Serialize, Deserialize)]
struct RotationMessage {
    anchor_id: [u8; 32],
    old_version: u32,
    new_version: u32,
    new_hybrid_public: HybridPublicKey,
    new_x25519_public: [u8; 32],
}

impl KeyBundlePublic {
    /// Verify a hybrid signature
    pub fn verify(&self, message: &[u8], signature: &HybridSignature) -> bool {
        signature.verify(message, &self.hybrid_public)
    }

    /// Verify a rotation request
    ///
    /// Checks that:
    /// 1. The new version is exactly old + 1
    /// 2. The signature is valid from the old bundle
    pub fn verify_rotation(&self, request: &RotationRequest) -> anyhow::Result<bool> {
        if request.old_version != self.version {
            anyhow::bail!(
                "Request old version {} doesn't match this bundle version {}",
                request.old_version,
                self.version
            );
        }

        if request.new_version != self.version + 1 {
            anyhow::bail!(
                "New version {} must be exactly one more than old version {}",
                request.new_version,
                self.version
            );
        }

        // Reconstruct the rotation message
        let rotation_message = RotationMessage {
            anchor_id: request.anchor_id,
            old_version: request.old_version,
            new_version: request.new_version,
            new_hybrid_public: request.new_public.hybrid_public.clone(),
            new_x25519_public: request.new_public.x25519_public,
        };

        let message_bytes =
            bincode::serde::encode_to_vec(&rotation_message, bincode::config::legacy())?;

        Ok(self.verify(&message_bytes, &request.old_signature))
    }

    /// Check if this bundle has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now > expires
        } else {
            false
        }
    }
}

impl Clone for KeyBundle {
    fn clone(&self) -> Self {
        Self {
            anchor: self.anchor.clone(),
            version: self.version,
            hybrid_keypair: self.hybrid_keypair.clone(),
            x25519_secret: self.x25519_secret,
            x25519_public: self.x25519_public,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_anchor() -> Anchor {
        Anchor::genesis("test")
    }

    #[test]
    fn test_keybundle_generate() {
        let anchor = test_anchor();
        let bundle = KeyBundle::generate(anchor, 1).unwrap();

        assert_eq!(bundle.version, 1);
        assert!(bundle.issued_at > 0);
        assert!(bundle.expires_at.is_none());
    }

    #[test]
    fn test_keybundle_sign_verify() {
        let anchor = test_anchor();
        let bundle = KeyBundle::generate(anchor, 1).unwrap();
        let message = b"test message";

        let signature = bundle.sign(message);
        let public_bundle = bundle.public_bundle();

        assert!(public_bundle.verify(message, &signature));
    }

    #[test]
    fn test_keybundle_sign_classical() {
        use ed25519_dalek::Verifier;

        let anchor = test_anchor();
        let bundle = KeyBundle::generate(anchor, 1).unwrap();
        let message = b"ephemeral message";

        let signature = bundle.sign_classical(message);
        let verifying_key = bundle.hybrid_keypair.classical_public();

        assert!(verifying_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_keybundle_did() {
        let anchor = test_anchor();
        let bundle = KeyBundle::generate(anchor.clone(), 1).unwrap();

        let did = bundle.did();
        assert!(did.as_str().starts_with("did:icn:"));
        assert_eq!(did.as_str(), anchor.to_did().as_str());
    }

    #[test]
    fn test_keybundle_rotation() {
        let anchor = test_anchor();
        let bundle_v1 = KeyBundle::generate(anchor.clone(), 1).unwrap();
        let bundle_v2 = KeyBundle::generate(anchor, 2).unwrap();

        let request = bundle_v1
            .create_rotation_request(&bundle_v2, Some(RotationReason::Scheduled))
            .unwrap();

        let public_v1 = bundle_v1.public_bundle();
        assert!(public_v1.verify_rotation(&request).unwrap());
    }

    #[test]
    fn test_keybundle_rotation_wrong_version() {
        let anchor = test_anchor();
        let bundle_v1 = KeyBundle::generate(anchor.clone(), 1).unwrap();
        let bundle_v3 = KeyBundle::generate(anchor, 3).unwrap(); // Skip v2

        let result = bundle_v1.create_rotation_request(&bundle_v3, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_keybundle_expiration() {
        let anchor = test_anchor();
        let mut bundle = KeyBundle::generate(anchor, 1).unwrap();

        assert!(!bundle.is_expired());

        // Set to past time
        bundle.set_expires_at(1);
        assert!(bundle.is_expired());

        // Set to future time
        bundle.set_expires_at(u64::MAX);
        assert!(!bundle.is_expired());
    }

    #[test]
    fn test_keybundle_public_serialization() {
        let anchor = test_anchor();
        let bundle = KeyBundle::generate(anchor, 1).unwrap();
        let public_bundle = bundle.public_bundle();

        let json = serde_json::to_string(&public_bundle).unwrap();
        let restored: KeyBundlePublic = serde_json::from_str(&json).unwrap();

        assert_eq!(public_bundle.anchor_id, restored.anchor_id);
        assert_eq!(public_bundle.version, restored.version);
        assert_eq!(public_bundle.x25519_public, restored.x25519_public);
    }

    #[test]
    fn test_keybundle_from_seed() {
        let anchor = test_anchor();
        let seed = [42u8; 64];

        let bundle1 = KeyBundle::from_seed(anchor.clone(), 1, &seed).unwrap();
        let bundle2 = KeyBundle::from_seed(anchor, 1, &seed).unwrap();

        // Same seed + version should give same X25519 keys
        assert_eq!(bundle1.x25519_public, bundle2.x25519_public);
    }
}
