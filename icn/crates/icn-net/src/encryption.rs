//! End-to-end message encryption using X25519 key exchange and ChaCha20-Poly1305 AEAD
//!
//! This module provides payload confidentiality on top of the SignedEnvelope authentication layer.
//!
//! ## Architecture
//!
//! ```text
//! App Layer:      EncryptedEnvelope (this module)
//! Message Layer:  SignedEnvelope (authentication)
//! Transport:      QUIC/TLS 1.3 (channel encryption)
//! ```
//!
//! ## Encryption Schemes
//!
//! ### Classical (Default)
//! - **Key Exchange**: X25519 static ECDH (reused across messages)
//! - **Symmetric Cipher**: ChaCha20-Poly1305 AEAD
//! - **Nonce**: Derived from sequence number (no transmission overhead)
//! - **Key Derivation**: Shared secret = x25519(sender_secret, recipient_public)
//!
//! ### Hybrid Post-Quantum (Feature-gated)
//! - **Key Exchange**: X25519 + ML-KEM-768 (both combined via HKDF)
//! - **Symmetric Cipher**: ChaCha20-Poly1305 AEAD
//! - **Security**: Defense-in-depth against quantum threats
//! - **Overhead**: ~1.2KB additional ciphertext for ML-KEM
//!
//! ## Security Properties
//!
//! - Payload confidentiality (gossip nodes can't read content)
//! - Authenticated encryption (tampering detected)
//! - Nonce uniqueness (derived from monotonic sequence)
//! - Replay protection (inherited from SignedEnvelope)
//!
//! ## Security Considerations
//!
//! ### Nonce Uniqueness (Sequence Persistence)
//!
//! Nonces are derived from: `SHA256(sequence || from_did || to_did)`.
//! To prevent nonce reuse across restarts, use [`OutgoingSequenceTracker`] which:
//!
//! - Persists sequence numbers to disk via [`icn_store::Store`]
//! - Applies a safety gap (+10,000) on restart to handle any unsaved sequences
//! - Provides per-(sender, recipient) sequence isolation
//!
//! **IMPORTANT**: Call `load_and_apply_safety_gap()` during node startup,
//! AFTER the store is initialized but BEFORE sending any encrypted messages.
//! This ensures the safety gap is applied exactly once per restart.
//!
//! **Usage**:
//! ```rust,ignore
//! use icn_net::{OutgoingSequenceTracker, EncryptedEnvelope};
//!
//! // During node startup (once per restart)
//! let tracker = OutgoingSequenceTracker::new(store)?;
//! tracker.load_and_apply_safety_gap().await?; // Apply safety gap
//!
//! // When sending encrypted messages
//! let seq = tracker.next_sequence(&my_did, &recipient_did).await?;
//! let envelope = EncryptedEnvelope::encrypt(&my_did, &recipient_did, seq, ...)?;
//! ```
//!
//! **Additional Protections**:
//! - TLS provides independent transport-layer encryption
//! - SignedEnvelope has separate replay protection
//!
//! [`OutgoingSequenceTracker`]: crate::sequence_tracker::OutgoingSequenceTracker
//!
//! ## Integration Status
//!
//! The `EncryptedEnvelope` is fully integrated into `NetworkActor`:
//! - `NetworkActor::send_encrypted()` - Sends encrypted messages
//! - `handlers::encrypted` - Processes incoming encrypted envelopes
//! - `OutgoingSequenceTracker` - Manages sequence numbers with persistence
//!
//! See [`crate::actor::NetworkActor`] for the full integration.
//!
//! ### Related Issues
//!
//! - #404 (closed): Initial wiring into NetworkActor
//! - #412 (closed): Documentation of integration TODO
//!
//! ## Limitations
//!
//! - No Perfect Forward Secrecy (static keys, add in Phase 11)
//! - Metadata visible (sender/recipient DIDs not hidden)
//! - Requires X25519 public key exchange before encryption

