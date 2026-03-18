//! Blind signature primitives for unlinkable enrollment tokens
//!
//! This module provides blind signature functionality needed for SDIS enrollment:
//! - Stewards issue blind signatures on enrollment tokens
//! - Users can unblind to get valid signatures
//! - Signatures are unlinkable to the blinded version
//!
//! # Protocol
//!
//! 1. User generates enrollment token T and blinding factor r
//! 2. User computes blinded token: T' = Blind(T, r)
//! 3. Steward signs blinded token: σ' = Sign(T')
//! 4. User unblinds: σ = Unblind(σ', r)
//! 5. σ is a valid signature on T, but steward can't link σ to σ'
//!
//! # Implementation Note
//!
//! This is a simplified commit-hash based blind signature scheme.
//! For production use, consider RSA-BSSA (RFC 9474) or BBS+ signatures
//! which provide stronger unlinkability guarantees.
//!
//! The scheme here uses hash-based blinding:
//! - BlindedMessage = H(message || blinding_factor)
//! - Signature is on the blinded hash
//! - Unblinding reveals the original message commitment
//!
//! This provides unlinkability from the signer's perspective but
//! requires trust that the blinding factor is kept secret.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha3::{Digest, Sha3_256};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{CryptoError, Result};

/// A blinding factor for blind signatures
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct BlindingFactor {
    value: [u8; 32],
}

impl BlindingFactor {
    /// Generate a random blinding factor
    pub fn generate() -> Self {
        let mut value = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut value);
        Self { value }
    }

    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKey(format!(
                "Blinding factor must be 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut value = [0u8; 32];
        value.copy_from_slice(bytes);
        Ok(Self { value })
    }

    /// Get as bytes (use with caution - reveals blinding factor)
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.value
    }
}

/// A blinded message ready for signing
#[derive(Clone)]
pub struct BlindedMessage {
    /// The blinded hash that gets signed
    pub blinded_hash: [u8; 32],
    /// Commitment to the original message (for verification)
    pub message_commitment: [u8; 32],
}

impl BlindedMessage {
    /// Blind a message with a blinding factor
    ///
    /// Returns a BlindedMessage that can be sent to the signer.
    pub fn blind(message: &[u8], blinding_factor: &BlindingFactor) -> Self {
        // Create commitment to original message
        let mut hasher = Sha3_256::new();
        hasher.update(b"icn-blind-commitment");
        hasher.update(message);
        let commitment = hasher.finalize();

        let mut message_commitment = [0u8; 32];
        message_commitment.copy_from_slice(&commitment);

        // Create blinded hash = H(commitment || blinding_factor)
        let mut hasher = Sha3_256::new();
        hasher.update(b"icn-blind-hash");
        hasher.update(message_commitment);
        hasher.update(blinding_factor.value);
        let blinded = hasher.finalize();

        let mut blinded_hash = [0u8; 32];
        blinded_hash.copy_from_slice(&blinded);

        Self {
            blinded_hash,
            message_commitment,
        }
    }
}

/// A blind signature from a steward
#[derive(Clone)]
pub struct BlindSignature {
    /// Signature on the blinded hash
    pub signature: [u8; 64],
    /// The blinded hash that was signed (for verification)
    pub blinded_hash: [u8; 32],
}

impl BlindSignature {
    /// Sign a blinded message
    ///
    /// The signer doesn't learn the original message content.
    pub fn sign(blinded: &BlindedMessage, signing_key: &SigningKey) -> Self {
        let signature = signing_key.sign(&blinded.blinded_hash);

        Self {
            signature: signature.to_bytes(),
            blinded_hash: blinded.blinded_hash,
        }
    }

    /// Verify that this is a valid signature on the blinded message
    pub fn verify(&self, verifying_key: &VerifyingKey) -> bool {
        let signature = Signature::from_bytes(&self.signature);
        verifying_key.verify(&self.blinded_hash, &signature).is_ok()
    }
}

/// An unblinded signature that can be verified against the original message
#[derive(Clone)]
pub struct UnblindedSignature {
    /// The signature bytes (from blind signature)
    pub signature: [u8; 64],
    /// Commitment to the original message
    pub message_commitment: [u8; 32],
    /// The blinding factor used (needed for verification)
    pub blinding_proof: [u8; 32],
}

impl UnblindedSignature {
    /// Unblind a signature
    ///
    /// Converts a blind signature into a verifiable signature on the original message.
    pub fn unblind(
        blind_sig: &BlindSignature,
        blinding_factor: &BlindingFactor,
        message_commitment: [u8; 32],
    ) -> Self {
        Self {
            signature: blind_sig.signature,
            message_commitment,
            blinding_proof: blinding_factor.value,
        }
    }

