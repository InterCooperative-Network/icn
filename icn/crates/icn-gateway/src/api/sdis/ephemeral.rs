//! Ephemeral Proof System
//!
//! Generates short-lived proofs suitable for QR codes and offline verification.
//!
//! Ephemeral proofs:
//! - Are small enough for QR codes (<1KB)
//! - Have short validity (minutes to hours)
//! - Use classical signatures for fast verification
//! - Can be upgraded to full STARK proofs via network channels

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use icn_identity::KeyBundle;
use icn_zkp::ProofType;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use utoipa::ToSchema;

/// Epoch for relative timestamps (2024-01-01 00:00:00 UTC)
pub const EPOCH_2024: u64 = 1704067200;

/// Maximum ephemeral proof validity (24 hours)
pub const MAX_VALIDITY: Duration = Duration::from_secs(86400);

/// Default ephemeral proof validity (1 hour)
pub const DEFAULT_VALIDITY: Duration = Duration::from_secs(3600);

/// Errors from ephemeral proof operations
#[derive(Debug, Error)]
pub enum EphemeralError {
    #[error("Proof expired")]
    Expired,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Replay detected")]
    ReplayDetected,

    #[error("Invalid proof format: {0}")]
    InvalidFormat(String),

    #[error("Binding mismatch")]
    BindingMismatch,

    #[error("Validity too long (max {MAX_VALIDITY:?})")]
    ValidityTooLong,

    #[error("Key error: {0}")]
    KeyError(String),
}

/// Available upgrade channels for Level 2+ verification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[repr(u8)]
pub enum Channel {
    /// Near-Field Communication
    Nfc = 0,
    /// Bluetooth Low Energy
    Ble = 1,
    /// Wi-Fi Direct
    WifiDirect = 2,
    /// HTTP (network required)
    Http = 3,
}

impl Channel {
    /// Convert channels to bitmap
    pub fn to_bitmap(channels: &[Channel]) -> u8 {
        let mut bitmap = 0u8;
        for ch in channels {
            bitmap |= 1 << (*ch as u8);
        }
        bitmap
    }

    /// Convert bitmap to channels
    pub fn from_bitmap(bitmap: u8) -> Vec<Channel> {
        let mut channels = Vec::new();
        if bitmap & 0x01 != 0 {
            channels.push(Channel::Nfc);
        }
        if bitmap & 0x02 != 0 {
            channels.push(Channel::Ble);
        }
        if bitmap & 0x04 != 0 {
            channels.push(Channel::WifiDirect);
        }
        if bitmap & 0x08 != 0 {
            channels.push(Channel::Http);
        }
        channels
    }
}

/// Ephemeral proof for QR codes
///
/// Small enough to fit in a QR code (~135 bytes base encoding).
/// Valid for a short time, verified with classical Ed25519.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EphemeralProof {
    /// Version byte
    pub v: u8,
    /// What's being proved
    pub t: ProofType,
    /// Truncated anchor hash (first 16 bytes)
    pub a: [u8; 16],
    /// Ephemeral Ed25519 public key
    pub k: [u8; 32],
    /// Expiry timestamp (Unix seconds)
    pub x: u64,
    /// Random nonce (prevents replay)
    pub n: [u8; 16],
    /// Ed25519 signature over [v, t, a, k, x, n] (stored as Vec for serde)
    #[serde(with = "serde_bytes")]
    pub s: Vec<u8>,
    /// Available upgrade channels
    pub c: Vec<Channel>,
}

impl EphemeralProof {
    /// Generate a new ephemeral proof
    ///
    /// Creates a short-lived proof signed by an ephemeral key derived from
    /// the KeyBundle. The ephemeral key is bound to the anchor via the binding.
    pub fn generate(
        keybundle: &KeyBundle,
        proof_type: ProofType,
        validity: Duration,
        channels: Vec<Channel>,
    ) -> Result<(Self, EphemeralBinding), EphemeralError> {
        if validity > MAX_VALIDITY {
            return Err(EphemeralError::ValidityTooLong);
        }

        // Generate ephemeral signing key
        let mut ephemeral_seed = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut ephemeral_seed);
        let ephemeral_signing = SigningKey::from_bytes(&ephemeral_seed);
        let ephemeral_pk = ephemeral_signing.verifying_key();

