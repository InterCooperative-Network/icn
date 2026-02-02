//! ICN Protocol - Unified protocol layer
//!
//! This crate provides a unified interface to ICN's gossip and networking layers.
//! It re-exports `icn-gossip` and `icn-net` under a single namespace.
//!
//! # Modules
//!
//! - `gossip`: Topic-based gossip protocol with ACLs
//! - `net`: Network transport, discovery, and session management
//!
//! # Example
//!
//! ```rust,ignore
//! use icn_protocol::{gossip::GossipActor, net::NetworkActor};
//! ```

#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// Gossip protocol re-exports
pub use icn_gossip as gossip;

/// Network transport re-exports
pub use icn_net as net;