use anyhow::{Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

#[cfg(feature = "post-quantum")]
use icn_crypto_pq::{HybridKemCiphertext, HybridKemKeypair, HybridKemPublicKey};

/// Encryption type discriminator for versioned envelope support
///
/// Allows backward compatibility with classical-only encryption while
/// enabling hybrid post-quantum encryption for new messages.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum EncryptionType {
    /// Classical X25519 + ChaCha20-Poly1305 only (64 bytes overhead)
    #[default]
    Classical = 0,
    /// Hybrid X25519 + ML-KEM-768 + ChaCha20-Poly1305 (~1.1KB overhead)
    Hybrid = 1,
}

/// Encrypted message envelope
///
/// Contains encrypted payload with metadata needed for decryption.
/// The payload is encrypted using ChaCha20-Poly1305 with a shared secret
/// derived from X25519 key exchange (classical) or X25519 + ML-KEM (hybrid).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEnvelope {
    /// Sender's DID (public, for key lookup)
    pub from: Did,

    /// Recipient's DID (public, for key lookup)
    pub to: Did,

    /// Sequence number (for nonce derivation and replay protection)
    pub sequence: u64,

    /// Encryption type (Classical or Hybrid)
    #[serde(default)]
    pub encryption_type: EncryptionType,

    /// Encrypted payload (includes Poly1305 authentication tag)
    pub ciphertext: Vec<u8>,

    /// Hybrid KEM ciphertext (only present when encryption_type = Hybrid)
    /// Contains X25519 ephemeral + ML-KEM ciphertext (~1.1KB)
    /// Note: Using `#[serde(default)]` without `skip_serializing_if` for bincode compatibility
    #[serde(default)]
    pub pq_ciphertext: Option<Vec<u8>>,
}

impl EncryptedEnvelope {
    /// Encrypt a message payload
    ///
    /// # Arguments
    ///
    /// * `from` - Sender's DID
    /// * `to` - Recipient's DID
    /// * `sequence` - Message sequence number (must be unique per sender-recipient pair)
    /// * `sender_secret` - Sender's X25519 private key
    /// * `recipient_public` - Recipient's X25519 public key
    /// * `plaintext` - Message payload to encrypt
    ///
    /// # Returns
    ///
    /// Encrypted envelope with ciphertext and metadata
    ///
    /// # Example
    ///
    /// ```ignore
    /// let envelope = EncryptedEnvelope::encrypt(
    ///     &sender_did,
    ///     &recipient_did,
    ///     sequence,
    ///     &sender_x25519_secret,
    ///     &recipient_x25519_public,
    ///     b"secret message",
    /// )?;
    /// ```
    pub fn encrypt(
        from: &Did,
        to: &Did,
        sequence: u64,
        sender_secret: &StaticSecret,
        recipient_public: &PublicKey,
        plaintext: &[u8],
    ) -> Result<Self> {
        // Derive shared secret using X25519 ECDH
        let shared_secret = sender_secret.diffie_hellman(recipient_public);

        // Derive encryption key from shared secret
        let key_bytes = Zeroizing::new(derive_encryption_key(shared_secret.as_bytes()));
        let cipher = ChaCha20Poly1305::new((&*key_bytes).into());

        // Derive nonce from sequence number and DIDs
        let nonce = derive_nonce(sequence, from, to)?;

        // Encrypt with AEAD (includes authentication tag)
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {e}"))?;

        Ok(EncryptedEnvelope {
            from: from.clone(),
            to: to.clone(),
            sequence,
            encryption_type: EncryptionType::Classical,
            ciphertext,
            pq_ciphertext: None,
        })
    }

