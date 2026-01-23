//! Application-level signed message envelopes
//!
//! Provides message integrity, authenticity, and replay protection through
//! Ed25519 signatures and sequence number tracking.
//!
//! ## Hybrid Post-Quantum Signatures
//!
//! When the `post-quantum` feature is enabled, envelopes can include both
//! Ed25519 (classical) and ML-DSA (post-quantum) signatures. The hybrid
//! signature model provides defense-in-depth against quantum threats.
//!
//! ### Verification Modes
//!
//! - **`verify()`**: Verifies Ed25519 signature. For hybrid envelopes, PQ
//!   verification is deferred (logged as warning) pending PQ key infrastructure.
//! - **`verify_with_pq_key()`**: Full hybrid verification requiring both
//!   Ed25519 AND ML-DSA signatures to pass. Use when PQ public key is available.
//!
//! ### Migration Path
//!
//! During the transition period, `verify()` accepts hybrid envelopes with only
//! classical verification to allow gradual infrastructure rollout. Once PQ key
//! distribution (via DID documents or Hello messages) is complete, callers
//! should use `verify_with_pq_key()` for full security.

use anyhow::{Context, Result};
use icn_identity::{Did, KeyPair};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Signature type discriminator for versioned envelope format
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum SignatureType {
    /// Classical Ed25519 signature only (64 bytes)
    /// This is the default for backward compatibility
    #[default]
    Classical = 0,
    /// Hybrid Ed25519 + ML-DSA signature (~3.4 KB total)
    /// Both signatures must verify for acceptance
    Hybrid = 1,
}

/// Application-level signed message envelope
///
/// Every message sent between ICN nodes must be wrapped in a SignedEnvelope
/// to ensure:
/// - **Integrity**: Message cannot be tampered with
/// - **Authenticity**: Sender is proven to control the DID private key
/// - **Freshness**: Timestamp prevents replay of old messages
/// - **Ordering**: Sequence number enables replay detection
///
/// ## Versioning
///
/// The envelope format supports both classical (Ed25519-only) and hybrid
/// (Ed25519 + ML-DSA) signatures. The `signature_type` field determines
/// which verification mode to use:
/// - `Classical`: Only `signature` field is present and verified
/// - `Hybrid`: Both `signature` and `pq_signature` must verify
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEnvelope {
    /// Sender DID (verified via signature)
    pub from: Did,

    /// Monotonic sequence number (per-sender)
    /// Increments with each message sent by this sender
    pub sequence: u64,

    /// Timestamp (milliseconds since Unix epoch)
    /// Used for age-based replay protection
    pub timestamp: u64,

    /// Payload type discriminator
    pub payload_type: PayloadType,

    /// Serialized payload bytes
    pub payload: Vec<u8>,

    /// Signature type (Classical or Hybrid)
    /// Defaults to Classical for backward compatibility with existing messages
    #[serde(default)]
    pub signature_type: SignatureType,

    /// Ed25519 signature over canonical encoding (always present)
    /// Signature = Sign_from(sequence || timestamp || payload_type || payload)
    pub signature: Vec<u8>,

    /// ML-DSA (post-quantum) signature (only present for Hybrid type)
    /// When present, both this and `signature` must verify
    /// Note: Using `#[serde(default)]` without `skip_serializing_if` for bincode compatibility
    #[serde(default)]
    pub pq_signature: Option<Vec<u8>>,
}

/// Message payload type discriminator
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum PayloadType {
    /// Gossip protocol message
    Gossip = 1,
    /// Ledger sync message
    Ledger = 2,
    /// Trust attestation message
    Trust = 3,
    /// Contract invocation message
    Contract = 4,
    /// RPC request/response
    Rpc = 5,
    /// Network control message (ping, handshake, etc)
    Control = 6,
    /// Encrypted payload (EncryptedEnvelope)
    /// Contains end-to-end encrypted application data
    Encrypted = 7,
}

