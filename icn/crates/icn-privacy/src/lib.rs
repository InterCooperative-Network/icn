//! ICN Privacy - Privacy primitives for metadata protection (Phase 20)
//!
//! Addresses ARCHITECTURE.md Gap 12.9 (Privacy & Metadata Leakage) by providing:
//!
//! ## Week 1-2: Encrypted Topic Metadata
//! - ChaCha20-Poly1305 AEAD encryption for topic names
//! - Bloom filter-based topic discovery (privacy-preserving)
//! - Plausible deniability via false positives
//!
//! ## Week 3-4: Onion Routing (Planned)
//! - Multi-hop message routing (Tor-like)
//! - Hide sender/receiver relationships
//! - Protect against traffic analysis
//!
//! ## Week 5-6: Traffic Obfuscation (Planned)
//! - Random message delays (timing attack resistance)
//! - Message size padding (hide content patterns)
//! - Cover traffic generation (decoy messages)
//!
//! ## Example
//!
//! ```rust
//! use icn_privacy::{TopicEncryptor, TopicBloomFilter};
//!
//! // Create encryptor with shared cooperative key
//! let shared_key = [0u8; 32]; // In production: derive from members
//! let encryptor = TopicEncryptor::new(shared_key);
//!
//! // Encrypt topic name (observer can't see "ledger:sync")
//! let encrypted = encryptor.encrypt("ledger:sync").unwrap();
//!
//! // Subscriber can check if their topic matches without decrypting
//! let mut filter = TopicBloomFilter::new();
//! filter.insert("ledger:sync");
//! assert!(filter.matches(&encrypted));
//!
//! // Decrypt to get original topic
//! let decrypted = encryptor.decrypt(&encrypted).unwrap();
//! assert_eq!(decrypted, "ledger:sync");
//! ```
//!
//! ## Security Properties
//!
//! - **Confidentiality**: Topic names encrypted, unreadable to network observers
//! - **Unlinkability**: Can't correlate multiple encrypted topics to same subscriber
//! - **Plausible Deniability**: Bloom filter false positives provide cover
//! - **Forward Secrecy**: Rotating shared keys limit exposure window (when implemented)

pub mod error;
pub mod topic_encryption;

// Week 3-4: Onion routing (production-ready)
pub mod onion_routing;

// Week 5-6: Traffic obfuscation
pub mod traffic_obfuscation;

pub use error::{PrivacyError, Result};
pub use onion_routing::{select_relays, Circuit, OnionMessage, OnionRouter};
pub use topic_encryption::{EncryptedTopic, TopicBloomFilter, TopicEncryptor};
pub use traffic_obfuscation::{ObfuscatedMessage, ObfuscationConfig, TrafficObfuscator};