    /// Decrypt the message payload
    ///
    /// # Arguments
    ///
    /// * `recipient_secret` - Recipient's X25519 private key
    /// * `sender_public` - Sender's X25519 public key
    ///
    /// # Returns
    ///
    /// Decrypted plaintext payload
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Shared secret derivation fails
    /// - Nonce derivation fails
    /// - Decryption fails (wrong key, tampering, or authentication failure)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let plaintext = envelope.decrypt(
    ///     &recipient_x25519_secret,
    ///     &sender_x25519_public,
    /// )?;
    /// ```
    pub fn decrypt(
        &self,
        recipient_secret: &StaticSecret,
        sender_public: &PublicKey,
    ) -> Result<Vec<u8>> {
        // Derive shared secret using X25519 ECDH
        let shared_secret = recipient_secret.diffie_hellman(sender_public);

        // Derive encryption key from shared secret
        let key_bytes = Zeroizing::new(derive_encryption_key(shared_secret.as_bytes()));
        let cipher = ChaCha20Poly1305::new((&*key_bytes).into());

        // Derive nonce from sequence number and DIDs
        let nonce = derive_nonce(self.sequence, &self.from, &self.to)?;

        // Decrypt and verify authentication tag
        let plaintext = cipher
            .decrypt(&nonce, self.ciphertext.as_ref())
            .map_err(|e| anyhow::anyhow!("Decryption failed: {e}"))?;

        Ok(plaintext)
    }

    /// Get the size of the encrypted message in bytes
    pub fn size(&self) -> usize {
        // Approximate: DID strings + sequence + ciphertext + overhead
        let base_size =
            self.from.as_str().len() + self.to.as_str().len() + 8 + self.ciphertext.len() + 50;
        // Add PQ ciphertext size if present (~1.1KB for ML-KEM)
        base_size + self.pq_ciphertext.as_ref().map_or(0, |ct| ct.len())
    }

    /// Check if this envelope uses hybrid encryption
    pub fn is_hybrid(&self) -> bool {
        matches!(self.encryption_type, EncryptionType::Hybrid)
    }

    /// Encrypt a message using hybrid X25519 + ML-KEM encryption
    ///
    /// This provides defense-in-depth against quantum attacks by combining:
    /// - X25519 ECDH (classical security)
    /// - ML-KEM-768 key encapsulation (post-quantum security)
    ///
    /// Both shared secrets are combined via HKDF before deriving the
    /// symmetric encryption key.
    ///
    /// # Arguments
    ///
    /// * `from` - Sender's DID
    /// * `to` - Recipient's DID
    /// * `sequence` - Message sequence number (must be unique per sender-recipient pair)
    /// * `recipient_public` - Recipient's hybrid public key (X25519 + ML-KEM)
    /// * `plaintext` - Message payload to encrypt
    ///
    /// # Returns
    ///
    /// Encrypted envelope with hybrid ciphertext
    #[cfg(feature = "post-quantum")]
    pub fn encrypt_hybrid(
        from: &Did,
        to: &Did,
        sequence: u64,
        recipient_public: &HybridKemPublicKey,
        plaintext: &[u8],
    ) -> Result<Self> {
        // Use hybrid KEM to encapsulate a shared secret
        // Context includes sender and recipient DIDs for domain separation
        let context = format!("ICN-HYBRID-KEM:{}:{}", from.as_str(), to.as_str());
        let (shared_secret, kem_ciphertext) =
            HybridKemKeypair::encapsulate(recipient_public, context.as_bytes())
                .map_err(|e| anyhow::anyhow!("Hybrid KEM encapsulation failed: {e}"))?;

        // Derive encryption key from the hybrid shared secret
        let key_bytes = Zeroizing::new(derive_encryption_key(&shared_secret));
        let cipher = ChaCha20Poly1305::new((&*key_bytes).into());

        // Derive nonce from sequence number and DIDs
        let nonce = derive_nonce(sequence, from, to)?;

        // Encrypt with AEAD (includes authentication tag)
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {e}"))?;

        Ok(EncryptedEnvelope {
            from: from.clone(),
            to: to.clone(),
            sequence,
            encryption_type: EncryptionType::Hybrid,
            ciphertext,
            pq_ciphertext: Some(kem_ciphertext.to_bytes()),
        })
    }