        // Calculate expiry
        let now = icn_time::current_timestamp_secs();
        let expiry = now + validity.as_secs();

        // Generate nonce
        let mut nonce = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut nonce);

        // Get anchor hash and truncate
        let anchor_id = keybundle.anchor.id_bytes();
        let mut truncated_anchor = [0u8; 16];
        truncated_anchor.copy_from_slice(&anchor_id[..16]);

        // Build signing payload
        let mut proof = EphemeralProof {
            v: 1,
            t: proof_type,
            a: truncated_anchor,
            k: ephemeral_pk.to_bytes(),
            x: expiry,
            n: nonce,
            s: vec![0u8; 64],
            c: channels,
        };

        // Sign with ephemeral key
        let payload = proof.signing_payload();
        let signature = ephemeral_signing.sign(&payload);
        proof.s = signature.to_bytes().to_vec();

        // Create binding (for Level 2+ verification)
        let binding = EphemeralBinding::create(keybundle, &ephemeral_pk, expiry)?;

        Ok((proof, binding))
    }

    /// Get the payload that is signed
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(128);
        payload.push(self.v);
        payload
            .extend_from_slice(&icn_encoding::encode_bincode_legacy(&self.t).unwrap_or_default());
        payload.extend_from_slice(&self.a);
        payload.extend_from_slice(&self.k);
        payload.extend_from_slice(&self.x.to_le_bytes());
        payload.extend_from_slice(&self.n);
        payload
    }

    /// Check if proof has expired
    pub fn is_expired(&self) -> bool {
        icn_time::current_timestamp_secs() > self.x
    }

    /// Verify the Ed25519 signature (Level 1)
    pub fn verify_signature(&self) -> Result<(), EphemeralError> {
        let pk = VerifyingKey::from_bytes(&self.k)
            .map_err(|e| EphemeralError::KeyError(e.to_string()))?;

        if self.s.len() != 64 {
            return Err(EphemeralError::InvalidSignature);
        }

        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&self.s);
        let signature = Signature::from_bytes(&sig_bytes);
        let payload = self.signing_payload();

        pk.verify(&payload, &signature)
            .map_err(|_| EphemeralError::InvalidSignature)
    }

    /// Get signature as fixed array (for QR encoding)
    pub fn signature_bytes(&self) -> [u8; 64] {
        let mut bytes = [0u8; 64];
        if self.s.len() >= 64 {
            bytes.copy_from_slice(&self.s[..64]);
        }
        bytes
    }

    /// Size in bytes (approximate)
    pub fn size(&self) -> usize {
        1 + // version
        32 + // proof type (estimated)
        16 + // truncated anchor
        32 + // ephemeral pk
        8 + // expiry
        16 + // nonce
        64 + // signature
        1 // channels bitmap
    }
}

/// Binding between ephemeral key and anchor (for Level 2+ verification)
///
/// This proves the ephemeral key is authorized by the identity's anchor.
/// It's larger (~2.5KB with hybrid signature) and sent via NFC/BLE.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EphemeralBinding {
    /// Full anchor hash
    pub anchor: [u8; 32],
    /// Ephemeral public key (must match proof.k)
    pub ephemeral_pk: [u8; 32],
    /// Expiry timestamp (must match proof.x)
    pub expires: u64,
    /// Signature from anchor key over binding
    #[serde(with = "serde_bytes")]
    pub signature: Vec<u8>,
    /// Optional: Hybrid (classical + PQ) signature
    #[serde(with = "serde_bytes")]
    pub hybrid_signature: Vec<u8>,
}

impl EphemeralBinding {
    /// Create a binding signed by the anchor key
    pub fn create(
        keybundle: &KeyBundle,
        ephemeral_pk: &VerifyingKey,
        expires: u64,
    ) -> Result<Self, EphemeralError> {
        let anchor = *keybundle.anchor.id_bytes();

        let mut binding = EphemeralBinding {
            anchor,
            ephemeral_pk: ephemeral_pk.to_bytes(),
            expires,
            signature: Vec::new(),
            hybrid_signature: Vec::new(),
        };

        // Sign with hybrid key (includes classical Ed25519)
        let payload = binding.signing_payload();
        let hybrid_sig = keybundle.sign(&payload);

        // Store the serialized hybrid signature
        binding.hybrid_signature =
            icn_encoding::encode_bincode_legacy(&hybrid_sig).unwrap_or_default();

        // Extract classical signature for compatibility
        binding.signature = hybrid_sig.classical.clone();

        Ok(binding)
    }