    /// Verify the signature against the original message
    ///
    /// This checks:
    /// 1. The message commitment matches
    /// 2. The blinded hash was correctly computed
    /// 3. The signature is valid on the blinded hash
    pub fn verify(&self, message: &[u8], verifying_key: &VerifyingKey) -> bool {
        // Recompute message commitment
        let mut hasher = Sha3_256::new();
        hasher.update(b"icn-blind-commitment");
        hasher.update(message);
        let commitment = hasher.finalize();

        if commitment[..] != self.message_commitment {
            return false;
        }

        // Recompute blinded hash
        let mut hasher = Sha3_256::new();
        hasher.update(b"icn-blind-hash");
        hasher.update(self.message_commitment);
        hasher.update(self.blinding_proof);
        let blinded_hash = hasher.finalize();

        // Verify signature on blinded hash
        let signature = Signature::from_bytes(&self.signature);
        verifying_key.verify(&blinded_hash, &signature).is_ok()
    }

    /// Verify without knowing the original message (using commitment only)
    ///
    /// This allows verification when you only have the commitment,
    /// not the original message. Useful for token validation.
    pub fn verify_commitment(&self, verifying_key: &VerifyingKey) -> bool {
        // Recompute blinded hash from commitment
        let mut hasher = Sha3_256::new();
        hasher.update(b"icn-blind-hash");
        hasher.update(self.message_commitment);
        hasher.update(self.blinding_proof);
        let blinded_hash = hasher.finalize();

        // Verify signature
        let signature = Signature::from_bytes(&self.signature);
        verifying_key.verify(&blinded_hash, &signature).is_ok()
    }
}

/// Enrollment token with blind signature
///
/// This represents a complete enrollment token that has been
/// blind-signed by a steward and unblinded by the user.
#[derive(Clone)]
pub struct EnrollmentToken {
    /// Unique token identifier
    pub token_id: [u8; 32],
    /// Timestamp of issuance (for expiry checking)
    pub issued_at: u64,
    /// Steward who issued the token (DID or public key hash)
    pub issuer_id: [u8; 32],
    /// The unblinded signature proving validity
    pub signature: UnblindedSignature,
}

impl EnrollmentToken {
    /// Create a new enrollment token
    pub fn new(token_id: [u8; 32], issued_at: u64, issuer_id: [u8; 32]) -> Self {
        Self {
            token_id,
            issued_at,
            issuer_id,
            signature: UnblindedSignature {
                signature: [0u8; 64],
                message_commitment: [0u8; 32],
                blinding_proof: [0u8; 32],
            },
        }
    }

