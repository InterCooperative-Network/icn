//! ICN Protocol - Unified gossip and networking layer
//!
//! This crate combines the gossip protocol and network transport into a single
//! protocol layer for ICN.
//!
//! # Gossip Protocol
//! 
//! The gossip module provides topic-based replication with:
//! - Vector clocks for causal ordering
//! - Bloom filters for efficient anti-entropy
//! - Topic-based routing with access control
//! - Hybrid push/pull sync protocol
//!
//! # Network Transport
//!
//! The net module provides QUIC/TLS transport with:
//! - DID-TLS binding
//! - mDNS discovery
//! - Session management
//! - Message encryption and signatures
//!
//! # Safety
//! This crate denies panicking on unwrap/expect to prevent runtime crashes.
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

// Gossip protocol modules
#[path = "gossip/mod.rs"]
pub mod gossip;

// Network transport modules
#[path = "net/mod.rs"]
pub mod net;
