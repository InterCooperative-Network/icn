//! Connection candidate exchange for NAT traversal
//!
//! This module provides types for advertising and discovering connection endpoints
//! through the gossip protocol. Nodes publish their connection information (local
//! IP, STUN-discovered public IP, relay addresses) to help peers establish
//! connections even when both parties are behind NAT.

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Connection candidate announced by a node
///
/// This message is published to the `network:candidates` gossip topic
/// to advertise how other nodes can reach this peer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionCandidate {
    /// DID of the node advertising this candidate
    pub did: Did,

    /// Local (private) address the node is listening on
    ///
    /// This is useful for direct connections on the same local network.
    pub local_addr: SocketAddr,

    /// Public address discovered via STUN (if NAT traversal is enabled)
    ///
    /// This is the IP and port visible from the internet. Other nodes
    /// can attempt to connect to this address for NAT hole punching.
    pub public_addr: Option<SocketAddr>,

    /// Relay address (if this node is willing to act as a relay)
    ///
    /// Future: TURN server address or peer relay capability.
    /// For now, this is reserved and will be None.
    pub relay_addr: Option<SocketAddr>,

    /// Unix timestamp (seconds since epoch) when this candidate was created
    ///
    /// Helps peers determine freshness of connection information.
    pub timestamp: u64,

    /// Protocol version for future compatibility
    ///
    /// Set to 1 for initial implementation. Future versions may add
    /// additional fields or change semantics.
    pub version: u8,
}

impl ConnectionCandidate {
    /// Create a new connection candidate
    ///
    /// # Arguments
    /// * `did` - DID of the node
    /// * `local_addr` - Local address the node is listening on
    /// * `public_addr` - Public address discovered via STUN (if available)
    /// * `relay_addr` - Relay address (if available)
    pub fn new(
        did: Did,
        local_addr: SocketAddr,
        public_addr: Option<SocketAddr>,
        relay_addr: Option<SocketAddr>,
    ) -> Self {
        let timestamp = icn_time::current_timestamp_secs();

        Self {
            did,
            local_addr,
            public_addr,
            relay_addr,
            timestamp,
            version: 1,
        }
    }

    /// Check if this candidate is still fresh
    ///
    /// Candidates older than the given max_age (in seconds) are considered stale.
    /// Default recommendation: 300 seconds (5 minutes).
    pub fn is_fresh(&self, max_age_secs: u64) -> bool {
        let now = icn_time::current_timestamp_secs();
        now.saturating_sub(self.timestamp) <= max_age_secs
    }

    /// Get age of this candidate in seconds
    pub fn age_secs(&self) -> u64 {
        let now = icn_time::current_timestamp_secs();
        now.saturating_sub(self.timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_candidate_creation() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let local = "192.168.1.100:5000".parse().unwrap();
        let public = Some("203.0.113.5:5000".parse().unwrap());

        let candidate = ConnectionCandidate::new(did.clone(), local, public, None);

        assert_eq!(candidate.did, did);
        assert_eq!(candidate.local_addr, local);
        assert_eq!(candidate.public_addr, public);
        assert_eq!(candidate.relay_addr, None);
        assert_eq!(candidate.version, 1);
        assert!(candidate.timestamp > 0);
    }

    #[test]
    fn test_candidate_freshness() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let local = "192.168.1.100:5000".parse().unwrap();

        let candidate = ConnectionCandidate::new(did, local, None, None);

        // Fresh candidate
        assert!(candidate.is_fresh(300));
        assert_eq!(candidate.age_secs(), 0);

        // Simulate old candidate (manually set timestamp to 10 minutes ago)
        let mut old_candidate = candidate.clone();
        old_candidate.timestamp -= 600; // 10 minutes ago

        assert!(!old_candidate.is_fresh(300)); // Max 5 minutes
        assert!(old_candidate.age_secs() >= 600);
    }

    #[test]
    fn test_candidate_serialization() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let local = "192.168.1.100:5000".parse().unwrap();
        let public = Some("203.0.113.5:5000".parse().unwrap());

        let candidate = ConnectionCandidate::new(did.clone(), local, public, None);

        // Serialize to JSON
        let json = serde_json::to_string(&candidate).unwrap();
        assert!(json.contains(&did.to_string()));

        // Deserialize back
        let deserialized: ConnectionCandidate = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.did, candidate.did);
        assert_eq!(deserialized.local_addr, candidate.local_addr);
        assert_eq!(deserialized.public_addr, candidate.public_addr);
        assert_eq!(deserialized.timestamp, candidate.timestamp);
    }

    #[test]
    fn test_candidate_with_all_addresses() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let local = "10.0.0.5:5000".parse().unwrap();
        let public = Some("198.51.100.42:5000".parse().unwrap());
        let relay = Some("192.0.2.100:3478".parse().unwrap()); // Use IP instead of hostname

        let candidate = ConnectionCandidate::new(did.clone(), local, public, relay);

        assert_eq!(candidate.local_addr, local);
        assert_eq!(candidate.public_addr, public);
        assert_eq!(candidate.relay_addr, relay);
    }
}
