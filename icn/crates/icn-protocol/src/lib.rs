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

#[cfg(test)]
mod tests {
    //! Smoke tests verifying facade re-exports work correctly.

    #[test]
    fn facade_exports_gossip_types() {
        // Verify gossip module types are accessible via facade
        let _: fn() -> &'static str = || {
            // GossipMessage exists in gossip module
            std::any::type_name::<crate::gossip::GossipMessage>()
        };
    }

    #[test]
    fn facade_exports_net_types() {
        // Verify net module types are accessible via facade
        let _: fn() -> &'static str = || {
            // NetworkMessage exists in net module
            std::any::type_name::<crate::net::NetworkMessage>()
        };
    }
}