    /// Decrypt a hybrid-encrypted message
    ///
    /// Requires the recipient's hybrid keypair to decapsulate both the
    /// X25519 and ML-KEM shared secrets.
    ///
    /// # Arguments
    ///
    /// * `recipient_keypair` - Recipient's hybrid keypair
    ///
    /// # Returns
    ///
    /// Decrypted plaintext payload
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Envelope is not hybrid-encrypted
    /// - PQ ciphertext is missing
    /// - Decapsulation fails
    /// - Decryption fails (wrong key, tampering, or authentication failure)
    #[cfg(feature = "post-quantum")]
    pub fn decrypt_hybrid(&self, recipient_keypair: &HybridKemKeypair) -> Result<Vec<u8>> {
        // Verify this is a hybrid envelope
        if !self.is_hybrid() {
            return Err(anyhow::anyhow!(
                "Cannot decrypt: envelope uses classical encryption, not hybrid"
            ));
        }

        // Get PQ ciphertext
        let pq_ciphertext_bytes = self.pq_ciphertext.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Missing PQ ciphertext for hybrid-encrypted envelope")
        })?;

        // Parse the KEM ciphertext
        let kem_ciphertext = HybridKemCiphertext::from_bytes(pq_ciphertext_bytes)
            .map_err(|e| anyhow::anyhow!("Invalid hybrid KEM ciphertext: {e}"))?;

        // Decapsulate to recover the shared secret
        let context = format!("ICN-HYBRID-KEM:{}:{}", self.from.as_str(), self.to.as_str());
        let shared_secret = recipient_keypair
            .decapsulate(&kem_ciphertext, context.as_bytes())
            .map_err(|e| anyhow::anyhow!("Hybrid KEM decapsulation failed: {e}"))?;

        // Derive encryption key from the hybrid shared secret
        let key_bytes = Zeroizing::new(derive_encryption_key(&shared_secret));
        let cipher = ChaCha20Poly1305::new((&*key_bytes).into());

        // Derive nonce from sequence number and DIDs
        let nonce = derive_nonce(self.sequence, &self.from, &self.to)?;

        // Decrypt and verify authentication tag
        let plaintext = cipher
            .decrypt(&nonce, self.ciphertext.as_ref())
            .map_err(|e| anyhow::anyhow!("Decryption failed: {e}"))?;

        Ok(plaintext)
    }

    /// Automatically select encryption type and encrypt
    ///
    /// Uses hybrid encryption if a PQ public key is provided, otherwise
    /// falls back to classical X25519 encryption.
    ///
    /// # Arguments
    ///
    /// * `from` - Sender's DID
    /// * `to` - Recipient's DID
    /// * `sequence` - Message sequence number
    /// * `sender_secret` - Sender's X25519 private key (for classical)
    /// * `recipient_public` - Recipient's X25519 public key (for classical)
    /// * `recipient_pq_public` - Optional recipient's hybrid public key (for hybrid)
    /// * `plaintext` - Message payload to encrypt
    #[cfg(feature = "post-quantum")]
    pub fn encrypt_auto(
        from: &Did,
        to: &Did,
        sequence: u64,
        sender_secret: &StaticSecret,
        recipient_public: &PublicKey,
        recipient_pq_public: Option<&HybridKemPublicKey>,
        plaintext: &[u8],
    ) -> Result<Self> {
        match recipient_pq_public {
            Some(pq_pk) => Self::encrypt_hybrid(from, to, sequence, pq_pk, plaintext),
            None => Self::encrypt(
                from,
                to,
                sequence,
                sender_secret,
                recipient_public,
                plaintext,
            ),
        }
    }
}