impl SignedEnvelope {
    /// Create and sign a new envelope with classical Ed25519 signature
    ///
    /// This creates a backward-compatible envelope that uses only Ed25519.
    /// For post-quantum security, use `new_hybrid()` instead.
    ///
    /// # Arguments
    /// * `from` - Sender DID
    /// * `keypair` - Sender's keypair for signing
    /// * `sequence` - Monotonic sequence number for this sender
    /// * `payload_type` - Type of payload being sent
    /// * `payload` - Serialized payload bytes
    pub fn new(
        from: &Did,
        keypair: &KeyPair,
        sequence: u64,
        payload_type: PayloadType,
        payload: Vec<u8>,
    ) -> Result<Self> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("System time before Unix epoch")?
            .as_millis() as u64;

        let mut envelope = SignedEnvelope {
            from: from.clone(),
            sequence,
            timestamp,
            payload_type,
            payload,
            signature_type: SignatureType::Classical,
            signature: Vec::new(),
            pq_signature: None,
        };

        // Compute signature over canonical encoding
        let sig_input = envelope.canonical_encoding();
        envelope.signature = keypair.sign(&sig_input).to_vec();

        Ok(envelope)
    }

    /// Create and sign a new envelope with hybrid Ed25519 + ML-DSA signature
    ///
    /// This creates a post-quantum secure envelope that includes both classical
    /// and post-quantum signatures. Both signatures must verify for the message
    /// to be accepted.
    ///
    /// # Arguments
    /// * `from` - Sender DID
    /// * `keypair` - Sender's keypair (must have PQ keys)
    /// * `sequence` - Monotonic sequence number for this sender
    /// * `payload_type` - Type of payload being sent
    /// * `payload` - Serialized payload bytes
    ///
    /// # Errors
    /// Returns an error if the keypair doesn't have PQ keys enabled.
    #[cfg(feature = "post-quantum")]
    pub fn new_hybrid(
        from: &Did,
        keypair: &KeyPair,
        sequence: u64,
        payload_type: PayloadType,
        payload: Vec<u8>,
    ) -> Result<Self> {
        use icn_identity::HybridSignatureOrClassical;

        if !keypair.has_pq_keys() {
            anyhow::bail!("Keypair does not have post-quantum keys for hybrid signing");
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("System time before Unix epoch")?
            .as_millis() as u64;

        let mut envelope = SignedEnvelope {
            from: from.clone(),
            sequence,
            timestamp,
            payload_type,
            payload,
            signature_type: SignatureType::Hybrid,
            signature: Vec::new(),
            pq_signature: None,
        };

        // Compute hybrid signature over canonical encoding
        let sig_input = envelope.canonical_encoding();
        let hybrid_sig = keypair.sign_hybrid(&sig_input)?;

        match hybrid_sig {
            HybridSignatureOrClassical::Hybrid(sig) => {
                envelope.signature = sig.classical.clone();
                envelope.pq_signature = Some(sig.pq.clone());
            }
            HybridSignatureOrClassical::Classical(_) => {
                anyhow::bail!("Expected hybrid signature but got classical");
            }
        }

        Ok(envelope)
    }

    /// Create envelope with automatic signature type selection
    ///
    /// Uses hybrid signature if the keypair has PQ keys, otherwise classical.
    /// This is the recommended method for new code.
    #[cfg(feature = "post-quantum")]
    pub fn new_auto(
        from: &Did,
        keypair: &KeyPair,
        sequence: u64,
        payload_type: PayloadType,
        payload: Vec<u8>,
    ) -> Result<Self> {
        if keypair.is_hybrid() {
            Self::new_hybrid(from, keypair, sequence, payload_type, payload)
        } else {
            Self::new(from, keypair, sequence, payload_type, payload)
        }
    }

    /// Verify signature and age
    ///
    /// Checks:
    /// 1. Ed25519 signature is valid for the sender's DID
    /// 2. For hybrid signatures, ML-DSA signature is also verified
    /// 3. Message is not older than max_age_secs
    ///
    /// Note: This does NOT check for replays (use ReplayGuard for that)
    pub fn verify(&self, max_age_secs: u64) -> Result<()> {
        let sig_input = self.canonical_encoding();

        // 1. Always verify classical Ed25519 signature
        self.verify_classical(&sig_input)?;

        // 2. For hybrid envelopes, validate PQ signature format (deferred verification)
        // Full PQ verification requires the sender's PQ public key - use verify_with_pq_key()
        #[cfg(feature = "post-quantum")]
        if self.signature_type == SignatureType::Hybrid {
            self.verify_pq_deferred()?;
        }

        // 3. Verify age
        self.verify_age(max_age_secs)?;

        Ok(())
    }

    /// Verify classical Ed25519 signature
    fn verify_classical(&self, sig_input: &[u8]) -> Result<()> {
        let verifying_key = self
            .from
            .to_verifying_key()
            .context("Failed to extract verifying key from DID")?;

        let signature = ed25519_dalek::Signature::from_slice(&self.signature)
            .context("Invalid Ed25519 signature format")?;

        use ed25519_dalek::Verifier;
        verifying_key
            .verify(sig_input, &signature)
            .context("Ed25519 signature verification failed")?;

        Ok(())
    }

    /// Verify ML-DSA post-quantum signature (deferred mode)
    ///
    /// This is called when no PQ public key is available. It validates the
    /// signature format but defers actual cryptographic verification.
    ///
    /// For full hybrid security, use `verify_with_pq_key()` when the
    /// sender's PQ public key is available.
    #[cfg(feature = "post-quantum")]
    fn verify_pq_deferred(&self) -> Result<()> {
        let pq_sig = self
            .pq_signature
            .as_ref()
            .context("Hybrid envelope missing PQ signature")?;

        // ML-DSA-65 signatures are ~3309 bytes
        if pq_sig.len() < 3000 {
            anyhow::bail!(
                "Invalid ML-DSA signature: expected ~3309 bytes, got {}",
                pq_sig.len()
            );
        }

        // Log warning that PQ verification is deferred
        // This is acceptable during migration but should be addressed
        tracing::warn!(
            "Hybrid envelope from {} - PQ verification DEFERRED (key not available). \
             Use verify_with_pq_key() for full security.",
            self.from
        );

        Ok(())
    }

    /// Verify message age
    fn verify_age(&self, max_age_secs: u64) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("System time before Unix epoch")?
            .as_millis() as u64;

        let age_ms = now.saturating_sub(self.timestamp);
        let max_age_ms = max_age_secs * 1000;

        if age_ms > max_age_ms {
            anyhow::bail!("Message too old: {age_ms}ms (max {max_age_ms}ms)");
        }

        // Also check for messages from the future (clock skew)
        if self.timestamp > now + max_age_ms {
            anyhow::bail!(
                "Message from future: timestamp {} > now {}",
                self.timestamp,
                now
            );
        }

        Ok(())
    }

    /// Verify with explicit PQ public key (full hybrid verification)
    ///
    /// This performs full both-must-verify hybrid verification:
    /// 1. Ed25519 signature must be valid for sender's DID
    /// 2. ML-DSA signature must be valid for provided PQ public key
    /// 3. Message age must be within max_age_secs
    ///
    /// Use this when you have the sender's PQ public key available
    /// (e.g., from DID document cache or Hello message).
    ///
    /// # Arguments
    /// * `max_age_secs` - Maximum message age in seconds
    /// * `pq_public_key` - Sender's ML-DSA public key
    ///
    /// # Errors
    /// Returns error if Ed25519 or ML-DSA signature verification fails,
    /// or if message age exceeds the maximum.
    #[cfg(feature = "post-quantum")]
    pub fn verify_with_pq_key(
        &self,
        max_age_secs: u64,
        pq_public_key: &icn_crypto_pq::MlDsaPublicKey,
    ) -> Result<()> {
        let sig_input = self.canonical_encoding();

        // 1. Verify classical signature
        self.verify_classical(&sig_input)?;

        // 2. Verify PQ signature if hybrid
        if self.signature_type == SignatureType::Hybrid {
            let pq_sig_bytes = self
                .pq_signature
                .as_ref()
                .context("Hybrid envelope missing PQ signature")?;

            let pq_sig = icn_crypto_pq::MlDsaSignature::from_bytes(pq_sig_bytes)
                .map_err(|e| anyhow::anyhow!("Invalid ML-DSA signature format: {e}"))?;

            if !icn_crypto_pq::MlDsaKeypair::verify(pq_public_key, &sig_input, &pq_sig) {
                anyhow::bail!("ML-DSA signature verification failed");
            }
        }

        // 3. Verify age
        self.verify_age(max_age_secs)?;

        Ok(())
    }

    /// Check if this envelope uses hybrid signatures
    pub fn is_hybrid(&self) -> bool {
        self.signature_type == SignatureType::Hybrid
    }

    /// Canonical encoding for signature computation
    ///
    /// Format: sequence (8 bytes BE) || timestamp (8 bytes BE) || payload_type (1 byte) || payload
    fn canonical_encoding(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + 8 + 1 + self.payload.len());
        buf.extend_from_slice(&self.sequence.to_be_bytes());
        buf.extend_from_slice(&self.timestamp.to_be_bytes());
        buf.push(self.payload_type as u8);
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Deserialize payload as the specified type
    pub fn decode_payload<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        icn_encoding::decode(&self.payload).context("Failed to deserialize payload")
    }

    /// Serialize and create envelope for a typed payload
    pub fn from_payload<T: serde::Serialize>(
        from: &Did,
        keypair: &KeyPair,
        sequence: u64,
        payload_type: PayloadType,
        payload: &T,
    ) -> Result<Self> {
        let payload_bytes =
            icn_encoding::encode(payload).context("Failed to serialize payload")?;
        Self::new(from, keypair, sequence, payload_type, payload_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_envelope_signing_and_verification() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let envelope = SignedEnvelope::new(
            &did,
            &keypair,
            1,
            PayloadType::Gossip,
            b"test payload".to_vec(),
        )
        .unwrap();

        // Should verify successfully with generous age window
        assert!(envelope.verify(300).is_ok());
    }

    #[test]
    fn test_tampered_payload_rejected() {
        let keypair = KeyPair::generate().unwrap();
        let mut envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test payload".to_vec(),
        )
        .unwrap();

        // Tamper with payload
        envelope.payload[0] ^= 0xFF;

        // Verification should fail
        assert!(envelope.verify(300).is_err());
    }

    #[test]
    fn test_tampered_sequence_rejected() {
        let keypair = KeyPair::generate().unwrap();
        let mut envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // Tamper with sequence
        envelope.sequence = 2;

        // Verification should fail
        assert!(envelope.verify(300).is_err());
    }

    #[test]
    fn test_wrong_signer_rejected() {
        let keypair1 = KeyPair::generate().unwrap();
        let keypair2 = KeyPair::generate().unwrap();

        let envelope = SignedEnvelope::new(
            keypair2.did(), // Claim to be keypair2
            &keypair1,      // But sign with keypair1
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // Verification should fail (DID doesn't match signature)
        assert!(envelope.verify(300).is_err());
    }

    #[test]
    fn test_age_validation() {
        let keypair = KeyPair::generate().unwrap();
        let mut envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // Make message appear very old
        envelope.timestamp = 1000; // Very old timestamp

        // Re-sign with old timestamp
        let sig_input = envelope.canonical_encoding();
        envelope.signature = keypair.sign(&sig_input).to_vec();

        // Should be rejected as too old
        assert!(envelope.verify(60).is_err());
    }

    #[test]
    fn test_future_timestamp_rejected() {
        let keypair = KeyPair::generate().unwrap();
        let mut envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"test".to_vec(),
        )
        .unwrap();

        // Make message appear from far future
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        envelope.timestamp = now + 1_000_000; // 1000 seconds in future

        // Re-sign with future timestamp
        let sig_input = envelope.canonical_encoding();
        envelope.signature = keypair.sign(&sig_input).to_vec();

        // Should be rejected as from future
        assert!(envelope.verify(60).is_err());
    }

    #[test]
    fn test_decode_payload() {
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct TestMessage {
            value: u32,
            text: String,
        }

        let keypair = KeyPair::generate().unwrap();
        let msg = TestMessage {
            value: 42,
            text: "hello".to_string(),
        };

        let envelope =
            SignedEnvelope::from_payload(keypair.did(), &keypair, 1, PayloadType::Gossip, &msg)
                .unwrap();

        // Should verify and decode
        assert!(envelope.verify(300).is_ok());
        let decoded: TestMessage = envelope.decode_payload().unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_hybrid_envelope_creation_and_verification() {
        let keypair = KeyPair::generate().unwrap();

        // With post-quantum feature, generated keypairs should have PQ keys
        assert!(
            keypair.has_pq_keys(),
            "Generated keypair should have PQ keys"
        );

        // Create hybrid envelope
        let envelope = SignedEnvelope::new_hybrid(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"hybrid test".to_vec(),
        )
        .unwrap();

        // Should be hybrid type
        assert_eq!(envelope.signature_type, SignatureType::Hybrid);
        assert!(envelope.pq_signature.is_some());

        // PQ signature should be ~3309 bytes (ML-DSA-65)
        let pq_sig_len = envelope.pq_signature.as_ref().unwrap().len();
        assert!(
            pq_sig_len > 3000,
            "PQ signature should be ~3309 bytes, got {pq_sig_len}"
        );

        // Basic verify() should pass (with deferred PQ verification)
        assert!(envelope.verify(300).is_ok());

        // Full verification with PQ key should also pass
        let pq_public = keypair.pq_public_key().unwrap();
        assert!(envelope.verify_with_pq_key(300, &pq_public).is_ok());
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_hybrid_envelope_tampered_pq_sig_rejected() {
        let keypair = KeyPair::generate().unwrap();

        let mut envelope = SignedEnvelope::new_hybrid(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"hybrid test".to_vec(),
        )
        .unwrap();

        // Tamper with PQ signature
        if let Some(ref mut pq_sig) = envelope.pq_signature {
            pq_sig[100] ^= 0xFF;
        }

        // Full verification should fail
        let pq_public = keypair.pq_public_key().unwrap();
        let result = envelope.verify_with_pq_key(300, &pq_public);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ML-DSA"));
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_new_auto_uses_hybrid_for_pq_keypair() {
        let keypair = KeyPair::generate().unwrap();
        assert!(keypair.has_pq_keys());

        let envelope = SignedEnvelope::new_auto(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"auto test".to_vec(),
        )
        .unwrap();

        // Should auto-select hybrid for PQ-enabled keypair
        assert_eq!(envelope.signature_type, SignatureType::Hybrid);
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_classical_envelope_backward_compat() {
        let keypair = KeyPair::generate().unwrap();

        // Explicitly create classical envelope
        let envelope = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"classical test".to_vec(),
        )
        .unwrap();

        // Should be classical type
        assert_eq!(envelope.signature_type, SignatureType::Classical);
        assert!(envelope.pq_signature.is_none());

        // Should verify with basic verify()
        assert!(envelope.verify(300).is_ok());

        // Should also work with verify_with_pq_key() (skips PQ check for classical)
        let pq_public = keypair.pq_public_key().unwrap();
        assert!(envelope.verify_with_pq_key(300, &pq_public).is_ok());
    }
}