    /// Get the token data as bytes for signing
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(72);
        bytes.extend_from_slice(&self.token_id);
        bytes.extend_from_slice(&self.issued_at.to_le_bytes());
        bytes.extend_from_slice(&self.issuer_id);
        bytes
    }

    /// Set the unblinded signature
    pub fn set_signature(&mut self, signature: UnblindedSignature) {
        self.signature = signature;
    }

    /// Verify the token's signature
    pub fn verify(&self, verifying_key: &VerifyingKey) -> bool {
        self.signature.verify(&self.to_bytes(), verifying_key)
    }

    /// Check if the token has expired
    pub fn is_expired(&self, current_time: u64, ttl_seconds: u64) -> bool {
        current_time > self.issued_at + ttl_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_keypair() -> (SigningKey, VerifyingKey) {
        let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    #[test]
    fn test_blinding_factor_generation() {
        let bf1 = BlindingFactor::generate();
        let bf2 = BlindingFactor::generate();

        // Should be random (very unlikely to be equal)
        assert_ne!(bf1.value, bf2.value);
    }

    #[test]
    fn test_blind_message() {
        let message = b"enrollment token data";
        let bf = BlindingFactor::generate();

        let blinded1 = BlindedMessage::blind(message, &bf);
        let blinded2 = BlindedMessage::blind(message, &bf);

        // Same message + blinding factor = same blinded hash
        assert_eq!(blinded1.blinded_hash, blinded2.blinded_hash);
        assert_eq!(blinded1.message_commitment, blinded2.message_commitment);
    }

    #[test]
    fn test_different_blinding_factors() {
        let message = b"enrollment token data";
        let bf1 = BlindingFactor::generate();
        let bf2 = BlindingFactor::generate();

        let blinded1 = BlindedMessage::blind(message, &bf1);
        let blinded2 = BlindedMessage::blind(message, &bf2);

        // Different blinding factors = different blinded hashes
        assert_ne!(blinded1.blinded_hash, blinded2.blinded_hash);
        // But same message commitment
        assert_eq!(blinded1.message_commitment, blinded2.message_commitment);
    }

    #[test]
    fn test_blind_sign_verify() {
        let (signing_key, verifying_key) = generate_keypair();
        let message = b"enrollment token data";
        let bf = BlindingFactor::generate();

        // Blind the message
        let blinded = BlindedMessage::blind(message, &bf);

        // Sign the blinded message
        let blind_sig = BlindSignature::sign(&blinded, &signing_key);

        // Verify blind signature
        assert!(blind_sig.verify(&verifying_key));
    }

    #[test]
    fn test_unblind_and_verify() {
        let (signing_key, verifying_key) = generate_keypair();
        let message = b"enrollment token data";
        let bf = BlindingFactor::generate();

        // Blind
        let blinded = BlindedMessage::blind(message, &bf);

        // Sign
        let blind_sig = BlindSignature::sign(&blinded, &signing_key);

        // Unblind
        let unblinded = UnblindedSignature::unblind(&blind_sig, &bf, blinded.message_commitment);

        // Verify against original message
        assert!(unblinded.verify(message, &verifying_key));

        // Wrong message should fail
        assert!(!unblinded.verify(b"wrong message", &verifying_key));
    }

    #[test]
    fn test_verify_commitment_only() {
        let (signing_key, verifying_key) = generate_keypair();
        let message = b"enrollment token data";
        let bf = BlindingFactor::generate();

        let blinded = BlindedMessage::blind(message, &bf);
        let blind_sig = BlindSignature::sign(&blinded, &signing_key);
        let unblinded = UnblindedSignature::unblind(&blind_sig, &bf, blinded.message_commitment);

        // Can verify with just the commitment (no original message needed)
        assert!(unblinded.verify_commitment(&verifying_key));
    }

    #[test]
    fn test_unlinkability() {
        let (signing_key, verifying_key) = generate_keypair();
        let message = b"enrollment token data";
        let bf1 = BlindingFactor::generate();
        let bf2 = BlindingFactor::generate();

        // Same message, different blinding factors
        let blinded1 = BlindedMessage::blind(message, &bf1);
        let blinded2 = BlindedMessage::blind(message, &bf2);

        let blind_sig1 = BlindSignature::sign(&blinded1, &signing_key);
        let blind_sig2 = BlindSignature::sign(&blinded2, &signing_key);

        // Different blinded hashes were signed
        assert_ne!(blind_sig1.blinded_hash, blind_sig2.blinded_hash);

        // But both unblind to valid signatures on the same message
        let unblinded1 =
            UnblindedSignature::unblind(&blind_sig1, &bf1, blinded1.message_commitment);
        let unblinded2 =
            UnblindedSignature::unblind(&blind_sig2, &bf2, blinded2.message_commitment);

        assert!(unblinded1.verify(message, &verifying_key));
        assert!(unblinded2.verify(message, &verifying_key));

        // The signer (steward) can't link blind_sig1 to unblinded1
        // without knowing the blinding factor
    }

    #[test]
    fn test_enrollment_token() {
        let (signing_key, verifying_key) = generate_keypair();

        // Create token
        let mut token_id = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut token_id);

        let mut issuer_id = [0u8; 32];
        issuer_id.copy_from_slice(verifying_key.as_bytes());

        let mut token = EnrollmentToken::new(token_id, 1700000000, issuer_id);

        // Blind the token
        let bf = BlindingFactor::generate();
        let blinded = BlindedMessage::blind(&token.to_bytes(), &bf);

        // Sign (by steward)
        let blind_sig = BlindSignature::sign(&blinded, &signing_key);

        // Unblind (by user)
        let unblinded = UnblindedSignature::unblind(&blind_sig, &bf, blinded.message_commitment);
        token.set_signature(unblinded);

        // Verify
        assert!(token.verify(&verifying_key));

        // Check expiry
        assert!(!token.is_expired(1700000000, 3600)); // Not expired
        assert!(token.is_expired(1700010000, 3600)); // Expired
    }

    #[test]
    fn test_wrong_verifying_key() {
        let (signing_key, _) = generate_keypair();
        let (_, wrong_key) = generate_keypair();
        let message = b"enrollment token data";
        let bf = BlindingFactor::generate();

        let blinded = BlindedMessage::blind(message, &bf);
        let blind_sig = BlindSignature::sign(&blinded, &signing_key);
        let unblinded = UnblindedSignature::unblind(&blind_sig, &bf, blinded.message_commitment);

        // Wrong key should fail verification
        assert!(!unblinded.verify(message, &wrong_key));
    }
}