    /// Get the payload that is signed
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(72);
        payload.extend_from_slice(&self.anchor);
        payload.extend_from_slice(&self.ephemeral_pk);
        payload.extend_from_slice(&self.expires.to_le_bytes());
        payload
    }

    /// Verify binding matches a proof
    pub fn matches_proof(&self, proof: &EphemeralProof) -> bool {
        self.ephemeral_pk == proof.k && self.expires == proof.x && self.anchor[..16] == proof.a
    }

    /// Size in bytes (approximate)
    pub fn size(&self) -> usize {
        32 + // anchor
        32 + // ephemeral pk
        8 + // expires
        self.signature.len() +
        self.hybrid_signature.len()
    }
}

/// Result of ephemeral proof verification
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerifyResult {
    /// Whether verification passed
    pub valid: bool,
    /// Verification level achieved
    pub level: u8,
    /// What was proved (if valid)
    pub proof_type: Option<ProofType>,
    /// Warnings or notes
    pub warnings: Vec<String>,
    /// Verification timestamp
    pub verified_at: u64,
}

impl VerifyResult {
    /// Create a valid result
    pub fn valid(level: u8, proof_type: ProofType) -> Self {
        Self {
            valid: true,
            level,
            proof_type: Some(proof_type),
            warnings: Vec::new(),
            verified_at: icn_time::current_timestamp_secs(),
        }
    }

    /// Create an invalid result
    pub fn invalid(reason: &str) -> Self {
        Self {
            valid: false,
            level: 0,
            proof_type: None,
            warnings: vec![reason.to_string()],
            verified_at: icn_time::current_timestamp_secs(),
        }
    }

    /// Add a warning
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::Anchor;

    fn test_keybundle() -> KeyBundle {
        let anchor = Anchor::genesis("test");
        KeyBundle::generate(anchor, 1).unwrap()
    }

    #[test]
    fn test_channel_bitmap() {
        let channels = vec![Channel::Nfc, Channel::Ble];
        let bitmap = Channel::to_bitmap(&channels);
        assert_eq!(bitmap, 0b00000011);

        let recovered = Channel::from_bitmap(bitmap);
        assert_eq!(recovered.len(), 2);
        assert!(recovered.contains(&Channel::Nfc));
        assert!(recovered.contains(&Channel::Ble));
    }

    #[test]
    fn test_ephemeral_proof_generation() {
        let keybundle = test_keybundle();
        let proof_type = ProofType::AgeAtLeast { threshold: 18 };

        let (proof, binding) = EphemeralProof::generate(
            &keybundle,
            proof_type.clone(),
            Duration::from_secs(3600),
            vec![Channel::Nfc],
        )
        .expect("should generate proof");

        // Check proof fields
        assert_eq!(proof.v, 1);
        assert!(!proof.is_expired());
        assert!(proof.size() < 200);

        // Verify signature
        assert!(proof.verify_signature().is_ok());

        // Check binding matches
        assert!(binding.matches_proof(&proof));
    }

    #[test]
    fn test_proof_expiry() {
        let keybundle = test_keybundle();
        let proof_type = ProofType::NonRevocation;

        let (mut proof, _) =
            EphemeralProof::generate(&keybundle, proof_type, Duration::from_secs(60), vec![])
                .unwrap();

        // Proof should not be expired yet
        assert!(!proof.is_expired());

        // Set expiry to past
        proof.x = 1;
        assert!(proof.is_expired());
    }

    #[test]
    fn test_validity_too_long() {
        let keybundle = test_keybundle();
        let result = EphemeralProof::generate(
            &keybundle,
            ProofType::NonRevocation,
            Duration::from_secs(100000), // > MAX_VALIDITY
            vec![],
        );

        assert!(matches!(result, Err(EphemeralError::ValidityTooLong)));
    }

    #[test]
    fn test_verify_result() {
        let valid = VerifyResult::valid(1, ProofType::AgeAtLeast { threshold: 21 });
        assert!(valid.valid);
        assert_eq!(valid.level, 1);

        let invalid = VerifyResult::invalid("test error");
        assert!(!invalid.valid);
        assert!(invalid.warnings.contains(&"test error".to_string()));
    }
}
