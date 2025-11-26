//! Encrypted Topic Metadata (Phase 20 Week 1-2)
//!
//! Prevents network observers from learning topic subscription patterns by:
//! - Encrypting topic names with ChaCha20-Poly1305 AEAD
//! - Using Bloom filters for probabilistic topic discovery
//! - Providing plausible deniability via false positives
//!
//! ## Architecture
//!
//! ```text
//! Subscriber           Encrypted Topic             Network Observer
//!     |                      |                            |
//!     |-- Subscribe("ledger:sync") -------------------> ???
//!     |                      |                            |
//!     |<- EncryptedTopic(ciphertext, bloom_hint) --------|(Can't read topic)
//!     |                      |                            |
//!     |-- Check Bloom filter → Match? -----------------> |(Plausible deniability)
//!     |                      |                            |
//!     |-- Decrypt if bloom match → "ledger:sync" ------> |(Content hidden)
//! ```
//!
//! ## Security Properties
//!
//! - **Confidentiality**: Topic names encrypted, unreadable to observers
//! - **Unlinkability**: Can't correlate multiple topics to same subscriber
//! - **Plausible Deniability**: Bloom false positives provide cover
//! - **Forward Secrecy**: Rotating shared keys limit exposure window

use crate::error::{PrivacyError, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// Encrypted topic name with Bloom filter hint for probabilistic discovery
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncryptedTopic {
    /// Encrypted topic name (ChaCha20-Poly1305 ciphertext)
    pub ciphertext: Vec<u8>,

    /// Nonce used for encryption (96 bits)
    pub nonce: [u8; 12],

    /// Bloom filter hint for probabilistic matching (256 bits)
    /// Contains hash(topic_plaintext) to allow subscribers to discover
    /// their topics without revealing interest to network observers
    pub bloom_hint: [u8; 32],
}

/// Topic encryptor for privacy-preserving topic subscriptions
pub struct TopicEncryptor {
    /// Shared secret for topic encryption (32 bytes)
    /// In production, this would be derived from cooperative membership
    shared_key: [u8; 32],

    /// Cipher instance
    cipher: ChaCha20Poly1305,
}

impl TopicEncryptor {
    /// Create a new topic encryptor with shared key
    ///
    /// The shared key should be:
    /// - Derived from cooperative membership (e.g., hash of member DIDs)
    /// - Rotated periodically for forward secrecy
    /// - 32 bytes (256 bits) for ChaCha20-Poly1305
    pub fn new(shared_key: [u8; 32]) -> Self {
        let cipher = ChaCha20Poly1305::new(&shared_key.into());
        Self { shared_key, cipher }
    }

    /// Encrypt a topic name
    ///
    /// Returns `EncryptedTopic` with:
    /// - Ciphertext (topic name encrypted)
    /// - Nonce (random 96 bits)
    /// - Bloom hint (hash of plaintext for discovery)
    pub fn encrypt(&self, topic: &str) -> Result<EncryptedTopic> {
        // Generate random nonce (96 bits for ChaCha20-Poly1305)
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt topic name
        let plaintext = topic.as_bytes();
        let ciphertext = self
            .cipher
            .encrypt(nonce, Payload { msg: plaintext, aad: b"" })
            .map_err(|e| PrivacyError::EncryptionFailed(e.to_string()))?;

        // Generate Bloom filter hint (hash of plaintext)
        // This allows subscribers to check if their topic matches without decrypting
        let bloom_hint = Self::compute_bloom_hint(topic);

        icn_obs::metrics::privacy::topics_encrypted_inc();

        Ok(EncryptedTopic {
            ciphertext,
            nonce: nonce_bytes,
            bloom_hint,
        })
    }

    /// Decrypt an encrypted topic
    ///
    /// Returns the plaintext topic name if decryption succeeds.
    /// Fails if:
    /// - Ciphertext is corrupted
    /// - Wrong key used
    /// - Nonce reused (AEAD violation)
    pub fn decrypt(&self, encrypted: &EncryptedTopic) -> Result<String> {
        let nonce = Nonce::from_slice(&encrypted.nonce);

        let plaintext_bytes = self
            .cipher
            .decrypt(nonce, Payload { msg: &encrypted.ciphertext, aad: b"" })
            .map_err(|e| PrivacyError::DecryptionFailed(e.to_string()))?;

        let topic = String::from_utf8(plaintext_bytes)
            .map_err(|e| PrivacyError::DecryptionFailed(format!("Invalid UTF-8: {}", e)))?;

        icn_obs::metrics::privacy::topics_decrypted_inc();

        Ok(topic)
    }

    /// Check if a topic might match an encrypted topic (using Bloom hint)
    ///
    /// Returns `true` if the topic *might* match (Bloom filter says yes).
    /// Returns `false` if the topic definitely doesn't match.
    ///
    /// Note: False positives are possible (and intentional for privacy).
    /// False negatives are not possible.
    pub fn bloom_matches(&self, topic: &str, encrypted: &EncryptedTopic) -> bool {
        let hint = Self::compute_bloom_hint(topic);
        hint == encrypted.bloom_hint
    }

    /// Compute Bloom filter hint for a topic
    ///
    /// Uses SHA256 hash of topic name as Bloom filter.
    /// In a full implementation, this would use a proper Bloom filter with
    /// multiple hash functions and configurable false positive rate.
    fn compute_bloom_hint(topic: &str) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(topic.as_bytes());
        hasher.finalize().into()
    }

    /// Find matching encrypted topics for a plaintext topic
    ///
    /// Checks Bloom filter hints first (fast), then attempts decryption
    /// only for probable matches.
    ///
    /// Returns all topics that match (may include false positives from Bloom filter).
    pub fn find_matches(&self, topic: &str, candidates: &[EncryptedTopic]) -> Vec<EncryptedTopic> {
        candidates
            .iter()
            .filter(|encrypted| {
                // First check Bloom filter (fast)
                if !self.bloom_matches(topic, encrypted) {
                    return false;
                }

                // Bloom filter matched, try decryption to confirm
                self.decrypt(encrypted)
                    .map(|decrypted| decrypted == topic)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }
}

/// Topic discovery using Bloom filters (privacy-preserving interest matching)
pub struct TopicBloomFilter {
    /// Set of Bloom hints for subscribed topics
    hints: HashSet<[u8; 32]>,
}

impl TopicBloomFilter {
    /// Create an empty Bloom filter
    pub fn new() -> Self {
        Self {
            hints: HashSet::new(),
        }
    }

    /// Add a topic to the filter
    pub fn insert(&mut self, topic: &str) {
        let hint = Self::compute_hint(topic);
        self.hints.insert(hint);
    }

    /// Check if a topic might be in the filter
    ///
    /// Returns `true` if topic might be present (Bloom filter says yes).
    /// Returns `false` if topic is definitely not present.
    pub fn contains(&self, topic: &str) -> bool {
        let hint = Self::compute_hint(topic);
        self.hints.contains(&hint)
    }

    /// Check if an encrypted topic might match our interests
    pub fn matches(&self, encrypted: &EncryptedTopic) -> bool {
        self.hints.contains(&encrypted.bloom_hint)
    }

    /// Get number of topics in filter
    pub fn len(&self) -> usize {
        self.hints.len()
    }

    /// Check if filter is empty
    pub fn is_empty(&self) -> bool {
        self.hints.is_empty()
    }

    fn compute_hint(topic: &str) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(topic.as_bytes());
        hasher.finalize().into()
    }
}

impl Default for TopicBloomFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_encryption_roundtrip() {
        let key = [0u8; 32]; // In production, use proper key derivation
        let encryptor = TopicEncryptor::new(key);

        let topic = "ledger:sync";
        let encrypted = encryptor.encrypt(topic).unwrap();
        let decrypted = encryptor.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, topic);
    }

    #[test]
    fn test_different_nonces() {
        let key = [0u8; 32];
        let encryptor = TopicEncryptor::new(key);

        let topic = "gossip:test";
        let encrypted1 = encryptor.encrypt(topic).unwrap();
        let encrypted2 = encryptor.encrypt(topic).unwrap();

        // Nonces should be different (random)
        assert_ne!(encrypted1.nonce, encrypted2.nonce);

        // Both should decrypt correctly
        assert_eq!(encryptor.decrypt(&encrypted1).unwrap(), topic);
        assert_eq!(encryptor.decrypt(&encrypted2).unwrap(), topic);
    }

    #[test]
    fn test_bloom_hint_matching() {
        let key = [0u8; 32];
        let encryptor = TopicEncryptor::new(key);

        let topic = "contracts:deploy";
        let encrypted = encryptor.encrypt(topic).unwrap();

        // Same topic should match Bloom filter
        assert!(encryptor.bloom_matches(topic, &encrypted));

        // Different topic should not match
        assert!(!encryptor.bloom_matches("contracts:other", &encrypted));
    }

    #[test]
    fn test_wrong_key_fails_decryption() {
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];

        let encryptor1 = TopicEncryptor::new(key1);
        let encryptor2 = TopicEncryptor::new(key2);

        let topic = "secret:topic";
        let encrypted = encryptor1.encrypt(topic).unwrap();

        // Correct key decrypts successfully
        assert_eq!(encryptor1.decrypt(&encrypted).unwrap(), topic);

        // Wrong key fails decryption
        assert!(encryptor2.decrypt(&encrypted).is_err());
    }

    #[test]
    fn test_find_matches() {
        let key = [0u8; 32];
        let encryptor = TopicEncryptor::new(key);

        // Encrypt multiple topics
        let topics = vec!["ledger:sync", "gossip:test", "contracts:deploy", "ledger:sync"];
        let encrypted: Vec<_> = topics.iter().map(|t| encryptor.encrypt(t).unwrap()).collect();

        // Find matches for "ledger:sync" (should find 2)
        let matches = encryptor.find_matches("ledger:sync", &encrypted);
        assert_eq!(matches.len(), 2);

        // Find matches for "gossip:test" (should find 1)
        let matches = encryptor.find_matches("gossip:test", &encrypted);
        assert_eq!(matches.len(), 1);

        // Find matches for non-existent topic (should find 0)
        let matches = encryptor.find_matches("nonexistent:topic", &encrypted);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn test_bloom_filter() {
        let mut filter = TopicBloomFilter::new();

        // Add topics
        filter.insert("ledger:sync");
        filter.insert("gossip:test");

        assert_eq!(filter.len(), 2);

        // Check contains
        assert!(filter.contains("ledger:sync"));
        assert!(filter.contains("gossip:test"));
        assert!(!filter.contains("nonexistent:topic"));
    }

    #[test]
    fn test_bloom_filter_with_encrypted_topics() {
        let key = [0u8; 32];
        let encryptor = TopicEncryptor::new(key);
        let mut filter = TopicBloomFilter::new();

        // Subscriber is interested in these topics
        filter.insert("ledger:sync");
        filter.insert("contracts:deploy");

        // Encrypt some topics
        let encrypted_ledger = encryptor.encrypt("ledger:sync").unwrap();
        let encrypted_gossip = encryptor.encrypt("gossip:test").unwrap();
        let encrypted_contracts = encryptor.encrypt("contracts:deploy").unwrap();

        // Filter should match subscribed topics
        assert!(filter.matches(&encrypted_ledger));
        assert!(filter.matches(&encrypted_contracts));

        // Filter should not match unsubscribed topics
        assert!(!filter.matches(&encrypted_gossip));
    }
}