/// Derive a 256-bit encryption key from the shared secret
///
/// Uses SHA-256 to hash the raw X25519 shared secret into a suitable
/// ChaCha20-Poly1305 key (32 bytes).
///
/// # Arguments
///
/// * `shared_secret` - 32-byte X25519 shared secret
///
/// # Returns
///
/// 32-byte encryption key
fn derive_encryption_key(shared_secret: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"ICN-PAYLOAD-ENCRYPTION-V1");
    hasher.update(shared_secret);
    hasher.finalize().into()
}

/// Derive a unique 96-bit nonce for ChaCha20-Poly1305
///
/// The nonce is derived deterministically from:
/// - Sequence number (monotonically increasing, prevents reuse)
/// - Sender DID (ensures uniqueness across senders)
/// - Recipient DID (ensures uniqueness across recipients)
///
/// This eliminates the need to transmit nonces, saving bandwidth.
///
/// # Arguments
///
/// * `sequence` - Message sequence number
/// * `from` - Sender's DID
/// * `to` - Recipient's DID
///
/// # Returns
///
/// 12-byte nonce for ChaCha20-Poly1305
///
/// # Security
///
/// - Nonces must NEVER be reused with the same key
/// - Monotonic sequence ensures uniqueness
/// - DID inclusion prevents cross-conversation reuse
fn derive_nonce(sequence: u64, from: &Did, to: &Did) -> Result<Nonce> {
    let mut hasher = Sha256::new();
    hasher.update(b"ICN-NONCE-V1");
    hasher.update(sequence.to_be_bytes());
    hasher.update(from.as_str().as_bytes());
    hasher.update(to.as_str().as_bytes());
    let hash = hasher.finalize();

    // Take first 12 bytes (96 bits) for ChaCha20-Poly1305 nonce
    let nonce_bytes: [u8; 12] = hash[..12]
        .try_into()
        .context("Failed to derive nonce from hash")?;

    Ok(Nonce::from(nonce_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use rand_core::OsRng;

    fn generate_x25519_keypair() -> (StaticSecret, PublicKey) {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        (secret, public)
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        // Generate keypairs
        let (alice_secret, alice_public) = generate_x25519_keypair();
        let (bob_secret, bob_public) = generate_x25519_keypair();

        // Create DIDs
        let alice_did = KeyPair::generate().unwrap().did().clone();
        let bob_did = KeyPair::generate().unwrap().did().clone();

        // Encrypt message from Alice to Bob
        let plaintext = b"Hello Bob, this is a secret message!";
        let envelope = EncryptedEnvelope::encrypt(
            &alice_did,
            &bob_did,
            1,
            &alice_secret,
            &bob_public,
            plaintext,
        )
        .unwrap();

        // Bob decrypts the message
        let decrypted = envelope.decrypt(&bob_secret, &alice_public).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_recipient_fails() {
        // Generate keypairs
        let (alice_secret, alice_public) = generate_x25519_keypair();
        let (_bob_secret, bob_public) = generate_x25519_keypair();
        let (charlie_secret, _charlie_public) = generate_x25519_keypair();

        // Create DIDs
        let alice_did = KeyPair::generate().unwrap().did().clone();
        let bob_did = KeyPair::generate().unwrap().did().clone();

        // Encrypt message from Alice to Bob
        let plaintext = b"Hello Bob!";
        let envelope = EncryptedEnvelope::encrypt(
            &alice_did,
            &bob_did,
            1,
            &alice_secret,
            &bob_public,
            plaintext,
        )
        .unwrap();

        // Charlie tries to decrypt (should fail)
        let result = envelope.decrypt(&charlie_secret, &alice_public);
        assert!(result.is_err());
    }

    #[test]
    fn test_tampering_detected() {
        // Generate keypairs
        let (alice_secret, alice_public) = generate_x25519_keypair();
        let (bob_secret, bob_public) = generate_x25519_keypair();

        // Create DIDs
        let alice_did = KeyPair::generate().unwrap().did().clone();
        let bob_did = KeyPair::generate().unwrap().did().clone();

        // Encrypt message
        let plaintext = b"Original message";
        let mut envelope = EncryptedEnvelope::encrypt(
            &alice_did,
            &bob_did,
            1,
            &alice_secret,
            &bob_public,
            plaintext,
        )
        .unwrap();

        // Tamper with ciphertext
        envelope.ciphertext[0] ^= 0xFF;

        // Decryption should fail (Poly1305 authentication fails)
        let result = envelope.decrypt(&bob_secret, &alice_public);
        assert!(result.is_err());
    }

    #[test]
    fn test_nonce_uniqueness() {
        let alice_did = KeyPair::generate().unwrap().did().clone();
        let bob_did = KeyPair::generate().unwrap().did().clone();

        // Different sequences produce different nonces
        let nonce1 = derive_nonce(1, &alice_did, &bob_did).unwrap();
        let nonce2 = derive_nonce(2, &alice_did, &bob_did).unwrap();
        assert_ne!(nonce1, nonce2);

        // Different senders produce different nonces
        let charlie_did = KeyPair::generate().unwrap().did().clone();
        let nonce3 = derive_nonce(1, &charlie_did, &bob_did).unwrap();
        assert_ne!(nonce1, nonce3);

        // Different recipients produce different nonces
        let nonce4 = derive_nonce(1, &alice_did, &charlie_did).unwrap();
        assert_ne!(nonce1, nonce4);
    }

    #[test]
    fn test_empty_message() {
        let (alice_secret, alice_public) = generate_x25519_keypair();
        let (bob_secret, bob_public) = generate_x25519_keypair();

        let alice_did = KeyPair::generate().unwrap().did().clone();
        let bob_did = KeyPair::generate().unwrap().did().clone();

        // Encrypt empty message
        let plaintext = b"";
        let envelope = EncryptedEnvelope::encrypt(
            &alice_did,
            &bob_did,
            1,
            &alice_secret,
            &bob_public,
            plaintext,
        )
        .unwrap();

        // Should decrypt successfully
        let decrypted = envelope.decrypt(&bob_secret, &alice_public).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_large_message() {
        let (alice_secret, alice_public) = generate_x25519_keypair();
        let (bob_secret, bob_public) = generate_x25519_keypair();

        let alice_did = KeyPair::generate().unwrap().did().clone();
        let bob_did = KeyPair::generate().unwrap().did().clone();

        // Encrypt 1MB message
        let plaintext = vec![0x42u8; 1024 * 1024];
        let envelope = EncryptedEnvelope::encrypt(
            &alice_did,
            &bob_did,
            1,
            &alice_secret,
            &bob_public,
            &plaintext,
        )
        .unwrap();

        // Should decrypt successfully
        let decrypted = envelope.decrypt(&bob_secret, &alice_public).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encryption_key_derivation() {
        let shared_secret = [0x42u8; 32];

        let key1 = derive_encryption_key(&shared_secret);
        let key2 = derive_encryption_key(&shared_secret);

        // Same input produces same key
        assert_eq!(key1, key2);

        // Different input produces different key
        let different_secret = [0x43u8; 32];
        let key3 = derive_encryption_key(&different_secret);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_sequential_messages() {
        let (alice_secret, alice_public) = generate_x25519_keypair();
        let (bob_secret, bob_public) = generate_x25519_keypair();

        let alice_did = KeyPair::generate().unwrap().did().clone();
        let bob_did = KeyPair::generate().unwrap().did().clone();

        // Send multiple messages with increasing sequence numbers
        for seq in 1..=10 {
            let plaintext = format!("Message #{seq}");
            let envelope = EncryptedEnvelope::encrypt(
                &alice_did,
                &bob_did,
                seq,
                &alice_secret,
                &bob_public,
                plaintext.as_bytes(),
            )
            .unwrap();

            let decrypted = envelope.decrypt(&bob_secret, &alice_public).unwrap();
            assert_eq!(decrypted, plaintext.as_bytes());
        }
    }
}
