//! Application-level signed message envelopes
//!
//! Provides message integrity, authenticity, and replay protection through
//! Ed25519 signatures and sequence number tracking.

use anyhow::{Context, Result};
use icn_identity::{Did, KeyPair};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Application-level signed message envelope
///
/// Every message sent between ICN nodes must be wrapped in a SignedEnvelope
/// to ensure:
/// - **Integrity**: Message cannot be tampered with
/// - **Authenticity**: Sender is proven to control the DID private key
/// - **Freshness**: Timestamp prevents replay of old messages
/// - **Ordering**: Sequence number enables replay detection
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

    /// Ed25519 signature over canonical encoding
    /// Signature = Sign_from(sequence || timestamp || payload_type || payload)
    pub signature: Vec<u8>,
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
    /// Create and sign a new envelope
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
            signature: Vec::new(),
        };

        // Compute signature over canonical encoding
        let sig_input = envelope.canonical_encoding();
        envelope.signature = keypair.sign(&sig_input).to_vec();

        Ok(envelope)
    }

    /// Verify signature and age
    ///
    /// Checks:
    /// 1. Ed25519 signature is valid for the sender's DID
    /// 2. Message is not older than max_age_secs
    ///
    /// Note: This does NOT check for replays (use ReplayGuard for that)
    pub fn verify(&self, max_age_secs: u64) -> Result<()> {
        // 1. Verify signature
        let sig_input = self.canonical_encoding();
        let verifying_key = self.from.to_verifying_key()
            .context("Failed to extract verifying key from DID")?;

        let signature = ed25519_dalek::Signature::from_slice(&self.signature)
            .context("Invalid signature format")?;

        use ed25519_dalek::Verifier;
        verifying_key
            .verify(&sig_input, &signature)
            .context("Signature verification failed")?;

        // 2. Verify age
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
            anyhow::bail!("Message from future: timestamp {} > now {}", self.timestamp, now);
        }

        Ok(())
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
        bincode::deserialize(&self.payload)
            .context("Failed to deserialize payload")
    }

    /// Serialize and create envelope for a typed payload
    pub fn from_payload<T: serde::Serialize>(
        from: &Did,
        keypair: &KeyPair,
        sequence: u64,
        payload_type: PayloadType,
        payload: &T,
    ) -> Result<Self> {
        let payload_bytes = bincode::serialize(payload)
            .context("Failed to serialize payload")?;
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

        let envelope = SignedEnvelope::from_payload(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            &msg,
        )
        .unwrap();

        // Should verify and decode
        assert!(envelope.verify(300).is_ok());
        let decoded: TestMessage = envelope.decode_payload().unwrap();
        assert_eq!(decoded, msg);
    }
}
